use std::fs;
#[cfg(test)]
use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::models::{build_client, requested_provider_from_env};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::resume::ResumeInputs;
use crate::session::{SessionState, SleepRecord, StateDirectoryResolution};
use crate::sleep::{SleepInputBundle, SleepReport, summarize_session};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::transcript_format::{
    append_labelled_value, append_recalled_items, append_retrieved_memory_block,
};

const SESSION_TEXT: &str = "Session transcript:\n- We introduced typed model roles and a deterministic mock model.\n- The runtime still routes observable behavior through explicit events and traces.\n- Sleep phase should summarize the session, extract candidate memories, surface open questions, and keep decision candidates provisional.\n- Nothing in the sleep phase should silently become an accepted decision.";

pub struct SleepPhaseSessionSummaryExperiment;

impl Experiment for SleepPhaseSessionSummaryExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::SleepPhaseSessionSummary
    }

    fn description(&self) -> &'static str {
        "Summarize a session into reviewable sleep-phase outputs and artifacts"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_provider(context, requested_provider_from_env())
    }
}

impl SleepPhaseSessionSummaryExperiment {
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
    ) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_provider_at_state_resolution(
            context,
            requested_provider,
            crate::session::resolve_shared_state_directory_from_env(),
        )
    }

    fn run_with_provider_at_state_resolution(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
        state_resolution: StateDirectoryResolution,
    ) -> anyhow::Result<ExperimentOutcome> {
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

        let client = build_client(requested_provider)?;
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

        let outcome = ExperimentOutcome {
            summary: format!(
                "The sleep session summary experiment summarized `{}` through the `{}` provider, wrote a reviewable sleep report artifact, and recorded explicit sleep-phase events and traces without promoting any candidate into an accepted decision.",
                input.source_label, summary.response.provider_name
            ),
            observations: vec![
                "The sleep phase reuses the model-role path, so provider selection, latency, and usage stay observable without special-case plumbing.".to_string(),
                "Sleep reports preserve separate fields for summary, memory candidates, open questions, decision candidates, and future context hints.".to_string(),
                format!("The sleep input source was `{}` ({}) rather than hidden state.", input.source_label, input.source_kind),
                "Review notes are carried into the artifact so the output stays explicitly provisional.".to_string(),
            ],
            failure_modes: vec![
                "The default mock provider keeps the experiment deterministic, but real-provider output quality still depends on prompt compliance and structured JSON output.".to_string(),
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

        commit_cross_session_sleep(context, &summary.report, outcome, &state_resolution)
    }

    #[cfg(test)]
    fn run_with_provider_at_state_dir(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
        state_dir: impl AsRef<Path>,
    ) -> anyhow::Result<ExperimentOutcome> {
        let state_dir = state_dir.as_ref().to_path_buf();
        self.run_with_provider_at_state_resolution(
            context,
            requested_provider,
            StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir,
                legacy_fallback_used: false,
            },
        )
    }
}

pub(crate) fn commit_cross_session_sleep(
    context: &RunContext,
    report: &SleepReport,
    mut outcome: ExperimentOutcome,
    state_resolution: &StateDirectoryResolution,
) -> anyhow::Result<ExperimentOutcome> {
    let resume_inputs =
        crate::session::resume::load_resume_inputs(&state_resolution.resume_state_dir)?;
    let Some(session) = resume_inputs.previous_session.clone() else {
        return Ok(outcome);
    };

    if resume_inputs
        .manifest
        .last_sleep_consumed_session_id
        .as_deref()
        == Some(session.session_id.as_str())
        && !resume_inputs.manifest.sleep_pending
    {
        return Ok(outcome);
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

    store.append_records(plan.new_records.clone());
    store.append_associations(plan.new_associations.clone());
    store
        .contents_mut()
        .processed_ranges
        .extend(plan.processed_ranges.clone());
    for (from_id, to_id, new_weight) in &plan.strengthened_associations {
        if let Some(existing) = store
            .contents_mut()
            .associations
            .iter_mut()
            .find(|association| {
                association.from_memory_id == *from_id && association.to_memory_id == *to_id
            })
        {
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

    crate::sleep::commit::SleepCommit {
        state_dir,
        new_store_contents: store.contents().clone(),
        brief,
        sleep_run_id: sleep_run_id.clone(),
        consumed_session_id: session.session_id.clone(),
        brief_archive_name: format!("sleep-{sleep_run_id}.json"),
    }
    .write()?;

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

    Ok(outcome)
}

fn build_sleep_input(resume_inputs: &ResumeInputs) -> SleepInputBundle {
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

fn session_sleep_input(session: &SessionState) -> SleepInputBundle {
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
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::{SleepPhaseSessionSummaryExperiment, build_sleep_input};
    use crate::context::{ContextAssembly, ContextBudget, ContextFragment, ContextSelection};
    use crate::experiments::ExperimentOutcome;
    use crate::memory::{MemoryFixture, MemoryRecord, MemoryRecordKind, MemoryStore};
    use crate::models::{MockModelClient, ModelClient, ModelRequest, ModelResponse};
    use crate::runtime::run_context::RunContext;
    use crate::session::manifest::{ContinuityManifest, ResumeMode};
    use crate::session::resume::ResumeInputs;
    use crate::session::{
        Exchange, ExchangeInput, ExchangeOutput, InterruptionAction, InterruptionRecord,
        InterruptionStopOutcome, MemorySourceConfig, ProviderEventKind, ProviderEventRecord,
        SessionConfig, SessionState, StateDirectoryResolution, TurnSummary,
    };
    use crate::sleep::{SleepMemoryCandidate, SleepReport};
    use time::OffsetDateTime;

    #[test]
    fn sleep_experiment_writes_sleep_report_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-phase7-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-seven-test").unwrap();
        let experiment = SleepPhaseSessionSummaryExperiment;

        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", base_dir.join("state/text-loop"))
            .unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let sleep_report = fs::read_to_string(context.run_dir().join("sleep-report.md")).unwrap();

        assert_eq!(context.trace_count(), 2);
        assert!(outcome.summary.contains("sleep session summary"));
        assert!(events.contains("SleepPhaseRequested"));
        assert!(events.contains("SleepPhaseCompleted"));
        assert!(traces.contains("sleep-phase-summary"));
        assert!(sleep_report.contains("Sleep Report"));
        assert!(sleep_report.contains("manual review"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sleep_input_uses_persisted_previous_turns_when_available() {
        let mut previous = SessionState::new_with_id("session-from-state".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        previous.summarized_turns.push(TurnSummary {
            turn_index: 0,
            summarized_after_turn_index: 1,
            summary: "The prior turn summary should be visible to sleep.".to_string(),
            model_id: "mock".to_string(),
            model_latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
        });
        previous.turns[0].user_input = "Please remember the continuity rule.".to_string();
        previous.turns[0].assistant_response =
            "I will keep sleep consolidation reviewable.".to_string();

        let input = build_sleep_input(&ResumeInputs {
            manifest: ContinuityManifest::default(),
            previous_session: Some(previous),
        });

        assert_eq!(input.source_kind, "session_state");
        assert_eq!(input.source_label, "session-from-state");
        assert!(
            input
                .session_text
                .contains("Please remember the continuity rule.")
        );
        assert!(
            input
                .session_text
                .contains("I will keep sleep consolidation reviewable.")
        );
        assert!(
            input
                .session_text
                .contains("The prior turn summary should be visible to sleep.")
        );
        assert!(
            input
                .review_notes
                .iter()
                .any(|note| note.contains("persisted previous session state"))
        );
    }

    #[test]
    fn sleep_input_orders_mixed_text_and_voice_records_by_timestamp() {
        let earlier = SystemTime::UNIX_EPOCH;
        let later = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let text_then_voice = mixed_session(true, earlier, later);
        let voice_then_text = mixed_session(false, earlier, later);

        assert_eq!(
            text_then_voice
                .sleep_records()
                .into_iter()
                .map(|record| record.kind())
                .collect::<Vec<_>>(),
            vec![
                crate::session::SleepRecordKind::Exchange,
                crate::session::SleepRecordKind::Turn
            ]
        );
        assert_eq!(
            voice_then_text
                .sleep_records()
                .into_iter()
                .map(|record| record.kind())
                .collect::<Vec<_>>(),
            vec![
                crate::session::SleepRecordKind::Exchange,
                crate::session::SleepRecordKind::Turn
            ]
        );

        let text_then_voice_input = super::session_sleep_input(&text_then_voice);
        let voice_then_text_input = super::session_sleep_input(&voice_then_text);
        let text_turn_position = text_then_voice_input.session_text.find("Turn 0:").unwrap();
        let text_voice_position = text_then_voice_input
            .session_text
            .find("Voice exchange 0:")
            .unwrap();
        let voice_turn_position = voice_then_text_input.session_text.find("Turn 0:").unwrap();
        let voice_voice_position = voice_then_text_input
            .session_text
            .find("Voice exchange 0:")
            .unwrap();

        assert!(text_voice_position < text_turn_position);
        assert!(voice_voice_position < voice_turn_position);
    }

    #[test]
    fn sleep_input_keeps_provider_preamble_out_of_promotable_prompt_and_commit_fields() {
        let preamble_text = "provider preamble should stay out of the prompt";
        let mut previous = SessionState::new_with_id("session-with-preamble".to_string(), config());
        previous.exchanges.push(voice_exchange_with_metadata(
            2,
            SystemTime::UNIX_EPOCH,
            Some("memory.voice".to_string()),
            Some(preamble_text),
            false,
        ));

        let input = build_sleep_input(&ResumeInputs {
            manifest: ContinuityManifest::default(),
            previous_session: Some(previous),
        });

        let mut context = RunContext::create_in(
            std::env::temp_dir().join(format!("qsf-sleep-boundary-{}", uuid::Uuid::new_v4())),
            "sleep-boundary-test",
        )
        .unwrap();
        let result = crate::sleep::summarize_session(
            &mut context,
            &PreambleAssertingSleepClient {
                prohibited_text: preamble_text.to_string(),
            },
            &input,
        )
        .unwrap();

        assert!(!result.report.session_summary.contains(preamble_text));
        assert!(
            input
                .session_text
                .contains("Voice exchange 2:\nFinal transcript")
        );
        assert!(
            input
                .diagnostic_notes
                .iter()
                .any(|note| note.starts_with("Voice exchange 2: Provider diagnostics"))
        );
        assert!(
            result
                .report
                .memory_candidates
                .iter()
                .all(|candidate| !candidate.summary.contains(preamble_text))
        );
        assert!(
            result
                .report
                .future_context_hints
                .iter()
                .all(|hint| !hint.contains(preamble_text))
        );
    }

    #[test]
    fn interrupted_voice_exchange_builds_coherent_sleep_input_without_panicking() {
        let mut previous = SessionState::new_with_id("interrupted-voice".to_string(), config());
        previous.exchanges.push(Exchange {
            index: 0,
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: None,
            input: ExchangeInput::Voice {
                final_transcript: String::new(),
                utterances: vec![],
            },
            output: None,
            context_assembly: None,
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            tool_requests: vec![],
            tool_executions: vec![],
            model: None,
            interruptions: vec![InterruptionRecord {
                exchange_index: 0,
                response_id: Some("response-1".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "realtime-provider".to_string(),
                action: InterruptionAction::Stop,
                stop_outcome: InterruptionStopOutcome::Stopped,
                partial_response_text: None,
            }],
            provider_events: vec![ProviderEventRecord {
                exchange_index: 0,
                event_kind: ProviderEventKind::Preamble,
                provider_id: "provider".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                call_id: None,
                event_id: None,
                item_id: None,
                previous_item_id: None,
                response_id: None,
                text: Some("provider preamble text".to_string()),
                status: Some("pending".to_string()),
                audio_marker: None,
            }],
            status: crate::session::ExchangeStatus::Interrupted,
        });

        let input = build_sleep_input(&ResumeInputs {
            manifest: ContinuityManifest::default(),
            previous_session: Some(previous),
        });

        assert!(!input.session_text.trim().is_empty());
        assert!(
            input
                .session_text
                .contains("(no final transcript recorded)")
        );
        assert!(input.session_text.contains("(no completed response)"));
        assert!(
            input
                .diagnostic_notes
                .iter()
                .any(|note| note.contains("Provider diagnostics"))
        );

        let mut context = RunContext::create_in(
            std::env::temp_dir().join(format!("qsf-sleep-interrupted-{}", uuid::Uuid::new_v4())),
            "sleep-interrupted-test",
        )
        .unwrap();
        crate::sleep::summarize_session(&mut context, &MockModelClient::default(), &input).unwrap();
    }

    #[test]
    fn sleep_experiment_marks_persisted_session_as_sleep_source() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-real-sleep-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state/text-loop");
        let mut previous = SessionState::new_with_id("session-to-sleep".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        let mut context = RunContext::create_in(&base_dir, "real-sleep-test").unwrap();
        let experiment = SleepPhaseSessionSummaryExperiment;
        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &state_dir)
            .unwrap();
        let sleep_report = fs::read_to_string(context.run_dir().join("sleep-report.md")).unwrap();
        let store = MemoryStore::load_or_empty(state_dir.join("memory-store.json")).unwrap();

        assert!(sleep_report.contains("Source: `session-to-sleep` (session_state)"));
        assert!(store.contents().records.is_empty());
        assert!(store.contents().associations.is_empty());
        assert!(
            outcome.extra_artifacts.contains(
                &state_dir
                    .join("consolidated-brief.json")
                    .display()
                    .to_string()
            )
        );
        assert!(
            outcome
                .extra_artifacts
                .contains(&state_dir.join("memory-store.json").display().to_string())
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sleep_legacy_fallback_writes_shared_state_without_rewriting_legacy() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-sleep-legacy-{}", uuid::Uuid::new_v4()));
        let legacy_state_dir = base_dir.join("state/text-loop");
        let shared_state_dir = base_dir.join("state/session");
        let mut previous =
            SessionState::new_with_id("legacy-session-to-sleep".to_string(), config());
        previous.turns.push(crate::session::tests::fake_turn(0));
        crate::session::persistence::persist_session_state(&previous, &legacy_state_dir).unwrap();
        let mut store =
            MemoryStore::load_or_empty(legacy_state_dir.join("memory-store.json")).unwrap();
        store
            .contents_mut()
            .records
            .push(memory_record("memory.legacy"));
        store.persist().unwrap();
        let legacy_session_before =
            fs::read_to_string(legacy_state_dir.join("session-state.json")).unwrap();
        let legacy_memory_before =
            fs::read_to_string(legacy_state_dir.join("memory-store.json")).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(legacy_state_dir.join("continuity-manifest.json"))
        .unwrap();

        let mut context = RunContext::create_in(base_dir.join("run"), "sleep-legacy-test").unwrap();
        let experiment = SleepPhaseSessionSummaryExperiment;
        experiment
            .run_with_provider_at_state_resolution(
                &mut context,
                "mock",
                StateDirectoryResolution {
                    resume_state_dir: legacy_state_dir.clone(),
                    persist_state_dir: shared_state_dir.clone(),
                    legacy_fallback_used: true,
                },
            )
            .unwrap();

        let shared_state = crate::session::persistence::load_session_state(
            shared_state_dir.join("session-state.json"),
        )
        .unwrap();
        let shared_manifest =
            ContinuityManifest::load_or_default(shared_state_dir.join("continuity-manifest.json"))
                .unwrap();
        let shared_store =
            MemoryStore::load_or_empty(shared_state_dir.join("memory-store.json")).unwrap();

        assert_eq!(shared_state.session_id, previous.session_id);
        assert!(!shared_manifest.sleep_pending);
        assert_eq!(
            legacy_session_before,
            fs::read_to_string(legacy_state_dir.join("session-state.json")).unwrap()
        );
        assert_eq!(
            legacy_memory_before,
            fs::read_to_string(legacy_state_dir.join("memory-store.json")).unwrap()
        );
        assert!(
            shared_store
                .contents()
                .records
                .iter()
                .any(|record| record.id == "memory.legacy")
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn mock_sleep_run_still_persists_cross_turn_associations() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-mock-sleep-full-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state/text-loop");
        let mut previous = SessionState::new_with_id("session-to-sleep".to_string(), config());
        previous
            .turns
            .push(turn_with_retrieved_memory(0, "memory.a"));
        previous
            .turns
            .push(turn_with_retrieved_memory(1, "memory.b"));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();

        let mut store = MemoryStore::load_or_empty(state_dir.join("memory-store.json")).unwrap();
        store.contents_mut().records.push(memory_record("memory.a"));
        store.contents_mut().records.push(memory_record("memory.b"));
        store.persist().unwrap();

        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        let mut context = RunContext::create_in(&base_dir, "mock-sleep-full-test").unwrap();
        let experiment = SleepPhaseSessionSummaryExperiment;
        experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &state_dir)
            .unwrap();

        let store = MemoryStore::load_or_empty(state_dir.join("memory-store.json")).unwrap();
        assert_eq!(store.contents().records.len(), 2);
        assert!(store.contents().associations.iter().any(|association| {
            association.from_memory_id == "memory.a" && association.to_memory_id == "memory.b"
        }));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn voice_sleep_commit_promotes_observations_and_writes_decision_draft() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-voice-sleep-commit-{}", uuid::Uuid::new_v4()));
        let state_dir = base_dir.join("state/session");
        let mut previous =
            SessionState::new_with_id("voice-session-to-sleep".to_string(), config());
        previous.exchanges.push(voice_exchange_with_metadata(
            0,
            SystemTime::UNIX_EPOCH,
            None,
            None,
            false,
        ));
        crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(state_dir.join("continuity-manifest.json"))
        .unwrap();

        let report = SleepReport {
            session_summary: "Voice session summary for the consolidated brief.".to_string(),
            memory_candidates: vec![SleepMemoryCandidate {
                summary: "Routine voice observation should auto-promote.".to_string(),
                importance: Some(0.72),
                source_reference: Some("voice-session:exchange-0".to_string()),
            }],
            association_candidates: vec![],
            open_questions: vec![],
            decision_candidates: vec![
                "Prefer voice memory policy changes to stay in reviewed drafts.".to_string(),
            ],
            future_context_hints: vec!["Resume voice continuity from this brief.".to_string()],
            review_notes: vec![],
        };
        let context =
            RunContext::create_in(base_dir.join("runs"), "sleep-voice-commit-test").unwrap();
        let outcome = ExperimentOutcome {
            summary: "test".to_string(),
            observations: vec![],
            failure_modes: vec![],
            follow_up_questions: vec![],
            decision_candidates: vec![],
            extra_artifacts: vec![],
        };

        let outcome = super::commit_cross_session_sleep(
            &context,
            &report,
            outcome,
            &StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir.clone(),
                legacy_fallback_used: false,
            },
        )
        .unwrap();

        let store = MemoryStore::load_or_empty(state_dir.join("memory-store.json")).unwrap();
        let brief: crate::sleep::commit::ConsolidatedBrief = serde_json::from_str(
            &fs::read_to_string(state_dir.join("consolidated-brief.json")).unwrap(),
        )
        .unwrap();
        let manifest =
            ContinuityManifest::load_or_default(state_dir.join("continuity-manifest.json"))
                .unwrap();
        let draft: MemoryFixture = serde_json::from_str(
            &fs::read_to_string(context.run_dir().join("reviewed-memory-draft.json")).unwrap(),
        )
        .unwrap();

        assert_eq!(store.contents().records.len(), 1);
        assert_eq!(
            store.contents().records[0].kind,
            MemoryRecordKind::Observation
        );
        assert_eq!(
            store.contents().records[0].summary,
            "Routine voice observation should auto-promote."
        );
        assert!(
            state_dir
                .join("archive")
                .join(format!("sleep-{}.json", context.run_id()))
                .exists()
        );
        assert_eq!(
            brief.previous_session_summary,
            "Voice session summary for the consolidated brief."
        );
        assert_eq!(manifest.resume_mode, ResumeMode::ConsolidatedBrief);
        assert_eq!(
            manifest.last_sleep_consumed_session_id.as_deref(),
            Some("voice-session-to-sleep")
        );
        assert_eq!(draft.records.len(), 1);
        assert_eq!(draft.records[0].kind, MemoryRecordKind::Decision);
        assert!(draft.records[0].summary.contains("reviewed drafts"));
        assert!(
            outcome
                .extra_artifacts
                .iter()
                .any(|artifact| artifact.ends_with("reviewed-memory-draft.json"))
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }

    fn memory_record(id: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            format!("{id} title"),
            format!("{id} summary"),
            vec![],
            OffsetDateTime::now_utc(),
            0.5,
            0,
            "tests",
            10,
        )
    }

    fn turn_with_retrieved_memory(index: usize, memory_id: &str) -> crate::session::Turn {
        let mut turn = crate::session::tests::fake_turn(index);
        turn.context_assembly = ContextAssembly {
            budget: ContextBudget::new(4, 600),
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: memory_id.to_string(),
                    source_kind: crate::context::ContextSourceKind::Memory,
                    summary: format!("{memory_id} summary"),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 10,
                    source_reference: "tests".to_string(),
                    selection_reason: "tests".to_string(),
                },
                cumulative_estimated_tokens: 10,
            }],
            omitted: vec![],
            used_estimated_tokens: 10,
        };
        turn
    }

    fn mixed_session(text_first: bool, earlier: SystemTime, later: SystemTime) -> SessionState {
        let mut session = SessionState::new_with_id(format!("mixed-{}", text_first), config());
        let mut turn = crate::session::tests::fake_turn(0);
        turn.started_at = later;
        turn.completed_at = later;
        turn.user_input = "Turn prompt".to_string();
        turn.assistant_response = "Turn response".to_string();
        let exchange = voice_exchange_with_metadata(0, earlier, None, None, false);

        if text_first {
            session.turns.push(turn);
            session.exchanges.push(exchange);
        } else {
            session.exchanges.push(exchange);
            session.turns.push(turn);
        }

        session
    }

    fn voice_exchange_with_metadata(
        index: usize,
        started_at: SystemTime,
        retrieved_memory_id: Option<String>,
        preamble_text: Option<&str>,
        interrupted: bool,
    ) -> Exchange {
        let mut exchange = Exchange {
            index,
            started_at,
            completed_at: Some(started_at),
            input: ExchangeInput::Voice {
                final_transcript: if interrupted {
                    String::new()
                } else {
                    "Voice prompt".to_string()
                },
                utterances: vec![],
            },
            output: if interrupted {
                None
            } else {
                Some(ExchangeOutput {
                    response_id: Some("response-1".to_string()),
                    text: "Voice response".to_string(),
                    produced_at: started_at,
                    provider_name: Some("provider".to_string()),
                    target: Some("speech-output-provider".to_string()),
                    audio_marker: None,
                })
            },
            context_assembly: retrieved_memory_id.map(|memory_id| ContextAssembly {
                budget: ContextBudget::new(4, 600),
                selected: vec![ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: memory_id,
                        source_kind: crate::context::ContextSourceKind::Memory,
                        summary: "summary".to_string(),
                        tags: vec![],
                        score: 1.0,
                        estimated_tokens: 10,
                        source_reference: "tests".to_string(),
                        selection_reason: "tests".to_string(),
                    },
                    cumulative_estimated_tokens: 10,
                }],
                omitted: vec![],
                used_estimated_tokens: 10,
            }),
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            model: None,
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            tool_executions: vec![],
            status: if interrupted {
                crate::session::ExchangeStatus::Interrupted
            } else {
                crate::session::ExchangeStatus::Completed
            },
        };

        if let Some(text) = preamble_text {
            exchange.provider_events.push(ProviderEventRecord {
                exchange_index: index,
                event_kind: ProviderEventKind::Preamble,
                provider_id: "provider".to_string(),
                received_at: started_at,
                call_id: None,
                event_id: None,
                item_id: None,
                previous_item_id: None,
                response_id: None,
                text: Some(text.to_string()),
                status: Some("pending".to_string()),
                audio_marker: None,
            });
        }

        exchange
    }

    struct PreambleAssertingSleepClient {
        prohibited_text: String,
    }

    impl ModelClient for PreambleAssertingSleepClient {
        fn client_name(&self) -> &str {
            "preamble-asserting-sleep-client"
        }

        fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            assert!(
                request
                    .messages
                    .iter()
                    .all(|message| !message.content.contains(&self.prohibited_text))
            );

            Ok(ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                r#"{
                    "session_summary": "Summary.",
                    "memory_candidates": ["Routine observation."],
                    "open_questions": [],
                    "decision_candidates": [],
                    "future_context_hints": ["Be prepared."],
                    "review_notes": []
                }"#,
            ))
        }
    }
}
