use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use serde_json::json;

use crate::memory::MemoryFixture;
use crate::memory::association::ensure_current_association_schema;
use crate::memory::memory_record::ensure_current_memory_schema;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const ACCEPT_MEMORY_DRAFT_ENV_VAR: &str = "QSF_ACCEPT_MEMORY_DRAFT";
const REVIEWED_MEMORY_TARGET: &str = "docs/Experiments/Fixtures/voice-memory.reviewed.json";

pub struct AcceptReviewedMemoryExperiment;

impl Experiment for AcceptReviewedMemoryExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::AcceptReviewedMemory
    }

    fn description(&self) -> &'static str {
        "Accept a reviewed memory draft and write it as the durable reviewed voice-memory fixture"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let draft_path = draft_path_from_env()?;
        self.run_with_draft_path(context, &draft_path, Path::new(REVIEWED_MEMORY_TARGET))
    }
}

impl AcceptReviewedMemoryExperiment {
    fn run_with_draft_path(
        &self,
        context: &mut RunContext,
        draft_path: &Path,
        target_path: &Path,
    ) -> anyhow::Result<ExperimentOutcome> {
        let fixture = load_and_validate_draft(draft_path)?;

        let record_count = fixture.records.len();
        let association_count = fixture.associations.len();

        context.record_event(
            EventType::InputReceived,
            json!({
                "source_draft_path": draft_path.display().to_string(),
                "target_path": target_path.display().to_string(),
                "record_count": record_count,
                "association_count": association_count,
                "source_env_var": ACCEPT_MEMORY_DRAFT_ENV_VAR,
            }),
            None,
        )?;

        write_accepted_fixture(target_path, &fixture)?;

        let trace = TraceRecord::new(
            context.experiment_id(),
            "accept-reviewed-memory",
            format!(
                "source_draft={} target={}",
                draft_path.display(),
                target_path.display()
            ),
            format!(
                "accepted {} records and {} associations as reviewed voice memory",
                record_count, association_count
            ),
        )
        .with_details(json!({
            "source_draft_path": draft_path.display().to_string(),
            "target_path": target_path.display().to_string(),
            "record_count": record_count,
            "association_count": association_count,
            "status": "accepted",
        }));
        let trace_id = trace.trace_id;
        context.record_trace(trace)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "target_path": target_path.display().to_string(),
                "record_count": record_count,
                "association_count": association_count,
                "status": "accepted",
            }),
            Some(trace_id),
        )?;

        let (summary, follow_up) = if record_count == 0 {
            (
                "Accepted an empty reviewed memory fixture. No records were promoted.".to_string(),
                vec!["Was the source draft intentionally empty?".to_string()],
            )
        } else {
            (
                format!(
                    "Accepted {} reviewed memory record{} and {} association{} into `{}`.",
                    record_count,
                    if record_count == 1 { "" } else { "s" },
                    association_count,
                    if association_count == 1 { "" } else { "s" },
                    target_path.display()
                ),
                vec![format!(
                    "Run the voice loop with `$env:QSF_VOICE_MEMORY_FILE=\"{}\"` to verify retrieval.",
                    target_path.display()
                )],
            )
        };

        Ok(ExperimentOutcome {
            summary,
            observations: vec![
                "The accepted fixture is now the durable reviewed voice-memory source.".to_string(),
                "Acceptance is an explicit experiment step; sleep output never promotes automatically.".to_string(),
                "Schema validation runs before the target file is written.".to_string(),
            ],
            failure_modes: vec![
                format!(
                    "`{ACCEPT_MEMORY_DRAFT_ENV_VAR}` must point at a readable reviewed-memory-draft.json artifact."
                ),
                "Malformed or schema-incompatible draft JSON fails before any file is written.".to_string(),
                "The target directory `docs/Experiments/Fixtures/` must be writable.".to_string(),
            ],
            follow_up_questions: follow_up,
            decision_candidates: vec![
                "Keep memory acceptance explicit and separate from both sleep summarization and live voice runs.".to_string(),
            ],
            extra_artifacts: vec![
                REVIEWED_MEMORY_TARGET.to_string(),
            ],
        })
    }
}

fn draft_path_from_env() -> anyhow::Result<PathBuf> {
    std::env::var(ACCEPT_MEMORY_DRAFT_ENV_VAR)
        .map(PathBuf::from)
        .map_err(|_| {
            anyhow!(
                "`{}` must point to a reviewed-memory-draft.json file",
                ACCEPT_MEMORY_DRAFT_ENV_VAR
            )
        })
}

fn load_and_validate_draft(path: &Path) -> anyhow::Result<MemoryFixture> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read reviewed memory draft `{}`", path.display()))?;
    let fixture: MemoryFixture = serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse reviewed memory draft JSON `{}`",
            path.display()
        )
    })?;

    ensure_current_memory_schema(&fixture.records).with_context(|| {
        format!(
            "memory record schema mismatch in draft `{}`",
            path.display()
        )
    })?;
    ensure_current_association_schema(&fixture.associations)
        .with_context(|| format!("association schema mismatch in draft `{}`", path.display()))?;

    Ok(fixture)
}

fn write_accepted_fixture(target_path: &Path, fixture: &MemoryFixture) -> anyhow::Result<()> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create target directory `{}`", parent.display()))?;
    }

    let serialized = serde_json::to_string_pretty(fixture)
        .with_context(|| "failed to serialize accepted memory fixture".to_string())?;

    fs::write(target_path, serialized).with_context(|| {
        format!(
            "failed to write accepted memory fixture `{}`",
            target_path.display()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::memory::MemoryFixture;
    use crate::runtime::run_context::RunContext;

    use super::AcceptReviewedMemoryExperiment;

    #[test]
    fn accept_reviewed_memory_writes_target_fixture() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-accept-memory-{}", uuid::Uuid::new_v4()));
        let runs_dir = base_dir.join("runs");
        let fixtures_dir = base_dir.join("docs/Experiments/Fixtures");
        fs::create_dir_all(&runs_dir).unwrap();
        fs::create_dir_all(&fixtures_dir).unwrap();

        let draft_path = base_dir.join("my-draft.json");
        fs::write(
            &draft_path,
            r#"{
  "records": [
    {
      "schema_version": 1,
      "id": "memory.sleep.source-run.001",
      "kind": "observation",
      "title": "Accepted memory should be durable.",
      "summary": "Accepted memory should be durable and reviewable.",
      "tags": [],
      "created_at": "2026-05-16T10:00:00Z",
      "importance": 0.75,
      "reinforcement_count": 0,
      "source_reference": "sleep-run:source-run#memory_candidates[001]",
      "estimated_tokens": 10
    }
  ],
  "associations": []
}"#,
        )
        .unwrap();

        let target_path = base_dir.join("docs/Experiments/Fixtures/voice-memory.reviewed.json");
        let mut context = RunContext::create_in(&runs_dir, "accept-reviewed-memory").unwrap();
        let experiment = AcceptReviewedMemoryExperiment;
        let outcome = experiment
            .run_with_draft_path(&mut context, &draft_path, &target_path)
            .unwrap();

        assert!(
            outcome
                .summary
                .contains("Accepted 1 reviewed memory record")
        );
        assert!(outcome.summary.contains("voice-memory.reviewed.json"));

        let accepted_contents = fs::read_to_string(&target_path).unwrap();
        let accepted: MemoryFixture = serde_json::from_str(&accepted_contents).unwrap();
        assert_eq!(accepted.records.len(), 1);
        assert_eq!(accepted.records[0].id, "memory.sleep.source-run.001");
        assert_eq!(
            accepted.records[0].summary,
            "Accepted memory should be durable and reviewable."
        );
        assert!(accepted.associations.is_empty());

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        assert!(events.contains("InputReceived"));
        assert!(events.contains("OutputProduced"));

        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        assert!(traces.contains("accept-reviewed-memory"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn accept_empty_draft_produces_valid_empty_fixture() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-accept-empty-{}", uuid::Uuid::new_v4()));
        let runs_dir = base_dir.join("runs");
        let fixtures_dir = base_dir.join("docs/Experiments/Fixtures");
        fs::create_dir_all(&fixtures_dir).unwrap();

        let draft_path = base_dir.join("empty-draft.json");
        fs::write(&draft_path, r#"{"records": [], "associations": []}"#).unwrap();

        let target_path = base_dir.join("docs/Experiments/Fixtures/voice-memory.reviewed.json");
        let mut context = RunContext::create_in(&runs_dir, "accept-reviewed-memory").unwrap();
        let experiment = AcceptReviewedMemoryExperiment;
        let outcome = experiment
            .run_with_draft_path(&mut context, &draft_path, &target_path)
            .unwrap();

        assert!(outcome.summary.contains("empty"));

        let accepted: MemoryFixture =
            serde_json::from_str(&fs::read_to_string(&target_path).unwrap()).unwrap();
        assert!(accepted.records.is_empty());
        assert!(accepted.associations.is_empty());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn accept_draft_with_associations_preserves_them() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-accept-assoc-{}", uuid::Uuid::new_v4()));
        let runs_dir = base_dir.join("runs");
        let fixtures_dir = base_dir.join("docs/Experiments/Fixtures");
        fs::create_dir_all(&fixtures_dir).unwrap();

        let draft_path = base_dir.join("draft-with-assoc.json");
        fs::write(
            &draft_path,
            r#"{
  "records": [
    {
      "schema_version": 1,
      "id": "memory.sleep.source.001",
      "kind": "observation",
      "title": "First.",
      "summary": "First memory.",
      "tags": [],
      "created_at": "2026-05-16T10:00:00Z",
      "importance": 0.5,
      "reinforcement_count": 0,
      "source_reference": "sleep-run:source#memory_candidates[001]",
      "estimated_tokens": 3
    },
    {
      "schema_version": 1,
      "id": "memory.sleep.source.002",
      "kind": "observation",
      "title": "Second.",
      "summary": "Second memory.",
      "tags": [],
      "created_at": "2026-05-16T10:00:00Z",
      "importance": 0.5,
      "reinforcement_count": 0,
      "source_reference": "sleep-run:source#memory_candidates[002]",
      "estimated_tokens": 3
    }
  ],
  "associations": [
    {
      "schema_version": 1,
      "from_memory_id": "memory.sleep.source.001",
      "to_memory_id": "memory.sleep.source.002",
      "weight": 0.64,
      "reason": "Second elaborates first.",
      "last_reinforced_at": "2026-05-16T10:00:00Z"
    }
  ]
}"#,
        )
        .unwrap();

        let target_path = base_dir.join("docs/Experiments/Fixtures/voice-memory.reviewed.json");
        let mut context = RunContext::create_in(&runs_dir, "accept-reviewed-memory").unwrap();
        let experiment = AcceptReviewedMemoryExperiment;
        let outcome = experiment
            .run_with_draft_path(&mut context, &draft_path, &target_path)
            .unwrap();

        assert!(outcome.summary.contains("2 reviewed memory records"));
        assert!(outcome.summary.contains("1 association"));

        let accepted: MemoryFixture =
            serde_json::from_str(&fs::read_to_string(&target_path).unwrap()).unwrap();
        assert_eq!(accepted.records.len(), 2);
        assert_eq!(accepted.associations.len(), 1);
        assert_eq!(
            accepted.associations[0].from_memory_id,
            "memory.sleep.source.001"
        );
        assert_eq!(
            accepted.associations[0].to_memory_id,
            "memory.sleep.source.002"
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn rejects_malformed_draft_json_before_writing() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-accept-malformed-{}", uuid::Uuid::new_v4()));
        let runs_dir = base_dir.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let draft_path = base_dir.join("malformed.json");
        fs::write(&draft_path, "not json").unwrap();

        let target_path = base_dir.join("docs/Experiments/Fixtures/voice-memory.reviewed.json");
        let mut context = RunContext::create_in(&runs_dir, "accept-reviewed-memory").unwrap();
        let experiment = AcceptReviewedMemoryExperiment;
        let result = experiment.run_with_draft_path(&mut context, &draft_path, &target_path);

        assert!(result.is_err());
        assert!(!target_path.exists());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn rejects_wrong_schema_version_before_writing() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-accept-schema-{}", uuid::Uuid::new_v4()));
        let runs_dir = base_dir.join("runs");
        let fixtures_dir = base_dir.join("docs/Experiments/Fixtures");
        fs::create_dir_all(&fixtures_dir).unwrap();

        let draft_path = base_dir.join("wrong-schema.json");
        fs::write(
            &draft_path,
            r#"{
  "records": [
    {
      "schema_version": 999,
      "id": "memory.test.001",
      "kind": "observation",
      "title": "Wrong schema.",
      "summary": "This has the wrong schema version.",
      "tags": [],
      "created_at": "2026-05-16T10:00:00Z",
      "importance": 0.5,
      "reinforcement_count": 0,
      "source_reference": "test",
      "estimated_tokens": 5
    }
  ],
  "associations": []
}"#,
        )
        .unwrap();

        let target_path = base_dir.join("docs/Experiments/Fixtures/voice-memory.reviewed.json");
        let mut context = RunContext::create_in(&runs_dir, "accept-reviewed-memory").unwrap();
        let experiment = AcceptReviewedMemoryExperiment;
        let result = experiment.run_with_draft_path(&mut context, &draft_path, &target_path);

        assert!(result.is_err());
        assert!(!target_path.exists());

        fs::remove_dir_all(base_dir).unwrap();
    }
}
