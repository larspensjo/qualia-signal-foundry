use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::arbitration::PROTECTED_TIER_FLOOR;
use crate::{
    AllowedEffect, EvidenceRef, Goal, GoalStatus, InitiativeOutput, Mode, ProposedGoalCandidate,
    VolitionFixture,
};

/// Dynamic, per-goal state tracked within a run. Separate from the read-only fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalDynamicState {
    pub status: GoalStatus,
    /// Integer salience points; rises on activation/progress, decays per tick linearly.
    pub salience: i32,
    pub reinforcement_count: u32,
    pub progress_evidence_refs: Vec<EvidenceRef>,
    pub last_activated_tick: Option<u64>,
    pub last_satisfied_tick: Option<u64>,
    /// Tick at which cooldown ends and the goal returns to Accepted.
    pub cooldown_until_tick: Option<u64>,
    /// The most recent initiative output for this goal, set by `InitiativeExecuted`.
    pub last_initiative_output: Option<InitiativeOutput>,
}

impl GoalDynamicState {
    pub fn initial() -> Self {
        Self {
            status: GoalStatus::Accepted,
            salience: 0,
            reinforcement_count: 0,
            progress_evidence_refs: Vec::new(),
            last_activated_tick: None,
            last_satisfied_tick: None,
            cooldown_until_tick: None,
            last_initiative_output: None,
        }
    }
}

/// Durable-within-a-run volition state: a logical tick and per-goal dynamic state for
/// all Accepted goals seeded from the fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionState {
    pub tick: u64,
    /// Keyed by goal id. Holds dynamic state for fixture-seeded goals and accepted candidates
    /// (wired into the selector and lifecycle reducer after `GoalCandidateAccepted`).
    pub goals: BTreeMap<String, GoalDynamicState>,
    /// Proposed goal candidates awaiting explicit accept or reject.
    pub pending_candidates: Vec<ProposedGoalCandidate>,
    /// Accepted goal data records keyed by goal id. Distinct from `goals`; holds the static
    /// `Goal` struct (title, tension_ids, activation_keywords, etc.) for accepted candidates.
    pub accepted_candidates: BTreeMap<String, Goal>,
    /// Active arbitration bias mode. Changed via `ModeChanged` event; default `Neutral`.
    #[serde(default)]
    pub mode: Mode,
}

impl VolitionState {
    /// Seed initial state from the fixture's Accepted goals.
    pub fn from_fixture(fixture: &crate::VolitionFixture) -> Self {
        let goals = fixture
            .goals
            .iter()
            .filter(|goal| goal.status == GoalStatus::Accepted)
            .map(|goal| (goal.id.clone(), GoalDynamicState::initial()))
            .collect();
        Self {
            tick: 0,
            goals,
            pending_candidates: Vec::new(),
            accepted_candidates: BTreeMap::new(),
            mode: Mode::Neutral,
        }
    }

    pub fn goal(&self, goal_id: &str) -> Option<&GoalDynamicState> {
        self.goals.get(goal_id)
    }
}

/// Salience points added when a goal is activated (first keyword match in a turn).
pub const SALIENCE_ACTIVATION_BONUS: i32 = 10;
/// Salience points added when progress evidence is recorded.
pub const SALIENCE_PROGRESS_BONUS: i32 = 5;
/// Salience points lost per tick from GoalDecayed.
pub const SALIENCE_DECAY_PER_TICK: i32 = 2;
/// Ticks of cooldown after a goal is satisfied.
pub const COOLDOWN_SPAN_TICKS: u64 = 3;
/// Ticks of inactivity after which an unproductive goal is retired.
pub const RETIREMENT_INACTIVITY_TICKS: u64 = 10;

/// One event per explicit lifecycle transition. The tick is the monotonic counter at the
/// time the event is produced; the reducer uses it to set timestamp fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VolitionEvent {
    GoalActivated {
        goal_id: String,
        tick: u64,
    },
    GoalProgressObserved {
        goal_id: String,
        evidence: EvidenceRef,
        tick: u64,
    },
    GoalSatisfied {
        goal_id: String,
        evidence: EvidenceRef,
        tick: u64,
    },
    GoalBlocked {
        goal_id: String,
        tick: u64,
    },
    /// Salience-only decay; never changes status.
    GoalDecayed {
        goal_id: String,
        tick: u64,
    },
    /// Transitions a Cooldown goal back to Accepted.
    GoalCooldownElapsed {
        goal_id: String,
        tick: u64,
    },
    GoalRetired {
        goal_id: String,
        tick: u64,
    },
    /// Advances the logical tick without modifying any goal lifecycle state.
    /// Applied unconditionally each turn to guarantee state.tick is monotonically
    /// increasing even when no lifecycle events are emitted.
    TickAdvanced {
        tick: u64,
    },
    /// Appends a proposed goal candidate to `pending_candidates`. Does not auto-accept.
    GoalCandidateAdded {
        candidate: ProposedGoalCandidate,
        tick: u64,
    },
    /// Moves a pending candidate to `accepted_candidates`. No-op if the candidate id is
    /// not in `pending_candidates`.
    GoalCandidateAccepted {
        goal_id: String,
        acceptance_evidence: EvidenceRef,
        tick: u64,
    },
    /// Removes a pending candidate from `pending_candidates`. Rejection reason is
    /// captured in the event log; no durable state for rejected candidates is kept.
    GoalCandidateRejected {
        goal_id: String,
        reason: String,
        tick: u64,
    },
    /// Records a bounded internal initiative output. Sets the goal to Active and stores
    /// the output in `GoalDynamicState::last_initiative_output`. Executes no external effect.
    InitiativeExecuted {
        goal_id: String,
        effect: AllowedEffect,
        output: InitiativeOutput,
        rationale: String,
        tick: u64,
    },
    /// Sets the active arbitration bias mode via the pure reducer. Replayable and traceable.
    ModeChanged {
        mode: Mode,
        tick: u64,
    },
}

/// Pure reducer: applies one event to state and returns the next state.
/// The only place lifecycle status changes; selectors never mutate lifecycle.
pub fn apply(mut state: VolitionState, event: VolitionEvent) -> VolitionState {
    state.tick = state.tick.max(event_tick(&event));
    match event {
        VolitionEvent::GoalActivated { goal_id, tick } => {
            let dynamic = state
                .goals
                .entry(goal_id)
                .or_insert_with(GoalDynamicState::initial);
            dynamic.status = GoalStatus::Active;
            dynamic.salience = (dynamic.salience + SALIENCE_ACTIVATION_BONUS).max(0);
            dynamic.last_activated_tick = Some(tick);
        }
        VolitionEvent::GoalProgressObserved {
            goal_id,
            evidence,
            tick: _,
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.reinforcement_count += 1;
                dynamic.salience = (dynamic.salience + SALIENCE_PROGRESS_BONUS).max(0);
                dynamic.progress_evidence_refs.push(evidence);
            }
        }
        VolitionEvent::GoalSatisfied {
            goal_id,
            evidence,
            tick,
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Cooldown;
                dynamic.salience = 0;
                dynamic.last_satisfied_tick = Some(tick);
                dynamic.cooldown_until_tick = Some(tick + COOLDOWN_SPAN_TICKS);
                dynamic.progress_evidence_refs.push(evidence);
            }
        }
        VolitionEvent::GoalBlocked { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Blocked;
            }
        }
        VolitionEvent::GoalDecayed { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.salience = (dynamic.salience - SALIENCE_DECAY_PER_TICK).max(0);
            }
        }
        VolitionEvent::GoalCooldownElapsed { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Accepted;
                dynamic.cooldown_until_tick = None;
            }
        }
        VolitionEvent::GoalRetired { goal_id, tick: _ } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Retired;
            }
        }
        VolitionEvent::TickAdvanced { .. } => {}
        VolitionEvent::GoalCandidateAdded { candidate, .. } => {
            state.pending_candidates.push(candidate);
        }
        VolitionEvent::GoalCandidateAccepted {
            goal_id,
            acceptance_evidence,
            ..
        } => {
            if let Some(pos) = state
                .pending_candidates
                .iter()
                .position(|c| c.id() == goal_id)
            {
                let candidate = state.pending_candidates.remove(pos);
                let goal = candidate.into_goal(acceptance_evidence);
                // Insert initial dynamic state so the accepted goal participates in
                // select_goals_with_salience with the same salience/cooldown paths as
                // fixture goals.
                state
                    .goals
                    .entry(goal_id.clone())
                    .or_insert_with(GoalDynamicState::initial);
                state.accepted_candidates.insert(goal_id, goal);
            }
        }
        VolitionEvent::GoalCandidateRejected { goal_id, .. } => {
            state.pending_candidates.retain(|c| c.id() != goal_id);
        }
        VolitionEvent::InitiativeExecuted {
            goal_id,
            output,
            tick,
            ..
        } => {
            if let Some(dynamic) = state.goals.get_mut(&goal_id) {
                dynamic.status = GoalStatus::Active;
                dynamic.last_activated_tick = Some(tick);
                dynamic.last_initiative_output = Some(output);
            }
        }
        VolitionEvent::ModeChanged { mode, .. } => {
            state.mode = mode;
        }
    }
    state
}

fn event_tick(event: &VolitionEvent) -> u64 {
    match event {
        VolitionEvent::GoalActivated { tick, .. }
        | VolitionEvent::GoalProgressObserved { tick, .. }
        | VolitionEvent::GoalSatisfied { tick, .. }
        | VolitionEvent::GoalBlocked { tick, .. }
        | VolitionEvent::GoalDecayed { tick, .. }
        | VolitionEvent::GoalCooldownElapsed { tick, .. }
        | VolitionEvent::GoalRetired { tick, .. }
        | VolitionEvent::TickAdvanced { tick }
        | VolitionEvent::GoalCandidateAdded { tick, .. }
        | VolitionEvent::GoalCandidateAccepted { tick, .. }
        | VolitionEvent::GoalCandidateRejected { tick, .. }
        | VolitionEvent::InitiativeExecuted { tick, .. }
        | VolitionEvent::ModeChanged { tick, .. } => *tick,
    }
}

/// Returns the minimum arbitration tier across a goal's parent tensions in the fixture.
/// Goals with no parent tensions in the fixture return `u8::MAX`.
fn goal_effective_tier(goal_id: &str, fixture: &VolitionFixture) -> u8 {
    let Some(goal) = fixture.goals.iter().find(|g| g.id == goal_id) else {
        return u8::MAX;
    };
    goal.tension_ids
        .iter()
        .filter_map(|tid| fixture.tensions.iter().find(|t| t.id == *tid))
        .map(|t| t.arbitration_tier)
        .min()
        .unwrap_or(u8::MAX)
}

/// Given the current state and the next tick, returns any tick-driven events that should
/// be applied: decay for all active/accepted goals, cooldown-elapsed for goals whose
/// cooldown has ended, retirement for goals that have been inactive too long.
///
/// Goals whose effective arbitration tier in the fixture is `<= PROTECTED_TIER_FLOOR` are
/// never retired by idle lifecycle — their safety guarantee must survive long sessions.
pub fn tick_events(
    state: &VolitionState,
    fixture: &VolitionFixture,
    new_tick: u64,
) -> Vec<VolitionEvent> {
    let mut events = Vec::new();
    for (goal_id, dynamic) in &state.goals {
        match dynamic.status {
            GoalStatus::Cooldown => {
                if let Some(cooldown_until) = dynamic.cooldown_until_tick {
                    if new_tick >= cooldown_until {
                        events.push(VolitionEvent::GoalCooldownElapsed {
                            goal_id: goal_id.clone(),
                            tick: new_tick,
                        });
                    }
                }
            }
            GoalStatus::Active | GoalStatus::Accepted => {
                if dynamic.salience > 0 {
                    events.push(VolitionEvent::GoalDecayed {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
                let is_protected = goal_effective_tier(goal_id, fixture) <= PROTECTED_TIER_FLOOR;
                let last_active = dynamic.last_activated_tick.unwrap_or(0);
                if !is_protected
                    && new_tick.saturating_sub(last_active) >= RETIREMENT_INACTIVITY_TICKS
                    && dynamic.reinforcement_count == 0
                    && dynamic.salience == 0
                {
                    events.push(VolitionEvent::GoalRetired {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
            }
            GoalStatus::Blocked => {
                if dynamic.salience > 0 {
                    events.push(VolitionEvent::GoalDecayed {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
            }
            GoalStatus::Proposed | GoalStatus::Satisfied | GoalStatus::Retired => {}
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, EvidenceRef, GoalScope, GoalStatus, InitiativeOutput, ProposedGoalCandidate,
        propose_goal_candidates, static_fixture,
    };

    // ── Test helpers ────────────────────────────────────────────────────────

    fn make_candidate(id: &str) -> ProposedGoalCandidate {
        let evidence = EvidenceRef::try_new(format!("open-question: {id}")).unwrap();
        ProposedGoalCandidate::try_new(
            id.to_string(),
            format!("Title {id}"),
            format!("Summary for {id}"),
            vec![],
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied when resolved.".to_string(),
            vec![evidence],
            format!("source: {id}"),
            vec![],
        )
        .unwrap()
    }

    // ── Reducer determinism ─────────────────────────────────────────────────

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

    // ── Goal lifecycle reducers ─────────────────────────────────────────────

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
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        let events = tick_events(&state, &fixture, RETIREMENT_INACTIVITY_TICKS);

        assert!(events.iter().any(|event| matches!(
            event,
            VolitionEvent::GoalRetired { goal_id: id, .. } if id == goal_id
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

    #[test]
    fn goal_candidate_added_appends_to_pending_candidates() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-1");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );

        assert_eq!(state.pending_candidates.len(), 1);
        assert_eq!(state.pending_candidates[0].id(), "cand-1");
    }

    #[test]
    fn goal_candidate_added_does_not_auto_accept() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-1");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );

        assert!(!state.accepted_candidates.contains_key("cand-1"));
    }

    #[test]
    fn goal_candidate_accepted_moves_candidate_to_accepted() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-accept");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("experiment: confirmed useful").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "cand-accept".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            !state
                .pending_candidates
                .iter()
                .any(|c| c.id() == "cand-accept")
        );
        assert!(state.accepted_candidates.contains_key("cand-accept"));
    }

    #[test]
    fn goal_candidate_accepted_without_prior_add_is_noop() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let acceptance_evidence = EvidenceRef::try_new("experiment: confirmed").unwrap();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "nonexistent".to_string(),
                acceptance_evidence,
                tick: 1,
            },
        );

        assert!(!state.accepted_candidates.contains_key("nonexistent"));
    }

    #[test]
    fn goal_candidate_rejected_removes_from_pending() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-reject");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let state = apply(
            state,
            VolitionEvent::GoalCandidateRejected {
                goal_id: "cand-reject".to_string(),
                reason: "Not relevant enough.".to_string(),
                tick: 2,
            },
        );

        assert!(
            !state
                .pending_candidates
                .iter()
                .any(|c| c.id() == "cand-reject")
        );
        assert!(!state.accepted_candidates.contains_key("cand-reject"));
    }

    #[test]
    fn remaining_candidate_stays_in_pending_across_tick() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("cand-stay");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let state = apply(state, VolitionEvent::TickAdvanced { tick: 2 });

        assert_eq!(
            state
                .pending_candidates
                .iter()
                .filter(|c| c.id() == "cand-stay")
                .count(),
            1
        );
        assert!(!state.accepted_candidates.contains_key("cand-stay"));
    }

    // ── Accepted-candidate selector wiring (reducer side) ───────────────────

    #[test]
    fn accepted_candidate_goal_data_in_accepted_candidates_dynamic_state_in_goals() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("new-cand");

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        let acceptance_evidence = EvidenceRef::try_new("trace-abc").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "new-cand".to_string(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            state.accepted_candidates.contains_key("new-cand"),
            "goal data must be in accepted_candidates"
        );
        assert!(
            state.goals.contains_key("new-cand"),
            "dynamic state must be in state.goals for selector and lifecycle wiring"
        );
        assert!(
            !fixture.goals.iter().any(|g| g.id == "new-cand"),
            "accepted candidate must not be in the static fixture"
        );
    }

    #[test]
    fn accepted_candidate_gets_goal_dynamic_state_on_acceptance() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: continuity accepted").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );

        assert!(
            state.goals.contains_key(&candidate_id),
            "accepted candidate must have a GoalDynamicState entry in state.goals"
        );
        let dynamic = state.goals.get(&candidate_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Accepted);
        assert_eq!(dynamic.salience, 0);
    }

    #[test]
    fn accepted_candidate_uses_same_dynamic_state_path_as_fixture_goals() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: accepted").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );
        // Apply an activation event — same reducer branch as fixture goals.
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: candidate_id.clone(),
                tick: 3,
            },
        );

        let dynamic = state.goals.get(&candidate_id).unwrap();
        assert_eq!(dynamic.status, GoalStatus::Active);
        assert_eq!(dynamic.salience, SALIENCE_ACTIVATION_BONUS);
    }

    // ── initiative_executed ──────────────────────────────────────────────────

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
}
