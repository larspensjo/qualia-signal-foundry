//! Pure derivation of display-only *functional signals* from recorded volition state.
//!
//! A functional signal is a named, evidence-derived readout of the recorded state — never a
//! felt state. Each signal points at the exact recorded state that justifies it (goal ids,
//! ticks, [`EvidenceRef`]s, declined-candidate conflicts) so a reader can verify it against the
//! state rather than trust a scripted phrase. Signals are recomputed on demand by
//! [`derive_signals`]; they are never stored on [`VolitionState`] or any other struct.
//!
//! This module is read-only over its inputs and has no side effects: [`derive_signals`]
//! allocates only the returned `Vec`. It must have no callers in arbitration, salience,
//! selection, initiative, or context-injection code — it is a display/trace concern.

use serde::{Deserialize, Serialize};

use crate::{DeclineReason, EvidenceRef, GoalStatus, VolitionFixture, VolitionState};

// ── Threshold constants ──────────────────────────────────────────────────────
//
// Named, discoverable thresholds that gate each signal. Kept here (not in `reducer.rs`) so the
// derivation module is self-contained and `reducer.rs` never depends on `signals.rs`; they are
// the signal-layer analogue of the salience constants declared alongside the reducer
// (`SALIENCE_ACTIVATION_BONUS`, `SALIENCE_DECAY_PER_TICK`, … in `reducer.rs`), which govern the
// salience values these thresholds are compared against.

/// Minimum `blocked_count` a Blocked goal must reach before it raises `frustration`. A single
/// block is ordinary friction; repeated blocks despite activation are what the signal names.
pub const FRUSTRATION_BLOCKED_COUNT_THRESHOLD: u32 = 2;

/// Recency window, in ticks: a `last_satisfied_tick` within this many ticks of `state.tick`
/// still raises `satisfaction`. Older satisfactions have faded and emit nothing.
pub const SATISFACTION_RECENCY_WINDOW_TICKS: u64 = 5;

/// Salience strictly below which a goal counts as disengaged. `boredom` requires *every*
/// non-retired goal to sit below this. Set below one activation bonus
/// (`SALIENCE_ACTIVATION_BONUS` in `reducer.rs`, currently 10) so a freshly activated goal is
/// engaged and suppresses boredom until it decays.
pub const BOREDOM_SALIENCE_THRESHOLD: i32 = 5;

/// Minimum elapsed tick that, on its own, admits `boredom` even with no prior activation. Below
/// this tick a fresh, never-active session is a cold start, not boredom, and emits nothing.
pub const BOREDOM_MIN_ELAPSED_TICKS: u64 = 3;

// ── Public signal types ──────────────────────────────────────────────────────

/// The four functional-signal kinds. Exactly these; there is deliberately no `tension` kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    CoherenceDecline,
    Frustration,
    Satisfaction,
    Boredom,
}

/// A `(goal id, salience)` pair recorded in boredom evidence, so a reader can see exactly which
/// goals were inspected and how far below the threshold each sat.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalSalience {
    pub goal_id: String,
    pub salience: i32,
}

/// Which guard admitted a `boredom` signal past the cold-start check.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoredomGuard {
    /// At least one goal had a prior activation (`last_activated_tick.is_some()`).
    PriorActivation,
    /// No prior activation, but `state.tick >= BOREDOM_MIN_ELAPSED_TICKS`.
    ElapsedTicks,
}

/// Structured, per-kind evidence naming the recorded state that justifies a signal. Every
/// variant's fields resolve to state actually present at derivation time (goal ids exist in
/// `state.goals`; ticks and refs are copied from that state), and no variant is empty. Serde
/// externally tagged (`{"frustration": { … }}`) so trace records and the websocket capture in
/// later tasks carry a self-describing shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalEvidence {
    /// A candidate the coherence engine declined this run.
    CoherenceDecline {
        candidate_title: String,
        conflict: DeclineReason,
        rationale: String,
        tick: u64,
    },
    /// A goal blocked repeatedly despite having been activated.
    Frustration {
        goal_id: String,
        blocked_count: u32,
        last_blocked_tick: u64,
        last_activated_tick: u64,
    },
    /// A goal satisfied recently, with the satisfaction evidence that closed it.
    Satisfaction {
        goal_id: String,
        last_satisfied_tick: u64,
        evidence_ref: EvidenceRef,
    },
    /// The whole non-retired goal set sitting below the engagement threshold.
    Boredom {
        inspected: Vec<GoalSalience>,
        threshold: i32,
        guard: BoredomGuard,
    },
}

impl SignalEvidence {
    /// The [`SignalKind`] this evidence justifies. Used to keep `FunctionalSignal.kind` and
    /// `FunctionalSignal.evidence` always in agreement.
    fn kind(&self) -> SignalKind {
        match self {
            Self::CoherenceDecline { .. } => SignalKind::CoherenceDecline,
            Self::Frustration { .. } => SignalKind::Frustration,
            Self::Satisfaction { .. } => SignalKind::Satisfaction,
            Self::Boredom { .. } => SignalKind::Boredom,
        }
    }
}

/// One derived functional signal: its `kind`, a display `intensity` in `[0.0, 1.0]`, and the
/// structured `evidence` that justifies it. Construct only via [`derive_signals`], which keeps
/// `kind` consistent with `evidence` and clamps `intensity`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FunctionalSignal {
    pub kind: SignalKind,
    pub intensity: f64,
    pub evidence: SignalEvidence,
}

impl FunctionalSignal {
    /// Build a signal from its evidence, deriving `kind` from the evidence and clamping
    /// `intensity` into `[0.0, 1.0]`.
    fn from_evidence(evidence: SignalEvidence, intensity: f64) -> Self {
        Self {
            kind: evidence.kind(),
            intensity: clamp01(intensity),
            evidence,
        }
    }
}

/// Clamp an intensity into the display range `[0.0, 1.0]`.
fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

// ── Derivation ───────────────────────────────────────────────────────────────

/// Derive the display-only functional signals implied by `state`. Pure, deterministic, and
/// read-only over its arguments: aside from allocating the returned `Vec` it has no side
/// effects and stores nothing.
///
/// Output ordering is deterministic — grouped by kind in the order coherence-decline,
/// frustration, satisfaction, boredom, and within each group by a stable secondary key
/// (declined candidates by `(tick, title)`; goals by goal id via the `BTreeMap` iteration
/// order). Identical `state` + `fixture` therefore always yield an identical `Vec`, order
/// included.
///
/// `fixture` is accepted for API stability: it carries no signal-bearing dynamic state (that all
/// lives in `state`), but later consumers (the offline harness, the realtime capture) resolve
/// signals against the same `(state, fixture)` pair.
pub fn derive_signals(state: &VolitionState, _fixture: &VolitionFixture) -> Vec<FunctionalSignal> {
    let mut signals = Vec::new();
    coherence_decline_signals(state, &mut signals);
    frustration_signals(state, &mut signals);
    satisfaction_signals(state, &mut signals);
    boredom_signal(state, &mut signals);
    signals
}

/// One `coherence_decline` per recorded declined candidate, emitted in `(tick, title)` order.
///
/// Intensity uses recency: `1 / (1 + age)` where `age = state.tick - decline.tick`. A decline
/// recorded this tick reads `1.0`; intensity decays harmonically with age (`0.5`, `0.33`,
/// `0.25`, ...) toward zero without reaching it, so an old-but-still-remembered decline stays
/// faintly present.
fn coherence_decline_signals(state: &VolitionState, out: &mut Vec<FunctionalSignal>) {
    let mut declined: Vec<&crate::DeclinedCandidate> = state.declined_candidates.iter().collect();
    declined.sort_by(|a, b| a.tick.cmp(&b.tick).then_with(|| a.title.cmp(&b.title)));
    for decline in declined {
        let age = state.tick.saturating_sub(decline.tick) as f64;
        let intensity = 1.0 / (1.0 + age);
        out.push(FunctionalSignal::from_evidence(
            SignalEvidence::CoherenceDecline {
                candidate_title: decline.title.clone(),
                conflict: decline.conflict.clone(),
                rationale: decline.rationale.clone(),
                tick: decline.tick,
            },
            intensity,
        ));
    }
}

/// `frustration` for every currently-Blocked goal blocked at least
/// `FRUSTRATION_BLOCKED_COUNT_THRESHOLD` times despite a prior activation. Goals are visited in
/// `state.goals` (`BTreeMap`, goal-id order), so output is deterministic.
///
/// Intensity scales with `blocked_count` relative to the threshold, saturating at twice the
/// threshold: `min(1.0, blocked_count / (2 * threshold))`. At the threshold it reads a partial
/// value and climbs with each further block up to `1.0`.
fn frustration_signals(state: &VolitionState, out: &mut Vec<FunctionalSignal>) {
    for (goal_id, dynamic) in &state.goals {
        if dynamic.status != GoalStatus::Blocked
            || dynamic.blocked_count < FRUSTRATION_BLOCKED_COUNT_THRESHOLD
        {
            continue;
        }
        // blocked_count >= threshold (>= 1) guarantees a recorded block; a Blocked goal blocked
        // "despite activation" further requires a prior activation. Both must resolve to a tick.
        let (Some(last_blocked_tick), Some(last_activated_tick)) =
            (dynamic.last_blocked_tick, dynamic.last_activated_tick)
        else {
            continue;
        };
        let intensity =
            dynamic.blocked_count as f64 / (2.0 * FRUSTRATION_BLOCKED_COUNT_THRESHOLD as f64);
        out.push(FunctionalSignal::from_evidence(
            SignalEvidence::Frustration {
                goal_id: goal_id.clone(),
                blocked_count: dynamic.blocked_count,
                last_blocked_tick,
                last_activated_tick,
            },
            intensity,
        ));
    }
}

/// `satisfaction` for every goal satisfied within `SATISFACTION_RECENCY_WINDOW_TICKS` of
/// `state.tick` that carries its satisfaction evidence. Goal-id order via `state.goals`.
///
/// Intensity decays linearly with age across the window: `1 - age / (window + 1)` where
/// `age = state.tick - last_satisfied_tick`. Freshest reads `1.0`; the oldest still-in-window
/// satisfaction stays strictly positive.
fn satisfaction_signals(state: &VolitionState, out: &mut Vec<FunctionalSignal>) {
    for (goal_id, dynamic) in &state.goals {
        let Some(last_satisfied_tick) = dynamic.last_satisfied_tick else {
            continue;
        };
        let Some(evidence_ref) = dynamic.last_satisfied_evidence_ref.as_ref() else {
            continue;
        };
        let age = state.tick.saturating_sub(last_satisfied_tick);
        if age > SATISFACTION_RECENCY_WINDOW_TICKS {
            continue;
        }
        let intensity = 1.0 - age as f64 / (SATISFACTION_RECENCY_WINDOW_TICKS as f64 + 1.0);
        out.push(FunctionalSignal::from_evidence(
            SignalEvidence::Satisfaction {
                goal_id: goal_id.clone(),
                last_satisfied_tick,
                evidence_ref: evidence_ref.clone(),
            },
            intensity,
        ));
    }
}

/// At most one `boredom` signal: every non-retired goal below `BOREDOM_SALIENCE_THRESHOLD`, past
/// the cold-start guard. Admitted when at least one goal has a prior activation
/// (`BoredomGuard::PriorActivation`, preferred) or `state.tick >= BOREDOM_MIN_ELAPSED_TICKS`
/// (`BoredomGuard::ElapsedTicks`); a fresh, never-active, low-tick session emits nothing.
///
/// Intensity scales with how far below the threshold the *maximum* non-retired salience sits:
/// `(threshold - max_salience) / threshold`. All-idle (max 0) reads `1.0`.
fn boredom_signal(state: &VolitionState, out: &mut Vec<FunctionalSignal>) {
    let mut inspected = Vec::new();
    let mut any_activation = false;
    let mut max_salience = 0;
    for (goal_id, dynamic) in &state.goals {
        if dynamic.status == GoalStatus::Retired {
            continue;
        }
        if dynamic.salience >= BOREDOM_SALIENCE_THRESHOLD {
            // An engaged goal suppresses boredom outright.
            return;
        }
        any_activation |= dynamic.last_activated_tick.is_some();
        max_salience = max_salience.max(dynamic.salience);
        inspected.push(GoalSalience {
            goal_id: goal_id.clone(),
            salience: dynamic.salience,
        });
    }

    // No non-retired goals: nothing to be bored about, and evidence would be empty.
    if inspected.is_empty() {
        return;
    }

    let guard = if any_activation {
        BoredomGuard::PriorActivation
    } else if state.tick >= BOREDOM_MIN_ELAPSED_TICKS {
        BoredomGuard::ElapsedTicks
    } else {
        // Cold start: no prior activity and too early to read as boredom.
        return;
    };

    let intensity =
        (BOREDOM_SALIENCE_THRESHOLD - max_salience) as f64 / BOREDOM_SALIENCE_THRESHOLD as f64;
    out.push(FunctionalSignal::from_evidence(
        SignalEvidence::Boredom {
            inspected,
            threshold: BOREDOM_SALIENCE_THRESHOLD,
            guard,
        },
        intensity,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllowedEffect, DeclineReason};
    use crate::{
        CoherenceDecline, EvidenceRef, GoalScope, GoalStatus, ProposedGoalCandidate, VolitionEvent,
        VolitionState, apply, static_fixture,
    };

    const CLARIFY: &str = "clarify-weak-evidence-topic";
    const AVOID: &str = "avoid-overstating-impl-status";

    fn fresh() -> (VolitionFixture, VolitionState) {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        (fixture, state)
    }

    fn evidence(text: &str) -> EvidenceRef {
        EvidenceRef::try_new(text).unwrap()
    }

    fn signals_of(
        state: &VolitionState,
        fixture: &VolitionFixture,
        kind: SignalKind,
    ) -> Vec<FunctionalSignal> {
        derive_signals(state, fixture)
            .into_iter()
            .filter(|s| s.kind == kind)
            .collect()
    }

    /// Adds a candidate then rejects it with a coherence decline, mirroring the reducer path
    /// that populates `declined_candidates`.
    fn reject_with_decline(
        state: VolitionState,
        candidate_id: &str,
        title: &str,
        conflict: DeclineReason,
        rationale: &str,
        tick: u64,
    ) -> VolitionState {
        let candidate = ProposedGoalCandidate::try_new(
            candidate_id.to_string(),
            title.to_string(),
            format!("Summary for {candidate_id}"),
            vec![],
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied when resolved.".to_string(),
            vec![evidence(&format!("open-question: {candidate_id}"))],
            format!("source: {candidate_id}"),
            vec![],
        )
        .unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate,
                tick: tick.saturating_sub(1),
            },
        );
        apply(
            state,
            VolitionEvent::GoalCandidateRejected {
                goal_id: candidate_id.to_string(),
                reason: "coherence check rejected".to_string(),
                coherence_decline: Some(CoherenceDecline {
                    conflict,
                    rationale: rationale.to_string(),
                }),
                tick,
            },
        )
    }

    // ── coherence_decline ────────────────────────────────────────────────────

    #[test]
    fn coherence_decline_absent_with_empty_declined_candidates() {
        let (fixture, state) = fresh();
        assert!(signals_of(&state, &fixture, SignalKind::CoherenceDecline).is_empty());
    }

    #[test]
    fn coherence_decline_one_signal_per_declined_candidate() {
        let (fixture, state) = fresh();
        let state = reject_with_decline(
            state,
            "cand-a",
            "Idea A",
            DeclineReason::ConflictingGoal {
                goal_id: "core-goal".to_string(),
            },
            "conflicts with a more fundamental goal",
            2,
        );
        let state = reject_with_decline(
            state,
            "cand-b",
            "Idea B",
            DeclineReason::ProtectedFloor,
            "would breach the protected floor",
            3,
        );

        let signals = signals_of(&state, &fixture, SignalKind::CoherenceDecline);
        assert_eq!(signals.len(), 2);

        // Evidence resolves to the recorded declined candidates.
        let titles: Vec<&str> = signals
            .iter()
            .map(|s| match &s.evidence {
                SignalEvidence::CoherenceDecline {
                    candidate_title, ..
                } => candidate_title.as_str(),
                other => panic!("unexpected evidence {other:?}"),
            })
            .collect();
        assert!(titles.contains(&"Idea A"));
        assert!(titles.contains(&"Idea B"));

        for s in &signals {
            if let SignalEvidence::CoherenceDecline {
                candidate_title,
                rationale,
                ..
            } = &s.evidence
            {
                assert!(!candidate_title.is_empty());
                assert!(!rationale.is_empty());
                assert!(
                    state
                        .declined_candidates
                        .iter()
                        .any(|d| &d.title == candidate_title)
                );
            }
        }
    }

    // ── frustration ──────────────────────────────────────────────────────────

    fn activate_then_block(state: VolitionState, goal_id: &str, blocks: u64) -> VolitionState {
        let mut state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        for tick in 2..(2 + blocks) {
            state = apply(
                state,
                VolitionEvent::GoalBlocked {
                    goal_id: goal_id.to_string(),
                    tick,
                },
            );
        }
        state
    }

    #[test]
    fn frustration_present_when_blocked_past_threshold_after_activation() {
        let (fixture, state) = fresh();
        let state = activate_then_block(state, CLARIFY, FRUSTRATION_BLOCKED_COUNT_THRESHOLD as u64);

        let signals = signals_of(&state, &fixture, SignalKind::Frustration);
        assert_eq!(signals.len(), 1);
        match &signals[0].evidence {
            SignalEvidence::Frustration {
                goal_id,
                blocked_count,
                last_blocked_tick,
                last_activated_tick,
            } => {
                assert_eq!(goal_id, CLARIFY);
                assert!(state.goals.contains_key(goal_id));
                assert_eq!(*blocked_count, FRUSTRATION_BLOCKED_COUNT_THRESHOLD);
                let dynamic = state.goal(CLARIFY).unwrap();
                assert_eq!(Some(*last_blocked_tick), dynamic.last_blocked_tick);
                assert_eq!(Some(*last_activated_tick), dynamic.last_activated_tick);
            }
            other => panic!("unexpected evidence {other:?}"),
        }
    }

    #[test]
    fn frustration_absent_when_blocked_count_below_threshold() {
        let (fixture, state) = fresh();
        // One block only, still below the threshold of 2.
        let state = activate_then_block(state, CLARIFY, 1);
        assert!(signals_of(&state, &fixture, SignalKind::Frustration).is_empty());
    }

    #[test]
    fn frustration_absent_when_never_activated() {
        let (fixture, mut state) = fresh();
        for tick in 1..=3 {
            state = apply(
                state,
                VolitionEvent::GoalBlocked {
                    goal_id: CLARIFY.to_string(),
                    tick,
                },
            );
        }
        // Blocked well past threshold, but never activated → no frustration.
        assert!(state.goal(CLARIFY).unwrap().last_activated_tick.is_none());
        assert!(signals_of(&state, &fixture, SignalKind::Frustration).is_empty());
    }

    #[test]
    fn frustration_absent_when_status_not_blocked() {
        let (fixture, state) = fresh();
        let state = activate_then_block(state, CLARIFY, 2);
        // Re-activate: status leaves Blocked while blocked_count stays at 2.
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: CLARIFY.to_string(),
                tick: 5,
            },
        );
        assert_eq!(state.goal(CLARIFY).unwrap().status, GoalStatus::Active);
        assert_eq!(state.goal(CLARIFY).unwrap().blocked_count, 2);
        assert!(signals_of(&state, &fixture, SignalKind::Frustration).is_empty());
    }

    #[test]
    fn frustration_intensity_grows_with_blocked_count() {
        let (fixture, base) = fresh();
        let two = activate_then_block(base.clone(), CLARIFY, 2);
        let three = activate_then_block(base, CLARIFY, 3);
        let i2 = signals_of(&two, &fixture, SignalKind::Frustration)[0].intensity;
        let i3 = signals_of(&three, &fixture, SignalKind::Frustration)[0].intensity;
        assert!(i3 > i2, "more blocks should read hotter: {i3} !> {i2}");
        assert!((0.0..=1.0).contains(&i2) && (0.0..=1.0).contains(&i3));
    }

    // ── satisfaction ─────────────────────────────────────────────────────────

    fn activate_then_satisfy(
        state: VolitionState,
        goal_id: &str,
        satisfied_tick: u64,
        ev: &str,
    ) -> VolitionState {
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: goal_id.to_string(),
                evidence: evidence(ev),
                tick: satisfied_tick,
            },
        )
    }

    #[test]
    fn satisfaction_present_within_recency_window() {
        let (fixture, state) = fresh();
        let state = activate_then_satisfy(state, CLARIFY, 2, "trace: satisfied");

        let signals = signals_of(&state, &fixture, SignalKind::Satisfaction);
        assert_eq!(signals.len(), 1);
        match &signals[0].evidence {
            SignalEvidence::Satisfaction {
                goal_id,
                last_satisfied_tick,
                evidence_ref,
            } => {
                assert_eq!(goal_id, CLARIFY);
                assert!(state.goals.contains_key(goal_id));
                let dynamic = state.goal(CLARIFY).unwrap();
                assert_eq!(Some(*last_satisfied_tick), dynamic.last_satisfied_tick);
                assert_eq!(
                    Some(evidence_ref),
                    dynamic.last_satisfied_evidence_ref.as_ref()
                );
            }
            other => panic!("unexpected evidence {other:?}"),
        }
    }

    #[test]
    fn satisfaction_absent_outside_recency_window() {
        let (fixture, state) = fresh();
        let state = activate_then_satisfy(state, CLARIFY, 2, "trace: satisfied");
        // Advance the clock past the recency window.
        let state = apply(
            state,
            VolitionEvent::TickAdvanced {
                tick: 2 + SATISFACTION_RECENCY_WINDOW_TICKS + 1,
            },
        );
        assert!(signals_of(&state, &fixture, SignalKind::Satisfaction).is_empty());
    }

    #[test]
    fn satisfaction_absent_with_only_progress_evidence() {
        let (fixture, state) = fresh();
        let state = apply(
            state,
            VolitionEvent::GoalProgressObserved {
                goal_id: CLARIFY.to_string(),
                evidence: evidence("trace: progress"),
                tick: 1,
            },
        );
        // Progress evidence does not populate the satisfaction-only fields.
        assert!(state.goal(CLARIFY).unwrap().last_satisfied_tick.is_none());
        assert!(signals_of(&state, &fixture, SignalKind::Satisfaction).is_empty());
    }

    #[test]
    fn satisfaction_intensity_decays_with_age() {
        let (fixture, state) = fresh();
        let fresh_sat = activate_then_satisfy(state, CLARIFY, 2, "trace: satisfied");
        let aged = apply(
            fresh_sat.clone(),
            VolitionEvent::TickAdvanced {
                tick: 2 + SATISFACTION_RECENCY_WINDOW_TICKS,
            },
        );
        let i_fresh = signals_of(&fresh_sat, &fixture, SignalKind::Satisfaction)[0].intensity;
        let i_aged = signals_of(&aged, &fixture, SignalKind::Satisfaction)[0].intensity;
        assert!(i_fresh > i_aged, "fresher satisfaction should read hotter");
    }

    // ── boredom ──────────────────────────────────────────────────────────────

    #[test]
    fn boredom_absent_on_cold_start() {
        let (fixture, state) = fresh();
        // tick 0, all salience 0, no activation.
        assert!(signals_of(&state, &fixture, SignalKind::Boredom).is_empty());
    }

    #[test]
    fn boredom_absent_when_a_goal_is_at_or_above_threshold() {
        let (fixture, state) = fresh();
        // Activation lifts salience to the activation bonus (>= threshold): an engaged goal.
        let mut state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: CLARIFY.to_string(),
                tick: 1,
            },
        );
        // Advance well past the elapsed guard so only the engaged goal blocks boredom.
        state = apply(
            state,
            VolitionEvent::TickAdvanced {
                tick: BOREDOM_MIN_ELAPSED_TICKS + 5,
            },
        );
        assert!(state.goal(CLARIFY).unwrap().salience >= BOREDOM_SALIENCE_THRESHOLD);
        assert!(signals_of(&state, &fixture, SignalKind::Boredom).is_empty());
    }

    #[test]
    fn boredom_present_via_activation_guard() {
        let (fixture, state) = fresh();
        // Activate then satisfy: salience returns to 0, but last_activated_tick stays set. Tick
        // is kept below the elapsed guard so only the activation guard can admit boredom.
        let state = activate_then_satisfy(state, CLARIFY, 2, "trace: satisfied");
        assert!(state.tick < BOREDOM_MIN_ELAPSED_TICKS);

        let signals = signals_of(&state, &fixture, SignalKind::Boredom);
        assert_eq!(signals.len(), 1);
        match &signals[0].evidence {
            SignalEvidence::Boredom {
                inspected,
                threshold,
                guard,
            } => {
                assert_eq!(*guard, BoredomGuard::PriorActivation);
                assert_eq!(*threshold, BOREDOM_SALIENCE_THRESHOLD);
                assert!(!inspected.is_empty());
                for gs in inspected {
                    assert!(state.goals.contains_key(&gs.goal_id));
                    assert!(gs.salience < BOREDOM_SALIENCE_THRESHOLD);
                }
            }
            other => panic!("unexpected evidence {other:?}"),
        }
    }

    #[test]
    fn boredom_present_via_elapsed_guard() {
        let (fixture, state) = fresh();
        // No activation anywhere; only the clock advances past the elapsed guard.
        let state = apply(
            state,
            VolitionEvent::TickAdvanced {
                tick: BOREDOM_MIN_ELAPSED_TICKS,
            },
        );
        assert!(
            state
                .goals
                .values()
                .all(|g| g.last_activated_tick.is_none())
        );

        let signals = signals_of(&state, &fixture, SignalKind::Boredom);
        assert_eq!(signals.len(), 1);
        match &signals[0].evidence {
            SignalEvidence::Boredom { guard, .. } => {
                assert_eq!(*guard, BoredomGuard::ElapsedTicks);
            }
            other => panic!("unexpected evidence {other:?}"),
        }
    }

    #[test]
    fn boredom_emitted_at_most_once() {
        let (fixture, state) = fresh();
        let state = apply(
            state,
            VolitionEvent::TickAdvanced {
                tick: BOREDOM_MIN_ELAPSED_TICKS + 2,
            },
        );
        assert_eq!(
            signals_of(&state, &fixture, SignalKind::Boredom).len(),
            1,
            "boredom is a whole-state signal, emitted at most once"
        );
    }

    #[test]
    fn boredom_intensity_is_full_when_all_salience_zero() {
        let (fixture, state) = fresh();
        let state = apply(
            state,
            VolitionEvent::TickAdvanced {
                tick: BOREDOM_MIN_ELAPSED_TICKS,
            },
        );
        let signals = signals_of(&state, &fixture, SignalKind::Boredom);
        assert_eq!(signals[0].intensity, 1.0);
    }

    // ── cross-cutting ────────────────────────────────────────────────────────

    /// Build a state that exercises all four signals at once.
    fn rich_state() -> (VolitionFixture, VolitionState) {
        let (fixture, state) = fresh();
        // coherence_decline
        let state = reject_with_decline(
            state,
            "cand-x",
            "Declined idea X",
            DeclineReason::ProtectedFloor,
            "breaches the protected floor",
            2,
        );
        // frustration on CLARIFY
        let state = activate_then_block(state, CLARIFY, 2);
        // satisfaction on AVOID (also gives it a prior activation for the boredom guard)
        let state = activate_then_satisfy(state, AVOID, 3, "trace: avoid satisfied");
        (fixture, state)
    }

    #[test]
    fn all_signal_evidence_resolves_to_present_state() {
        let (fixture, state) = rich_state();
        let signals = derive_signals(&state, &fixture);
        assert!(
            signals
                .iter()
                .any(|s| s.kind == SignalKind::CoherenceDecline)
        );
        assert!(signals.iter().any(|s| s.kind == SignalKind::Frustration));
        assert!(signals.iter().any(|s| s.kind == SignalKind::Satisfaction));

        for signal in &signals {
            assert!((0.0..=1.0).contains(&signal.intensity));
            match &signal.evidence {
                SignalEvidence::CoherenceDecline {
                    candidate_title,
                    rationale,
                    ..
                } => {
                    assert!(!candidate_title.is_empty());
                    assert!(!rationale.is_empty());
                    assert!(
                        state
                            .declined_candidates
                            .iter()
                            .any(|d| &d.title == candidate_title)
                    );
                }
                SignalEvidence::Frustration {
                    goal_id,
                    blocked_count,
                    last_blocked_tick,
                    last_activated_tick,
                } => {
                    let dynamic = state.goal(goal_id).expect("frustration goal must exist");
                    assert_eq!(dynamic.blocked_count, *blocked_count);
                    assert_eq!(dynamic.last_blocked_tick, Some(*last_blocked_tick));
                    assert_eq!(dynamic.last_activated_tick, Some(*last_activated_tick));
                }
                SignalEvidence::Satisfaction {
                    goal_id,
                    last_satisfied_tick,
                    evidence_ref,
                } => {
                    let dynamic = state.goal(goal_id).expect("satisfaction goal must exist");
                    assert_eq!(dynamic.last_satisfied_tick, Some(*last_satisfied_tick));
                    assert_eq!(
                        dynamic.last_satisfied_evidence_ref.as_ref(),
                        Some(evidence_ref)
                    );
                }
                SignalEvidence::Boredom {
                    inspected,
                    threshold,
                    ..
                } => {
                    assert_eq!(*threshold, BOREDOM_SALIENCE_THRESHOLD);
                    assert!(!inspected.is_empty());
                    for gs in inspected {
                        let dynamic = state.goal(&gs.goal_id).expect("inspected goal must exist");
                        assert_eq!(dynamic.salience, gs.salience);
                    }
                }
            }
        }
    }

    #[test]
    fn derive_signals_is_deterministic_including_order() {
        let (fixture, state) = rich_state();
        assert_eq!(
            derive_signals(&state, &fixture),
            derive_signals(&state, &fixture)
        );
    }

    #[test]
    fn signals_are_grouped_by_kind_in_canonical_order() {
        let (fixture, state) = rich_state();
        let kinds: Vec<SignalKind> = derive_signals(&state, &fixture)
            .into_iter()
            .map(|s| s.kind)
            .collect();
        // Grouping order: coherence_decline, frustration, satisfaction, boredom.
        let rank = |k: &SignalKind| match k {
            SignalKind::CoherenceDecline => 0,
            SignalKind::Frustration => 1,
            SignalKind::Satisfaction => 2,
            SignalKind::Boredom => 3,
        };
        let mut sorted = kinds.clone();
        sorted.sort_by_key(rank);
        assert_eq!(kinds, sorted, "signals must be grouped by kind in order");
    }

    #[test]
    fn signal_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(SignalKind::CoherenceDecline).unwrap(),
            serde_json::json!("coherence_decline")
        );
        assert_eq!(
            serde_json::to_value(SignalKind::Frustration).unwrap(),
            serde_json::json!("frustration")
        );
        assert_eq!(
            serde_json::to_value(SignalKind::Satisfaction).unwrap(),
            serde_json::json!("satisfaction")
        );
        assert_eq!(
            serde_json::to_value(SignalKind::Boredom).unwrap(),
            serde_json::json!("boredom")
        );
    }

    #[test]
    fn functional_signal_roundtrips_through_serde() {
        let (fixture, state) = rich_state();
        for signal in derive_signals(&state, &fixture) {
            let json = serde_json::to_string(&signal).unwrap();
            let back: FunctionalSignal = serde_json::from_str(&json).unwrap();
            assert_eq!(signal, back);
        }
    }
}
