use serde::{Deserialize, Serialize};

use crate::{
    GoalStatus, GoalVisibility, InitiativeOutput, Mode, VolitionFixture, VolitionState,
    goal_visibility,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalStatusSummary {
    pub id: String,
    pub title: String,
    pub salience: i32,
    pub cooldown_until_tick: Option<u64>,
    pub last_activated_tick: Option<u64>,
    /// Narration visibility, resolved from the goal definition. `#[serde(default)]` = `Conscious`
    /// keeps inspection captures serialized before this field parseable. `build_state_inspection`
    /// stays complete and unfiltered — subconscious goals appear here too; the *tool* layer, not
    /// this builder, decides what to section for the simulator (the operator panel keeps all of
    /// it).
    #[serde(default)]
    pub visibility: GoalVisibility,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InitiativeSummary {
    pub goal_id: String,
    pub goal_title: String,
    pub output_kind: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionStateInspection {
    pub mode: Mode,
    pub tick: u64,
    pub active_goals: Vec<GoalStatusSummary>,
    pub accepted_goals: Vec<GoalStatusSummary>,
    pub blocked_goals: Vec<GoalStatusSummary>,
    pub cooldown_goals: Vec<GoalStatusSummary>,
    pub retired_goals: Vec<GoalStatusSummary>,
    pub pending_candidate_count: usize,
    pub accepted_candidate_count: usize,
    pub last_initiative_summaries: Vec<InitiativeSummary>,
}

pub fn build_state_inspection(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> VolitionStateInspection {
    let mut active_goals = Vec::new();
    let mut accepted_goals = Vec::new();
    let mut blocked_goals = Vec::new();
    let mut cooldown_goals = Vec::new();
    let mut retired_goals = Vec::new();
    let mut last_initiative_summaries = Vec::new();

    for (goal_id, dynamic) in &state.goals {
        let title = goal_title(goal_id, state, fixture);
        let summary = GoalStatusSummary {
            id: goal_id.clone(),
            title: title.clone(),
            salience: dynamic.salience,
            cooldown_until_tick: dynamic.cooldown_until_tick,
            last_activated_tick: dynamic.last_activated_tick,
            visibility: goal_visibility(goal_id, state, fixture),
        };

        match dynamic.status {
            GoalStatus::Active => active_goals.push(summary),
            GoalStatus::Accepted => accepted_goals.push(summary),
            GoalStatus::Blocked => blocked_goals.push(summary),
            GoalStatus::Cooldown => cooldown_goals.push(summary),
            GoalStatus::Retired => retired_goals.push(summary),
            GoalStatus::Proposed | GoalStatus::Satisfied => {}
        }

        if let Some(output) = &dynamic.last_initiative_output {
            last_initiative_summaries.push(InitiativeSummary {
                goal_id: goal_id.clone(),
                goal_title: title,
                output_kind: initiative_output_kind(output).to_string(),
            });
        }
    }

    VolitionStateInspection {
        mode: state.mode,
        tick: state.tick,
        active_goals,
        accepted_goals,
        blocked_goals,
        cooldown_goals,
        retired_goals,
        pending_candidate_count: state.pending_candidates.len(),
        accepted_candidate_count: state.accepted_candidates.len(),
        last_initiative_summaries,
    }
}

fn goal_title(goal_id: &str, state: &VolitionState, fixture: &VolitionFixture) -> String {
    fixture
        .goals
        .iter()
        .find(|goal| goal.id == goal_id)
        .map(|goal| goal.title.clone())
        .or_else(|| {
            state
                .accepted_candidates
                .get(goal_id)
                .map(|goal| goal.title.clone())
        })
        .unwrap_or_default()
}

fn initiative_output_kind(output: &InitiativeOutput) -> &'static str {
    match output {
        InitiativeOutput::ReflectionRequested { .. } => "reflection_requested",
        InitiativeOutput::ContextRetrievalRequested { .. } => "context_retrieval_requested",
        InitiativeOutput::WorldConsultationRequested { .. } => "world_consultation_requested",
        InitiativeOutput::ExperimentProposed { .. } => "experiment_proposed",
        InitiativeOutput::OpenThreadSurfaced { .. } => "open_thread_surfaced",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, EvidenceRef, GoalScope, InitiativeOutput, ProposedGoalCandidate,
        VolitionEvent, apply, realtime_seed_fixture,
    };

    fn fresh_state(fixture: &VolitionFixture) -> VolitionState {
        VolitionState::from_fixture(fixture)
    }

    #[test]
    fn build_state_inspection_groups_goals_by_status() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "serve-the-present-person".to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: "serve-the-present-person".to_string(),
                tick: 2,
            },
        );
        let evidence = EvidenceRef::try_new("test").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: "keep-theses-distinct-from-fact".to_string(),
                evidence,
                tick: 3,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalRetired {
                goal_id: "grow-the-library".to_string(),
                tick: 4,
            },
        );

        let inspection = build_state_inspection(&state, &fixture);

        assert_eq!(inspection.mode, state.mode);
        assert_eq!(inspection.tick, 4);
        assert!(
            inspection
                .blocked_goals
                .iter()
                .any(|goal| goal.id == "serve-the-present-person")
        );
        assert!(
            inspection
                .cooldown_goals
                .iter()
                .any(|goal| goal.id == "keep-theses-distinct-from-fact")
        );
        assert!(
            inspection
                .retired_goals
                .iter()
                .any(|goal| goal.id == "grow-the-library")
        );
    }

    #[test]
    fn build_state_inspection_reflects_last_initiative_summary() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What should we focus on?".to_string(),
        };
        let state = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: "serve-the-present-person".to_string(),
                effect: AllowedEffect::Reflect,
                output,
                rationale: "test".to_string(),
                tick: 1,
                rendered_ref: None,
            },
        );

        let inspection = build_state_inspection(&state, &fixture);

        assert!(inspection.last_initiative_summaries.iter().any(|summary| {
            summary.goal_id == "serve-the-present-person"
                && summary.goal_title == "Serve the present person"
                && summary.output_kind == "reflection_requested"
        }));
    }

    #[test]
    fn build_state_inspection_reports_candidate_counts() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);

        let inspection = build_state_inspection(&state, &fixture);

        assert_eq!(inspection.pending_candidate_count, 0);
        assert_eq!(inspection.accepted_candidate_count, 0);
        assert!(inspection.last_initiative_summaries.is_empty());
    }

    #[test]
    fn build_state_inspection_uses_accepted_candidate_title_when_not_in_fixture() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let evidence = EvidenceRef::try_new("test accepted candidate").unwrap();
        let candidate = ProposedGoalCandidate::try_new(
            "accepted-candidate".to_string(),
            "Accepted Candidate".to_string(),
            "Accepted candidate summary".to_string(),
            vec!["research-curiosity".to_string()],
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied when accepted candidate is inspected.".to_string(),
            vec![evidence],
            "test source".to_string(),
            vec!["accepted".to_string()],
        )
        .unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("accepted").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "accepted-candidate".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        let inspection = build_state_inspection(&state, &fixture);

        assert!(
            inspection.accepted_goals.iter().any(|goal| {
                goal.id == "accepted-candidate" && goal.title == "Accepted Candidate"
            })
        );
    }

    #[test]
    fn build_state_inspection_carries_visibility_including_subconscious_seed_goal() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let inspection = build_state_inspection(&state, &fixture);

        // The seed fixture marks `assemble-world-picture` subconscious; build_state_inspection is
        // unfiltered, so the summary is present in its ordinary status list carrying Subconscious.
        let world = inspection
            .accepted_goals
            .iter()
            .find(|g| g.id == "assemble-world-picture")
            .expect("subconscious seed goal must still appear in the unfiltered inspection");
        assert_eq!(world.visibility, GoalVisibility::Subconscious);

        // Every other seed goal reads Conscious.
        for summary in &inspection.accepted_goals {
            if summary.id != "assemble-world-picture" {
                assert_eq!(
                    summary.visibility,
                    GoalVisibility::Conscious,
                    "goal {} should be conscious",
                    summary.id
                );
            }
        }
    }

    #[test]
    fn goal_status_summary_deserializes_without_visibility_as_conscious() {
        let json = serde_json::json!({
            "id": "g1",
            "title": "G1",
            "salience": 0,
            "cooldown_until_tick": null,
            "last_activated_tick": null
        });
        let summary: GoalStatusSummary = serde_json::from_value(json).unwrap();
        assert_eq!(summary.visibility, GoalVisibility::Conscious);
    }

    #[test]
    fn build_state_inspection_goal_titles_are_populated() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);

        let inspection = build_state_inspection(&state, &fixture);

        for summary in inspection
            .accepted_goals
            .iter()
            .chain(inspection.active_goals.iter())
            .chain(inspection.blocked_goals.iter())
            .chain(inspection.cooldown_goals.iter())
            .chain(inspection.retired_goals.iter())
        {
            assert!(
                !summary.title.is_empty(),
                "goal summary title must not be empty for goal {}",
                summary.id
            );
        }
    }
}
