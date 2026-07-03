use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::arbitration::PROTECTED_TIER_FLOOR;
use crate::{
    Goal, GoalDynamicState, GoalStatus, VolitionFixture, VolitionState, VolitionStateInspection,
};

pub const REALTIME_SEED_FIXTURE_ID: &str = "realtime_seed_fixture";
pub const VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION: u16 = 2;
pub const REVIEWED_VOLITION_SEED_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionContinuitySnapshot {
    pub schema_version: u16,
    pub recorded_at: String,
    pub qsf_session_id: String,
    pub seed_fixture_id: String,
    pub state: VolitionState,
    pub inspection: VolitionStateInspection,
}

impl VolitionContinuitySnapshot {
    pub fn new(
        qsf_session_id: impl Into<String>,
        recorded_at: impl Into<String>,
        seed_fixture_id: impl Into<String>,
        state: VolitionState,
        inspection: VolitionStateInspection,
    ) -> Self {
        Self {
            schema_version: VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION,
            recorded_at: recorded_at.into(),
            qsf_session_id: qsf_session_id.into(),
            seed_fixture_id: seed_fixture_id.into(),
            state,
            inspection,
        }
    }

    pub fn upgrade_schema_version(&mut self) {
        if self.schema_version < VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION {
            self.schema_version = VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION;
        }
    }

    pub fn load_or_upgrade(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read volition snapshot `{}`", path.display()))?;
        let mut parsed: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse volition snapshot `{}`", path.display()))?;
        if parsed.schema_version > VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported volition snapshot schema_version: found {} expected <= {}",
                parsed.schema_version,
                VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION
            );
        }
        parsed.upgrade_schema_version();
        Ok(parsed)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewedVolitionSeed {
    pub schema_version: u16,
    pub promoted_by: String,
    pub promoted_at: String,
    pub source_artifact_reference: String,
    #[serde(default)]
    pub accepted_goals: BTreeMap<String, Goal>,
}

impl ReviewedVolitionSeed {
    pub fn load_or_upgrade(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read reviewed volition seed `{}`", path.display())
        })?;
        let parsed: Self = serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse reviewed volition seed `{}`",
                path.display()
            )
        })?;
        if parsed.schema_version != REVIEWED_VOLITION_SEED_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported reviewed volition seed schema_version: found {} expected {}",
                parsed.schema_version,
                REVIEWED_VOLITION_SEED_SCHEMA_VERSION
            );
        }
        Ok(parsed)
    }

    pub fn persist(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        persist_json(path, self).map(|_| ())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnOutcome {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    pub recorded_at: String,
    pub response_create_event_ref: String,
    pub winning_goal_id: String,
    pub initiative_output: crate::InitiativeOutput,
    pub surfaced: bool,
    #[serde(default)]
    pub suppression_reason: Option<VolitionSuppressionReason>,
    pub rendered_line_present: bool,
    pub artifact_reference: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VolitionSuppressionReason {
    Intensity,
    ProtectedNoOpportunity,
    AntiNagRepeat,
    NonRenderableOutput,
}

pub fn persist_volition_continuity_snapshot(
    snapshot: &VolitionContinuitySnapshot,
    path: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    persist_json(path, snapshot)
}

pub fn load_reviewed_volition_seed(path: impl AsRef<Path>) -> anyhow::Result<ReviewedVolitionSeed> {
    ReviewedVolitionSeed::load_or_upgrade(path)
}

pub fn apply_reviewed_seed(
    fixture: &VolitionFixture,
    reviewed_seed: &ReviewedVolitionSeed,
) -> anyhow::Result<VolitionState> {
    let mut state = VolitionState::from_fixture(fixture);
    apply_reviewed_seed_in_place(&mut state, fixture, reviewed_seed)?;
    Ok(state)
}

/// Merge reviewed seed goals into an existing `VolitionState` without resetting tick or
/// existing goal dynamics. Applies the same validation rules as `apply_reviewed_seed`.
pub fn apply_reviewed_seed_in_place(
    state: &mut VolitionState,
    fixture: &VolitionFixture,
    reviewed_seed: &ReviewedVolitionSeed,
) -> anyhow::Result<()> {
    let mut seen_ids = BTreeSet::new();

    for (goal_id, goal) in &reviewed_seed.accepted_goals {
        if !seen_ids.insert(goal_id.clone()) {
            anyhow::bail!("reviewed goal id `{goal_id}` is duplicated");
        }
        if fixture
            .goals
            .iter()
            .any(|fixture_goal| fixture_goal.id == *goal_id)
        {
            anyhow::bail!("reviewed goal id `{goal_id}` would overwrite a fixture goal");
        }
        if goal.id != *goal_id {
            anyhow::bail!(
                "reviewed goal map key `{goal_id}` does not match goal id `{}`",
                goal.id
            );
        }
        if goal.status != GoalStatus::Accepted {
            anyhow::bail!(
                "reviewed goal `{goal_id}` must be Accepted, found {}",
                goal.status
            );
        }

        let tier = goal_effective_tier(goal, fixture);
        if tier <= PROTECTED_TIER_FLOOR {
            anyhow::bail!("reviewed goal `{goal_id}` cannot enter at protected tier {tier}");
        }

        state
            .accepted_candidates
            .insert(goal_id.clone(), goal.clone());
        state
            .goals
            .insert(goal_id.clone(), GoalDynamicState::initial());
    }

    Ok(())
}

fn goal_effective_tier(goal: &Goal, fixture: &VolitionFixture) -> u8 {
    goal.tension_ids
        .iter()
        .filter_map(|tid| fixture.tensions.iter().find(|t| t.id == *tid))
        .map(|t| t.arbitration_tier)
        .min()
        .unwrap_or(u8::MAX)
}

fn persist_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> anyhow::Result<PathBuf> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent dir `{}`", parent.display()))?;
    let mut temp = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create temporary JSON file in `{}`",
            parent.display()
        )
    })?;
    temp.as_file_mut()
        .write_all(serde_json::to_string_pretty(value)?.as_bytes())
        .with_context(|| format!("failed to write temporary JSON `{}`", temp.path().display()))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("failed to sync temporary JSON `{}`", temp.path().display()))?;
    temp.persist(path).map_err(|error| {
        anyhow::anyhow!("failed to persist `{}`: {}", path.display(), error.error)
    })?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, EvidenceRef, Goal, GoalScope, GoalStatus, VolitionFixture, VolitionState,
        build_state_inspection, realtime_seed_fixture,
    };
    use tempfile::TempDir;

    fn sample_snapshot_fixture() -> VolitionFixture {
        realtime_seed_fixture()
    }

    fn sample_snapshot() -> VolitionContinuitySnapshot {
        let fixture = sample_snapshot_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        VolitionContinuitySnapshot::new(
            "session-1",
            "2026-06-30T12:00:00Z",
            REALTIME_SEED_FIXTURE_ID,
            state,
            inspection,
        )
    }

    #[test]
    fn snapshot_roundtrips_and_upgrades_legacy_schema() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("volition-state.json");
        let original = sample_snapshot();
        persist_volition_continuity_snapshot(&original, &path).unwrap();

        let reloaded = VolitionContinuitySnapshot::load_or_upgrade(&path).unwrap();
        assert_eq!(reloaded, original);

        let legacy = serde_json::json!({
            "schema_version": 1,
            "recorded_at": original.recorded_at,
            "qsf_session_id": original.qsf_session_id,
            "seed_fixture_id": original.seed_fixture_id,
            "state": original.state,
            "inspection": original.inspection,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();
        let upgraded = VolitionContinuitySnapshot::load_or_upgrade(&path).unwrap();
        assert_eq!(
            upgraded.schema_version,
            VOLITION_CONTINUITY_SNAPSHOT_SCHEMA_VERSION
        );
    }

    #[test]
    fn snapshot_serializes_to_identical_bytes_twice() {
        let snapshot = sample_snapshot();
        let first = serde_json::to_vec_pretty(&snapshot).unwrap();
        let second = serde_json::to_vec_pretty(&snapshot).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn reviewed_seed_merge_preserves_fixture_protected_goals() {
        let fixture = realtime_seed_fixture();
        let reviewed_goal = Goal {
            id: "reviewed-continuity-check".to_string(),
            title: "Reviewed continuity check".to_string(),
            summary: "Carry a reviewed continuity candidate forward.".to_string(),
            tension_ids: vec!["coherence-maintenance".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority: 77,
            activation_keywords: vec!["continuity".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "Test review seed".to_string(),
            evidence_refs: vec!["tests".to_string()],
            estimated_tokens: 12,
            source_reference: "tests".to_string(),
        };
        let reviewed_seed = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "continuity/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([(reviewed_goal.id.clone(), reviewed_goal.clone())]),
        };

        let merged = apply_reviewed_seed(&fixture, &reviewed_seed).unwrap();

        assert!(merged.goals.contains_key("serve-the-present-person"));
        assert!(merged.goals.contains_key("keep-theses-distinct-from-fact"));
        assert!(merged.accepted_candidates.contains_key(&reviewed_goal.id));
        assert_eq!(
            merged.accepted_candidates.get(&reviewed_goal.id).unwrap(),
            &reviewed_goal
        );
    }

    #[test]
    fn reviewed_seed_rejects_fixture_id_overwrite_and_protected_tier_additions() {
        let fixture = realtime_seed_fixture();
        let protected_goal = fixture
            .goals
            .iter()
            .find(|goal| goal.id == "serve-the-present-person")
            .unwrap()
            .clone();
        let illegal_goal = Goal {
            id: "protected-reviewed".to_string(),
            title: "Protected reviewed".to_string(),
            summary: "Should be rejected".to_string(),
            tension_ids: vec!["present-person-priority".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Input,
            base_priority: 99,
            activation_keywords: vec!["protected".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "Should fail".to_string(),
            evidence_refs: vec!["tests".to_string()],
            estimated_tokens: 10,
            source_reference: "tests".to_string(),
        };
        let overwrite_seed = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "continuity/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([(protected_goal.id.clone(), protected_goal)]),
        };
        let protected_seed = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "continuity/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([(illegal_goal.id.clone(), illegal_goal)]),
        };

        assert!(apply_reviewed_seed(&fixture, &overwrite_seed).is_err());
        assert!(apply_reviewed_seed(&fixture, &protected_seed).is_err());
    }

    #[test]
    fn reviewed_seed_merge_is_order_independent_and_keeps_protected_goals_dominant() {
        let fixture = realtime_seed_fixture();
        let first = Goal {
            id: "reviewed-alpha".to_string(),
            title: "Reviewed alpha".to_string(),
            summary: "Alpha".to_string(),
            tension_ids: vec!["coherence-maintenance".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority: 70,
            activation_keywords: vec!["alpha".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "Alpha done".to_string(),
            evidence_refs: vec!["tests".to_string()],
            estimated_tokens: 8,
            source_reference: "tests".to_string(),
        };
        let second = Goal {
            id: "reviewed-beta".to_string(),
            title: "Reviewed beta".to_string(),
            summary: "Beta".to_string(),
            tension_ids: vec!["coherence-maintenance".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority: 69,
            activation_keywords: vec!["beta".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "Beta done".to_string(),
            evidence_refs: vec!["tests".to_string()],
            estimated_tokens: 8,
            source_reference: "tests".to_string(),
        };
        let seed_a = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "continuity/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([
                (first.id.clone(), first.clone()),
                (second.id.clone(), second.clone()),
            ]),
        };
        let seed_b = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "continuity/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([
                (second.id.clone(), second),
                (first.id.clone(), first),
            ]),
        };

        let merged_a = apply_reviewed_seed(&fixture, &seed_a).unwrap();
        let merged_b = apply_reviewed_seed(&fixture, &seed_b).unwrap();

        assert_eq!(merged_a, merged_b);

        for mode in [
            crate::Mode::Neutral,
            crate::Mode::Focused,
            crate::Mode::Exploratory,
        ] {
            let result = crate::arbitrate_with_mode(
                vec![
                    crate::GoalSelection {
                        goal: merged_a
                            .accepted_candidates
                            .get("reviewed-alpha")
                            .unwrap()
                            .clone(),
                        relevance_score: 10.0,
                        matched_terms: vec!["alpha".to_string()],
                        initiative: crate::InitiativeProposal {
                            goal_id: "reviewed-alpha".to_string(),
                            goal_title: "Reviewed alpha".to_string(),
                            effect: AllowedEffect::Reflect,
                            rationale: "test".to_string(),
                            matched_terms: vec!["alpha".to_string()],
                            scope: GoalScope::Session,
                        },
                    },
                    crate::GoalSelection {
                        goal: fixture
                            .goals
                            .iter()
                            .find(|goal| goal.id == "serve-the-present-person")
                            .unwrap()
                            .clone(),
                        relevance_score: 10.0,
                        matched_terms: vec!["help".to_string()],
                        initiative: crate::InitiativeProposal {
                            goal_id: "serve-the-present-person".to_string(),
                            goal_title: "Serve the present person".to_string(),
                            effect: AllowedEffect::Reflect,
                            rationale: "test".to_string(),
                            matched_terms: vec!["help".to_string()],
                            scope: GoalScope::Input,
                        },
                    },
                ],
                &fixture,
                mode,
            )
            .unwrap();
            assert_eq!(result.winner.goal.id, "serve-the-present-person");
        }
    }

    #[test]
    fn review_seed_loader_and_persist_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("volition-seed.reviewed.json");
        let seed = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "tests/seed-review.json".to_string(),
            accepted_goals: BTreeMap::new(),
        };

        seed.persist(&path).unwrap();
        let reloaded = ReviewedVolitionSeed::load_or_upgrade(&path).unwrap();
        assert_eq!(reloaded, seed);
    }

    #[test]
    fn review_seed_rejects_candidate_with_low_tier_and_loader_accepts_seed_path() {
        let fixture = realtime_seed_fixture();
        let reviewed_goal = Goal {
            id: "reviewed-weak".to_string(),
            title: "Reviewed weak".to_string(),
            summary: "Rejected tier".to_string(),
            tension_ids: vec!["present-person-priority".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Input,
            base_priority: 90,
            activation_keywords: vec!["weak".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "Should fail".to_string(),
            evidence_refs: vec![EvidenceRef::try_new("tests").unwrap().to_string()],
            estimated_tokens: 9,
            source_reference: "tests".to_string(),
        };
        let reviewed_seed = ReviewedVolitionSeed {
            schema_version: REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "tester".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "tests/seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([(reviewed_goal.id.clone(), reviewed_goal)]),
        };

        assert!(apply_reviewed_seed(&fixture, &reviewed_seed).is_err());
    }
}
