//! Pure derivation of which **subconscious** goals are *forced surfaced* this run, from recorded
//! volition state alone.
//!
//! Visibility ([`GoalVisibility`]) is a presentation/surfacing filter, never a runtime input: it
//! does not change [`select_goals_ranked`](crate::select_goals_ranked),
//! [`arbitrate_with_mode`](crate::arbitrate_with_mode), salience, or coherence. A `Subconscious`
//! goal biases behavior identically to a `Conscious` one but is narrated only on introspection or
//! when its behavior forces an explanation. This module derives that forced-surfacing set on
//! demand and stores nothing, mirroring [`crate::signals`].
//!
//! Two forcing conditions, each grounded in recorded facts:
//! - **rendered initiative** — the goal has a recorded *rendered* initiative
//!   ([`GoalDynamicState::last_rendered_initiative_tick`]); a suppressed internal initiative
//!   (recorded only in `last_initiative_tick`) does **not** count, because hiding a suppressed
//!   line changes nothing model-visible.
//! - **coherence conflict** — the goal is named as the conflicting goal in a
//!   [`DeclinedCandidate`]; hiding it would make the coherence layer incoherent.
//!
//! The brief's third condition — "the user asks for introspection" — needs no derivation: calling
//! `inspect_volition_state` *is* the ask, so the introspection tool always reports subconscious
//! goals in its own labeled section.

use serde::{Deserialize, Serialize};

use crate::{
    DeclineReason, EvidenceRef, GoalVisibility, VolitionFixture, VolitionState, goal_visibility,
};

/// Which recorded fact forces a subconscious goal to surface this run.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForcingCondition {
    /// The goal rendered an initiative line into model-visible turn text.
    RenderedInitiative {
        tick: u64,
        rendered_ref: Option<EvidenceRef>,
    },
    /// The goal is named as the conflicting goal in a coherence decline.
    CoherenceConflict {
        candidate_id: String,
        candidate_title: String,
        tick: u64,
    },
}

impl ForcingCondition {
    /// The tick the forcing fact was recorded at, for trace ordering.
    pub fn tick(&self) -> u64 {
        match self {
            Self::RenderedInitiative { tick, .. } => *tick,
            Self::CoherenceConflict { tick, .. } => *tick,
        }
    }
}

/// One subconscious goal forced to surface, with the recorded condition that forces it. A goal
/// forced by both a rendered initiative *and* a coherence conflict yields one entry per condition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ForcedSurfacing {
    pub goal_id: String,
    pub condition: ForcingCondition,
}

/// Derive the forced-surfacing set: for every **subconscious** goal, each recorded forcing
/// condition it satisfies. Pure, deterministic, and read-only over its arguments — it allocates
/// only the returned `Vec` and stores nothing.
///
/// Ordering is deterministic: goals in `state.goals` (`BTreeMap`, goal-id order); within a goal,
/// rendered-initiative before coherence-conflict; coherence conflicts in `declined_candidates`
/// order. `Conscious` goals never appear — they are always narrated, so "forced" is meaningless
/// for them.
pub fn forced_surfaced_goals(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> Vec<ForcedSurfacing> {
    let mut forced = Vec::new();
    for goal_id in state.goals.keys() {
        if goal_visibility(goal_id, state, fixture) != GoalVisibility::Subconscious {
            continue;
        }
        let dynamic = &state.goals[goal_id];
        // Rendered initiative: only a *rendered* line counts. A suppressed internal initiative
        // leaves `last_rendered_initiative_tick` None despite `last_initiative_tick` being set.
        if let Some(tick) = dynamic.last_rendered_initiative_tick {
            forced.push(ForcedSurfacing {
                goal_id: goal_id.clone(),
                condition: ForcingCondition::RenderedInitiative {
                    tick,
                    rendered_ref: dynamic.last_rendered_initiative_ref.clone(),
                },
            });
        }
        // Coherence conflict: the goal is named as the conflicting goal in a decline record.
        for declined in &state.declined_candidates {
            if let DeclineReason::ConflictingGoal {
                goal_id: conflicting,
            } = &declined.conflict
            {
                if conflicting == goal_id {
                    forced.push(ForcedSurfacing {
                        goal_id: goal_id.clone(),
                        condition: ForcingCondition::CoherenceConflict {
                            candidate_id: declined.candidate_id.clone(),
                            candidate_title: declined.title.clone(),
                            tick: declined.tick,
                        },
                    });
                }
            }
        }
    }
    forced
}

/// Whether `goal_id` is forced surfaced this run (any condition). Convenience over
/// [`forced_surfaced_goals`] for callers that only need membership (the tools, the ambient
/// packet); derives the same recorded facts.
pub fn is_forced_surfaced(goal_id: &str, state: &VolitionState, fixture: &VolitionFixture) -> bool {
    forced_surfaced_goals(state, fixture)
        .iter()
        .any(|entry| entry.goal_id == goal_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, CoherenceDecline, EvidenceRef, GoalScope, GoalStatus, InitiativeOutput,
        ProposedGoalCandidate, VolitionEvent, apply, arbitrate_with_mode, realtime_seed_fixture,
        select_goals_ranked,
    };

    const SUBCONSCIOUS_GOAL: &str = "assemble-world-picture";
    const CONSCIOUS_GOAL: &str = "serve-the-present-person";

    fn seed() -> (VolitionFixture, VolitionState) {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        (fixture, state)
    }

    fn evidence(text: &str) -> EvidenceRef {
        EvidenceRef::try_new(text).unwrap()
    }

    fn reflect_output() -> InitiativeOutput {
        InitiativeOutput::ReflectionRequested {
            proposed_question: "How does this connect to the larger picture?".to_string(),
        }
    }

    fn execute_initiative(
        state: VolitionState,
        goal_id: &str,
        tick: u64,
        rendered_ref: Option<EvidenceRef>,
    ) -> VolitionState {
        apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: goal_id.to_string(),
                effect: AllowedEffect::Reflect,
                output: reflect_output(),
                rationale: "test".to_string(),
                tick,
                rendered_ref,
            },
        )
    }

    fn decline_conflicting_with(
        state: VolitionState,
        candidate_id: &str,
        title: &str,
        conflicting_goal_id: &str,
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
                    conflict: DeclineReason::ConflictingGoal {
                        goal_id: conflicting_goal_id.to_string(),
                    },
                    rationale: "would derail the background world picture".to_string(),
                }),
                tick,
            },
        )
    }

    // ── rendered-initiative forcing ──────────────────────────────────────────

    #[test]
    fn rendered_initiative_forces_a_subconscious_goal() {
        let (fixture, state) = seed();
        let state = execute_initiative(
            state,
            SUBCONSCIOUS_GOAL,
            2,
            Some(evidence(
                "exchange:1/diagnostic:realtime_bounded_initiative",
            )),
        );

        let forced = forced_surfaced_goals(&state, &fixture);
        assert_eq!(forced.len(), 1);
        assert_eq!(forced[0].goal_id, SUBCONSCIOUS_GOAL);
        match &forced[0].condition {
            ForcingCondition::RenderedInitiative { tick, rendered_ref } => {
                assert_eq!(*tick, 2);
                assert_eq!(
                    rendered_ref.as_ref(),
                    state
                        .goal(SUBCONSCIOUS_GOAL)
                        .unwrap()
                        .last_rendered_initiative_ref
                        .as_ref()
                );
            }
            other => panic!("unexpected condition {other:?}"),
        }
        assert!(is_forced_surfaced(SUBCONSCIOUS_GOAL, &state, &fixture));
    }

    #[test]
    fn suppressed_initiative_does_not_force_surface() {
        let (fixture, state) = seed();
        // Executed but suppressed: rendered_ref None, so only last_initiative_tick is set.
        let state = execute_initiative(state, SUBCONSCIOUS_GOAL, 2, None);
        assert!(
            state
                .goal(SUBCONSCIOUS_GOAL)
                .unwrap()
                .last_initiative_tick
                .is_some(),
            "the initiative did execute"
        );
        assert!(
            state
                .goal(SUBCONSCIOUS_GOAL)
                .unwrap()
                .last_rendered_initiative_tick
                .is_none(),
            "but it did not render"
        );
        assert!(forced_surfaced_goals(&state, &fixture).is_empty());
        assert!(!is_forced_surfaced(SUBCONSCIOUS_GOAL, &state, &fixture));
    }

    #[test]
    fn rendered_initiative_on_a_conscious_goal_is_not_forced_surfacing() {
        let (fixture, state) = seed();
        let state = execute_initiative(
            state,
            CONSCIOUS_GOAL,
            2,
            Some(evidence(
                "exchange:1/diagnostic:realtime_bounded_initiative",
            )),
        );
        // A conscious goal is always narrated, so "forced surfacing" does not apply to it.
        assert!(forced_surfaced_goals(&state, &fixture).is_empty());
    }

    // ── coherence-conflict forcing ───────────────────────────────────────────

    #[test]
    fn coherence_conflict_naming_a_subconscious_goal_forces_it() {
        let (fixture, state) = seed();
        let state = decline_conflicting_with(state, "cand-x", "A tangent", SUBCONSCIOUS_GOAL, 3);

        let forced = forced_surfaced_goals(&state, &fixture);
        assert_eq!(forced.len(), 1);
        assert_eq!(forced[0].goal_id, SUBCONSCIOUS_GOAL);
        match &forced[0].condition {
            ForcingCondition::CoherenceConflict {
                candidate_id,
                candidate_title,
                tick,
            } => {
                assert_eq!(candidate_id, "cand-x");
                assert_eq!(candidate_title, "A tangent");
                assert_eq!(*tick, 3);
            }
            other => panic!("unexpected condition {other:?}"),
        }
    }

    #[test]
    fn coherence_conflict_naming_a_conscious_goal_does_not_force_a_subconscious_one() {
        let (fixture, state) = seed();
        // Decline conflicts with a conscious goal; no subconscious goal is named.
        let state = decline_conflicting_with(state, "cand-y", "Another tangent", CONSCIOUS_GOAL, 3);
        assert!(forced_surfaced_goals(&state, &fixture).is_empty());
    }

    #[test]
    fn both_conditions_yield_one_entry_each() {
        let (fixture, state) = seed();
        let state = execute_initiative(
            state,
            SUBCONSCIOUS_GOAL,
            2,
            Some(evidence(
                "exchange:1/diagnostic:realtime_bounded_initiative",
            )),
        );
        let state = decline_conflicting_with(state, "cand-x", "A tangent", SUBCONSCIOUS_GOAL, 3);

        let forced = forced_surfaced_goals(&state, &fixture);
        assert_eq!(forced.len(), 2, "one entry per satisfied condition");
        assert!(
            forced
                .iter()
                .any(|f| matches!(f.condition, ForcingCondition::RenderedInitiative { .. }))
        );
        assert!(
            forced
                .iter()
                .any(|f| matches!(f.condition, ForcingCondition::CoherenceConflict { .. }))
        );
        // Rendered-initiative is ordered before coherence-conflict for a single goal.
        assert!(matches!(
            forced[0].condition,
            ForcingCondition::RenderedInitiative { .. }
        ));
    }

    #[test]
    fn no_forcing_conditions_on_a_fresh_seed_state() {
        let (fixture, state) = seed();
        assert!(forced_surfaced_goals(&state, &fixture).is_empty());
    }

    #[test]
    fn derivation_is_deterministic() {
        let (fixture, state) = seed();
        let state = execute_initiative(
            state,
            SUBCONSCIOUS_GOAL,
            2,
            Some(evidence(
                "exchange:1/diagnostic:realtime_bounded_initiative",
            )),
        );
        let state = decline_conflicting_with(state, "cand-x", "A tangent", SUBCONSCIOUS_GOAL, 3);
        assert_eq!(
            forced_surfaced_goals(&state, &fixture),
            forced_surfaced_goals(&state, &fixture)
        );
    }

    // ── the no-runtime-effect invariant ──────────────────────────────────────

    /// The core guarantee: visibility is a presentation filter, not a runtime input. Flipping the
    /// seed's subconscious goal to `Conscious` must leave `select_goals_ranked` and
    /// `arbitrate_with_mode` bit-identical.
    #[test]
    fn visibility_flip_does_not_change_selection_or_arbitration() {
        let subconscious_fixture = realtime_seed_fixture();
        let mut conscious_fixture = subconscious_fixture.clone();
        for goal in &mut conscious_fixture.goals {
            if goal.id == SUBCONSCIOUS_GOAL {
                assert_eq!(goal.visibility, GoalVisibility::Subconscious);
                goal.visibility = GoalVisibility::Conscious;
            }
        }

        let sub_state = VolitionState::from_fixture(&subconscious_fixture);
        let con_state = VolitionState::from_fixture(&conscious_fixture);

        // A query that activates the subconscious world-picture goal alongside others.
        for query in [
            "how does the world and its history change over time",
            "what is happening in society and politics",
            "how can you help me",
        ] {
            let sub_ranked = select_goals_ranked(query, &sub_state, &subconscious_fixture);
            let con_ranked = select_goals_ranked(query, &con_state, &conscious_fixture);
            assert_eq!(
                sub_ranked
                    .selected
                    .iter()
                    .map(|s| &s.goal.id)
                    .collect::<Vec<_>>(),
                con_ranked
                    .selected
                    .iter()
                    .map(|s| &s.goal.id)
                    .collect::<Vec<_>>(),
                "selection order changed under a visibility flip for query: {query}"
            );

            let sub_arb = arbitrate_with_mode(
                sub_ranked.selected.clone(),
                &subconscious_fixture,
                sub_state.mode,
            );
            let con_arb = arbitrate_with_mode(
                con_ranked.selected.clone(),
                &conscious_fixture,
                con_state.mode,
            );
            let sub_winner = sub_arb
                .as_ref()
                .and_then(|o| o.qualified.as_ref())
                .map(|q| q.winner.goal.id.clone());
            let con_winner = con_arb
                .as_ref()
                .and_then(|o| o.qualified.as_ref())
                .map(|q| q.winner.goal.id.clone());
            assert_eq!(
                sub_winner, con_winner,
                "arbitration winner changed under a visibility flip for query: {query}"
            );
        }
    }

    #[test]
    fn forced_surfacing_ignores_retired_goal_visibility_default_but_reads_definition() {
        // A subconscious goal that rendered stays forced even after status churn — forcing is a
        // recorded fact keyed on the goal definition's visibility, not the live status.
        let (fixture, state) = seed();
        let state = execute_initiative(
            state,
            SUBCONSCIOUS_GOAL,
            2,
            Some(evidence(
                "exchange:1/diagnostic:realtime_bounded_initiative",
            )),
        );
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: SUBCONSCIOUS_GOAL.to_string(),
                tick: 3,
            },
        );
        assert_eq!(
            state.goal(SUBCONSCIOUS_GOAL).unwrap().status,
            GoalStatus::Blocked
        );
        assert!(is_forced_surfaced(SUBCONSCIOUS_GOAL, &state, &fixture));
    }
}
