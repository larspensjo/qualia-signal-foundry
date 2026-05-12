use std::fs;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::models::{build_client, requested_provider_from_env};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
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
        let input =
            SleepInputBundle::new("session_transcript", "phase-7-sleep-session", SESSION_TEXT)
                .with_review_notes(vec![
                "All memory and decision outputs remain pending human review.".to_string(),
                "Trace the sleep report back to the session transcript rather than hidden state."
                    .to_string(),
            ]);

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
        let elapsed_ns = started_at
            .elapsed()
            .as_nanos()
            .try_into()
            .unwrap_or(u64::MAX);

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

        Ok(ExperimentOutcome {
            summary: format!(
                "Phase 7 sleep MVP summarized a short session through the `{}` provider, wrote a reviewable sleep report artifact, and recorded explicit sleep-phase events and traces without promoting any candidate into an accepted decision.",
                summary.response.provider_name
            ),
            observations: vec![
                "The sleep phase now reuses the Phase 6 model-role path, so provider selection, latency, and usage stay observable without special-case plumbing.".to_string(),
                "Sleep reports preserve separate fields for summary, memory candidates, open questions, decision candidates, and future context hints.".to_string(),
                "Review notes are carried into the artifact so the output stays explicitly provisional.".to_string(),
            ],
            failure_modes: vec![
                "The default mock provider keeps the experiment deterministic, but real-provider output quality still depends on prompt compliance and structured JSON output.".to_string(),
                "The current experiment uses a short inline transcript rather than replaying a full prior run's event log.".to_string(),
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
        })
    }
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
    markdown.push_str("# Phase 7 Sleep Report\n\n");
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

    use super::SleepPhaseSessionSummaryExperiment;
    use crate::runtime::run_context::RunContext;

    #[test]
    fn sleep_experiment_writes_sleep_report_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-phase7-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "phase-seven-test").unwrap();
        let experiment = SleepPhaseSessionSummaryExperiment;

        let outcome = experiment.run_with_provider(&mut context, "mock").unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let sleep_report = fs::read_to_string(context.run_dir().join("sleep-report.md")).unwrap();

        assert_eq!(context.event_count(), 6);
        assert_eq!(context.trace_count(), 2);
        assert!(outcome.summary.contains("Phase 7 sleep MVP"));
        assert!(events.contains("SleepPhaseRequested"));
        assert!(events.contains("SleepPhaseCompleted"));
        assert!(traces.contains("sleep-phase-summary"));
        assert!(sleep_report.contains("Phase 7 Sleep Report"));
        assert!(sleep_report.contains("manual review"));

        fs::remove_dir_all(base_dir).unwrap();
    }
}
