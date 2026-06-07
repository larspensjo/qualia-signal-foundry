use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;

use super::manifest::{ContinuityManifest, ResumeMode};
use super::{
    LiveSessionEvent, MemorySourceConfig, PromptPrefixInvalidation, SessionConfig, SessionEvent,
    SessionLimit, SessionState, reduce_live_session,
};

pub struct SessionBootRequest {
    pub resume_state_dir: PathBuf,
    pub persist_state_dir: PathBuf,
    pub config: SessionConfig,
    pub legacy_fallback_used: bool,
}

pub struct BootedSession {
    pub state: SessionState,
    pub resume_inputs: super::resume::ResumeInputs,
    pub resume_mode: ResumeMode,
    pub classified_resume_mode: ResumeMode,
    pub previous_session_id: Option<String>,
    pub pending_boot_brief: Option<crate::sleep::commit::ConsolidatedBrief>,
}

pub fn boot_session(
    context: &mut RunContext,
    request: SessionBootRequest,
) -> anyhow::Result<BootedSession> {
    let SessionBootRequest {
        resume_state_dir,
        persist_state_dir,
        config,
        legacy_fallback_used,
    } = request;
    let resume_inputs = super::resume::load_resume_inputs(&resume_state_dir)?;
    let classified_resume_mode = super::resume::classify_resume_mode(&resume_inputs);
    let config_changed = resume_inputs
        .previous_session
        .as_ref()
        .map(|session| resume_breaking_config_changed(&session.config, &config))
        .unwrap_or(false);
    let downgraded_for_config =
        matches!(classified_resume_mode, ResumeMode::AwakeContinuation) && config_changed;
    let resume_mode = if downgraded_for_config {
        ResumeMode::ColdStart
    } else {
        classified_resume_mode
    };
    let previous_session_id = resume_inputs
        .previous_session
        .as_ref()
        .map(|session| session.session_id.clone());
    let brief_path = resume_inputs.manifest.last_sleep_brief_path.clone();
    let mut pending_boot_brief: Option<crate::sleep::commit::ConsolidatedBrief> = None;
    let mut state = match resume_mode {
        ResumeMode::ColdStart => SessionState::new(config.clone()),
        ResumeMode::AwakeContinuation => {
            let previous = resume_inputs
                .previous_session
                .clone()
                .context("awake continuation requires a previous session")?;
            super::continuation::prepare_awake_continuation(previous, &config)
        }
        ResumeMode::ConsolidatedBrief => {
            if let Some(path) = &brief_path {
                let absolute_path = if path.is_absolute() {
                    path.clone()
                } else {
                    resume_state_dir.join(path)
                };
                if absolute_path.exists() {
                    let raw = fs::read_to_string(&absolute_path).with_context(|| {
                        format!(
                            "failed to read consolidated brief `{}`",
                            absolute_path.display()
                        )
                    })?;
                    pending_boot_brief = Some(serde_json::from_str(&raw).with_context(|| {
                        format!(
                            "failed to parse consolidated brief `{}`",
                            absolute_path.display()
                        )
                    })?);
                }
            }

            let mut fresh = SessionState::new(config.clone());
            fresh.previous_session_id = previous_session_id.clone();
            fresh
        }
    };

    context.record_event(
        EventType::SessionResumed,
        json!({
            "mode": resume_mode,
            "classified_mode": classified_resume_mode,
            "config_changed": config_changed,
            "downgraded_for_config": downgraded_for_config,
            "session_id": state.session_id.clone(),
            "previous_session_id": previous_session_id,
            "brief_path": brief_path,
            "resume_state_dir": resume_state_dir.display().to_string(),
            "persist_state_dir": persist_state_dir.display().to_string(),
            "legacy_fallback_used": legacy_fallback_used,
        }),
        None,
    )?;
    engine_logging::engine_info!(
        "session boot resolved: experiment_id={} run_id={} session_id={} resume_mode={:?} resume_state_dir={} persist_state_dir={} legacy_fallback_used={}",
        context.experiment_id(),
        context.run_id(),
        state.session_id,
        resume_mode,
        resume_state_dir.display(),
        persist_state_dir.display(),
        legacy_fallback_used
    );

    apply_session_event(
        context,
        &mut state,
        SessionEvent::SessionStarted(config.clone()),
    )?;
    apply_live_session_event(
        &mut state,
        if matches!(resume_mode, ResumeMode::AwakeContinuation) {
            LiveSessionEvent::SessionResumed
        } else {
            LiveSessionEvent::SessionStarted
        },
    );

    Ok(BootedSession {
        state,
        resume_inputs,
        resume_mode,
        classified_resume_mode,
        previous_session_id,
        pending_boot_brief,
    })
}

pub fn resume_breaking_config_changed(previous: &SessionConfig, current: &SessionConfig) -> bool {
    previous.model_id != current.model_id
        || previous.max_turns != current.max_turns
        || previous.warm_threshold != current.warm_threshold
        || previous.memory_source != current.memory_source
}

pub fn reduce_session(mut state: SessionState, event: SessionEvent) -> SessionState {
    reduce_session_in_place(&mut state, event);
    state
}

pub fn reduce_session_in_place(state: &mut SessionState, event: SessionEvent) {
    match event {
        SessionEvent::SessionStarted(config) => {
            state.config = config;
        }
        SessionEvent::InputReceived { input } => {
            state.last_input = Some(input);
            state.last_model_error = None;
        }
        SessionEvent::MemoryRetrieved | SessionEvent::ContextAssembled(_) => {}
        SessionEvent::PromptAssembled {
            full_request_hash, ..
        } => {
            state.last_prompt_hash = Some(full_request_hash);
            state.prefix_invalidated_since_last_prompt = false;
        }
        SessionEvent::ModelRoleCompleted { .. } => {
            state.last_model_error = None;
        }
        SessionEvent::ModelRoleFailed { error_summary } => {
            state.last_model_error = Some(error_summary);
        }
        SessionEvent::TurnCompleted(turn) => {
            state.turns.push(turn);
        }
        SessionEvent::PromptPrefixInvalidated {
            after_turn_index,
            reason,
        } => {
            state
                .prompt_prefix_invalidations
                .push(PromptPrefixInvalidation {
                    after_turn_index,
                    reason,
                });
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::ExchangeRecorded { exchange, .. } => {
            state.exchanges.push(*exchange);
        }
        SessionEvent::TurnSummarized(summary) => {
            state.summarized_turns.push(summary);
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::TurnsAgedAndCoRetrieved {
            range, summaries, ..
        } => {
            debug_assert!(range.last_index >= range.first_index);
            assert_eq!(
                summaries.len(),
                range.last_index + 1 - range.first_index,
                "TurnsAgedAndCoRetrieved summaries must match the aged range"
            );
            state.summarized_turns.extend(summaries);
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::ToolCompleted(_) => {}
        SessionEvent::SessionLimitReached {
            current,
            max,
            override_active,
        } => {
            state.limit_reached = Some(SessionLimit {
                current,
                max,
                override_active,
            });
        }
        SessionEvent::SessionEnded { reason } => {
            state.ended_reason = Some(reason);
        }
    }
}

pub fn apply_session_event(
    context: &mut RunContext,
    state: &mut SessionState,
    event: SessionEvent,
) -> anyhow::Result<()> {
    reduce_session_in_place(state, event.clone());
    record_session_event(context, &event)?;
    Ok(())
}

pub fn apply_live_session_event(state: &mut SessionState, event: LiveSessionEvent) {
    let live_state = std::mem::take(&mut state.live);
    state.live = reduce_live_session(live_state, event);
}

pub fn record_session_event(context: &mut RunContext, event: &SessionEvent) -> anyhow::Result<()> {
    match event {
        SessionEvent::SessionStarted(config) => {
            context.record_event(EventType::SessionStarted, json!({ "config": config }), None)?;
        }
        SessionEvent::InputReceived { input } => {
            context.record_event(
                EventType::InputReceived,
                json!({
                    "session_id": context.run_id(),
                    "input": input,
                    "input_chars": input.chars().count(),
                }),
                None,
            )?;
        }
        SessionEvent::MemoryRetrieved => {}
        SessionEvent::ContextAssembled(assembly) => {
            context.record_event(
                EventType::ContextAssembled,
                json!({
                    "session_id": context.run_id(),
                    "selected_count": assembly.selected.len(),
                    "omitted_count": assembly.omitted.len(),
                    "used_estimated_tokens": assembly.used_estimated_tokens,
                    "selected": &assembly.selected,
                    "omitted": &assembly.omitted,
                }),
                None,
            )?;
        }
        SessionEvent::PromptAssembled {
            full_request_hash,
            message_count,
            total_bytes,
        } => {
            context.record_event(
                EventType::PromptAssembled,
                json!({
                    "session_id": context.run_id(),
                    "full_request_hash": full_request_hash.hex(),
                    "message_count": message_count,
                    "total_bytes": total_bytes,
                }),
                None,
            )?;
        }
        SessionEvent::ModelRoleCompleted { .. } => {}
        SessionEvent::ModelRoleFailed { .. } => {}
        SessionEvent::TurnCompleted(turn) => {
            context.record_event(
                EventType::TurnCompleted,
                json!({
                    "session_id": context.run_id(),
                    "turn": turn,
                    "full_request_hash": turn.full_request_hash.hex(),
                }),
                None,
            )?;
        }
        SessionEvent::PromptPrefixInvalidated {
            after_turn_index,
            reason,
        } => {
            context.record_event(
                EventType::PromptPrefixInvalidated,
                json!({
                    "session_id": context.run_id(),
                    "after_turn_index": after_turn_index,
                    "reason": reason,
                }),
                None,
            )?;
        }
        SessionEvent::ExchangeRecorded {
            session_id,
            exchange,
        } => {
            engine_logging::engine_info!(
                "exchange recorded: experiment_id={} run_id={} session_id={} exchange_index={} status={:?}",
                context.experiment_id(),
                context.run_id(),
                session_id,
                exchange.index,
                exchange.status
            );
        }
        SessionEvent::TurnSummarized(summary) => {
            context.record_event(
                EventType::TurnSummarized,
                json!({
                    "session_id": context.run_id(),
                    "turn_index": summary.turn_index,
                    "summary": summary,
                }),
                None,
            )?;
        }
        SessionEvent::TurnsAgedAndCoRetrieved {
            range,
            new_associations,
            strengthened_associations,
            persisted_at,
            summaries,
        } => {
            context.record_event(
                EventType::TurnsAgedAndCoRetrieved,
                json!({
                    "session_id": context.run_id(),
                    "range": range,
                    "new_associations": new_associations,
                    "strengthened_associations": strengthened_associations,
                    "persisted_at": persisted_at,
                    "summary_count": summaries.len(),
                    "summaries": summaries,
                }),
                None,
            )?;
        }
        SessionEvent::ToolCompleted(recall) => {
            context.record_event(
                EventType::ToolCompleted,
                json!({
                    "session_id": context.run_id(),
                    "tool_name": &recall.tool_name,
                    "call_id": &recall.call_id,
                    "turn_id": recall.turn_id,
                    "category": recall.category,
                    "side_effect_level": recall.side_effect_level,
                    "latency_ms": recall.latency_ms,
                    "scope": "multi_turn_text_loop",
                }),
                None,
            )?;
        }
        SessionEvent::SessionLimitReached {
            current,
            max,
            override_active,
        } => {
            context.record_event(
                EventType::SessionLimitReached,
                json!({
                    "session_id": context.run_id(),
                    "current": current,
                    "max": max,
                    "override_active": override_active,
                }),
                None,
            )?;
        }
        SessionEvent::SessionEnded { reason } => {
            context.record_event(
                EventType::SessionEnded,
                json!({
                    "session_id": context.run_id(),
                    "reason": reason,
                }),
                None,
            )?;
        }
    }

    Ok(())
}

pub fn persist_continuity_state(
    state: &SessionState,
    state_dir: &Path,
    previous_manifest: &ContinuityManifest,
) -> anyhow::Result<()> {
    persist_continuity_state_from_dirs(state, state_dir, state_dir, previous_manifest)
}

pub fn persist_continuity_state_from_dirs(
    state: &SessionState,
    resume_state_dir: &Path,
    persist_state_dir: &Path,
    previous_manifest: &ContinuityManifest,
) -> anyhow::Result<()> {
    copy_forward_memory_store(resume_state_dir, persist_state_dir)?;
    let state_path = super::persistence::persist_session_state(state, persist_state_dir)?;
    let mut manifest = previous_manifest.clone();
    manifest.current_session_id = Some(state.session_id.clone());
    manifest.current_session_state_path = Some(
        state_path
            .strip_prefix(persist_state_dir)
            .unwrap_or(&state_path)
            .to_path_buf(),
    );
    manifest.sleep_pending = true;
    manifest.resume_mode = ResumeMode::AwakeContinuation;
    manifest.persist(persist_state_dir.join("continuity-manifest.json"))?;
    Ok(())
}

fn copy_forward_memory_store(
    resume_state_dir: &Path,
    persist_state_dir: &Path,
) -> anyhow::Result<()> {
    if resume_state_dir == persist_state_dir {
        return Ok(());
    }

    let source_path = resume_state_dir.join("memory-store.json");
    if !source_path.exists() {
        return Ok(());
    }

    let destination_path = persist_state_dir.join("memory-store.json");
    let source = crate::memory::MemoryStore::load_or_empty(&source_path)?;
    let destination = crate::memory::MemoryStore::load_or_empty(&destination_path)?;
    let mut merged = source.contents().clone();
    merge_memory_store_contents(&mut merged, destination.contents().clone());

    let mut merged_store = crate::memory::MemoryStore::load_or_empty(destination_path)?;
    *merged_store.contents_mut() = merged;
    merged_store.persist()
}

fn merge_memory_store_contents(
    target: &mut crate::memory::MemoryStoreContents,
    incoming: crate::memory::MemoryStoreContents,
) {
    for record in incoming.records {
        match target
            .records
            .iter()
            .position(|existing| existing.id == record.id)
        {
            Some(index) => target.records[index] = record,
            None => target.records.push(record),
        }
    }

    for association in incoming.associations {
        match target.associations.iter().position(|existing| {
            existing.from_memory_id == association.from_memory_id
                && existing.to_memory_id == association.to_memory_id
        }) {
            Some(index) => target.associations[index] = association,
            None => target.associations.push(association),
        }
    }

    for range in incoming.processed_ranges {
        let already_present = target.processed_ranges.iter().any(|existing| {
            existing.session_id == range.session_id
                && existing.first_turn_index == range.first_turn_index
                && existing.last_turn_index == range.last_turn_index
                && existing.kind == range.kind
        });
        if !already_present {
            target.processed_ranges.push(range);
        }
    }
}

pub fn format_boot_brief_for_context(brief: &crate::sleep::commit::ConsolidatedBrief) -> String {
    let mut text = String::new();
    text.push_str("Previous session summary:\n");
    text.push_str(&brief.previous_session_summary);
    text.push('\n');

    if !brief.future_context_hints.is_empty() {
        text.push_str("\nFuture context hints:\n");
        for hint in &brief.future_context_hints {
            text.push_str("- ");
            text.push_str(hint);
            text.push('\n');
        }
    }

    if !brief.open_questions.is_empty() {
        text.push_str("\nOpen questions:\n");
        for question in &brief.open_questions {
            text.push_str("- ");
            text.push_str(question);
            text.push('\n');
        }
    }

    text
}

#[allow(dead_code)]
fn _assert_memory_source_config_is_shared(_: &MemorySourceConfig) {}
