use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::resume::load_resume_inputs;
use crate::session::{Exchange, SessionState, StateDirectoryResolution, Turn};
use crate::sleep::{SleepInputBundle, summarize_session};
use qsf_models::{ModelClient, build_client, requested_provider_from_env};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::sleep_phase_session_summary::{commit_cross_session_sleep, write_sleep_artifacts};
use super::transcript_format::{
    append_labelled_value, append_recalled_items, append_retrieved_memory_block,
};

const DEFAULT_CONTINUITY_SESSION_ID: &str = "default";
const FALLBACK_SESSION_TEXT: &str = "Realtime continuity root unavailable. This smoke input keeps live memory extraction runnable without persisted trusted turns.";

pub struct LiveMemoryExtractionExperiment;

impl Experiment for LiveMemoryExtractionExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::LiveMemoryExtraction
    }

    fn description(&self) -> &'static str {
        "Extract reviewable memory candidates from trusted realtime continuity artifacts"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_provider(context, requested_provider_from_env())
    }
}

impl LiveMemoryExtractionExperiment {
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
    ) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_provider_at_state_resolution(
            context,
            requested_provider,
            resolve_realtime_continuity_state_directory_from_env(),
        )
    }

    fn run_with_provider_at_state_resolution(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
        state_resolution: StateDirectoryResolution,
    ) -> anyhow::Result<ExperimentOutcome> {
        let continuity_root = state_resolution.persist_state_dir.display().to_string();
        let load_result = load_resume_inputs(&state_resolution.resume_state_dir);
        let (mut previous_session, load_note) = match load_result {
            Ok(inputs) => (inputs.previous_session, None),
            Err(error) => {
                engine_logging::engine_warn!(
                    "live memory extraction fell back to smoke input: continuity_root={} error={}",
                    continuity_root,
                    error
                );
                (
                    None,
                    Some(format!(
                        "Failed to load realtime continuity root `{continuity_root}`: {error}"
                    )),
                )
            }
        };
        let input = build_live_memory_extraction_input(
            continuity_root.clone(),
            previous_session.as_ref(),
            load_note.as_deref(),
        );

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

        let extraction_trace = TraceRecord::new(
            context.experiment_id(),
            "live-memory-extraction-summary",
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
        let extraction_trace_id = extraction_trace.trace_id;
        context.record_trace(extraction_trace)?;

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
            Some(extraction_trace_id),
        )?;

        write_sleep_artifacts(context, requested_provider, &input, &summary.report)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "message": &summary.report.session_summary,
                "memory_candidate_count": summary.report.memory_candidates.len(),
                "decision_candidate_count": summary.report.decision_candidates.len(),
            }),
            Some(extraction_trace_id),
        )?;

        let mut outcome = ExperimentOutcome {
            summary: format!(
                "The live memory extraction experiment summarized trusted realtime continuity from `{}` through the `{}` provider, wrote reviewable sleep-report artifacts, and kept candidate promotion outside the live loop.",
                input.source_label, summary.response.provider_name
            ),
            observations: vec![
                "Trusted promoted turns are the canonical extraction source; persisted exchanges are only metadata.".to_string(),
                "Extraction provenance is labeled in the input transcript so the summarizer can distinguish input transcription from assistant output.".to_string(),
                "The review path still routes through the existing sleep commit and auto-promote machinery.".to_string(),
            ],
            failure_modes: vec![
                "The mock provider keeps the extraction path deterministic, but real-provider report quality still depends on structured JSON compliance.".to_string(),
                "When the realtime continuity root is absent or malformed, the experiment falls back to a smoke transcript rather than blocking the workflow.".to_string(),
            ],
            follow_up_questions: vec![
                "Should the extraction path eventually consume a dedicated continuity manifest instead of the smoke-friendly resume loader?".to_string(),
                "Should provenance from the persisted exchanges be surfaced in a separate review artifact rather than inline notes?".to_string(),
            ],
            decision_candidates: vec![
                "Keep realtime memory extraction as an explicit offline pass over trusted continuity artifacts.".to_string(),
                "Keep the canonical extraction transcript on SessionState.turns and treat exchange records as metadata only.".to_string(),
            ],
            extra_artifacts: vec!["sleep-report.json".to_string(), "sleep-report.md".to_string()],
        };

        if load_note.is_none() {
            if let Some(session) = previous_session.as_mut() {
                let aged_count = apply_realtime_continuity_ageing(
                    context,
                    session,
                    &state_resolution.persist_state_dir,
                    client.as_ref(),
                )?;
                if aged_count > 0 {
                    outcome.observations.push(format!(
                        "Realtime ageing summarized {} trusted turns before consolidation.",
                        aged_count
                    ));
                }
            }
        }

        if load_note.is_none() && previous_session.is_some() {
            return commit_cross_session_sleep(
                context,
                &summary.report,
                outcome,
                &state_resolution,
            );
        }

        Ok(outcome)
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

pub(crate) fn build_live_memory_extraction_input(
    source_label: impl Into<String>,
    previous_session: Option<&SessionState>,
    load_note: Option<&str>,
) -> SleepInputBundle {
    let source_label = source_label.into();
    match previous_session {
        Some(session) => live_session_input(session, source_label, load_note),
        None => fallback_live_memory_input(source_label, load_note),
    }
}

fn live_session_input(
    session: &SessionState,
    source_label: String,
    load_note: Option<&str>,
) -> SleepInputBundle {
    let mut transcript = String::new();
    transcript.push_str("Live memory extraction input from trusted realtime continuity.\n");
    transcript.push_str(&format!("Realtime continuity root: {source_label}\n"));
    transcript.push_str(&format!("Session id: {}\n", session.session_id));
    if let Some(previous_session_id) = &session.previous_session_id {
        transcript.push_str(&format!("Previous session id: {previous_session_id}\n"));
    }
    transcript.push_str(&format!(
        "Trusted promoted turns: {}\n",
        session.turns.len()
    ));
    transcript.push_str("Canonical transcript source: SessionState.turns\n");
    transcript.push_str(
        "Persisted exchanges are metadata only; they are not re-transcribed into the canonical input.\n",
    );

    if !session.summarized_turns.is_empty() {
        transcript.push_str("\nPrior turn summaries:\n");
        for summary in &session.summarized_turns {
            transcript.push_str(&format!(
                "- Turn {} summarized after turn {}: {}\n",
                summary.turn_index, summary.summarized_after_turn_index, summary.summary
            ));
        }
    }

    let mut review_notes = vec![
        "Trusted promoted turns are the only extraction-eligible material; degraded, interrupted, and diagnostic-only material stays out of this input.".to_string(),
        "Canonical transcript provenance is labeled inline: input transcription is separate from assistant output.".to_string(),
        "Persisted exchanges are consulted only as metadata for already-promoted turn indices.".to_string(),
    ];
    if let Some(load_note) = load_note {
        review_notes.push(load_note.to_string());
    }

    transcript.push_str("\nPromoted turns:\n");
    if session.turns.is_empty() {
        transcript.push_str("- None recorded.\n");
    } else {
        for turn in &session.turns {
            let matching_exchange = session
                .exchanges
                .iter()
                .find(|exchange| exchange.index == turn.index);
            append_turn_block(&mut transcript, turn, matching_exchange);
        }
    }

    let mut diagnostic_notes = Vec::new();
    if let Some(load_note) = load_note {
        diagnostic_notes.push(load_note.to_string());
    }

    SleepInputBundle::new("realtime_continuity_root", source_label, transcript)
        .with_review_notes(review_notes)
        .with_diagnostic_notes(diagnostic_notes)
}

fn fallback_live_memory_input(source_label: String, load_note: Option<&str>) -> SleepInputBundle {
    let mut review_notes = vec![
        "No trusted realtime continuity root was available, so the extraction pass used a smoke transcript.".to_string(),
        "The smoke path still exercises the explicit live-memory extraction entry point.".to_string(),
    ];
    let mut diagnostic_notes = vec![
        "Fallback smoke transcript was used because the trusted realtime continuity root was unavailable.".to_string(),
    ];
    if let Some(load_note) = load_note {
        review_notes.push(load_note.to_string());
        diagnostic_notes.push(load_note.to_string());
    }

    SleepInputBundle::new(
        "realtime_continuity_root",
        source_label,
        FALLBACK_SESSION_TEXT,
    )
    .with_review_notes(review_notes)
    .with_diagnostic_notes(diagnostic_notes)
}

fn append_turn_block(transcript: &mut String, turn: &Turn, exchange: Option<&Exchange>) {
    transcript.push_str(&format!("\nTurn {} [trusted promoted turn]\n", turn.index));
    match exchange {
        Some(exchange) => transcript.push_str(&format!(
            "Matching persisted exchange: present (status={:?}, interruptions={}, tool_requests={}, tool_executions={}, provider_events={})\n",
            exchange.status,
            exchange.interruptions.len(),
            exchange.tool_requests.len(),
            exchange.tool_executions.len(),
            exchange.provider_events.len()
        )),
        None => transcript.push_str("Matching persisted exchange: absent\n"),
    }

    append_labelled_value(
        transcript,
        "Input transcript (source=input transcription)",
        &turn.user_input,
        "(empty user transcript)",
    );
    append_labelled_value(
        transcript,
        "Assistant output (source=model response)",
        &turn.assistant_response,
        "(empty assistant response)",
    );
    append_retrieved_memory_block(transcript, &turn.retrieved_memory_block);
    append_recalled_items(transcript, "Recalled turns", &turn.recalled_turns);
    append_turn_tool_requests(transcript, &turn.tool_requests);
    append_turn_tool_executions(transcript, &turn.tool_executions);
}

fn append_turn_tool_requests(
    transcript: &mut String,
    tool_requests: &[crate::session::ToolRequestRecord],
) {
    if tool_requests.is_empty() {
        return;
    }

    transcript.push_str("Tool requests:\n");
    for request in tool_requests {
        transcript.push_str(&format!(
            "- call_id={} tool_name={} source={} auto_executed={}{} arguments={}\n",
            request.call_id,
            request.tool_name,
            request.source,
            request.auto_executed,
            request
                .routed_to
                .as_ref()
                .map(|routed_to| format!(" routed_to={routed_to}"))
                .unwrap_or_default(),
            request.arguments_summary
        ));
    }
}

fn append_turn_tool_executions(
    transcript: &mut String,
    tool_executions: &[qsf_session::ToolExecutionRecord],
) {
    if tool_executions.is_empty() {
        return;
    }

    transcript.push_str("Tool executions:\n");
    for execution in tool_executions {
        transcript.push_str(&format!(
            "- call_id={} tool_name={} permission={:?} status={:?} result={}{}\n",
            execution.call_id,
            execution.tool_name,
            execution.permission_decision,
            execution.status,
            execution.result_summary,
            execution
                .error
                .as_ref()
                .map(|error| format!(" error={error}"))
                .unwrap_or_default()
        ));
    }
}

fn apply_realtime_continuity_ageing(
    context: &mut RunContext,
    session: &mut SessionState,
    state_dir: &Path,
    model_client: &dyn ModelClient,
) -> anyhow::Result<usize> {
    let active_turns = session
        .turns
        .len()
        .saturating_sub(session.summarized_turns.len());
    if active_turns <= session.config.warm_threshold {
        return Ok(0);
    }

    let store_path = state_dir.join("memory-store.json");
    if !store_path.exists() {
        crate::memory::MemoryStore::load_or_empty(&store_path)?.persist()?;
    }

    let summarized_before = session.summarized_turns.len();
    crate::session::ageing::age_out_warm_turns(context, session, state_dir, model_client)?;
    let aged_count = session
        .summarized_turns
        .len()
        .saturating_sub(summarized_before);
    if aged_count > 0 {
        crate::session::persistence::persist_session_state(session, state_dir)?;
    }

    Ok(aged_count)
}

fn resolve_realtime_continuity_state_directory_from_env() -> StateDirectoryResolution {
    let session_state_dir =
        crate::session::resolve_shared_state_directory_from_env().persist_state_dir;
    let state_root = session_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let continuity_root = state_root
        .join("realtime")
        .join("continuity")
        .join(DEFAULT_CONTINUITY_SESSION_ID);

    StateDirectoryResolution {
        resume_state_dir: continuity_root.clone(),
        persist_state_dir: continuity_root,
        legacy_fallback_used: false,
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use uuid::Uuid;

    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{LiveMemoryExtractionExperiment, build_live_memory_extraction_input};
    use crate::context::{ContextAssembly, ContextBudget, ContextFragment, ContextSelection};
    use crate::memory::MemoryStore;
    use crate::runtime::run_context::RunContext;
    use crate::session::manifest::{ContinuityManifest, ResumeMode};
    use crate::session::persistence::persist_session_state;
    use crate::session::{
        Exchange, ExchangeInput, ExchangeOutput, ExchangeStatus, InterruptionAction,
        InterruptionRecord, InterruptionStopOutcome, MemorySourceConfig, ProviderEventKind,
        ProviderEventRecord, SessionConfig, SessionState, ToolRequestRecord,
    };
    use qsf_models::MockModelClient;
    use qsf_session::{ToolExecutionRecord, ToolExecutionStatus, ToolPermissionDecision};
    use qsf_volition::{
        REALTIME_SEED_FIXTURE_ID, VolitionContinuitySnapshot, VolitionState,
        build_state_inspection, persist_volition_continuity_snapshot, realtime_seed_fixture,
    };

    #[test]
    fn live_memory_input_uses_turns_as_canonical_source_without_duplicating_exchange_text() {
        let mut session = SessionState::new_with_id("live-memory-session".to_string(), config());
        session.turns.push(turn_with_memory(
            0,
            "canonical user input",
            "canonical assistant output",
        ));
        session.turns[0].tool_requests.push(ToolRequestRecord {
            exchange_index: 0,
            call_id: "call-turn-0".to_string(),
            tool_name: "search_memory".to_string(),
            arguments_summary: "{\"query\":\"signal\"}".to_string(),
            requested_at: SystemTime::UNIX_EPOCH,
            source: "test".to_string(),
            routed_to: Some("qsf_tool_permission_boundary".to_string()),
            auto_executed: false,
        });
        session.turns[0].tool_executions.push(ToolExecutionRecord {
            exchange_index: 0,
            call_id: "call-turn-0".to_string(),
            tool_name: "search_memory".to_string(),
            permission_decision: ToolPermissionDecision::Allowed,
            status: ToolExecutionStatus::Completed,
            result_summary: "found a useful memory".to_string(),
            error: None,
            requested_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            response_model_use: None,
            returning_event_id: None,
        });
        session.exchanges.push(Exchange {
            index: 0,
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            input: ExchangeInput::Voice {
                final_transcript: "exchange text should stay out".to_string(),
                utterances: vec![],
            },
            output: Some(ExchangeOutput {
                response_id: Some("response-0".to_string()),
                text: "exchange output should stay out".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("provider".to_string()),
                target: Some("speech".to_string()),
                audio_marker: None,
            }),
            context_assembly: Some(ContextAssembly {
                budget: ContextBudget::new(2, 120),
                selected: vec![ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory.1".to_string(),
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
            interruptions: vec![InterruptionRecord {
                exchange_index: 0,
                response_id: Some("response-0".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "sideband".to_string(),
                action: InterruptionAction::MarkInterrupted,
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
                response_id: Some("response-0".to_string()),
                text: Some("diagnostic only".to_string()),
                status: Some("completed".to_string()),
                audio_marker: None,
            }],
            tool_requests: vec![],
            tool_executions: vec![],
            status: ExchangeStatus::Interrupted,
        });

        let input = build_live_memory_extraction_input("root/default", Some(&session), None);

        assert_eq!(input.source_kind, "realtime_continuity_root");
        assert_eq!(input.source_label, "root/default");
        assert!(input.session_text.contains("canonical user input"));
        assert!(input.session_text.contains("canonical assistant output"));
        assert!(!input.session_text.contains("exchange text should stay out"));
        assert!(
            !input
                .session_text
                .contains("exchange output should stay out")
        );
        assert!(
            input
                .session_text
                .contains("Matching persisted exchange: present")
        );
        assert!(
            !input
                .session_text
                .contains("Exchange metadata not reflected in canonical text")
        );
        assert!(input.session_text.contains("Tool requests:"));
        assert!(input.session_text.contains("Tool executions:"));
        assert!(
            input
                .review_notes
                .iter()
                .any(|note| note.contains("Canonical transcript provenance"))
        );
    }

    #[test]
    fn live_memory_input_handles_absent_root_with_smoke_fallback() {
        let input = build_live_memory_extraction_input("root/default", None, None);

        assert_eq!(input.source_kind, "realtime_continuity_root");
        assert!(
            input
                .session_text
                .contains("Realtime continuity root unavailable")
        );
        assert!(
            input
                .review_notes
                .iter()
                .any(|note| note.contains("smoke transcript"))
        );
    }

    #[test]
    fn live_memory_input_carries_load_notes_into_review_notes() {
        let input = build_live_memory_extraction_input(
            "root/default",
            None,
            Some("Failed to load realtime continuity root `root/default`: malformed"),
        );

        assert!(
            input
                .review_notes
                .iter()
                .any(|note| note.contains("malformed"))
        );
    }

    #[test]
    fn smoke_provider_request_stays_deterministic() {
        let base_dir = std::env::temp_dir().join(format!("qsf-live-memory-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "live-memory-test").unwrap();
        let input = build_live_memory_extraction_input("root/default", None, None);

        let result =
            crate::sleep::summarize_session(&mut context, &MockModelClient::default(), &input)
                .unwrap();

        assert!(
            result
                .report
                .session_summary
                .contains("Mock sleep summarizer")
        );
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn live_memory_experiment_marks_persisted_session_as_review_source() {
        let base_dir = std::env::temp_dir().join(format!("qsf-live-memory-run-{}", Uuid::new_v4()));
        let continuity_root = base_dir.join("state/realtime/continuity/default");
        let mut previous =
            SessionState::new_with_id("continuity-session".to_string(), config_with_warm(0));
        previous.turns.push(turn_with_memory(
            0,
            "Remember the presence signals.",
            "We will keep the live loop observable.",
        ));
        previous.turns.push(turn_with_memory(
            1,
            "Keep realtime extraction offline.",
            "Extraction will reuse sleep consolidation.",
        ));
        persist_session_state(&previous, &continuity_root).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            current_volition_snapshot_path: None,
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(continuity_root.join("continuity-manifest.json"))
        .unwrap();

        let mut context = RunContext::create_in(&base_dir, "live-memory-run").unwrap();
        let experiment = LiveMemoryExtractionExperiment;
        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &continuity_root)
            .unwrap();
        let sleep_report = fs::read_to_string(context.run_dir().join("sleep-report.md")).unwrap();
        let store = MemoryStore::load_or_empty(continuity_root.join("memory-store.json")).unwrap();
        let persisted_session = crate::session::persistence::load_session_state(
            continuity_root.join("session-state.json"),
        )
        .unwrap();

        assert!(sleep_report.contains(&format!(
            "Source: `{}` (realtime_continuity_root)",
            continuity_root.display()
        )));
        assert!(
            outcome
                .observations
                .iter()
                .any(|observation| observation.contains("canonical extraction source"))
        );
        assert!(
            outcome
                .observations
                .iter()
                .any(|observation| observation.contains("Realtime ageing summarized 2"))
        );
        assert!(store.contents().records.is_empty());
        assert_eq!(store.contents().processed_ranges.len(), 2);
        assert_eq!(persisted_session.summarized_turns.len(), 2);

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn live_memory_sleep_runs_volition_maintenance_from_resolved_continuity_root() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-live-memory-volition-run-{}", Uuid::new_v4()));
        let continuity_root = base_dir.join("state/realtime/continuity/default");
        let mut previous =
            SessionState::new_with_id("continuity-session".to_string(), config_with_warm(0));
        previous.turns.push(turn_with_memory(
            0,
            "Remember to preserve sleep volition continuity.",
            "The sleep pass should see the realtime volition snapshot.",
        ));
        persist_session_state(&previous, &continuity_root).unwrap();
        write_volition_snapshot_fixture(&continuity_root, &previous.session_id).unwrap();
        ContinuityManifest {
            current_session_id: Some(previous.session_id.clone()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            current_volition_snapshot_path: Some(PathBuf::from("volition-state.json")),
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
            ..ContinuityManifest::default()
        }
        .persist(continuity_root.join("continuity-manifest.json"))
        .unwrap();

        let mut context = RunContext::create_in(&base_dir, "live-memory-volition-run").unwrap();
        let experiment = LiveMemoryExtractionExperiment;
        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &continuity_root)
            .unwrap();

        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        assert!(traces.contains(r#""operation":"live-goal-formation""#));
        assert!(traces.contains(r#""operation":"goal-coherence-check""#));
        assert!(
            continuity_root
                .join("volition-continuity-report.json")
                .exists()
        );
        assert!(
            outcome
                .observations
                .iter()
                .any(|observation| observation.contains("Volition consolidation"))
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn live_memory_experiment_skips_commit_when_root_is_absent() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-live-memory-empty-{}", Uuid::new_v4()));
        let continuity_root = base_dir.join("state/realtime/continuity/default");
        let mut context = RunContext::create_in(&base_dir, "live-memory-empty").unwrap();
        let experiment = LiveMemoryExtractionExperiment;

        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &continuity_root)
            .unwrap();

        assert!(
            outcome
                .summary
                .contains("live memory extraction experiment summarized")
        );
        assert!(
            fs::metadata(continuity_root.join("memory-store.json")).is_err(),
            "smoke fallback should not write the realtime memory store"
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn malformed_continuity_root_falls_back_to_smoke_input() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-live-memory-malformed-{}", Uuid::new_v4()));
        let continuity_root = base_dir.join("state/realtime/continuity/default");
        fs::create_dir_all(&continuity_root).unwrap();
        fs::write(
            continuity_root.join("continuity-manifest.json"),
            "{not-json",
        )
        .unwrap();

        let mut context = RunContext::create_in(&base_dir, "live-memory-malformed").unwrap();
        let experiment = LiveMemoryExtractionExperiment;
        let outcome = experiment
            .run_with_provider_at_state_dir(&mut context, "mock", &continuity_root)
            .unwrap();
        let report = fs::read_to_string(context.run_dir().join("sleep-report.md")).unwrap();

        assert!(
            outcome
                .summary
                .contains("live memory extraction experiment")
        );
        assert!(report.contains("smoke transcript"));
        assert!(report.contains("Failed to load realtime continuity root"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    fn config() -> SessionConfig {
        config_with_warm(2)
    }

    fn write_volition_snapshot_fixture(
        continuity_dir: &Path,
        session_id: &str,
    ) -> anyhow::Result<()> {
        fs::create_dir_all(continuity_dir)?;
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let snapshot = VolitionContinuitySnapshot::new(
            session_id,
            "2026-07-02T00:00:00Z",
            REALTIME_SEED_FIXTURE_ID,
            state,
            inspection,
        );
        persist_volition_continuity_snapshot(
            &snapshot,
            continuity_dir.join("volition-state.json"),
        )?;
        Ok(())
    }

    fn config_with_warm(warm_threshold: usize) -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }

    fn turn_with_memory(
        index: usize,
        user_input: &str,
        assistant_response: &str,
    ) -> crate::session::Turn {
        let mut turn = crate::session::tests::fake_turn(index);
        turn.user_input = user_input.to_string();
        turn.assistant_response = assistant_response.to_string();
        turn.context_assembly = ContextAssembly {
            budget: ContextBudget::new(2, 120),
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: format!("memory.{index}"),
                    source_kind: crate::context::ContextSourceKind::Memory,
                    summary: format!("memory summary {index}"),
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
}
