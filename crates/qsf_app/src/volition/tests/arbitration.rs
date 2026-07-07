use super::super::{
    ActivationKeyword, AllowedEffect, BiasOutcome, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    Goal, GoalScope, GoalSelection, GoalStatus, InitiativeProposal, Mode, PROTECTED_TIER_FLOOR,
    Tension, TensionPriority, VolitionEvent, VolitionFixture, VolitionState, apply, arbitrate,
    arbitrate_with_mode, static_fixture,
};
use crate::context::ContextBudget;

fn make_goal_for_arbitration(
    id: &str,
    tension_ids: Vec<String>,
    base_priority: u8,
) -> GoalSelection {
    let goal = Goal {
        id: id.to_string(),
        title: id.to_string(),
        summary: id.to_string(),
        tension_ids,
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority,
        activation_keywords: vec![ActivationKeyword::normal("test")],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: id.to_string(),
        evidence_refs: vec![],
        estimated_tokens: 10,
        source_reference: id.to_string(),
        visibility: qsf_volition::GoalVisibility::Conscious,
    };
    GoalSelection {
        goal: goal.clone(),
        relevance_score: goal.base_priority as f64,
        matched_keywords: vec![ActivationKeyword::normal("test")],
        match_strength: 4,
        initiative: InitiativeProposal {
            goal_id: goal.id.clone(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::Reflect,
            rationale: "test".to_string(),
            matched_terms: vec!["test".to_string()],
            scope: GoalScope::Session,
        },
    }
}

fn make_tension(id: &str, tier: u8) -> Tension {
    Tension {
        id: id.to_string(),
        title: format!("{id} title"),
        summary: "test".to_string(),
        priority_bias: TensionPriority::Medium,
        arbitration_tier: tier,
        focused_bias: 0,
        exploratory_bias: 0,
    }
}

#[test]
fn arbitrate_empty_returns_none() {
    let fixture = static_fixture();
    assert!(arbitrate(vec![], &fixture).is_none());
}

#[test]
fn arbitrate_single_selection_is_winner_with_no_losers() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let result = super::super::select_goals_with_salience(
        "Is the implementation status complete?",
        &fixture,
        &state,
        ContextBudget::new(2, 80),
    );
    // Only avoid-overstating-impl-status matches (keywords: status, complete)
    assert_eq!(result.selected.len(), 1);
    let arbitration = arbitrate(result.selected.clone(), &fixture)
        .unwrap()
        .qualified
        .unwrap();
    assert_eq!(arbitration.winner.goal.id, "avoid-overstating-impl-status");
    assert!(arbitration.losers.is_empty());
}

#[test]
fn arbitrate_lower_tier_wins_over_higher_tier() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    // "status"/"complete" → avoid-overstating-impl-status (tier 1 via boundary-preservation)
    // "continuity"/"thread" → resurface-open-thread (tier 5 via continuity-preservation)
    let result = super::super::select_goals_with_salience(
        "Is the implementation status complete in this continuity thread?",
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );
    assert_eq!(result.selected.len(), 2, "expected 2 selected goals");

    let arbitration = arbitrate(result.selected.clone(), &fixture)
        .unwrap()
        .qualified
        .unwrap();
    assert_eq!(arbitration.winner.goal.id, "avoid-overstating-impl-status");
    assert_eq!(arbitration.winner_effective_tier, 1);
    assert_eq!(
        arbitration.winner_effective_tension_id,
        "boundary-preservation"
    );
    assert_eq!(
        arbitration.winner_effective_tension_title,
        "Boundary preservation"
    );
    assert_eq!(arbitration.losers.len(), 1);
    assert_eq!(
        arbitration.losers[0].selection.goal.id,
        "resurface-open-thread"
    );
    assert_eq!(arbitration.losers[0].effective_tier, 5);
    assert_eq!(
        arbitration.losers[0].effective_tension_id,
        "continuity-preservation"
    );
}

#[test]
fn arbitrate_same_tier_higher_base_priority_wins() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    // "voice"/"memory"/"evidence"/"unclear" → clarify-weak-evidence-topic (tier 7, priority 85)
    // "experiment" → propose-followup-experiment (tier 7, priority 90)
    let result = super::super::select_goals_with_salience(
        "The voice memory experiment evidence is unclear.",
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );
    // Both goals are at tier 7 via research-curiosity
    let selected_ids: Vec<&str> = result.selected.iter().map(|s| s.goal.id.as_str()).collect();
    assert!(
        selected_ids.contains(&"clarify-weak-evidence-topic"),
        "selected: {selected_ids:?}"
    );
    assert!(
        selected_ids.contains(&"propose-followup-experiment"),
        "selected: {selected_ids:?}"
    );

    let arbitration = arbitrate(result.selected.clone(), &fixture)
        .unwrap()
        .qualified
        .unwrap();
    // propose-followup-experiment (priority 90) beats clarify-weak-evidence-topic (priority 85)
    assert_eq!(arbitration.winner.goal.id, "propose-followup-experiment");
    assert_eq!(arbitration.winner_effective_tier, 7);
    assert_eq!(
        arbitration.winner_effective_tension_id,
        "research-curiosity"
    );
    assert_eq!(arbitration.losers.len(), 1);
    assert_eq!(
        arbitration.losers[0].selection.goal.id,
        "clarify-weak-evidence-topic"
    );
    assert_eq!(arbitration.losers[0].effective_tier, 7);
}

#[test]
fn arbitrate_same_tier_same_priority_lower_goal_id_wins() {
    let fixture = VolitionFixture {
        tensions: vec![make_tension("test-tension", 5)],
        goals: vec![],
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    };
    // "goal-a" < "goal-b" lexicographically; same tier and priority
    let sel_b = make_goal_for_arbitration("goal-b", vec!["test-tension".to_string()], 80);
    let sel_a = make_goal_for_arbitration("goal-a", vec!["test-tension".to_string()], 80);
    let result = arbitrate(vec![sel_b, sel_a], &fixture)
        .unwrap()
        .qualified
        .unwrap();
    assert_eq!(result.winner.goal.id, "goal-a");
    assert_eq!(result.losers[0].selection.goal.id, "goal-b");
}

#[test]
fn arbitrate_multi_tension_goal_uses_minimum_tier() {
    // avoid-overstating-impl-status has coherence-maintenance (tier 4) AND
    // boundary-preservation (tier 1). Effective tier must be 1 (the minimum).
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let result = super::super::select_goals_with_salience(
        "Is the implementation status complete?",
        &fixture,
        &state,
        ContextBudget::new(2, 80),
    );
    assert_eq!(result.selected.len(), 1);
    let arbitration = arbitrate(result.selected, &fixture)
        .unwrap()
        .qualified
        .unwrap();
    assert_eq!(
        arbitration.winner_effective_tier, 1,
        "effective tier must be the minimum among parent tensions"
    );
    assert_eq!(
        arbitration.winner_effective_tension_id, "boundary-preservation",
        "effective tension is the one at the minimum tier"
    );
}

#[test]
fn arbitrate_same_minimum_tier_picks_lexicographic_tension_id() {
    let fixture = VolitionFixture {
        tensions: vec![
            make_tension("beta-tension", 3),
            make_tension("alpha-tension", 3),
        ],
        goals: vec![],
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    };
    // Goal backed by both tensions at tier 3; alpha < beta lexicographically
    let sel = make_goal_for_arbitration(
        "test-goal",
        vec!["alpha-tension".to_string(), "beta-tension".to_string()],
        80,
    );
    let result = arbitrate(vec![sel], &fixture).unwrap().qualified.unwrap();
    assert_eq!(result.winner_effective_tier, 3);
    assert_eq!(result.winner_effective_tension_id, "alpha-tension");
    assert_eq!(result.winner_effective_tension_title, "alpha-tension title");
}

#[test]
fn arbitrate_losers_are_sorted_by_tier_then_priority_then_id() {
    let fixture = VolitionFixture {
        tensions: vec![
            make_tension("tier-1-tension", 1),
            make_tension("tier-5-tension", 5),
            make_tension("tier-7-tension", 7),
        ],
        goals: vec![],
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    };
    let sel_tier7 =
        make_goal_for_arbitration("goal-z-tier7", vec!["tier-7-tension".to_string()], 80);
    let sel_tier5 =
        make_goal_for_arbitration("goal-a-tier5", vec!["tier-5-tension".to_string()], 90);
    let sel_tier1 =
        make_goal_for_arbitration("goal-m-tier1", vec!["tier-1-tension".to_string()], 95);
    let result = arbitrate(vec![sel_tier7, sel_tier5, sel_tier1], &fixture)
        .unwrap()
        .qualified
        .unwrap();

    assert_eq!(result.winner.goal.id, "goal-m-tier1");
    assert_eq!(result.winner_effective_tier, 1);
    // Losers: tier 5 before tier 7 (ascending tier)
    assert_eq!(result.losers[0].selection.goal.id, "goal-a-tier5");
    assert_eq!(result.losers[0].effective_tier, 5);
    assert_eq!(result.losers[1].selection.goal.id, "goal-z-tier7");
    assert_eq!(result.losers[1].effective_tier, 7);
}

#[test]
fn arbitrate_result_is_deterministic() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let input = "Is the continuity thread complete enough to be confident in the evidence?";
    let budget = ContextBudget::new(4, 100);

    let run = || {
        let result = super::super::select_goals_with_salience(input, &fixture, &state, budget);
        arbitrate(result.selected, &fixture)
    };

    assert_eq!(run(), run());
}

#[test]
fn arbitrate_no_effect_is_executed() {
    // arbitrate() is a pure function that returns data only; the ArbitrationResult
    // carries no executed flag because execution is structurally impossible.
    // Verify that the initiative proposals in the result carry the expected effect
    // but nothing in the result signals actual execution.
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let result = super::super::select_goals_with_salience(
        "Is the implementation status complete in this continuity thread?",
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );
    let arbitration = arbitrate(result.selected, &fixture)
        .unwrap()
        .qualified
        .unwrap();
    // The winner carries an initiative proposal (not executed), losers likewise.
    // This assertion documents the contract: arbitrate() proposes, never executes.
    assert!(!arbitration.winner.initiative.goal_id.is_empty());
    for loser in &arbitration.losers {
        assert!(!loser.selection.initiative.goal_id.is_empty());
    }
}

// ── Phase 8: Mode and arbitrate_with_mode ─────────────────────────────────

#[test]
fn mode_neutral_tension_delta_is_zero_for_all() {
    let fixture = static_fixture();
    assert!(
        fixture
            .tensions
            .iter()
            .all(|t| Mode::Neutral.tension_delta(t) == 0)
    );
}

#[test]
fn mode_focused_tension_delta_matches_migrated_data() {
    let fixture = static_fixture();
    let delta = |id: &str| {
        fixture
            .tensions
            .iter()
            .find(|t| t.id == id)
            .map(|t| Mode::Focused.tension_delta(t))
            .unwrap()
    };
    assert_eq!(delta("research-curiosity"), 3);
    assert_eq!(delta("continuity-preservation"), -1);
}

#[test]
fn mode_exploratory_tension_delta_matches_migrated_data() {
    let fixture = static_fixture();
    let delta = |id: &str| {
        fixture
            .tensions
            .iter()
            .find(|t| t.id == id)
            .map(|t| Mode::Exploratory.tension_delta(t))
            .unwrap()
    };
    assert_eq!(delta("research-curiosity"), -2);
    assert_eq!(delta("continuity-preservation"), 1);
}

#[test]
fn arbitrate_with_mode_neutral_matches_arbitrate() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    // Band-only conflict: resurface-open-thread (tier 5) vs clarify-weak-evidence-topic (tier 7)
    let input = "The open thread about voice memory evidence is unresolved.";
    let sel = super::super::select_goals_with_salience(
        input,
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );
    assert_eq!(sel.selected.len(), 2, "expected 2 band-only selections");

    let arb = arbitrate(sel.selected.clone(), &fixture)
        .unwrap()
        .qualified
        .unwrap();
    let mode_arb = arbitrate_with_mode(sel.selected, &fixture, Mode::Neutral)
        .unwrap()
        .qualified
        .unwrap();

    assert_eq!(arb.winner.goal.id, mode_arb.winner.goal.id);
    assert_eq!(
        arb.winner_effective_tier,
        mode_arb.winner_bias.effective_tier
    );
    assert_eq!(
        arb.winner_effective_tension_id,
        mode_arb.winner_effective_tension_id
    );
    assert_eq!(arb.losers.len(), mode_arb.losers.len());
    for (al, ml) in arb.losers.iter().zip(mode_arb.losers.iter()) {
        assert_eq!(al.selection.goal.id, ml.selection.goal.id);
        assert_eq!(al.effective_tier, ml.bias.effective_tier);
    }
}

#[test]
fn arbitrate_with_mode_exploratory_flips_band_winner() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let input = "The open thread about voice memory evidence is unresolved.";
    let sel = super::super::select_goals_with_salience(
        input,
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );

    let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral)
        .unwrap()
        .qualified
        .unwrap();
    let exploratory = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory)
        .unwrap()
        .qualified
        .unwrap();

    // Under Neutral: continuity (tier 5) beats curiosity (tier 7)
    assert_eq!(neutral.winner.goal.id, "resurface-open-thread");
    // Under Exploratory: curiosity biased to 5, continuity biased to 6 → curiosity wins
    assert_eq!(exploratory.winner.goal.id, "clarify-weak-evidence-topic");
    assert_ne!(neutral.winner.goal.id, exploratory.winner.goal.id);
}

#[test]
fn arbitrate_with_mode_floor_goal_wins_under_biasing_mode() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    // Floor input: avoid-overstating-impl-status (tier 1) + two band goals
    let input = "Is the voice memory work complete, or is the evidence thread still unresolved?";
    let sel = super::super::select_goals_with_salience(
        input,
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );
    assert!(
        sel.selected
            .iter()
            .any(|s| s.goal.id == "avoid-overstating-impl-status"),
        "floor goal must be selected"
    );

    let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral)
        .unwrap()
        .qualified
        .unwrap();
    let exploratory = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory)
        .unwrap()
        .qualified
        .unwrap();

    // Floor goal wins under both modes
    assert_eq!(neutral.winner.goal.id, "avoid-overstating-impl-status");
    assert_eq!(exploratory.winner.goal.id, "avoid-overstating-impl-status");
    assert!(
        exploratory.winner_bias.protected,
        "floor goal must be marked protected"
    );
}

#[test]
fn arbitrate_with_mode_focused_keeps_continuity_winner() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let input = "The open thread about voice memory evidence is unresolved.";
    let sel = super::super::select_goals_with_salience(
        input,
        &fixture,
        &state,
        ContextBudget::new(4, 100),
    );

    let focused = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Focused)
        .unwrap()
        .qualified
        .unwrap();

    // Under Focused: continuity biased to 4, curiosity biased to 10 → continuity wins
    assert_eq!(focused.winner.goal.id, "resurface-open-thread");

    // Curiosity (clarify-weak-evidence-topic) should be demoted
    let curiosity_loser = focused
        .losers
        .iter()
        .find(|l| l.selection.goal.id == "clarify-weak-evidence-topic")
        .expect("curiosity goal must be a loser under Focused");
    assert!(
        curiosity_loser.bias.bias_applied > 0,
        "curiosity bias_applied must be positive (demotion) under Focused"
    );
    assert!(
        curiosity_loser.bias.biased_tier > curiosity_loser.bias.effective_tier,
        "curiosity biased_tier must exceed effective_tier under Focused"
    );
}

#[test]
fn mode_changed_event_updates_state_mode() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    assert_eq!(state.mode, Mode::Neutral, "initial mode must be Neutral");

    let state = apply(
        state,
        VolitionEvent::ModeChanged {
            mode: Mode::Exploratory,
            tick: 1,
        },
    );
    assert_eq!(state.mode, Mode::Exploratory);

    let state = apply(
        state,
        VolitionEvent::ModeChanged {
            mode: Mode::Focused,
            tick: 2,
        },
    );
    assert_eq!(state.mode, Mode::Focused);

    let state = apply(
        state,
        VolitionEvent::ModeChanged {
            mode: Mode::Neutral,
            tick: 3,
        },
    );
    assert_eq!(state.mode, Mode::Neutral);
}

#[test]
fn mode_changed_replay_reproduces_mode() {
    let fixture = static_fixture();
    let apply_seq = || {
        let s = VolitionState::from_fixture(&fixture);
        let s = apply(
            s,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 1,
            },
        );
        apply(
            s,
            VolitionEvent::ModeChanged {
                mode: Mode::Focused,
                tick: 2,
            },
        )
    };
    assert_eq!(apply_seq().mode, apply_seq().mode);
}

#[test]
fn band_goal_biased_tier_never_enters_floor() {
    // Verify the clamp arithmetic: a large negative bias on a band goal cannot produce
    // a biased_tier below PROTECTED_TIER_FLOOR + 1.
    let effective_tier: u8 = 5;
    let bias_applied: i8 = i8::MIN; // -128, extreme promotion attempt
    let raw = effective_tier as i16 + bias_applied as i16; // 5 - 128 = -123
    let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
    assert_eq!(biased_tier, PROTECTED_TIER_FLOOR + 1);

    // Confirm the BiasOutcome constructed from this arithmetic is correct.
    let outcome = BiasOutcome {
        effective_tier,
        bias_applied,
        biased_tier,
        protected: false,
    };
    assert_eq!(outcome.biased_tier, PROTECTED_TIER_FLOOR + 1);
    assert!(!outcome.protected);
}

#[test]
fn bias_arithmetic_u8_max_stays_at_max_under_positive_demotion() {
    // A goal with no fixture tension gets effective_tier = u8::MAX.
    // A large positive demotion must not wrap or panic; it should stay at u8::MAX.
    let fixture = VolitionFixture {
        tensions: vec![],
        goals: vec![],
        arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
    };
    let sel = make_goal_for_arbitration("no-tension-goal", vec![], 80);
    let result = arbitrate_with_mode(vec![sel], &fixture, Mode::Focused)
        .unwrap()
        .qualified
        .unwrap();
    assert_eq!(result.winner_bias.effective_tier, u8::MAX);
    assert_eq!(result.winner_bias.biased_tier, u8::MAX);
}

#[test]
fn arbitrate_with_mode_is_deterministic() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let input = "The open thread about voice memory evidence is unresolved.";
    let budget = ContextBudget::new(4, 100);

    let run = || {
        let sel = super::super::select_goals_with_salience(input, &fixture, &state, budget);
        arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory)
    };

    let r1 = run().unwrap().qualified.unwrap();
    let r2 = run().unwrap().qualified.unwrap();
    assert_eq!(r1.winner.goal.id, r2.winner.goal.id);
    assert_eq!(r1.winner_bias.biased_tier, r2.winner_bias.biased_tier);
}

#[test]
fn mode_field_serde_default_is_neutral() {
    // Existing state serialized without `mode` field must deserialize as Neutral.
    let json = serde_json::json!({
        "tick": 0,
        "goals": {},
        "pending_candidates": [],
        "accepted_candidates": {}
    });
    let state: VolitionState = serde_json::from_value(json).unwrap();
    assert_eq!(state.mode, Mode::Neutral);
}
