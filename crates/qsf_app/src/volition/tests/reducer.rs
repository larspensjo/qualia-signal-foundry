use super::super::{
    AllowedEffect, COOLDOWN_SPAN_TICKS, EvidenceRef, GoalScope, GoalStatus, ProposedGoalCandidate,
    RETIREMENT_INACTIVITY_TICKS, SALIENCE_ACTIVATION_BONUS, SALIENCE_DECAY_PER_TICK, VolitionEvent,
    VolitionState, apply, static_fixture, tick_events,
};

// ── EvidenceRef validation ──────────────────────────────────────────────

#[test]
fn evidence_ref_rejects_empty_string() {
    assert!(EvidenceRef::try_new("").is_err());
}

#[test]
fn evidence_ref_rejects_whitespace_only() {
    assert!(EvidenceRef::try_new("   ").is_err());
    assert!(EvidenceRef::try_new("\t\n").is_err());
}

#[test]
fn evidence_ref_accepts_non_empty() {
    let r = EvidenceRef::try_new("docs/Experiment.md").unwrap();
    assert_eq!(r.as_str(), "docs/Experiment.md");
}

#[test]
fn evidence_ref_try_from_string_works() {
    let r = EvidenceRef::try_from("trace-42".to_string()).unwrap();
    assert_eq!(r.as_str(), "trace-42");
}

// ── GoalActivated ───────────────────────────────────────────────────────

#[test]
fn goal_activated_sets_active_and_raises_salience() {
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

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.status, GoalStatus::Active);
    assert_eq!(dynamic.salience, SALIENCE_ACTIVATION_BONUS);
    assert_eq!(dynamic.last_activated_tick, Some(1));
}

#[test]
fn repeated_activations_raise_salience_monotonically_before_decay() {
    let fixture = static_fixture();
    let mut state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";

    let mut prev_salience = 0;
    for tick in 1..=5 {
        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick,
            },
        );
        let s = state.goal(goal_id).unwrap().salience;
        assert!(
            s > prev_salience,
            "salience should rise monotonically, tick={tick}"
        );
        prev_salience = s;
    }
}

#[test]
fn irrelevant_goal_stays_at_zero_salience() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let activated_id = "clarify-weak-evidence-topic";
    let other_id = "avoid-overstating-impl-status";

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: activated_id.to_string(),
            tick: 1,
        },
    );

    assert_eq!(state.goal(other_id).unwrap().salience, 0);
}

// ── GoalProgressObserved ────────────────────────────────────────────────

#[test]
fn progress_appends_evidence_ref_and_increments_reinforcement() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";
    let evidence = EvidenceRef::try_new("trace-42").unwrap();

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: goal_id.to_string(),
            tick: 1,
        },
    );
    let state = apply(
        state,
        VolitionEvent::GoalProgressObserved {
            goal_id: goal_id.to_string(),
            evidence: evidence.clone(),
            tick: 2,
        },
    );

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.reinforcement_count, 1);
    assert!(dynamic.progress_evidence_refs.contains(&evidence));
    assert!(dynamic.salience > SALIENCE_ACTIVATION_BONUS);
}

// ── GoalDecayed ─────────────────────────────────────────────────────────

#[test]
fn decay_lowers_salience_by_deterministic_amount() {
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
    let salience_before = state.goal(goal_id).unwrap().salience;

    let state = apply(
        state,
        VolitionEvent::GoalDecayed {
            goal_id: goal_id.to_string(),
            tick: 2,
        },
    );

    let salience_after = state.goal(goal_id).unwrap().salience;
    assert_eq!(salience_before - salience_after, SALIENCE_DECAY_PER_TICK);
    assert_eq!(
        state.goal(goal_id).unwrap().status,
        GoalStatus::Active,
        "decay must not change status"
    );
}

#[test]
fn decay_does_not_go_below_zero() {
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
    let mut state = state;
    for tick in 2..=20 {
        state = apply(
            state,
            VolitionEvent::GoalDecayed {
                goal_id: goal_id.to_string(),
                tick,
            },
        );
    }

    assert_eq!(state.goal(goal_id).unwrap().salience, 0);
}

// ── GoalSatisfied + GoalCooldownElapsed ─────────────────────────────────

#[test]
fn satisfaction_enters_cooldown_and_resets_salience() {
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

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.status, GoalStatus::Cooldown);
    assert_eq!(dynamic.salience, 0);
    assert_eq!(dynamic.last_satisfied_tick, Some(2));
    assert_eq!(dynamic.cooldown_until_tick, Some(2 + COOLDOWN_SPAN_TICKS));
}

#[test]
fn cooldown_elapsed_returns_goal_to_accepted() {
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
    let state = apply(
        state,
        VolitionEvent::GoalCooldownElapsed {
            goal_id: goal_id.to_string(),
            tick: 2 + COOLDOWN_SPAN_TICKS,
        },
    );

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.status, GoalStatus::Accepted);
    assert!(dynamic.cooldown_until_tick.is_none());
}

// ── GoalBlocked ─────────────────────────────────────────────────────────

#[test]
fn blocked_goal_keeps_status_and_nonzero_salience() {
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
    let salience_before = state.goal(goal_id).unwrap().salience;
    let state = apply(
        state,
        VolitionEvent::GoalBlocked {
            goal_id: goal_id.to_string(),
            tick: 2,
        },
    );

    let dynamic = state.goal(goal_id).unwrap();
    assert_eq!(dynamic.status, GoalStatus::Blocked);
    assert_eq!(
        dynamic.salience, salience_before,
        "blocked must preserve salience"
    );
}

// ── GoalRetired ──────────────────────────────────────────────────────────

#[test]
fn retired_goal_reaches_retired_status() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";

    let state = apply(
        state,
        VolitionEvent::GoalRetired {
            goal_id: goal_id.to_string(),
            tick: 1,
        },
    );

    assert_eq!(state.goal(goal_id).unwrap().status, GoalStatus::Retired);
}

// ── tick_events ──────────────────────────────────────────────────────────

#[test]
fn tick_events_emits_decay_for_active_goal_with_salience() {
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
    let events = tick_events(&state, &fixture, 2);

    assert!(events.iter().any(|event| matches!(
        event,
        VolitionEvent::GoalDecayed { goal_id: id, .. } if id == "clarify-weak-evidence-topic"
    )));
}

#[test]
fn tick_events_emits_retirement_for_zero_salience_inactive_goal() {
    let fixture = static_fixture();
    let mut state = VolitionState::from_fixture(&fixture);
    let candidate = ProposedGoalCandidate::try_new(
        "live-formed-tangent".to_string(),
        "Live-formed tangent".to_string(),
        "A malleable, non-fixture candidate.".to_string(),
        vec!["research-curiosity".to_string()], // tier 7, above the floor
        GoalScope::Session,
        88,
        vec![AllowedEffect::Reflect],
        "Satisfied when resolved.".to_string(),
        vec![EvidenceRef::try_new("test").unwrap()],
        "test".to_string(),
        vec![],
    )
    .unwrap();
    state = apply(
        state,
        VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
    );
    let acceptance_evidence = EvidenceRef::try_new("test-accept").unwrap();
    state = apply(
        state,
        VolitionEvent::GoalCandidateAccepted {
            goal_id: "live-formed-tangent".to_string(),
            acceptance_evidence,
            tick: 2,
        },
    );

    let events = tick_events(&state, &fixture, 2 + RETIREMENT_INACTIVITY_TICKS);

    assert!(events.iter().any(|event| matches!(
        event,
        VolitionEvent::GoalRetired { goal_id: id, .. } if id == "live-formed-tangent"
    )));
}

#[test]
fn tick_events_emits_cooldown_elapsed_after_span() {
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

    let events = tick_events(&state, &fixture, 2 + COOLDOWN_SPAN_TICKS);

    assert!(events.iter().any(|event| matches!(
        event,
        VolitionEvent::GoalCooldownElapsed { goal_id: id, .. } if id == goal_id
    )));
}

// ── Tick monotonicity ────────────────────────────────────────────────────

#[test]
fn reducer_tick_never_decreases_on_lower_tick_event() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: goal_id.to_string(),
            tick: 5,
        },
    );
    assert_eq!(state.tick, 5);

    let state = apply(
        state,
        VolitionEvent::GoalDecayed {
            goal_id: goal_id.to_string(),
            tick: 3,
        },
    );
    assert_eq!(state.tick, 5, "lower-tick event must not regress tick");
}

#[test]
fn reducer_tick_is_stable_across_same_tick_events() {
    let fixture = static_fixture();
    let state = VolitionState::from_fixture(&fixture);
    let goal_id = "clarify-weak-evidence-topic";

    let state = apply(
        state,
        VolitionEvent::GoalActivated {
            goal_id: goal_id.to_string(),
            tick: 4,
        },
    );
    let state = apply(
        state,
        VolitionEvent::GoalDecayed {
            goal_id: goal_id.to_string(),
            tick: 4,
        },
    );
    assert_eq!(state.tick, 4, "duplicate-tick event must not move tick");
}

// ── Replay determinism ───────────────────────────────────────────────────

#[test]
fn same_event_sequence_yields_identical_state() {
    let fixture = static_fixture();
    let evidence = EvidenceRef::try_new("docs/trace.md").unwrap();

    let run = || {
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
            VolitionEvent::GoalProgressObserved {
                goal_id: goal_id.to_string(),
                evidence: evidence.clone(),
                tick: 2,
            },
        );
        apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: goal_id.to_string(),
                tick: 3,
            },
        )
    };

    assert_eq!(run(), run());
}
