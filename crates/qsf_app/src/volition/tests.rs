use super::{
    DeltaAssessment, EvidenceRef, GoalSelectionResult, VolitionEvent, VolitionState, apply,
    build_pre_initiative_traces, select_goals, static_fixture,
};
use crate::context::ContextBudget;

// ── select_goals_with_salience ───────────────────────────────────────────

#[test]
fn salience_aware_selector_matches_stateless_when_state_is_empty() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let input = "We never settled how voice memory affects continuity.";
    let budget = ContextBudget::new(2, 80);

    let stateless = select_goals(input, &fixture, budget);
    let salience_result = super::select_goals_with_salience(input, &fixture, &state, budget);

    let stateless_ids: Vec<_> = stateless.selected.iter().map(|s| &s.goal.id).collect();
    let salience_ids: Vec<_> = salience_result
        .selected
        .iter()
        .map(|s| &s.goal.id)
        .collect();
    assert_eq!(
        stateless_ids, salience_ids,
        "empty state must not alter selection"
    );
}

#[test]
fn cooldown_goal_is_suppressed_from_selection() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";
    let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: goal_id.to_string(),
            tick: 1,
        },
    );
    let state = apply(
        state,
        VolitionEvent::GoalSatisfied {
            goal_id: goal_id.to_string(),
            evidence,
            tick: 2,
        },
    );

    let input = "We never settled how voice memory affects continuity.";
    let result =
        super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(2, 80));

    assert!(
        result.selected.iter().all(|s| s.goal.id != goal_id),
        "cooldown goal must not appear in selected"
    );
    assert!(
        result
            .suppressed_cooldown
            .iter()
            .any(|s| s.goal.id == goal_id),
        "cooldown goal must appear in suppressed_cooldown"
    );
}

#[test]
fn blocked_goal_stays_visible_but_not_selected() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: goal_id.to_string(),
            tick: 1,
        },
    );
    let state = apply(
        state,
        VolitionEvent::GoalBlocked {
            goal_id: goal_id.to_string(),
            tick: 2,
        },
    );

    let input = "We never settled how voice memory affects continuity.";
    let result =
        super::select_goals_with_salience(input, &fixture, &state, ContextBudget::new(2, 80));

    assert!(
        result.selected.iter().all(|s| s.goal.id != goal_id),
        "blocked goal must not appear in selected"
    );
    assert!(
        result.visible_blocked.iter().any(|s| s.goal.id == goal_id),
        "blocked goal must stay visible in visible_blocked"
    );
    assert!(
        result.visible_blocked.iter().all(|s| !s.reason.is_empty()),
        "blocked goal must carry a reason"
    );
}

#[test]
fn baseline_input_selects_no_goals() {
    let fixture = static_fixture();
    let result = select_goals(
        "Give me the build command.",
        &fixture,
        ContextBudget::new(2, 80),
    );

    assert!(result.selected.is_empty());
    assert!(
        result
            .omitted
            .iter()
            .all(|omitted| omitted.reason == "no activation keywords matched")
    );
}

#[test]
fn selection_is_deterministic_for_the_same_input() {
    let fixture = static_fixture();
    let first = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 80),
    );
    let second = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 80),
    );

    assert_eq!(first, second);
}

#[test]
fn token_budget_limits_selected_goals() {
    let fixture = static_fixture();
    let result = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 40),
    );

    // Under weighted activation, `resurface-open-thread` matches `continuity` (curated
    // Strong = 8), tying the strength `clarify-weak-evidence-topic` gets from two Normal
    // keywords (voice + memory = 8) while carrying a higher base priority and tension bias,
    // so it now ranks first and consumes the token budget. Deliberate curation consequence.
    assert_eq!(result.selected.len(), 1);
    assert_eq!(result.selected[0].goal.id, "resurface-open-thread");
    assert!(result.assembly.used_estimated_tokens <= 40);
    assert!(
        result
            .omitted
            .iter()
            .any(|omitted| omitted.goal.id == "clarify-weak-evidence-topic")
    );
}

#[test]
fn perturbing_the_fixture_changes_selection_predictably() {
    let fixture = static_fixture();
    let base = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 80),
    );

    let mut perturbed = fixture.clone();
    let goal = perturbed
        .goals
        .iter_mut()
        .find(|goal| goal.id == "resurface-open-thread")
        .unwrap();
    goal.activation_keywords
        .retain(|keyword| keyword.term != "continuity");

    let changed = select_goals(
        "We never settled how voice memory affects continuity.",
        &perturbed,
        ContextBudget::new(2, 80),
    );

    assert!(
        base.selected
            .iter()
            .any(|selection| selection.goal.id == "resurface-open-thread")
    );
    assert!(
        !changed
            .selected
            .iter()
            .any(|selection| selection.goal.id == "resurface-open-thread")
    );
    assert!(
        changed
            .selected
            .iter()
            .any(|selection| selection.goal.id == "clarify-weak-evidence-topic")
    );
}

#[test]
fn goal_selection_result_serializes() {
    let fixture = static_fixture();
    let result: GoalSelectionResult = select_goals(
        "Is the goal system implemented yet?",
        &fixture,
        ContextBudget::new(2, 80),
    );

    let json = serde_json::to_value(result).unwrap();

    assert_eq!(
        json["selected"][0]["goal"]["id"],
        "avoid-overstating-impl-status"
    );
}

#[test]
fn selected_goal_trace_records_delta_tensions_and_choice() {
    let fixture = static_fixture();
    let result = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 80),
    );
    let traces = build_pre_initiative_traces(&result, &fixture);

    let trace = traces
        .iter()
        .find(|trace| trace.goal_id.as_deref() == Some("clarify-weak-evidence-topic"))
        .expect("continuity input should trace the weak-evidence goal");

    match &trace.delta {
        DeltaAssessment::Delta(delta) => {
            assert!(!delta.matched_evidence.is_empty());
            assert!(!delta.goal_concern_summary.is_empty());
        }
        DeltaAssessment::NoDelta { reason } => {
            panic!("expected a delta, got no-delta reason: {reason}")
        }
    }

    assert!(
        !trace.tensions.is_empty(),
        "selected goal should record tension provenance"
    );

    assert_eq!(
        trace.goal_summary.as_deref(),
        Some(
            "Surface a research question when the input points at uncertain or under-explained material."
        ),
        "selected-goal trace should be self-contained with the goal summary"
    );

    let choice = trace
        .choice
        .as_ref()
        .expect("selected goal proposes an effect");
    assert_eq!(choice.proposed.effect.to_string(), "reflect");
    assert_eq!(choice.losing.len(), 1);
    assert_eq!(
        choice.losing[0].proposal.effect.to_string(),
        "propose-experiment"
    );
    assert!(!choice.losing[0].reason.is_empty());
}

#[test]
fn baseline_input_produces_single_no_delta_trace() {
    let fixture = static_fixture();
    let result = select_goals(
        "Give me the build command.",
        &fixture,
        ContextBudget::new(2, 80),
    );
    let traces = build_pre_initiative_traces(&result, &fixture);

    assert_eq!(traces.len(), 1);
    let trace = &traces[0];
    assert!(trace.goal_id.is_none());
    assert!(trace.goal_summary.is_none());
    assert!(trace.choice.is_none());
    assert!(matches!(trace.delta, DeltaAssessment::NoDelta { .. }));
}

#[test]
fn traces_never_execute_an_effect() {
    let fixture = static_fixture();
    for input in [
        "We never settled how voice memory affects continuity.",
        "Give me the build command.",
        "Is the goal system implemented yet?",
    ] {
        let result = select_goals(input, &fixture, ContextBudget::new(2, 80));
        let traces = build_pre_initiative_traces(&result, &fixture);
        assert!(traces.iter().all(|trace| !trace.executed));
    }
}

#[test]
fn traces_are_deterministic_for_the_same_input() {
    let fixture = static_fixture();
    let input = "We never settled how voice memory affects continuity.";
    let first = build_pre_initiative_traces(
        &select_goals(input, &fixture, ContextBudget::new(2, 80)),
        &fixture,
    );
    let second = build_pre_initiative_traces(
        &select_goals(input, &fixture, ContextBudget::new(2, 80)),
        &fixture,
    );

    assert_eq!(first, second);
}

#[test]
fn every_selected_goal_trace_carries_a_proposed_effect() {
    let fixture = static_fixture();
    let result = select_goals(
        "We never settled how voice memory affects continuity.",
        &fixture,
        ContextBudget::new(2, 80),
    );
    let traces = build_pre_initiative_traces(&result, &fixture);

    assert!(!traces.is_empty());
    for trace in &traces {
        assert!(trace.goal_id.is_some());
        assert!(matches!(trace.delta, DeltaAssessment::Delta(_)));
        assert!(trace.choice.is_some());
        assert!(trace.allowed_rationale.is_some());
    }
}

#[test]
fn serialized_traces_are_deterministic_for_the_full_scripted_set() {
    let fixture = static_fixture();
    let scripted_inputs = [
        "Is the goal system implemented yet?",
        "We never settled how voice memory affects continuity.",
        "Give me the build command.",
        "Should we turn the volition note into a tiny experiment?",
    ];

    let serialize_all = || {
        let mut serialized = String::new();
        for input in scripted_inputs {
            let result = select_goals(input, &fixture, ContextBudget::new(2, 80));
            for trace in build_pre_initiative_traces(&result, &fixture) {
                serialized.push_str(&serde_json::to_string(&trace).unwrap());
                serialized.push('\n');
            }
        }
        serialized
    };

    assert_eq!(serialize_all(), serialize_all());
}

mod arbitration;
mod candidates;
mod initiative;
mod reducer;
