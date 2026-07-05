use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::resume::ResumeInputs;
use crate::session::{SessionState, SleepRecord, StateDirectoryResolution};
use crate::sleep::change_view::{
    NewAssociationChange, NewMemoryChange, SleepChangeRecord, SleepStateOutcome,
    StrengthenedAssociationChange,
};
use crate::sleep::{SleepInputBundle, SleepReport, summarize_session};

const SESSION_TEXT: &str = "Session transcript:\n- We introduced typed model roles and a deterministic mock model.\n- The runtime still routes observable behavior through explicit events and traces.\n- Sleep phase should summarize the session, extract candidate memories, surface open questions, and keep decision candidates provisional.\n- Nothing in the sleep phase should silently become an accepted decision.";
const SLEEP_UPDATE_ID: &str = "sleep-update";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SleepUpdateOptions {
    pub state_dir: PathBuf,
    pub requested_provider: String,
    pub workspace_root: Option<PathBuf>,
}

impl SleepUpdateOptions {
    pub fn realtime(
        requested_provider: impl Into<String>,
        workspace_root: Option<PathBuf>,
    ) -> Self {
        Self {
            state_dir: PathBuf::from("state/realtime"),
            requested_provider: requested_provider.into(),
            workspace_root,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SleepUpdateOutcome {
    pub summary: String,
    pub observations: Vec<String>,
    pub failure_modes: Vec<String>,
    pub follow_up_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
    pub extra_artifacts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SleepUpdateRunSummary {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub state_dir: PathBuf,
    pub status: String,
    pub event_count: usize,
    pub trace_count: usize,
    pub outcome: SleepUpdateOutcome,
    pub change_record: SleepChangeRecord,
}

pub fn run_sleep_update(options: SleepUpdateOptions) -> anyhow::Result<SleepUpdateRunSummary> {
    // The realtime server nests continuity state under `<state_dir>/continuity/<session_id>/`,
    // so the raw `--state-dir` (e.g. `state/realtime`) is a root, not the session directory.
    // Resolve it to the concrete session dir; a flat layout resolves to itself, and an absent
    // session falls back to the raw dir so the smoke path still reports `NoPersistedSession`.
    let session_state_dir = qsf_session::resolve_continuity_session_dir(&options.state_dir)?
        .session_dir()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| options.state_dir.clone());
    let state_resolution = StateDirectoryResolution {
        resume_state_dir: session_state_dir.clone(),
        persist_state_dir: session_state_dir,
        legacy_fallback_used: false,
    };
    let mut context = RunContext::create_in_with_workspace_root(
        "runs",
        SLEEP_UPDATE_ID,
        options.workspace_root.as_deref(),
    )?;
    context.initialize_engine_logging();

    engine_logging::engine_info!(
        "starting sleep update: run_id={} state_dir={} provider={}",
        context.run_id(),
        options.state_dir.display(),
        options.requested_provider
    );

    let started_at = Instant::now();
    context.record_event(
        EventType::ExperimentStarted,
        json!({
            "run_id": context.run_id(),
            "command": "sleep",
            "state_dir": options.state_dir,
            "requested_provider": options.requested_provider,
        }),
        None,
    )?;

    let (outcome, change_record) = match run_sleep_update_with_context(
        &mut context,
        &options.requested_provider,
        &state_resolution,
    ) {
        Ok((outcome, change_record)) => (outcome, change_record),
        Err(error) => {
            let error_message = error.to_string();
            engine_logging::engine_error!(
                "sleep update failed: run_id={} error={}",
                context.run_id(),
                error_message
            );
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "error": error_message,
                }),
                None,
            )?;
            context.record_event(
                EventType::ExperimentCompleted,
                json!({
                    "status": "failed",
                    "elapsed_ms": started_at.elapsed().as_millis(),
                }),
                None,
            )?;

            return Err(error).with_context(|| {
                format!(
                    "sleep update failed; run artifacts: {}",
                    context.run_dir().display()
                )
            });
        }
    };

    context.record_event(
        EventType::ExperimentCompleted,
        json!({
            "status": "completed",
            "elapsed_ms": started_at.elapsed().as_millis(),
        }),
        None,
    )?;

    engine_logging::engine_info!(
        "completed sleep update: run_id={} state_dir={}",
        context.run_id(),
        state_resolution.persist_state_dir.display()
    );

    let changes_path = context.run_dir().join("sleep-changes.json");
    fs::write(&changes_path, serde_json::to_string_pretty(&change_record)?).with_context(|| {
        format!(
            "failed to write sleep changes JSON for run {}",
            context.run_id()
        )
    })?;

    Ok(SleepUpdateRunSummary {
        run_id: context.run_id().to_string(),
        run_dir: context.run_dir().to_path_buf(),
        state_dir: state_resolution.persist_state_dir,
        status: "completed".to_string(),
        event_count: context.event_count(),
        trace_count: context.trace_count(),
        outcome,
        change_record,
    })
}

pub(crate) fn run_sleep_update_with_context(
    context: &mut RunContext,
    requested_provider: &str,
    state_resolution: &StateDirectoryResolution,
) -> anyhow::Result<(SleepUpdateOutcome, SleepChangeRecord)> {
    let resume_inputs =
        crate::session::resume::load_resume_inputs(&state_resolution.resume_state_dir)?;
    let input = build_sleep_input(&resume_inputs);

    context.record_event(
        EventType::InputReceived,
        json!({
            "source_kind": input.source_kind,
            "source_label": input.source_label,
            "source_excerpt": input.source_excerpt(120),
            "review_note_count": input.review_notes.len(),
            "requested_provider": requested_provider,
        }),
        None,
    )?;

    context.record_event(
        EventType::SleepPhaseRequested,
        json!({
            "source_kind": input.source_kind,
            "source_label": input.source_label,
            "requested_provider": requested_provider,
            "review_required": true,
        }),
        None,
    )?;

    let client = qsf_models::build_client(requested_provider)?;
    let started_at = Instant::now();
    let summary = summarize_session(context, client.as_ref(), &input)?;
    let elapsed_ns = elapsed_ns(started_at);

    let sleep_trace = TraceRecord::new(
        context.experiment_id(),
        "sleep-phase-summary",
        format!(
            "source={} provider={} excerpt={}",
            input.source_label,
            requested_provider,
            input.source_excerpt(72)
        ),
        summary.report.counts_summary(),
    )
    .with_details(json!({
        "input": &input,
        "provider_name": &summary.response.provider_name,
        "model_name": &summary.response.model_name,
        "usage": &summary.response.usage,
        "report": &summary.report,
    }))
    .with_latency_ns(elapsed_ns);
    let sleep_trace_id = sleep_trace.trace_id;

    context.record_trace(sleep_trace)?;
    context.record_event(
        EventType::SleepPhaseCompleted,
        json!({
            "provider_name": &summary.response.provider_name,
            "model_name": &summary.response.model_name,
            "memory_candidate_count": summary.report.memory_candidates.len(),
            "association_candidate_count": summary.report.association_candidates.len(),
            "open_question_count": summary.report.open_questions.len(),
            "decision_candidate_count": summary.report.decision_candidates.len(),
            "future_context_hint_count": summary.report.future_context_hints.len(),
            "review_note_count": summary.report.review_notes.len(),
            "latency_ns": elapsed_ns,
            "latency_ms": elapsed_ns / 1_000_000,
        }),
        Some(sleep_trace_id),
    )?;

    write_sleep_artifacts(context, requested_provider, &input, &summary.report)?;

    context.record_event(
        EventType::OutputProduced,
        json!({
            "message": &summary.report.session_summary,
            "memory_candidate_count": summary.report.memory_candidates.len(),
            "decision_candidate_count": summary.report.decision_candidates.len(),
        }),
        Some(sleep_trace_id),
    )?;

    let outcome = SleepUpdateOutcome {
        summary: format!(
            "The sleep update summarized `{}` through the `{}` provider, wrote a reviewable sleep report artifact, and recorded explicit sleep-phase events and traces.",
            input.source_label, summary.response.provider_name
        ),
        observations: vec![
            "The sleep phase reuses the model-role path, so provider selection, latency, and usage stay observable without special-case plumbing.".to_string(),
            "Sleep reports preserve separate fields for summary, memory candidates, open questions, decision candidates, and future context hints.".to_string(),
            format!("The sleep input source was `{}` ({}) rather than hidden state.", input.source_label, input.source_kind),
            "Review notes are carried into the artifact so the output stays explicitly provisional.".to_string(),
        ],
        failure_modes: vec![
            "The default mock provider keeps the command deterministic, but real-provider output quality still depends on prompt compliance and structured JSON output.".to_string(),
            "Cold-start runs with no persisted prior session still use a short inline transcript so the smoke path remains runnable.".to_string(),
        ],
        follow_up_questions: vec![
            "Should sleep summaries later ingest raw event logs directly instead of preassembled transcript text?".to_string(),
            "Should decision candidates eventually become structured objects with rationale and source references instead of plain strings?".to_string(),
        ],
        decision_candidates: vec![
            "Keep sleep-phase outputs explicitly reviewable artifacts rather than direct state mutations.".to_string(),
            "Reuse the shared model-role invocation path for sleep summarization until a distinct effect boundary is required.".to_string(),
        ],
        extra_artifacts: vec!["sleep-report.json".to_string(), "sleep-report.md".to_string()],
    };

    commit_cross_session_sleep(
        context,
        requested_provider,
        &summary.report,
        outcome,
        state_resolution,
    )
}

pub(crate) fn commit_cross_session_sleep(
    context: &mut RunContext,
    requested_provider: &str,
    report: &SleepReport,
    mut outcome: SleepUpdateOutcome,
    state_resolution: &StateDirectoryResolution,
) -> anyhow::Result<(SleepUpdateOutcome, SleepChangeRecord)> {
    let resume_inputs =
        crate::session::resume::load_resume_inputs(&state_resolution.resume_state_dir)?;
    let Some(session) = resume_inputs.previous_session.clone() else {
        return Ok((
            outcome,
            SleepChangeRecord {
                state_outcome: SleepStateOutcome::NoPersistedSession,
                session_id: None,
                state_dir: state_resolution.persist_state_dir.display().to_string(),
                new_memories: vec![],
                skipped_duplicates: vec![],
                new_associations: vec![],
                strengthened_associations: vec![],
                admitted_goal_id: None,
                declined_goal_candidate_id: None,
                swept_goal_ids: vec![],
                open_question_count: report.open_questions.len(),
                decision_candidate_count: report.decision_candidates.len(),
                state_files_written: vec![],
            },
        ));
    };

    if resume_inputs
        .manifest
        .last_sleep_consumed_session_id
        .as_deref()
        == Some(session.session_id.as_str())
        && !resume_inputs.manifest.sleep_pending
    {
        outcome.observations.push(format!(
            "Session `{}` was already consumed by sleep; state was left unchanged.",
            session.session_id
        ));
        return Ok((
            outcome,
            SleepChangeRecord {
                state_outcome: SleepStateOutcome::AlreadyConsumed,
                session_id: Some(session.session_id.clone()),
                state_dir: state_resolution.persist_state_dir.display().to_string(),
                new_memories: vec![],
                skipped_duplicates: vec![],
                new_associations: vec![],
                strengthened_associations: vec![],
                admitted_goal_id: None,
                declined_goal_candidate_id: None,
                swept_goal_ids: vec![],
                open_question_count: report.open_questions.len(),
                decision_candidate_count: report.decision_candidates.len(),
                state_files_written: vec![],
            },
        ));
    }

    let as_of = latest_sleep_record_completion(&session)
        .map(time::OffsetDateTime::from)
        .unwrap_or_else(time::OffsetDateTime::now_utc);
    let sleep_run_id = context.run_id().to_string();
    if state_resolution.resume_state_dir != state_resolution.persist_state_dir {
        crate::session::persist_continuity_state_from_dirs(
            &session,
            &state_resolution.resume_state_dir,
            &state_resolution.persist_state_dir,
            &resume_inputs.manifest,
        )?;
    }

    let state_dir = &state_resolution.persist_state_dir;
    let store_path = state_dir.join("memory-store.json");
    let mut store = crate::memory::MemoryStore::load_or_empty(&store_path)?;
    let plan = crate::sleep::auto_promote::build_promotion_plan(
        report,
        &session,
        store.contents(),
        as_of,
        &sleep_run_id,
    );

    let new_memories = plan
        .new_records
        .iter()
        .map(|record| NewMemoryChange {
            id: record.id.clone(),
            title: record.title.clone(),
            importance: record.importance,
        })
        .collect::<Vec<_>>();
    let new_association_changes = plan
        .new_associations
        .iter()
        .map(|association| NewAssociationChange {
            from_id: association.from_memory_id.clone(),
            to_id: association.to_memory_id.clone(),
            weight: association.weight,
            reason: association.reason.clone(),
        })
        .collect::<Vec<_>>();
    let skipped_duplicates = plan.skipped_duplicates.clone();

    store.append_records(plan.new_records.clone());
    store.append_associations(plan.new_associations.clone());
    store
        .contents_mut()
        .processed_ranges
        .extend(plan.processed_ranges.clone());
    let mut strengthened_changes = Vec::new();
    for (from_id, to_id, new_weight) in &plan.strengthened_associations {
        if let Some(existing) = store
            .contents_mut()
            .associations
            .iter_mut()
            .find(|association| {
                association.from_memory_id == *from_id && association.to_memory_id == *to_id
            })
        {
            strengthened_changes.push(StrengthenedAssociationChange {
                from_id: from_id.clone(),
                to_id: to_id.clone(),
                old_weight: existing.weight,
                new_weight: *new_weight,
            });
            existing.weight = *new_weight;
            existing.last_reinforced_at = as_of;
        }
    }

    crate::memory::reviewed_memory_draft::write_decision_candidates_draft(
        &report.decision_candidates,
        &sleep_run_id,
        context.run_dir(),
        as_of,
    )?;

    let promoted_count = plan.new_records.len();
    let new_associations_count = plan.new_associations.len();
    let brief = crate::sleep::commit::ConsolidatedBrief {
        previous_session_summary: report.session_summary.clone(),
        future_context_hints: report.future_context_hints.clone(),
        open_questions: report.open_questions.clone(),
        promoted_count,
        new_associations_count,
    };

    let whole_history_input = session_sleep_input(&session).session_text;
    let maintenance_client = qsf_models::build_client(requested_provider)?;
    let mut admitted_goal_id = None;
    let mut declined_goal_candidate_id = None;
    let mut swept_goal_ids = Vec::new();
    let mut volition_snapshot_path = None;
    if let Some(maintenance) = crate::experiments::run_sleep_volition_goal_maintenance(
        context,
        maintenance_client.as_ref(),
        state_dir,
        &session.session_id,
        &whole_history_input,
    )? {
        admitted_goal_id = maintenance.admitted_goal_id.clone();
        volition_snapshot_path = maintenance.persisted_snapshot_path.clone();
        declined_goal_candidate_id = maintenance
            .declined_candidate
            .as_ref()
            .map(|candidate| candidate.candidate_id.clone());
        swept_goal_ids = maintenance.swept_goal_ids.clone();
        if let Some(admitted) = &maintenance.admitted_goal_id {
            outcome.observations.push(format!(
                "Sleep whole-history formation admitted durable goal `{admitted}`."
            ));
        }
        if let Some(declined) = &maintenance.declined_candidate {
            outcome.observations.push(format!(
                "Sleep whole-history formation declined candidate `{}`.",
                declined.candidate_id
            ));
        }
        if !maintenance.swept_goal_ids.is_empty() {
            outcome.observations.push(format!(
                "Sleep whole-set sweep cancelled goal(s): {}.",
                maintenance.swept_goal_ids.join(", ")
            ));
        }
    }

    crate::sleep::commit::SleepCommit {
        state_dir,
        new_store_contents: store.contents().clone(),
        brief,
        sleep_run_id: sleep_run_id.clone(),
        consumed_session_id: session.session_id.clone(),
        brief_archive_name: format!("sleep-{sleep_run_id}.json"),
    }
    .write()?;

    let mut state_files_written = vec![
        state_dir.join("memory-store.json").display().to_string(),
        state_dir
            .join("consolidated-brief.json")
            .display()
            .to_string(),
        state_dir
            .join("archive")
            .join(format!("sleep-{sleep_run_id}.json"))
            .display()
            .to_string(),
        state_dir
            .join("continuity-manifest.json")
            .display()
            .to_string(),
    ];

    // On a consumed realtime session with volition continuity, the maintenance pass re-persists
    // the continuity snapshot inside the state dir; list it so the change view is complete.
    if let Some(snapshot_path) = volition_snapshot_path {
        state_files_written.push(snapshot_path.display().to_string());
    }

    if let Some(report) =
        crate::experiments::consolidate_session_volition(state_dir, &session.session_id)?
    {
        let report_json = serde_json::to_string_pretty(&report)
            .context("failed to serialize volition continuity report")?;
        let report_json_path = state_dir.join("volition-continuity-report.json");
        let report_md_path = state_dir.join("volition-continuity-report.md");
        fs::write(&report_json_path, report_json)
            .context("failed to write volition continuity report json")?;
        fs::write(
            &report_md_path,
            crate::experiments::render_volition_continuity_markdown_report(&report),
        )
        .context("failed to write volition continuity report markdown")?;
        outcome.observations.push(format!(
            "Volition consolidation for session `{}` produced {} continuity item(s).",
            session.session_id,
            report.items.len()
        ));
        outcome
            .extra_artifacts
            .push(report_json_path.display().to_string());
        outcome
            .extra_artifacts
            .push(report_md_path.display().to_string());
        state_files_written.push(report_json_path.display().to_string());
        state_files_written.push(report_md_path.display().to_string());
    }

    outcome.observations.push(format!(
        "Cross-session sleep consumed session `{}` and promoted {} routine memories with {} new associations.",
        session.session_id, promoted_count, new_associations_count
    ));
    outcome.extra_artifacts.push(
        state_dir
            .join("consolidated-brief.json")
            .display()
            .to_string(),
    );
    outcome
        .extra_artifacts
        .push(state_dir.join("memory-store.json").display().to_string());
    if !report.decision_candidates.is_empty() {
        outcome
            .extra_artifacts
            .push("reviewed-memory-draft.json".to_string());
        outcome
            .extra_artifacts
            .push("reviewed-memory-draft.md".to_string());
    }

    let change_record = SleepChangeRecord {
        state_outcome: SleepStateOutcome::ConsumedSession,
        session_id: Some(session.session_id.clone()),
        state_dir: state_resolution.persist_state_dir.display().to_string(),
        new_memories,
        skipped_duplicates,
        new_associations: new_association_changes,
        strengthened_associations: strengthened_changes,
        admitted_goal_id,
        declined_goal_candidate_id,
        swept_goal_ids,
        open_question_count: report.open_questions.len(),
        decision_candidate_count: report.decision_candidates.len(),
        state_files_written,
    };

    Ok((outcome, change_record))
}

pub(crate) fn build_sleep_input(resume_inputs: &ResumeInputs) -> SleepInputBundle {
    match &resume_inputs.previous_session {
        Some(session) => session_sleep_input(session),
        None => SleepInputBundle::new(
            "session_transcript",
            "sleep-session-summary",
            SESSION_TEXT,
        )
        .with_review_notes(vec![
            "No persisted prior session was available; this run uses the built-in smoke-test transcript."
                .to_string(),
            "All memory and decision outputs remain pending human review.".to_string(),
            "Trace the sleep report back to the session transcript rather than hidden state."
                .to_string(),
        ]),
    }
}

pub(crate) fn session_sleep_input(session: &SessionState) -> SleepInputBundle {
    let mut transcript = String::new();
    transcript.push_str("Persisted previous session for sleep consolidation.\n");
    transcript.push_str(&format!("Session id: {}\n", session.session_id));
    if let Some(previous_session_id) = &session.previous_session_id {
        transcript.push_str(&format!("Previous session id: {previous_session_id}\n"));
    }
    transcript.push_str(&format!("Completed turn count: {}\n", session.turns.len()));
    if let Some(reason) = &session.ended_reason {
        transcript.push_str(&format!("Ended reason: {reason:?}\n"));
    }

    if !session.summarized_turns.is_empty() {
        transcript.push_str("\nPrior turn summaries:\n");
        for summary in &session.summarized_turns {
            transcript.push_str(&format!(
                "- Turn {} summarized after turn {}: {}\n",
                summary.turn_index, summary.summarized_after_turn_index, summary.summary
            ));
        }
    }

    let sleep_records = session.sleep_records();
    engine_logging::engine_info!(
        "sleep session input assembled: session_id={} turn_count={} exchange_count={} sleep_record_count={}",
        session.session_id,
        session.turns.len(),
        session.exchanges.len(),
        sleep_records.len()
    );
    transcript.push_str("\nCompleted turns and exchanges:\n");
    if sleep_records.is_empty() {
        transcript.push_str("- None recorded.\n");
    } else {
        let mut diagnostic_notes = Vec::new();
        for record in &sleep_records {
            let source_index = record.source_index();
            match record {
                SleepRecord::Turn(_) => {
                    engine_logging::engine_info!(
                        "sleep record: session_id={} record_kind=turn turn_index={} status=completed interruptions=0",
                        session.session_id,
                        source_index
                    );
                    transcript.push_str(&format!("\nTurn {}:\n", source_index));
                    append_labelled_value(
                        &mut transcript,
                        "User",
                        record.user_input_text(),
                        "(empty user input)",
                    );
                    append_labelled_value(
                        &mut transcript,
                        "Assistant",
                        record.assistant_output_text().unwrap_or_default(),
                        "(empty assistant response)",
                    );
                    append_retrieved_memory_block(&mut transcript, record.retrieved_memory_block());
                    append_recalled_items(
                        &mut transcript,
                        "Recalled turns",
                        record.recalled_items(),
                    );
                }
                SleepRecord::Exchange(exchange) => {
                    engine_logging::engine_info!(
                        "sleep record: session_id={} record_kind=exchange exchange_index={} status={:?} interruptions={}",
                        session.session_id,
                        source_index,
                        exchange.status,
                        record.interruption_records().len()
                    );
                    transcript.push_str(&format!("\nVoice exchange {}:\n", source_index));
                    append_labelled_value(
                        &mut transcript,
                        "Final transcript",
                        record.final_transcript().unwrap_or_default(),
                        "(no final transcript recorded)",
                    );
                    let assistant_response = record
                        .assistant_output_text()
                        .unwrap_or("(no completed response)");
                    append_labelled_value(
                        &mut transcript,
                        "Assistant spoken response",
                        assistant_response,
                        "(no completed response)",
                    );
                    append_retrieved_memory_block(&mut transcript, record.retrieved_memory_block());
                    append_recalled_items(
                        &mut transcript,
                        "Recalled items",
                        record.recalled_items(),
                    );
                    append_interruption_block(&mut transcript, record.interruption_records());
                    diagnostic_notes.extend(
                        record
                            .provider_diagnostic_notes()
                            .into_iter()
                            .map(|note| format!("Voice exchange {}: {note}", source_index)),
                    );
                }
            }
        }

        return SleepInputBundle::new("session_state", session.session_id.clone(), transcript)
            .with_review_notes(session_state_review_notes())
            .with_diagnostic_notes(diagnostic_notes);
    }

    SleepInputBundle::new("session_state", session.session_id.clone(), transcript)
        .with_review_notes(session_state_review_notes())
}

fn session_state_review_notes() -> Vec<String> {
    vec![
        "This sleep pass uses the persisted previous session state as its source.".to_string(),
        "All memory and decision outputs remain pending human review.".to_string(),
        "Trace the sleep report back to concrete prior turns rather than hidden state.".to_string(),
    ]
}

fn latest_sleep_record_completion(session: &SessionState) -> Option<std::time::SystemTime> {
    session
        .sleep_records()
        .into_iter()
        .map(|record| record.completed_at())
        .max()
}

fn append_labelled_value(transcript: &mut String, label: &str, value: &str, placeholder: &str) {
    transcript.push_str(label);
    transcript.push_str(":\n");
    let rendered = if value.trim().is_empty() {
        placeholder
    } else {
        value.trim()
    };
    transcript.push_str(rendered);
    transcript.push('\n');
}

fn append_retrieved_memory_block(transcript: &mut String, retrieved_memory_block: &str) {
    if retrieved_memory_block.trim().is_empty() {
        return;
    }

    transcript.push_str("Retrieved memory block:\n");
    transcript.push_str(retrieved_memory_block.trim());
    transcript.push('\n');
}

fn append_recalled_items(
    transcript: &mut String,
    label: &str,
    recalled_items: &[crate::session::RecallRecord],
) {
    if recalled_items.is_empty() {
        return;
    }

    transcript.push_str(label);
    transcript.push_str(":\n");
    for recall in recalled_items {
        transcript.push_str(&format!(
            "- {} recalled turn {} via {}\n",
            recall.call_id, recall.turn_id, recall.tool_name
        ));
    }
}

fn append_interruption_block(
    transcript: &mut String,
    interruptions: &[crate::session::InterruptionRecord],
) {
    if interruptions.is_empty() {
        return;
    }

    transcript.push_str(&format!("Interruptions ({}):\n", interruptions.len()));
    for interruption in interruptions {
        let partial_response_present = interruption
            .partial_response_text
            .as_ref()
            .map(|text| !text.trim().is_empty())
            .unwrap_or(false);
        transcript.push_str(&format!(
            "- source={} action={:?} stop_outcome={:?} partial_response_present={partial_response_present}",
            interruption.source, interruption.action, interruption.stop_outcome
        ));
        if let Some(response_id) = &interruption.response_id {
            transcript.push_str(&format!(" response_id={response_id}"));
        }
        transcript.push('\n');
    }
}

pub(crate) fn write_sleep_artifacts(
    context: &RunContext,
    requested_provider: &str,
    input: &SleepInputBundle,
    report: &SleepReport,
) -> anyhow::Result<()> {
    let json_path = context.run_dir().join("sleep-report.json");
    let markdown_path = context.run_dir().join("sleep-report.md");

    fs::write(&json_path, serde_json::to_string_pretty(report)?).with_context(|| {
        format!(
            "failed to write sleep report JSON for run {}",
            context.run_id()
        )
    })?;

    let mut markdown = String::new();
    markdown.push_str("# Sleep Report\n\n");
    markdown.push_str(&format!(
        "- Source: `{}` ({})\n",
        input.source_label, input.source_kind
    ));
    markdown.push_str(&format!("- Requested provider: `{}`\n", requested_provider));
    markdown.push_str(
        "- Review policy: all extracted items remain provisional until manually reviewed\n",
    );
    if !input.diagnostic_notes.is_empty() {
        markdown.push_str("\n## Sleep Input Diagnostics\n\n");
        push_markdown_list(&mut markdown, &input.diagnostic_notes);
    }
    markdown.push_str("\n## Session Summary\n\n");
    markdown.push_str(&report.session_summary);
    markdown.push_str("\n\n## Memory Candidates\n\n");
    if report.memory_candidates.is_empty() {
        markdown.push_str("- None recorded.\n");
    } else {
        for candidate in &report.memory_candidates {
            markdown.push_str(&format!("- {}", candidate.summary));
            if let Some(importance) = candidate.importance {
                markdown.push_str(&format!(" (importance {:.2})", importance));
            }
            if let Some(source_reference) = &candidate.source_reference {
                markdown.push_str(&format!(" [{}]", source_reference));
            }
            markdown.push('\n');
        }
    }
    markdown.push_str("\n## Association Candidates\n\n");
    if report.association_candidates.is_empty() {
        markdown.push_str("- None recorded.\n");
    } else {
        for candidate in &report.association_candidates {
            markdown.push_str(&format!(
                "- memory_candidates[{:03}] -> memory_candidates[{:03}]",
                candidate.from_memory_candidate_index, candidate.to_memory_candidate_index
            ));
            if let Some(weight) = candidate.weight {
                markdown.push_str(&format!(" (weight {:.2})", weight));
            }
            if let Some(reason) = &candidate.reason {
                markdown.push_str(&format!(" [{reason}]"));
            }
            markdown.push('\n');
        }
    }
    markdown.push_str("\n## Open Questions\n\n");
    push_markdown_list(&mut markdown, &report.open_questions);
    markdown.push_str("\n## Decision Candidates\n\n");
    push_markdown_list(&mut markdown, &report.decision_candidates);
    markdown.push_str("\n## Future Context Hints\n\n");
    push_markdown_list(&mut markdown, &report.future_context_hints);
    markdown.push_str("\n## Review Notes\n\n");
    push_markdown_list(&mut markdown, &report.review_notes);

    fs::write(&markdown_path, markdown).with_context(|| {
        format!(
            "failed to write sleep report Markdown for run {}",
            context.run_id()
        )
    })?;

    Ok(())
}

fn push_markdown_list(markdown: &mut String, items: &[String]) {
    if items.is_empty() {
        markdown.push_str("- None recorded.\n");
        return;
    }

    for item in items {
        markdown.push_str(&format!("- {item}\n"));
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use crate::runtime::run_context::RunContext;
    use crate::session::manifest::{ContinuityManifest, ResumeMode};
    use crate::session::{
        MemorySourceConfig, SessionConfig, SessionState, StateDirectoryResolution,
    };
    use crate::sleep::{SleepMemoryCandidate, SleepReport};

    use super::{SleepUpdateOptions, build_sleep_input, run_sleep_update};

    // The current directory is process-global, so every cwd-mutating test must serialize
    // through this lock; otherwise concurrent tests swap cwd out from under each other and
    // sleep artifacts land in the wrong tree.
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CurrentDirGuard {
        original: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurrentDirGuard {
        fn enter(path: &Path) -> Self {
            let lock = CWD_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self {
                original,
                _lock: lock,
            }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.original).unwrap();
        }
    }

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: true,
            memory_source: MemorySourceConfig {
                source: "file".to_string(),
                file: None,
            },
        }
    }

    #[test]
    fn realtime_options_default_to_realtime_state() {
        let options = SleepUpdateOptions::realtime("mock", None);

        assert_eq!(
            options.state_dir,
            std::path::PathBuf::from("state/realtime")
        );
        assert_eq!(options.requested_provider, "mock");
    }

    #[test]
    fn sleep_update_command_consumes_a_pending_session() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-command-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state").join("realtime");
        fs::create_dir_all(&state_dir).unwrap();

        let mut previous =
            SessionState::new_with_id("realtime-session-to-sleep".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some("session-state.json".into()),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        // Seed a volition continuity snapshot so the sleep maintenance pass re-persists it.
        // The change view must then list that snapshot path among `state_files_written`.
        {
            use qsf_volition::{
                REALTIME_SEED_FIXTURE_ID, VolitionContinuitySnapshot, VolitionState,
                build_state_inspection, persist_volition_continuity_snapshot,
                realtime_seed_fixture,
            };
            let fixture = realtime_seed_fixture();
            let vol_state = VolitionState::from_fixture(&fixture);
            let inspection = build_state_inspection(&vol_state, &fixture);
            let snapshot = VolitionContinuitySnapshot::new(
                previous.session_id.clone(),
                "2026-07-02T00:00:00Z",
                REALTIME_SEED_FIXTURE_ID,
                vol_state,
                inspection,
            );
            persist_volition_continuity_snapshot(&snapshot, state_dir.join("volition-state.json"))
                .unwrap();
        }

        let cwd_guard = CurrentDirGuard::enter(&base_dir);
        let summary = run_sleep_update(SleepUpdateOptions {
            state_dir: std::path::PathBuf::from("state/realtime"),
            requested_provider: "mock".to_string(),
            workspace_root: None,
        })
        .unwrap();

        let manifest =
            ContinuityManifest::load_or_default(state_dir.join("continuity-manifest.json"))
                .unwrap();
        assert_eq!(summary.status, "completed");
        assert!(!manifest.sleep_pending);
        assert_eq!(
            manifest.last_sleep_consumed_session_id.as_deref(),
            Some("realtime-session-to-sleep")
        );
        assert!(state_dir.join("consolidated-brief.json").exists());
        assert!(state_dir.join("memory-store.json").exists());

        use crate::sleep::change_view::SleepStateOutcome;

        assert_eq!(
            summary.change_record.state_outcome,
            SleepStateOutcome::ConsumedSession
        );
        assert_eq!(
            summary.change_record.session_id.as_deref(),
            Some("realtime-session-to-sleep")
        );
        use crate::sleep::change_view::SleepChangeRecord;

        let changes_path = summary.run_dir.join("sleep-changes.json");
        assert!(changes_path.exists());
        // Parse as the structured record (not untyped Value) so the durable
        // artifact cannot drift from `SleepChangeRecord`'s shape. Also assert the
        // written file round-trips back to the in-memory record.
        let parsed: SleepChangeRecord =
            serde_json::from_str(&fs::read_to_string(&changes_path).unwrap()).unwrap();
        assert_eq!(parsed.state_outcome, SleepStateOutcome::ConsumedSession);
        assert_eq!(
            parsed.session_id.as_deref(),
            Some("realtime-session-to-sleep")
        );
        // At least one nested vector field must survive the round-trip.
        assert_eq!(parsed.new_memories, summary.change_record.new_memories);
        assert_eq!(parsed, summary.change_record);

        // The maintenance pass re-persisted the volition continuity snapshot inside the state
        // dir, so the change view must list it (both in memory and in the written artifact).
        assert!(
            summary
                .change_record
                .state_files_written
                .iter()
                .any(|file| file.ends_with("volition-state.json")),
            "state_files_written should include the volition continuity snapshot path, got {:?}",
            summary.change_record.state_files_written
        );
        assert!(
            parsed
                .state_files_written
                .iter()
                .any(|file| file.ends_with("volition-state.json"))
        );

        // A second run over the same state must report AlreadyConsumed and write
        // nothing. The process is still in `base_dir` from the first run, so the
        // relative `state/realtime` resolves correctly; do not restore cwd yet.
        let second = run_sleep_update(SleepUpdateOptions {
            state_dir: std::path::PathBuf::from("state/realtime"),
            requested_provider: "mock".to_string(),
            workspace_root: None,
        })
        .unwrap();
        assert_eq!(
            second.change_record.state_outcome,
            SleepStateOutcome::AlreadyConsumed
        );
        assert!(second.change_record.state_files_written.is_empty());

        drop(cwd_guard);

        fs::remove_dir_all(base_dir).unwrap();
    }

    // Regression: the realtime server nests continuity state under
    // `state/realtime/continuity/<session_id>/`, but sleep used to read the manifest directly
    // from `state/realtime`, so `qsf sleep` reported `NoPersistedSession` against a real
    // session. Sleeping the root must now resolve and consume the nested `default` session.
    #[test]
    fn sleep_update_command_consumes_a_nested_realtime_session() {
        use crate::sleep::change_view::SleepStateOutcome;

        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-nested-{}", uuid::Uuid::new_v4()));
        let session_dir = base_dir
            .join("state")
            .join("realtime")
            .join("continuity")
            .join("default");
        fs::create_dir_all(&session_dir).unwrap();

        let mut previous = SessionState::new_with_id("default".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &session_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some("session-state.json".into()),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(session_dir.join("continuity-manifest.json"))
        .unwrap();

        let cwd_guard = CurrentDirGuard::enter(&base_dir);
        // Sleep the *root*, exactly as `qsf.ps1 sleep` does with its default `state/realtime`.
        let summary = run_sleep_update(SleepUpdateOptions {
            state_dir: std::path::PathBuf::from("state/realtime"),
            requested_provider: "mock".to_string(),
            workspace_root: None,
        })
        .unwrap();
        drop(cwd_guard);

        assert_eq!(summary.status, "completed");
        assert_eq!(
            summary.change_record.state_outcome,
            SleepStateOutcome::ConsumedSession
        );
        assert_eq!(summary.change_record.session_id.as_deref(), Some("default"));

        let manifest =
            ContinuityManifest::load_or_default(session_dir.join("continuity-manifest.json"))
                .unwrap();
        assert!(!manifest.sleep_pending);
        assert_eq!(
            manifest.last_sleep_consumed_session_id.as_deref(),
            Some("default")
        );
        assert!(session_dir.join("consolidated-brief.json").exists());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sleep_input_uses_persisted_session_when_available() {
        let mut previous = SessionState::new_with_id("session-from-state".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        let input = build_sleep_input(&crate::session::resume::ResumeInputs {
            previous_session: Some(previous),
            manifest: ContinuityManifest::default(),
        });

        assert_eq!(input.source_kind, "session_state");
        assert_eq!(input.source_label, "session-from-state");
        assert!(input.session_text.contains("Completed turns and exchanges"));
    }

    #[test]
    fn commit_cross_session_sleep_promotes_candidates() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-update-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state");
        fs::create_dir_all(&state_dir).unwrap();

        let mut previous = SessionState::new_with_id("session-to-sleep".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some("session-state.json".into()),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        let mut context =
            RunContext::create_in(base_dir.join("runs"), "sleep-update-test").unwrap();
        let (outcome, change_record) = super::commit_cross_session_sleep(
            &mut context,
            "mock",
            &SleepReport {
                session_summary: "Summary.".to_string(),
                memory_candidates: vec![SleepMemoryCandidate {
                    summary: "Candidate memory.".to_string(),
                    importance: Some(0.8),
                    source_reference: Some("turn:0".to_string()),
                }],
                association_candidates: vec![],
                open_questions: vec![],
                decision_candidates: vec!["Keep this draft provisional.".to_string()],
                future_context_hints: vec![],
                review_notes: vec![],
            },
            super::SleepUpdateOutcome {
                summary: "ok".to_string(),
                observations: vec![],
                failure_modes: vec![],
                follow_up_questions: vec![],
                decision_candidates: vec![],
                extra_artifacts: vec![],
            },
            &StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir.clone(),
                legacy_fallback_used: false,
            },
        )
        .unwrap();

        assert!(outcome.summary.contains("ok"));
        assert_eq!(change_record.new_memories.len(), 1);
        assert_eq!(change_record.new_memories[0].title, "Candidate memory.");
        assert!((change_record.new_memories[0].importance - 0.8).abs() < 1e-9);
        assert_eq!(change_record.open_question_count, 0);
        assert!(
            outcome
                .extra_artifacts
                .iter()
                .any(|artifact| artifact == "reviewed-memory-draft.json")
        );
        assert!(
            context
                .run_dir()
                .join("reviewed-memory-draft.json")
                .exists()
        );
        assert!(
            !change_record
                .state_files_written
                .iter()
                .any(|file| file.contains("reviewed-memory-draft")),
            "run artifacts must not be listed as state files: {:?}",
            change_record.state_files_written
        );
        assert!(
            change_record
                .state_files_written
                .iter()
                .any(|file| file.ends_with("memory-store.json"))
        );
        let manifest =
            ContinuityManifest::load_or_default(state_dir.join("continuity-manifest.json"))
                .unwrap();
        assert!(!manifest.sleep_pending);
        assert!(state_dir.join("memory-store.json").exists());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn commit_cross_session_sleep_records_skipped_duplicates() {
        let base_dir = std::env::temp_dir().join(format!("qsf-sleep-dup-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state");
        fs::create_dir_all(&state_dir).unwrap();

        let mut previous = SessionState::new_with_id("session-to-sleep".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some("session-state.json".into()),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        // Seed an existing memory whose title matches the report candidate so
        // auto-promotion skips it as a duplicate.
        let store_path = state_dir.join("memory-store.json");
        let mut store = crate::memory::MemoryStore::load_or_empty(&store_path).unwrap();
        store.append_records([crate::memory::MemoryRecord::new(
            "memory.existing",
            crate::memory::MemoryRecordKind::Observation,
            "Candidate memory.",
            "Candidate memory.",
            vec![],
            time::OffsetDateTime::now_utc(),
            0.5,
            0,
            "tests",
            10,
        )]);
        store.persist().unwrap();

        let mut context =
            RunContext::create_in(base_dir.join("runs"), "sleep-update-test").unwrap();
        let (_outcome, change_record) = super::commit_cross_session_sleep(
            &mut context,
            "mock",
            &SleepReport {
                session_summary: "Summary.".to_string(),
                memory_candidates: vec![SleepMemoryCandidate {
                    summary: "Candidate memory.".to_string(),
                    importance: Some(0.8),
                    source_reference: Some("turn:0".to_string()),
                }],
                association_candidates: vec![],
                open_questions: vec![],
                decision_candidates: vec![],
                future_context_hints: vec![],
                review_notes: vec![],
            },
            super::SleepUpdateOutcome {
                summary: "ok".to_string(),
                observations: vec![],
                failure_modes: vec![],
                follow_up_questions: vec![],
                decision_candidates: vec![],
                extra_artifacts: vec![],
            },
            &StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir.clone(),
                legacy_fallback_used: false,
            },
        )
        .unwrap();

        assert!(change_record.new_memories.is_empty());
        assert!(
            change_record
                .skipped_duplicates
                .iter()
                .any(|title| title == "Candidate memory.")
        );

        fs::remove_dir_all(base_dir).unwrap();
    }
}
