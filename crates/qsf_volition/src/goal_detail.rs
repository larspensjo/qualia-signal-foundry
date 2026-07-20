use serde::{Deserialize, Serialize};

use crate::{
    ActivationKeyword, AllowedEffect, BiasOutcome, Goal, GoalDynamicState, GoalScope, GoalStatus,
    GoalVisibility, Mode, VolitionFixture, VolitionState, effective_tension_for_goal,
    mode_biased_tier_for_goal,
};

/// Full, pure inspection view over the goals represented by a volition snapshot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDetailReport {
    pub mode: Mode,
    pub goals: Vec<GoalDetail>,
}

/// Where the definition shown for a goal originated.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalDefinitionProvenance {
    ReconstructedFromFixture,
    RecordedInSnapshot,
    DefinitionUnavailable,
}

/// The immutable definition fields shown by the goal-detail view.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDefinitionDetail {
    pub title: String,
    pub summary: String,
    pub scope: GoalScope,
    pub base_priority: u8,
    pub tension_ids: Vec<String>,
    pub activation_keywords: Vec<ActivationKeyword>,
    pub allowed_effects: Vec<AllowedEffect>,
    pub satisfaction_condition_summary: String,
    pub evidence_refs: Vec<String>,
    pub source_reference: String,
    pub visibility: GoalVisibility,
}

/// The mutable lifecycle fields represented in a snapshot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDynamicDetail {
    pub status: GoalStatus,
    pub salience: i32,
    pub cooldown_until_tick: Option<u64>,
    pub last_activated_tick: Option<u64>,
    pub admitted_tick: Option<u64>,
}

/// A parent tension shown inline with a goal definition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalTensionDetail {
    pub id: String,
    pub title: String,
    pub arbitration_tier: u8,
}

/// One full goal-detail record. A missing definition or dynamic record is intentionally
/// preserved as a drift diagnostic instead of silently dropping the goal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDetail {
    pub id: String,
    pub definition: Option<GoalDefinitionDetail>,
    pub dynamic: Option<GoalDynamicDetail>,
    pub parent_tensions: Vec<GoalTensionDetail>,
    pub effective_tension_id: String,
    pub effective_tension_title: String,
    pub effective_tier: u8,
    pub mode_bias: BiasOutcome,
    pub provenance: GoalDefinitionProvenance,
    pub missing_tension_assignment: bool,
    /// True for an accepted-candidate definition without a corresponding lifecycle record.
    pub missing_dynamic_state: bool,
}

/// Builds a deterministic full-detail inspection report from already-loaded state and fixture.
/// No fixture lookup, snapshot loading, or rendering happens here.
pub fn build_goal_detail_report(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> GoalDetailReport {
    let mut goals: Vec<GoalDetail> = state
        .goals
        .iter()
        .map(|(id, dynamic)| {
            let (goal, provenance) = definition_for_goal(id, state, fixture);
            goal_detail(id, goal, Some(dynamic), provenance, fixture, state.mode)
        })
        .collect();

    for (id, goal) in &state.accepted_candidates {
        if !state.goals.contains_key(id) {
            goals.push(goal_detail(
                id,
                Some(goal),
                None,
                GoalDefinitionProvenance::RecordedInSnapshot,
                fixture,
                state.mode,
            ));
        }
    }

    goals.sort_by(|left, right| {
        status_group(left.dynamic.as_ref().map(|dynamic| dynamic.status))
            .cmp(&status_group(
                right.dynamic.as_ref().map(|dynamic| dynamic.status),
            ))
            .then(left.id.cmp(&right.id))
    });

    GoalDetailReport {
        mode: state.mode,
        goals,
    }
}

fn definition_for_goal<'a>(
    id: &str,
    state: &'a VolitionState,
    fixture: &'a VolitionFixture,
) -> (Option<&'a Goal>, GoalDefinitionProvenance) {
    if let Some(goal) = fixture.goals.iter().find(|goal| goal.id == id) {
        (
            Some(goal),
            GoalDefinitionProvenance::ReconstructedFromFixture,
        )
    } else if let Some(goal) = state.accepted_candidates.get(id) {
        (Some(goal), GoalDefinitionProvenance::RecordedInSnapshot)
    } else {
        (None, GoalDefinitionProvenance::DefinitionUnavailable)
    }
}

fn goal_detail(
    id: &str,
    goal: Option<&Goal>,
    dynamic: Option<&GoalDynamicState>,
    provenance: GoalDefinitionProvenance,
    fixture: &VolitionFixture,
    mode: Mode,
) -> GoalDetail {
    let (effective_tier, effective_tension_id, effective_tension_title, mode_bias) =
        if let Some(goal) = goal {
            let (tier, tension_id, tension_title) = effective_tension_for_goal(goal, fixture);
            let bias = mode_biased_tier_for_goal(goal, fixture, mode);
            (tier, tension_id, tension_title, bias)
        } else {
            (
                u8::MAX,
                String::new(),
                String::new(),
                BiasOutcome {
                    effective_tier: u8::MAX,
                    bias_applied: 0,
                    biased_tier: u8::MAX,
                    protected: false,
                },
            )
        };

    GoalDetail {
        id: id.to_string(),
        definition: goal.map(definition_detail),
        dynamic: dynamic.map(dynamic_detail),
        parent_tensions: goal.map_or_else(Vec::new, |goal| parent_tensions(goal, fixture)),
        effective_tension_id,
        effective_tension_title,
        effective_tier,
        mode_bias,
        provenance,
        missing_tension_assignment: effective_tier == u8::MAX,
        missing_dynamic_state: dynamic.is_none(),
    }
}

fn definition_detail(goal: &Goal) -> GoalDefinitionDetail {
    GoalDefinitionDetail {
        title: goal.title.clone(),
        summary: goal.summary.clone(),
        scope: goal.scope,
        base_priority: goal.base_priority,
        tension_ids: goal.tension_ids.clone(),
        activation_keywords: goal.activation_keywords.clone(),
        allowed_effects: goal.allowed_effects.clone(),
        satisfaction_condition_summary: goal.satisfaction_condition_summary.clone(),
        evidence_refs: goal.evidence_refs.clone(),
        source_reference: goal.source_reference.clone(),
        visibility: goal.visibility,
    }
}

fn dynamic_detail(dynamic: &GoalDynamicState) -> GoalDynamicDetail {
    GoalDynamicDetail {
        status: dynamic.status,
        salience: dynamic.salience,
        cooldown_until_tick: dynamic.cooldown_until_tick,
        last_activated_tick: dynamic.last_activated_tick,
        admitted_tick: dynamic.admitted_tick,
    }
}

fn parent_tensions(goal: &Goal, fixture: &VolitionFixture) -> Vec<GoalTensionDetail> {
    goal.tension_ids
        .iter()
        .filter_map(|id| fixture.tensions.iter().find(|tension| tension.id == *id))
        .map(|tension| GoalTensionDetail {
            id: tension.id.clone(),
            title: tension.title.clone(),
            arbitration_tier: tension.arbitration_tier,
        })
        .collect()
}

fn status_group(status: Option<GoalStatus>) -> u8 {
    match status {
        Some(GoalStatus::Active) => 0,
        Some(GoalStatus::Accepted) => 1,
        Some(GoalStatus::Blocked) => 2,
        Some(GoalStatus::Cooldown) => 3,
        Some(GoalStatus::Retired) => 4,
        Some(GoalStatus::Proposed) | Some(GoalStatus::Satisfied) => 5,
        None => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, EvidenceRef, GoalScope, GoalStatus, GoalVisibility, ProposedGoalCandidate,
        VolitionEvent, apply, realtime_seed_fixture,
    };

    fn accepted_candidate(state: VolitionState, id: &str) -> VolitionState {
        let candidate = ProposedGoalCandidate::try_new(
            id.to_string(),
            "Live formed goal".to_string(),
            "A persisted live-formed definition.".to_string(),
            vec!["person-curiosity".to_string()],
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied after inspection.".to_string(),
            vec![EvidenceRef::try_new("test evidence").unwrap()],
            "test source".to_string(),
            vec!["live".to_string()],
        )
        .unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: id.to_string(),
                acceptance_evidence: EvidenceRef::try_new("accepted").unwrap(),
                tick: 2,
            },
        )
    }

    #[test]
    fn fixture_goal_has_full_definition_and_reconstructed_provenance() {
        let fixture = realtime_seed_fixture();
        let report = build_goal_detail_report(&VolitionState::from_fixture(&fixture), &fixture);
        let goal = report
            .goals
            .iter()
            .find(|goal| goal.id == "track-the-ai-transition")
            .unwrap();

        assert_eq!(
            goal.provenance,
            GoalDefinitionProvenance::ReconstructedFromFixture
        );
        assert_eq!(
            goal.definition.as_ref().unwrap().title,
            "Track the AI transition"
        );
        assert!(
            !goal
                .definition
                .as_ref()
                .unwrap()
                .activation_keywords
                .is_empty()
        );
    }

    #[test]
    fn live_formed_goal_has_recorded_definition_and_retired_goal_keeps_it() {
        let fixture = realtime_seed_fixture();
        let state = accepted_candidate(VolitionState::from_fixture(&fixture), "live-goal");
        let state = apply(
            state,
            VolitionEvent::GoalRetired {
                goal_id: "live-goal".to_string(),
                tick: 3,
            },
        );
        let report = build_goal_detail_report(&state, &fixture);
        let goal = report
            .goals
            .iter()
            .find(|goal| goal.id == "live-goal")
            .unwrap();

        assert_eq!(
            goal.provenance,
            GoalDefinitionProvenance::RecordedInSnapshot
        );
        assert_eq!(goal.definition.as_ref().unwrap().title, "Live formed goal");
        assert_eq!(goal.dynamic.as_ref().unwrap().status, GoalStatus::Retired);
    }

    #[test]
    fn accepted_candidate_without_dynamic_state_remains_in_report() {
        let fixture = realtime_seed_fixture();
        let mut state = accepted_candidate(VolitionState::from_fixture(&fixture), "live-goal");
        assert!(state.goals.remove("live-goal").is_some());

        let report = build_goal_detail_report(&state, &fixture);
        let goal = report
            .goals
            .iter()
            .find(|goal| goal.id == "live-goal")
            .unwrap();

        assert!(goal.missing_dynamic_state);
        assert!(goal.dynamic.is_none());
        assert_eq!(
            goal.provenance,
            GoalDefinitionProvenance::RecordedInSnapshot
        );
        assert_eq!(
            report.goals.last().map(|goal| goal.id.as_str()),
            Some("live-goal")
        );
    }

    #[test]
    fn subconscious_seed_goal_is_present_unfiltered() {
        let fixture = realtime_seed_fixture();
        let report = build_goal_detail_report(&VolitionState::from_fixture(&fixture), &fixture);
        let goal = report
            .goals
            .iter()
            .find(|goal| goal.id == "assemble-world-picture")
            .unwrap();

        assert_eq!(
            goal.definition.as_ref().unwrap().visibility,
            GoalVisibility::Subconscious
        );
    }

    #[test]
    fn effective_tension_matches_shared_helper() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let expected_goal = fixture
            .goals
            .iter()
            .find(|goal| goal.id == "track-the-ai-transition")
            .unwrap();
        let expected = effective_tension_for_goal(expected_goal, &fixture);
        let report = build_goal_detail_report(&state, &fixture);
        let detail = report
            .goals
            .iter()
            .find(|goal| goal.id == expected_goal.id)
            .unwrap();

        assert_eq!(detail.effective_tier, expected.0);
        assert_eq!(detail.effective_tension_id, expected.1);
        assert_eq!(detail.effective_tension_title, expected.2);
    }

    #[test]
    fn no_parent_tension_is_flagged() {
        let fixture = realtime_seed_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        state
            .goals
            .insert("drift-goal".to_string(), GoalDynamicState::initial());
        let report = build_goal_detail_report(&state, &fixture);
        let goal = report
            .goals
            .iter()
            .find(|goal| goal.id == "drift-goal")
            .unwrap();

        assert_eq!(goal.effective_tier, u8::MAX);
        assert!(goal.missing_tension_assignment);
        assert_eq!(
            goal.provenance,
            GoalDefinitionProvenance::DefinitionUnavailable
        );
    }

    #[test]
    fn mode_bias_is_exposed_for_band_and_protected_goals() {
        let fixture = realtime_seed_fixture();
        let state = apply(
            VolitionState::from_fixture(&fixture),
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 1,
            },
        );
        let report = build_goal_detail_report(&state, &fixture);
        let band = report
            .goals
            .iter()
            .find(|goal| goal.id == "assemble-world-picture")
            .unwrap();
        let protected = report
            .goals
            .iter()
            .find(|goal| goal.id == "respect-persons-boundaries")
            .unwrap();

        assert_eq!(report.mode, Mode::Exploratory);
        assert_ne!(band.mode_bias.biased_tier, band.effective_tier);
        assert_eq!(protected.mode_bias.biased_tier, protected.effective_tier);
        assert!(protected.mode_bias.protected);
    }
}
