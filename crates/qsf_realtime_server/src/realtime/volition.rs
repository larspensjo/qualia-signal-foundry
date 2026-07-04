use qsf_volition::{
    GoalStatus, VolitionEvent, VolitionFixture, VolitionState, normalize_terms,
    realtime_seed_fixture, tick_events,
};

/// Per-session in-memory volition state owned by the realtime server. Seeded from
/// `realtime_seed_fixture()` on session creation; in-memory only, not persisted mid-session.
///
/// Coherence-declined candidates are reducer-derived state: they live in
/// `state.declined_candidates` (`qsf_volition::DeclinedCandidate`), populated automatically when
/// `apply_events` applies a `GoalCandidateRejected` event carrying a `CoherenceDecline` — there
/// is no separate side-channel write here, so replaying `state.declined_candidates` from the
/// applied event stream reproduces the same durable context (see `volition_injection.rs`).
pub struct VolitionRuntimeState {
    pub state: VolitionState,
    pub fixture: VolitionFixture,
    /// Hash of the goal-set prefix sent to the live-goal-formation judge on the last trusted
    /// turn, so the next turn can tell whether its own prefix is cache-hit-eligible (unchanged)
    /// or was just invalidated by an admission/retirement/sweep.
    pub last_goal_set_prefix_hash: Option<String>,
}

impl VolitionRuntimeState {
    pub fn new() -> Self {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        Self {
            state,
            fixture,
            last_goal_set_prefix_hash: None,
        }
    }

    pub fn apply_events(&mut self, events: Vec<VolitionEvent>) {
        for event in events {
            self.state = qsf_volition::apply(self.state.clone(), event);
        }
    }
}

/// Map a trusted user transcript to candidate `VolitionEvent`s. Pure and side-effect-free.
///
/// Produces tick-lifecycle events (decay, cooldown-elapsed, retirement) for the new tick,
/// then a `TickAdvanced` event, then `GoalActivated` for each fixture or accepted-candidate
/// goal whose `activation_keywords` appear in the normalized transcript.
pub fn events_for_trusted_transcript(
    transcript: &str,
    state: &VolitionState,
    fixture: &VolitionFixture,
    new_tick: u64,
) -> Vec<VolitionEvent> {
    let input_terms = normalize_terms(transcript);
    let mut events = tick_events(state, fixture, new_tick);
    events.push(VolitionEvent::TickAdvanced { tick: new_tick });

    for goal in &fixture.goals {
        let Some(dynamic) = state.goals.get(&goal.id) else {
            continue;
        };
        if !matches!(dynamic.status, GoalStatus::Accepted | GoalStatus::Active) {
            continue;
        }
        if goal
            .activation_keywords
            .iter()
            .any(|kw| input_terms.contains(&kw.term))
        {
            events.push(VolitionEvent::GoalActivated {
                goal_id: goal.id.clone(),
                tick: new_tick,
            });
        }
    }

    for (goal_id, goal) in &state.accepted_candidates {
        let Some(dynamic) = state.goals.get(goal_id) else {
            continue;
        };
        if !matches!(dynamic.status, GoalStatus::Accepted | GoalStatus::Active) {
            continue;
        }
        if goal
            .activation_keywords
            .iter()
            .any(|kw| input_terms.contains(&kw.term))
        {
            events.push(VolitionEvent::GoalActivated {
                goal_id: goal_id.clone(),
                tick: new_tick,
            });
        }
    }

    events
}

/// A loaded continuity snapshot is compatible with the active fixture only if every Accepted
/// fixture goal has a dynamic entry in it. After a persona swap (all goal ids change) an old
/// snapshot fails this check and must be discarded rather than installed, since installing it
/// would yield a mixed runtime keyed by dead ids with no dynamic state for the new goals.
pub fn snapshot_is_fixture_compatible(snapshot: &VolitionState, fixture: &VolitionFixture) -> bool {
    fixture
        .goals
        .iter()
        .filter(|g| g.status == GoalStatus::Accepted)
        .all(|g| snapshot.goals.contains_key(&g.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_volition::{GoalStatus, VolitionState, realtime_seed_fixture};

    // ── VolitionRuntimeState ─────────────────────────────────────────────────

    #[test]
    fn new_runtime_state_starts_with_no_declined_candidates() {
        let runtime = VolitionRuntimeState::new();
        assert!(runtime.state.declined_candidates.is_empty());
    }

    #[test]
    fn applying_a_coherence_rejection_populates_reducer_derived_declined_candidates() {
        use qsf_volition::{
            AllowedEffect, CoherenceDecline, DeclineReason, EvidenceRef, GoalScope,
            ProposedGoalCandidate,
        };

        let mut runtime = VolitionRuntimeState::new();
        let candidate = ProposedGoalCandidate::try_new(
            "candidate-1".to_string(),
            "candidate title".to_string(),
            "candidate summary".to_string(),
            vec![],
            GoalScope::Session,
            50,
            vec![AllowedEffect::Reflect],
            "satisfied when discussed".to_string(),
            vec![EvidenceRef::try_new("turn transcript evidence").unwrap()],
            "formed from live discussion".to_string(),
            vec![],
        )
        .unwrap();
        runtime.apply_events(vec![
            VolitionEvent::GoalCandidateAdded { candidate, tick: 3 },
            VolitionEvent::GoalCandidateRejected {
                goal_id: "candidate-1".to_string(),
                reason:
                    "contradicts more-or-equally fundamental goal(s): keep-theses-distinct-from-fact"
                        .to_string(),
                coherence_decline: Some(CoherenceDecline {
                    conflict: DeclineReason::ConflictingGoal {
                        goal_id: "keep-theses-distinct-from-fact".to_string(),
                    },
                    rationale: "would undermine the current task".to_string(),
                }),
                tick: 3,
            },
        ]);

        assert_eq!(runtime.state.declined_candidates.len(), 1);
        assert_eq!(
            runtime.state.declined_candidates[0].candidate_id,
            "candidate-1"
        );
        assert_eq!(
            runtime.state.declined_candidates[0].conflict,
            DeclineReason::ConflictingGoal {
                goal_id: "keep-theses-distinct-from-fact".to_string()
            }
        );
    }

    #[test]
    fn new_runtime_state_is_seeded_from_realtime_fixture() {
        let runtime = VolitionRuntimeState::new();
        assert_eq!(runtime.state.tick, 0);
        assert!(
            runtime.state.goals.contains_key("serve-the-present-person"),
            "protected tier-3 goal must be present"
        );
        assert!(
            runtime
                .state
                .goals
                .contains_key("keep-theses-distinct-from-fact"),
            "protected tier-2 goal must be present"
        );
    }

    #[test]
    fn new_runtime_state_starts_with_all_goals_accepted() {
        let runtime = VolitionRuntimeState::new();
        for (goal_id, dynamic) in &runtime.state.goals {
            assert_eq!(
                dynamic.status,
                GoalStatus::Accepted,
                "goal '{goal_id}' must start as Accepted"
            );
        }
    }

    #[test]
    fn two_independent_runtime_states_are_isolated() {
        let mut a = VolitionRuntimeState::new();
        let b = VolitionRuntimeState::new();
        let transcript = "how can you help me";
        let tick = a.state.tick + 1;
        let events = events_for_trusted_transcript(transcript, &a.state, &a.fixture, tick);
        a.apply_events(events);
        assert_ne!(
            a.state.tick, b.state.tick,
            "mutating one runtime must not affect the other"
        );
    }

    // ── events_for_trusted_transcript ────────────────────────────────────────

    #[test]
    fn matching_transcript_activates_goal_with_keyword() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let transcript = "how can you help me with this";
        let events = events_for_trusted_transcript(transcript, &state, &fixture, 1);
        assert!(
            events.iter().any(|e| matches!(
                e,
                VolitionEvent::GoalActivated { goal_id, .. } if goal_id == "serve-the-present-person"
            )),
            "transcript with 'how'/'can'/'help' must activate serve-the-present-person"
        );
    }

    #[test]
    fn non_matching_transcript_produces_no_activation_events() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let transcript = "xyzzy frobnicator quux";
        let events = events_for_trusted_transcript(transcript, &state, &fixture, 1);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VolitionEvent::GoalActivated { .. })),
            "transcript with no keyword matches must not produce GoalActivated events"
        );
    }

    #[test]
    fn events_for_trusted_transcript_always_includes_tick_advanced() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let events = events_for_trusted_transcript("anything", &state, &fixture, 5);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, VolitionEvent::TickAdvanced { tick: 5 })),
            "TickAdvanced must always be produced"
        );
    }

    #[test]
    fn events_for_trusted_transcript_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let transcript = "can you help explain this task";
        let first = events_for_trusted_transcript(transcript, &state, &fixture, 1);
        let second = events_for_trusted_transcript(transcript, &state, &fixture, 1);
        assert_eq!(
            first, second,
            "mapping must be deterministic for the same input"
        );
    }

    #[test]
    fn applying_transcript_events_advances_tick_and_activates_goals() {
        let mut runtime = VolitionRuntimeState::new();
        let transcript = "how can you help with this task";
        let new_tick = runtime.state.tick + 1;
        let events =
            events_for_trusted_transcript(transcript, &runtime.state, &runtime.fixture, new_tick);
        runtime.apply_events(events);
        assert_eq!(runtime.state.tick, new_tick);
        let present_person_status = runtime
            .state
            .goals
            .get("serve-the-present-person")
            .map(|d| d.status);
        assert_eq!(
            present_person_status,
            Some(GoalStatus::Active),
            "serve-the-present-person must be Active after matching transcript"
        );
    }

    #[test]
    fn cooldown_goal_is_not_activated_by_matching_transcript() {
        use qsf_volition::{EvidenceRef, VolitionEvent, apply};

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let evidence = EvidenceRef::try_new("test-evidence").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "serve-the-present-person".to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: "serve-the-present-person".to_string(),
                evidence,
                tick: 2,
            },
        );
        assert_eq!(
            state.goals.get("serve-the-present-person").unwrap().status,
            GoalStatus::Cooldown
        );

        let events = events_for_trusted_transcript("how can you help", &state, &fixture, 3);
        assert!(
            !events.iter().any(|e| matches!(
                e,
                VolitionEvent::GoalActivated { goal_id, .. } if goal_id == "serve-the-present-person"
            )),
            "cooldown goal must not be activated"
        );
    }

    #[test]
    fn events_for_trusted_transcript_mapping_overhead_is_under_10ms() {
        use std::time::Instant;

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let transcript = "how can you help me with this task";
        let iterations = 200u64;

        // Baseline: just normalize terms (the shared precondition for both paths).
        let baseline_start = Instant::now();
        for i in 1..=iterations {
            let _ = qsf_volition::normalize_terms(transcript);
            let _ = i; // suppress unused warning
        }
        let baseline_elapsed = baseline_start.elapsed();

        // Full mapping path.
        let mapping_start = Instant::now();
        for i in 1..=iterations {
            let _ = events_for_trusted_transcript(transcript, &state, &fixture, i);
        }
        let mapping_elapsed = mapping_start.elapsed();

        let delta_ms = mapping_elapsed
            .as_millis()
            .saturating_sub(baseline_elapsed.as_millis());
        assert!(
            delta_ms < 10,
            "volition mapping overhead must be under 10 ms over {iterations} iterations; \
             baseline={baseline_elapsed:?} mapping={mapping_elapsed:?} delta={delta_ms}ms"
        );
    }

    #[test]
    fn protected_goals_survive_retirement_inactivity_threshold() {
        use qsf_volition::{RETIREMENT_INACTIVITY_TICKS, apply};

        let fixture = realtime_seed_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        // Apply RETIREMENT_INACTIVITY_TICKS + 1 non-matching transcripts — enough to retire
        // any unprotected goal, but protected tier-2 and tier-3 goals must remain selectable.
        let non_matching = "xyzzy frobnicator quux";
        for i in 1..=(RETIREMENT_INACTIVITY_TICKS + 1) {
            let events = events_for_trusted_transcript(non_matching, &state, &fixture, i);
            for event in events {
                state = apply(state, event);
            }
        }

        let present_person_status = state
            .goals
            .get("serve-the-present-person")
            .map(|d| d.status);
        assert_ne!(
            present_person_status,
            Some(GoalStatus::Retired),
            "protected tier-3 goal must not be retired after idle ticks"
        );
        assert!(
            matches!(
                present_person_status,
                Some(GoalStatus::Accepted | GoalStatus::Active)
            ),
            "protected tier-3 goal must remain Accepted or Active; got {present_person_status:?}"
        );

        let epistemic_status = state
            .goals
            .get("keep-theses-distinct-from-fact")
            .map(|d| d.status);
        assert_ne!(
            epistemic_status,
            Some(GoalStatus::Retired),
            "protected tier-2 goal must not be retired after idle ticks"
        );
        assert!(
            matches!(
                epistemic_status,
                Some(GoalStatus::Accepted | GoalStatus::Active)
            ),
            "protected tier-2 goal must remain Accepted or Active; got {epistemic_status:?}"
        );

        // Verify they can still activate — the safety guarantee is intact.
        let activating = "how can you help me with this task";
        let activate_tick = state.tick + 1;
        let events = events_for_trusted_transcript(activating, &state, &fixture, activate_tick);
        assert!(
            events.iter().any(|e| matches!(
                e,
                VolitionEvent::GoalActivated { goal_id, .. } if goal_id == "serve-the-present-person"
            )),
            "protected tier-3 goal must still activate from transcript evidence after idle period"
        );
    }

    // ── snapshot_is_fixture_compatible ───────────────────────────────────────

    #[test]
    fn snapshot_from_current_fixture_is_compatible() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        assert!(snapshot_is_fixture_compatible(&state, &fixture));
    }

    #[test]
    fn snapshot_missing_new_fixture_goals_is_incompatible() {
        let fixture = realtime_seed_fixture();
        // An "old" snapshot that knows nothing about the current persona's goals.
        let mut stale = VolitionState::from_fixture(&fixture);
        stale.goals.clear(); // simulate a snapshot whose goal ids predate this fixture
        assert!(!snapshot_is_fixture_compatible(&stale, &fixture));
    }
}
