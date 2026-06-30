use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::session::resolve_shared_state_directory_from_env;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const REVIEWED_VOLITION_DRAFT_ENV_VAR: &str = "QSF_REVIEWED_VOLITION_DRAFT";
const REVIEWED_VOLITION_TARGET_ENV_VAR: &str = "QSF_REVIEWED_VOLITION_TARGET";
const CONTINUITY_SESSION_ID_ENV_VAR: &str = "QSF_VOLITION_CONTINUITY_SESSION_ID";

pub struct AcceptReviewedVolitionSeedExperiment;

impl Experiment for AcceptReviewedVolitionSeedExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::AcceptReviewedVolitionSeed
    }

    fn description(&self) -> &'static str {
        "Accept a reviewed volition seed draft and write it as the durable reviewed seed artifact"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let draft_path = draft_path_from_env()?;
        let target_path = target_path_from_env()?;
        self.run_with_paths(context, &draft_path, &target_path)
    }
}

impl AcceptReviewedVolitionSeedExperiment {
    fn run_with_paths(
        &self,
        context: &mut RunContext,
        draft_path: &Path,
        target_path: &Path,
    ) -> anyhow::Result<ExperimentOutcome> {
        let reviewed_seed = load_reviewed_seed_draft(draft_path)?;

        context.record_event(
            EventType::InputReceived,
            json!({
                "source_draft_path": draft_path.display().to_string(),
                "target_path": target_path.display().to_string(),
                "accepted_goal_count": reviewed_seed.accepted_goals.len(),
                "source_env_var": REVIEWED_VOLITION_DRAFT_ENV_VAR,
            }),
            None,
        )?;

        reviewed_seed.persist(target_path)?;

        let trace = TraceRecord::new(
            context.experiment_id(),
            "accept-reviewed-volition-seed",
            format!(
                "source_draft={} target={}",
                draft_path.display(),
                target_path.display()
            ),
            format!(
                "accepted {} reviewed volition goal{} into durable continuity seed",
                reviewed_seed.accepted_goals.len(),
                if reviewed_seed.accepted_goals.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
        )
        .with_details(json!({
            "source_draft_path": draft_path.display().to_string(),
            "target_path": target_path.display().to_string(),
            "accepted_goal_count": reviewed_seed.accepted_goals.len(),
            "promoted_by": reviewed_seed.promoted_by.clone(),
            "promoted_at": reviewed_seed.promoted_at.clone(),
            "source_artifact_reference": reviewed_seed.source_artifact_reference.clone(),
            "status": "accepted",
        }));
        let trace_id = trace.trace_id;
        context.record_trace(trace)?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "target_path": target_path.display().to_string(),
                "accepted_goal_count": reviewed_seed.accepted_goals.len(),
                "status": "accepted",
            }),
            Some(trace_id),
        )?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Accepted {} reviewed volition goal{} and wrote the durable reviewed seed artifact `{}`.",
                reviewed_seed.accepted_goals.len(),
                if reviewed_seed.accepted_goals.len() == 1 {
                    ""
                } else {
                    "s"
                },
                target_path.display()
            ),
            observations: vec![
                "The reviewed seed is explicit and human-run; the realtime server only consumes it when creating a session.".to_string(),
                "Protected fixture goals remain fixture-owned and cannot be overwritten by the reviewed seed.".to_string(),
            ],
            failure_modes: vec![
                format!(
                    "`{REVIEWED_VOLITION_DRAFT_ENV_VAR}` must point at a readable reviewed-volition-seed draft."
                ),
                "Malformed or schema-incompatible draft JSON fails before any file is written.".to_string(),
            ],
            follow_up_questions: vec![
                "Should reviewed volition seeds eventually include an optional reviewer note field?".to_string(),
            ],
            decision_candidates: vec![
                "Keep reviewed volition acceptance explicit and separate from sleep summarization.".to_string(),
            ],
            extra_artifacts: vec![target_path.display().to_string()],
        })
    }
}

fn draft_path_from_env() -> anyhow::Result<PathBuf> {
    std::env::var(REVIEWED_VOLITION_DRAFT_ENV_VAR)
        .map(PathBuf::from)
        .map_err(|_| {
            anyhow!(
                "`{}` must point to a reviewed-volition-seed draft JSON file",
                REVIEWED_VOLITION_DRAFT_ENV_VAR
            )
        })
}

fn target_path_from_env() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var(REVIEWED_VOLITION_TARGET_ENV_VAR) {
        return Ok(PathBuf::from(path));
    }

    let state_dir = resolve_shared_state_directory_from_env().persist_state_dir;
    let session_id =
        std::env::var(CONTINUITY_SESSION_ID_ENV_VAR).unwrap_or_else(|_| "default".to_string());
    Ok(state_dir
        .join("continuity")
        .join(session_id)
        .join("volition-seed.reviewed.json"))
}

fn load_reviewed_seed_draft(path: &Path) -> anyhow::Result<qsf_volition::ReviewedVolitionSeed> {
    qsf_volition::load_reviewed_volition_seed(path).with_context(|| {
        format!(
            "failed to load reviewed volition seed draft `{}`",
            path.display()
        )
    })
}
