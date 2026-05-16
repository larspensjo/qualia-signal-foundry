use std::path::PathBuf;

use anyhow::anyhow;
use time::OffsetDateTime;

use serde_json::json;

use crate::memory::{
    REVIEWED_MEMORY_DRAFT_JSON, REVIEWED_MEMORY_DRAFT_MARKDOWN, ReviewedMemoryDraft,
    load_reviewed_memory_draft, write_reviewed_memory_draft,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const REVIEWED_MEMORY_SLEEP_REPORT_ENV_VAR: &str = "QSF_REVIEWED_MEMORY_SLEEP_REPORT";

pub struct ReviewedMemoryDraftExperiment;

impl Experiment for ReviewedMemoryDraftExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::ReviewedMemoryDraft
    }

    fn description(&self) -> &'static str {
        "Convert provisional sleep memory candidates into a reviewable file-backed memory draft"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let source_report_path = source_report_path_from_env()?;
        let draft = load_reviewed_memory_draft(&source_report_path, OffsetDateTime::now_utc())?;
        self.write_draft_outcome(context, &draft)
    }
}

impl ReviewedMemoryDraftExperiment {
    #[cfg(test)]
    fn run_with_source_report(
        &self,
        context: &mut RunContext,
        source_report_path: impl AsRef<std::path::Path>,
        created_at: OffsetDateTime,
    ) -> anyhow::Result<ExperimentOutcome> {
        let draft = load_reviewed_memory_draft(source_report_path, created_at)?;
        self.write_draft_outcome(context, &draft)
    }

    fn write_draft_outcome(
        &self,
        context: &mut RunContext,
        draft: &ReviewedMemoryDraft,
    ) -> anyhow::Result<ExperimentOutcome> {
        context.record_event(
            EventType::InputReceived,
            json!({
                "source_sleep_run_id": &draft.source_sleep_run_id,
                "source_sleep_report_path": draft.source_sleep_report_path.display().to_string(),
                "memory_candidate_count": draft.record_count(),
                "review_required": true,
                "source_env_var": REVIEWED_MEMORY_SLEEP_REPORT_ENV_VAR,
            }),
            None,
        )?;

        write_reviewed_memory_draft(context.run_dir(), draft)?;

        let trace = TraceRecord::new(
            context.experiment_id(),
            "reviewed-memory-draft-conversion",
            format!(
                "source_sleep_run={} source_report={}",
                draft.source_sleep_run_id,
                draft.source_sleep_report_path.display()
            ),
            format!(
                "wrote {} draft records and 0 associations",
                draft.record_count()
            ),
        )
        .with_details(json!({
            "source_sleep_run_id": &draft.source_sleep_run_id,
            "source_sleep_report_path": draft.source_sleep_report_path.display().to_string(),
            "draft_json": REVIEWED_MEMORY_DRAFT_JSON,
            "draft_markdown": REVIEWED_MEMORY_DRAFT_MARKDOWN,
            "record_count": draft.record_count(),
            "association_count": draft.fixture.associations.len(),
            "review_required": true,
        }));
        let trace_id = trace.trace_id;
        context.record_trace(trace)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "draft_json": REVIEWED_MEMORY_DRAFT_JSON,
                "draft_markdown": REVIEWED_MEMORY_DRAFT_MARKDOWN,
                "record_count": draft.record_count(),
                "association_count": draft.fixture.associations.len(),
                "status": "draft",
            }),
            Some(trace_id),
        )?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Converted {} provisional sleep memory candidates from `{}` into a separate reviewed memory draft run without promoting them into durable voice memory.",
                draft.record_count(),
                draft.source_sleep_run_id
            ),
            observations: vec![
                "The conversion writes a MemoryFixture JSON with empty associations for explicit review.".to_string(),
                "The Markdown companion keeps the source sleep run and candidate indexes visible without requiring event-log inspection.".to_string(),
                "The draft remains disconnected from the text-owned voice loop until selected explicitly as a file-backed memory source.".to_string(),
            ],
            failure_modes: vec![
                format!(
                    "`{REVIEWED_MEMORY_SLEEP_REPORT_ENV_VAR}` must point at a readable sleep-report.json artifact."
                ),
                "Malformed sleep report JSON fails before any draft artifacts are written.".to_string(),
            ],
            follow_up_questions: vec![
                "Should later review artifacts include per-record source excerpts and suggested edits?".to_string(),
                "Should accepted memory live in a dedicated reviewed fixture file or a separate memory directory?".to_string(),
            ],
            decision_candidates: vec![
                "Keep sleep-to-memory conversion explicit and separate from both sleep summarization and live voice runs.".to_string(),
            ],
            extra_artifacts: vec![
                REVIEWED_MEMORY_DRAFT_JSON.to_string(),
                REVIEWED_MEMORY_DRAFT_MARKDOWN.to_string(),
            ],
        })
    }
}

fn source_report_path_from_env() -> anyhow::Result<PathBuf> {
    std::env::var(REVIEWED_MEMORY_SLEEP_REPORT_ENV_VAR)
        .map(PathBuf::from)
        .map_err(|_| {
            anyhow!(
                "`{}` must point to a sleep-report.json file",
                REVIEWED_MEMORY_SLEEP_REPORT_ENV_VAR
            )
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::ReviewedMemoryDraftExperiment;
    use crate::memory::MemoryFixture;
    use crate::runtime::run_context::RunContext;

    #[test]
    fn reviewed_memory_draft_experiment_writes_artifacts_in_conversion_run() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-memory-draft-{}", uuid::Uuid::new_v4()));
        let source_run_dir = base_dir.join("source-sleep-run");
        fs::create_dir_all(&source_run_dir).unwrap();
        fs::write(
            source_run_dir.join("sleep-report.json"),
            r#"{
  "session_summary": "Short summary.",
  "memory_candidates": [
    {
      "summary": "Draft memory should remain reviewable.",
      "importance": 0.7,
      "source_reference": "events.jsonl#1"
    }
  ],
  "open_questions": [],
  "decision_candidates": [],
  "future_context_hints": [],
  "review_notes": []
}"#,
        )
        .unwrap();
        let mut context = RunContext::create_in(&base_dir, "reviewed-memory-draft-test").unwrap();
        let experiment = ReviewedMemoryDraftExperiment;

        let outcome = experiment
            .run_with_source_report(
                &mut context,
                source_run_dir.join("sleep-report.json"),
                timestamp(),
            )
            .unwrap();

        let draft_json_text =
            fs::read_to_string(context.run_dir().join("reviewed-memory-draft.json")).unwrap();
        let draft_json: MemoryFixture = serde_json::from_str(&draft_json_text).unwrap();
        let draft_markdown =
            fs::read_to_string(context.run_dir().join("reviewed-memory-draft.md")).unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();

        assert!(outcome.summary.contains("Converted 1 provisional"));
        assert_eq!(draft_json.records.len(), 1);
        assert!(draft_json.associations.is_empty());
        assert_eq!(
            draft_json.records[0].id,
            "memory.sleep.source-sleep-run.001"
        );
        assert_eq!(draft_json.records[0].source_reference, "events.jsonl#1");
        assert!(draft_markdown.contains("Source sleep run: `source-sleep-run`"));
        assert!(draft_markdown.contains("Review policy: provisional until manually accepted"));
        assert!(draft_markdown.contains("memory_candidates[001]"));
        assert!(draft_markdown.contains("Importance: `0.70`"));
        assert!(draft_markdown.contains("$env:QSF_VOICE_MEMORY_FILE="));
        assert!(events.contains("OutputProduced"));
        assert!(!source_run_dir.join("reviewed-memory-draft.json").exists());

        fs::remove_dir_all(base_dir).unwrap();
    }

    fn timestamp() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-12T10:11:12Z", &Rfc3339).unwrap()
    }
}
