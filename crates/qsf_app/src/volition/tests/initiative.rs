use super::super::{
    AllowedEffect, GoalStatus, InitiativeOutput, InitiativeProposal, VolitionEvent, VolitionState,
    apply, execute_initiative, static_fixture,
};

#[test]
fn execute_initiative_reflect_returns_reflection_requested() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "clarify-weak-evidence-topic")
        .unwrap();
    let initiative = InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect: AllowedEffect::Reflect,
        rationale: "test".to_string(),
        matched_terms: vec!["memory".to_string()],
        scope: goal.scope,
    };
    let output = execute_initiative(&initiative, goal);
    assert!(
        matches!(output, InitiativeOutput::ReflectionRequested { .. }),
        "Reflect effect must produce ReflectionRequested"
    );
}

#[test]
fn execute_initiative_retrieve_context_returns_context_retrieval_requested() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "resurface-open-thread")
        .unwrap();
    let initiative = InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect: AllowedEffect::RetrieveContext,
        rationale: "test".to_string(),
        matched_terms: vec!["continuity".to_string(), "thread".to_string()],
        scope: goal.scope,
    };
    let output = execute_initiative(&initiative, goal);
    match output {
        InitiativeOutput::ContextRetrievalRequested { query_terms } => {
            assert_eq!(
                query_terms,
                vec!["continuity".to_string(), "thread".to_string()]
            );
        }
        other => panic!("expected ContextRetrievalRequested, got: {other:?}"),
    }
}

#[test]
fn execute_initiative_propose_experiment_returns_experiment_proposed() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "propose-followup-experiment")
        .unwrap();
    let initiative = InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect: AllowedEffect::ProposeExperiment,
        rationale: "test".to_string(),
        matched_terms: vec!["experiment".to_string()],
        scope: goal.scope,
    };
    let output = execute_initiative(&initiative, goal);
    assert!(
        matches!(output, InitiativeOutput::ExperimentProposed { .. }),
        "ProposeExperiment effect must produce ExperimentProposed"
    );
}

#[test]
fn execute_initiative_surface_thread_returns_open_thread_surfaced() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "resurface-open-thread")
        .unwrap();
    let initiative = InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect: AllowedEffect::SurfaceOpenThread,
        rationale: "test".to_string(),
        matched_terms: vec!["thread".to_string()],
        scope: goal.scope,
    };
    let output = execute_initiative(&initiative, goal);
    assert!(
        matches!(output, InitiativeOutput::OpenThreadSurfaced { .. }),
        "SurfaceOpenThread effect must produce OpenThreadSurfaced"
    );
}

#[test]
fn execute_initiative_is_deterministic() {
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "clarify-weak-evidence-topic")
        .unwrap();
    let initiative = InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect: AllowedEffect::Reflect,
        rationale: "test".to_string(),
        matched_terms: vec!["memory".to_string()],
        scope: goal.scope,
    };
    assert_eq!(
        execute_initiative(&initiative, goal),
        execute_initiative(&initiative, goal)
    );
}

#[test]
fn initiative_executed_sets_goal_active_and_records_tick() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";
    let output = InitiativeOutput::ReflectionRequested {
        proposed_question: "What is unclear about voice memory?".to_string(),
    };

    let state = apply(
        state,
        VolitionEvent::InitiativeExecuted {
            goal_id: goal_id.to_string(),
            effect: AllowedEffect::Reflect,
            output,
            rationale: "test rationale".to_string(),
            tick: 3,
        },
    );

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.status, GoalStatus::Active);
    assert_eq!(dynamic.last_activated_tick, Some(3));
    assert!(dynamic.last_initiative_output.is_some());
}

#[test]
fn initiative_executed_stores_output_in_dynamic_state() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";
    let expected_output = InitiativeOutput::ReflectionRequested {
        proposed_question: "What is unclear about voice memory?".to_string(),
    };

    let state = apply(
        state,
        VolitionEvent::InitiativeExecuted {
            goal_id: goal_id.to_string(),
            effect: AllowedEffect::Reflect,
            output: expected_output.clone(),
            rationale: "test".to_string(),
            tick: 1,
        },
    );

    assert_eq!(
        state.goal(goal_id).unwrap().last_initiative_output.as_ref(),
        Some(&expected_output)
    );
}

#[test]
fn initiative_executed_unknown_goal_id_is_noop_on_goals() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goals_before = state.goals.clone();
    let output = InitiativeOutput::ReflectionRequested {
        proposed_question: "Unused".to_string(),
    };

    let state_after = apply(
        state,
        VolitionEvent::InitiativeExecuted {
            goal_id: "nonexistent-goal".to_string(),
            effect: AllowedEffect::Reflect,
            output,
            rationale: "test".to_string(),
            tick: 1,
        },
    );

    assert_eq!(state_after.goals, goals_before);
}
