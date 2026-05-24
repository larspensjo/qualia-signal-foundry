use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::models::{build_client, requested_provider_from_env};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::session::SessionState;
use crate::session::resume::ResumeInputs;
use crate::sleep::{SleepInputBundle, SleepReport, summarize_session};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

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
        self.run_with_provider_at_state_dir(
            context,
            requested_provider,
            crate::session::resume::state_dir_from_env(),
        )
    }

    fn run_with_provider_at_state_dir(
        &self,
        context: &mut RunContext,
        requested_provider: &str,
        state_dir: impl AsRef<Path>,
    ) -> anyhow::Result<ExperimentOutcome> {
        let state_dir = state_dir.as_ref().to_path_buf();
        let resume_inputs = crate::session::resume::load_resume_inputs(&state_dir)?;
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

        commit_cross_session_sleep(context, &summary.report, outcome, &state_dir)
    }
}

fn commit_cross_session_sleep(
    context: &RunContext,
    report: &SleepReport,
    mut outcome: ExperimentOutcome,
    state_dir: &Path,
) -> anyhow::Result<ExperimentOutcome> {
    let resume_inputs = crate::session::resume::load_resume_inputs(state_dir)?;
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

    let as_of = session
        .turns
        .iter()
        .map(|turn| turn.completed_at)
        .max()
        .map(time::OffsetDateTime::from)
        .unwrap_or_else(time::OffsetDateTime::now_utc);
    let sleep_run_id = context.run_id().to_string();
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

    transcript.push_str("\nCompleted turns:\n");
    if session.turns.is_empty() {
        transcript.push_str("- None recorded.\n");
    } else {
        for turn in &session.turns {
            transcript.push_str(&format!("\nTurn {}:\n", turn.index));
            transcript.push_str("User:\n");
            transcript.push_str(turn.user_input.trim());
            transcript.push_str("\nAssistant:\n");
            transcript.push_str(turn.assistant_response.trim());
            transcript.push('\n');

            if !turn.retrieved_memory_block.trim().is_empty() {
                transcript.push_str("Retrieved memory block:\n");
                transcript.push_str(turn.retrieved_memory_block.trim());
                transcript.push('\n');
            }

            if !turn.recalled_turns.is_empty() {
                transcript.push_str("Recalled turns:\n");
                for recall in &turn.recalled_turns {
                    transcript.push_str(&format!(
                        "- {} recalled turn {} via {}\n",
                        recall.call_id, recall.turn_id, recall.tool_name
                    ));
                }
            }
        }
    }

    SleepInputBundle::new("session_state", session.session_id.clone(), transcript)
        .with_review_notes(vec![
            "This sleep pass uses the persisted previous session state as its source.".to_string(),
            "All memory and decision outputs remain pending human review.".to_string(),
            "Trace the sleep report back to concrete prior turns rather than hidden state."
                .to_string(),
        ])
}

fn write_sleep_artifacts(
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

    use super::{SleepPhaseSessionSummaryExperiment, build_sleep_input};
    use crate::runtime::run_context::RunContext;
    use crate::session::manifest::{ContinuityManifest, ResumeMode};
    use crate::session::resume::ResumeInputs;
    use crate::session::{MemorySourceConfig, SessionConfig, SessionState, TurnSummary};

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

        assert!(sleep_report.contains("Source: `session-to-sleep` (session_state)"));
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
}
