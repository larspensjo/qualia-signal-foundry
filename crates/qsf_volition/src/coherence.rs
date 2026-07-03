//! Goal coherence: a model *detects* contradictions between goals (`CoherenceVerdict`), and
//! these pure functions *resolve* the verdict deterministically into existing goal-lifecycle
//! events (`GoalCandidateAccepted` / `GoalCandidateRejected` / `GoalRetired`). No new event
//! types. See `docs/Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::arbitration::PROTECTED_TIER_FLOOR;
use crate::reducer::goal_effective_tier;
use crate::{
    CoherenceDecline, DeclineReason, DeclinedCandidate, EvidenceRef, ProposedGoalCandidate,
    VolitionEvent, VolitionFixture, VolitionState,
};

/// A single contradiction between two goals (by id), as judged by the model. Field order
/// carries no meaning; a contradiction between A and B is the same fact regardless of which
/// field holds which id.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Contradiction {
    pub goal_a: String,
    pub goal_b: String,
    pub rationale: String,
}

/// Identifies the model role and prompt version that produced a `CoherenceVerdict`. A
/// pre-judge floor rejection makes no model call, so it has no `CoherenceVerdict` at all.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CoherenceJudgeRef {
    pub model_role: String,
    pub prompt_version: String,
}

/// The model judge's contradiction verdict for one coherence check (admission or sweep).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CoherenceVerdict {
    pub contradictions: Vec<Contradiction>,
    pub judge_ref: CoherenceJudgeRef,
}

/// Deterministic resolution of an admission check. `RejectProtectedFloor` is the pre-judge
/// gate outcome (no model call, no contradictions consulted); `Judged` resolves a model
/// verdict.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdmissionResolution {
    RejectProtectedFloor,
    Judged {
        admitted: bool,
        rejected_by: Vec<String>,
        cancelled_goal_ids: Vec<String>,
        suppressed_cancellations_due_to_rejection: Vec<String>,
    },
}

/// Which deterministic rule broke an equal-tier sweep contradiction.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepTieBreak {
    GreaterActivationTick,
    GreaterGoalId,
}

/// One cancellation decided by a sweep. `tie_break` is `None` when tiers differed (no tie to
/// break).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SweepCancellation {
    pub cancelled_goal_id: String,
    pub conflicting_goal_id: String,
    pub tie_break: Option<SweepTieBreak>,
}

/// A contradiction between two protected-floor goals. Never auto-resolved — flagged for
/// human review as a trace-only record; fixture curation error, not a runtime state change.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FlaggedFloorContradiction {
    pub goal_a: String,
    pub goal_b: String,
}

/// Deterministic resolution of a whole-set sweep.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SweepResolution {
    pub cancellations: Vec<SweepCancellation>,
    pub flagged: Vec<FlaggedFloorContradiction>,
}

/// True when `candidate`'s effective tier (from `tension_ids`) is at or below the protected
/// floor. The hard tier-floor gate: callers must check this *before* consulting the model —
/// nothing at or below the floor may be judged, only rejected outright.
pub fn candidate_hard_tier_floor_rejected(
    candidate: &ProposedGoalCandidate,
    fixture: &VolitionFixture,
) -> bool {
    crate::effective_tier_from_tension_ids(candidate.tension_ids(), fixture) <= PROTECTED_TIER_FLOOR
}

/// Resolves the pre-judge hard tier-floor gate: rejects the candidate without any model
/// verdict and without cancelling anything.
pub fn resolve_protected_floor_rejection(
    candidate: &ProposedGoalCandidate,
    tick: u64,
) -> (AdmissionResolution, Vec<VolitionEvent>) {
    let events = vec![VolitionEvent::GoalCandidateRejected {
        goal_id: candidate.id().to_string(),
        reason: "candidate effective tier is at or below the protected floor".to_string(),
        coherence_decline: Some(CoherenceDecline {
            conflict: DeclineReason::ProtectedFloor,
            rationale: "the candidate's own effective tier is at or below the protected floor"
                .to_string(),
        }),
        tick,
    }];
    (AdmissionResolution::RejectProtectedFloor, events)
}

/// Resolves one formed candidate end to end: the hard tier-floor gate first, then (if it
/// passes) a judged admission against `verdict`. Emits `GoalCandidateAdded` followed by
/// whatever the gate/admission resolves to. Applying the returned events is sufficient to
/// update `VolitionState::declined_candidates` — callers no longer need to separately construct
/// a declined-candidate record (see `VolitionState::declined_candidates`).
pub fn resolve_formed_candidate(
    candidate: &ProposedGoalCandidate,
    verdict: &CoherenceVerdict,
    state: &VolitionState,
    fixture: &VolitionFixture,
    tick: u64,
) -> (Vec<VolitionEvent>, AdmissionResolution) {
    let mut events = vec![VolitionEvent::GoalCandidateAdded {
        candidate: candidate.clone(),
        tick,
    }];
    let (resolution, resolution_events) = if candidate_hard_tier_floor_rejected(candidate, fixture)
    {
        resolve_protected_floor_rejection(candidate, tick)
    } else {
        resolve_admission(candidate, verdict, state, fixture, tick)
    };
    events.extend(resolution_events);
    (events, resolution)
}

/// Returns the declined-candidate record newly introduced for `candidate_id` by applying a
/// formation resolution, if any. This compares candidate identity rather than window length so it
/// still works when `VolitionState::declined_candidates` is already at its retention cap and the
/// reducer drops the oldest entry while appending the new one.
pub fn newly_declined_candidate(
    before: &VolitionState,
    after: &VolitionState,
    candidate_id: &str,
) -> Option<DeclinedCandidate> {
    after
        .declined_candidates
        .iter()
        .find(|declined| {
            declined.candidate_id == candidate_id
                && !before
                    .declined_candidates
                    .iter()
                    .any(|existing| existing.candidate_id == declined.candidate_id)
        })
        .cloned()
}

fn contradiction_counterpart<'a>(contradiction: &'a Contradiction, id: &str) -> Option<&'a str> {
    if contradiction.goal_a == id {
        Some(&contradiction.goal_b)
    } else if contradiction.goal_b == id {
        Some(&contradiction.goal_a)
    } else {
        None
    }
}

/// Resolves a model verdict for one candidate against the existing goal set. Assumes the hard
/// tier-floor gate already passed — callers must check `candidate_hard_tier_floor_rejected`
/// first and use `resolve_protected_floor_rejection` instead when it returns `true`.
///
/// Per contradiction pair, the candidate is rejected if it is equal-or-less fundamental
/// (tier number >=) than the conflicting goal, otherwise the conflicting goal is a
/// cancellation target. Reject dominates: any rejection suppresses all cancellations for this
/// candidate. Emitted event order (when admitted) is cancellations first, then acceptance.
pub fn resolve_admission(
    candidate: &ProposedGoalCandidate,
    verdict: &CoherenceVerdict,
    state: &VolitionState,
    fixture: &VolitionFixture,
    tick: u64,
) -> (AdmissionResolution, Vec<VolitionEvent>) {
    let candidate_id = candidate.id();
    let candidate_tier = crate::effective_tier_from_tension_ids(candidate.tension_ids(), fixture);

    let mut rejected_by = BTreeSet::new();
    let mut cancellation_targets = BTreeSet::new();

    for contradiction in &verdict.contradictions {
        let Some(other_id) = contradiction_counterpart(contradiction, candidate_id) else {
            continue;
        };
        let other_tier = goal_effective_tier(other_id, state, fixture);
        if candidate_tier >= other_tier {
            rejected_by.insert(other_id.to_string());
        } else {
            cancellation_targets.insert(other_id.to_string());
        }
    }

    if !rejected_by.is_empty() {
        let rejected_by: Vec<String> = rejected_by.into_iter().collect();
        let resolution = AdmissionResolution::Judged {
            admitted: false,
            rejected_by: rejected_by.clone(),
            cancelled_goal_ids: vec![],
            suppressed_cancellations_due_to_rejection: cancellation_targets.into_iter().collect(),
        };
        // Only the primary (first, deterministically ordered) conflicting goal is carried into
        // the declined-candidate record — matches the injected-context contract, which grounds
        // a decline in one conflicting goal, not the full rejected_by set.
        let coherence_decline = rejected_by.first().map(|conflicting_goal_id| {
            let rationale = verdict
                .contradictions
                .iter()
                .find(|contradiction| {
                    (contradiction.goal_a == candidate_id
                        && contradiction.goal_b == *conflicting_goal_id)
                        || (contradiction.goal_b == candidate_id
                            && contradiction.goal_a == *conflicting_goal_id)
                })
                .map(|contradiction| contradiction.rationale.clone())
                .unwrap_or_default();
            CoherenceDecline {
                conflict: DeclineReason::ConflictingGoal {
                    goal_id: conflicting_goal_id.clone(),
                },
                rationale,
            }
        });
        let events = vec![VolitionEvent::GoalCandidateRejected {
            goal_id: candidate_id.to_string(),
            reason: format!(
                "contradicts more-or-equally fundamental goal(s): {}",
                rejected_by.join(", ")
            ),
            coherence_decline,
            tick,
        }];
        return (resolution, events);
    }

    let cancelled_goal_ids: Vec<String> = cancellation_targets.into_iter().collect();
    let mut events: Vec<VolitionEvent> = cancelled_goal_ids
        .iter()
        .map(|goal_id| VolitionEvent::GoalRetired {
            goal_id: goal_id.clone(),
            tick,
        })
        .collect();
    let acceptance_evidence = EvidenceRef::try_new(format!(
        "coherence-check: admitted at tick {tick} with no more-fundamental contradiction"
    ))
    .expect("non-empty format string cannot fail EvidenceRef construction");
    events.push(VolitionEvent::GoalCandidateAccepted {
        goal_id: candidate_id.to_string(),
        acceptance_evidence,
        tick,
    });

    let resolution = AdmissionResolution::Judged {
        admitted: true,
        rejected_by: vec![],
        cancelled_goal_ids,
        suppressed_cancellations_due_to_rejection: vec![],
    };
    (resolution, events)
}

fn last_activated_tick(goal_id: &str, state: &VolitionState) -> u64 {
    state
        .goals
        .get(goal_id)
        .and_then(|dynamic| dynamic.last_activated_tick)
        .unwrap_or(0)
}

/// Resolves a whole-set sweep verdict. Per contradicting pair: both at or below the protected
/// floor is flagged for human review (never auto-resolved); otherwise the higher-tier-number
/// (less fundamental) goal is cancelled, with a deterministic tie-break on equal tiers
/// (greater `last_activated_tick`, missing treated as 0, then greater goal id
/// lexicographically). A goal cancelled by one pair is not cancelled again by another.
pub fn resolve_sweep(
    verdict: &CoherenceVerdict,
    state: &VolitionState,
    fixture: &VolitionFixture,
    tick: u64,
) -> (SweepResolution, Vec<VolitionEvent>) {
    let mut sorted_contradictions = verdict.contradictions.clone();
    sorted_contradictions.sort_by(|a, b| {
        (a.goal_a.as_str(), a.goal_b.as_str()).cmp(&(b.goal_a.as_str(), b.goal_b.as_str()))
    });

    let mut cancellations: Vec<SweepCancellation> = Vec::new();
    let mut flagged: Vec<FlaggedFloorContradiction> = Vec::new();
    let mut already_cancelled: BTreeSet<String> = BTreeSet::new();

    for contradiction in &sorted_contradictions {
        let a_tier = goal_effective_tier(&contradiction.goal_a, state, fixture);
        let b_tier = goal_effective_tier(&contradiction.goal_b, state, fixture);

        if a_tier <= PROTECTED_TIER_FLOOR && b_tier <= PROTECTED_TIER_FLOOR {
            flagged.push(FlaggedFloorContradiction {
                goal_a: contradiction.goal_a.clone(),
                goal_b: contradiction.goal_b.clone(),
            });
            continue;
        }

        let (cancel_id, keep_id, tie_break) = if a_tier != b_tier {
            if a_tier > b_tier {
                (
                    contradiction.goal_a.clone(),
                    contradiction.goal_b.clone(),
                    None,
                )
            } else {
                (
                    contradiction.goal_b.clone(),
                    contradiction.goal_a.clone(),
                    None,
                )
            }
        } else {
            let a_tick = last_activated_tick(&contradiction.goal_a, state);
            let b_tick = last_activated_tick(&contradiction.goal_b, state);
            if a_tick != b_tick {
                if a_tick > b_tick {
                    (
                        contradiction.goal_a.clone(),
                        contradiction.goal_b.clone(),
                        Some(SweepTieBreak::GreaterActivationTick),
                    )
                } else {
                    (
                        contradiction.goal_b.clone(),
                        contradiction.goal_a.clone(),
                        Some(SweepTieBreak::GreaterActivationTick),
                    )
                }
            } else if contradiction.goal_a > contradiction.goal_b {
                (
                    contradiction.goal_a.clone(),
                    contradiction.goal_b.clone(),
                    Some(SweepTieBreak::GreaterGoalId),
                )
            } else {
                (
                    contradiction.goal_b.clone(),
                    contradiction.goal_a.clone(),
                    Some(SweepTieBreak::GreaterGoalId),
                )
            }
        };

        if !already_cancelled.insert(cancel_id.clone()) {
            continue;
        }

        cancellations.push(SweepCancellation {
            cancelled_goal_id: cancel_id,
            conflicting_goal_id: keep_id,
            tie_break,
        });
    }

    cancellations.sort_by(|a, b| a.cancelled_goal_id.cmp(&b.cancelled_goal_id));
    let events = cancellations
        .iter()
        .map(|cancellation| VolitionEvent::GoalRetired {
            goal_id: cancellation.cancelled_goal_id.clone(),
            tick,
        })
        .collect();

    (
        SweepResolution {
            cancellations,
            flagged,
        },
        events,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllowedEffect, Goal, GoalScope, GoalStatus, Tension, TensionPriority, apply};

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

    fn make_goal(id: &str, tension_ids: Vec<String>, base_priority: u8) -> Goal {
        Goal {
            id: id.to_string(),
            title: id.to_string(),
            summary: id.to_string(),
            tension_ids,
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority,
            activation_keywords: vec!["test".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: id.to_string(),
            evidence_refs: vec![],
            estimated_tokens: 10,
            source_reference: id.to_string(),
        }
    }

    fn make_candidate(id: &str, tension_ids: Vec<String>) -> ProposedGoalCandidate {
        ProposedGoalCandidate::try_new(
            id.to_string(),
            format!("{id} title"),
            format!("{id} summary"),
            tension_ids,
            GoalScope::Session,
            70,
            vec![AllowedEffect::Reflect],
            "Satisfied when resolved.".to_string(),
            vec![EvidenceRef::try_new(format!("evidence: {id}")).unwrap()],
            format!("source: {id}"),
            vec![],
        )
        .unwrap()
    }

    fn verdict(contradictions: Vec<Contradiction>) -> CoherenceVerdict {
        CoherenceVerdict {
            contradictions,
            judge_ref: CoherenceJudgeRef {
                model_role: "test-judge".to_string(),
                prompt_version: "v1".to_string(),
            },
        }
    }

    fn contradiction(a: &str, b: &str) -> Contradiction {
        Contradiction {
            goal_a: a.to_string(),
            goal_b: b.to_string(),
            rationale: format!("{a} vs {b}"),
        }
    }

    // ── Hard tier-floor gate ─────────────────────────────────────────────────

    #[test]
    fn hard_tier_floor_gate_rejects_protected_tier_candidate() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("floor-tension", 2)],
            goals: vec![],
        };
        let candidate = make_candidate("floor-candidate", vec!["floor-tension".to_string()]);
        assert!(candidate_hard_tier_floor_rejected(&candidate, &fixture));
    }

    #[test]
    fn hard_tier_floor_gate_allows_band_tier_candidate() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("band-tension", 7)],
            goals: vec![],
        };
        let candidate = make_candidate("band-candidate", vec!["band-tension".to_string()]);
        assert!(!candidate_hard_tier_floor_rejected(&candidate, &fixture));
    }

    #[test]
    fn resolve_protected_floor_rejection_has_no_verdict_and_rejects_candidate() {
        let candidate = make_candidate("floor-candidate", vec!["floor-tension".to_string()]);

        let (resolution, events) = resolve_protected_floor_rejection(&candidate, 1);

        assert_eq!(resolution, AdmissionResolution::RejectProtectedFloor);
        assert_eq!(
            events,
            vec![VolitionEvent::GoalCandidateRejected {
                goal_id: "floor-candidate".to_string(),
                reason: "candidate effective tier is at or below the protected floor".to_string(),
                coherence_decline: Some(CoherenceDecline {
                    conflict: DeclineReason::ProtectedFloor,
                    rationale:
                        "the candidate's own effective tier is at or below the protected floor"
                            .to_string(),
                }),
                tick: 1,
            }]
        );
    }

    // ── Admission ────────────────────────────────────────────────────────────

    #[test]
    fn resolve_admission_admits_candidate_with_no_contradiction() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("band-tension", 7)],
            goals: vec![],
        };
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("new-candidate", vec!["band-tension".to_string()]);
        let verdict = verdict(vec![]);

        let (resolution, events) = resolve_admission(&candidate, &verdict, &state, &fixture, 5);

        assert_eq!(
            resolution,
            AdmissionResolution::Judged {
                admitted: true,
                rejected_by: vec![],
                cancelled_goal_ids: vec![],
                suppressed_cancellations_due_to_rejection: vec![],
            }
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            VolitionEvent::GoalCandidateAccepted { goal_id, tick, .. }
                if goal_id == "new-candidate" && *tick == 5
        ));
    }

    #[test]
    fn resolve_admission_rejects_candidate_contradicting_more_fundamental_goal() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("core-tension", 4),
                make_tension("band-tension", 7),
            ],
            goals: vec![make_goal("core-goal", vec!["core-tension".to_string()], 90)],
        };
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("tangent-candidate", vec!["band-tension".to_string()]);
        let verdict = verdict(vec![contradiction("tangent-candidate", "core-goal")]);

        let (resolution, events) = resolve_admission(&candidate, &verdict, &state, &fixture, 3);

        assert_eq!(
            resolution,
            AdmissionResolution::Judged {
                admitted: false,
                rejected_by: vec!["core-goal".to_string()],
                cancelled_goal_ids: vec![],
                suppressed_cancellations_due_to_rejection: vec![],
            }
        );
        assert_eq!(
            events,
            vec![VolitionEvent::GoalCandidateRejected {
                goal_id: "tangent-candidate".to_string(),
                reason: "contradicts more-or-equally fundamental goal(s): core-goal".to_string(),
                coherence_decline: Some(CoherenceDecline {
                    conflict: DeclineReason::ConflictingGoal {
                        goal_id: "core-goal".to_string(),
                    },
                    rationale: "tangent-candidate vs core-goal".to_string(),
                }),
                tick: 3,
            }]
        );
    }

    #[test]
    fn resolve_admission_rejects_when_candidate_tier_equals_conflicting_goal_tier() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("shared-tension", 5)],
            goals: vec![make_goal(
                "existing-goal",
                vec!["shared-tension".to_string()],
                80,
            )],
        };
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("equal-candidate", vec!["shared-tension".to_string()]);
        let verdict = verdict(vec![contradiction("equal-candidate", "existing-goal")]);

        let (resolution, _events) = resolve_admission(&candidate, &verdict, &state, &fixture, 1);

        assert!(matches!(
            resolution,
            AdmissionResolution::Judged {
                admitted: false,
                ..
            }
        ));
    }

    #[test]
    fn resolve_admission_admits_and_cancels_less_fundamental_goal() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("core-tension", 4),
                make_tension("band-tension", 7),
            ],
            goals: vec![make_goal(
                "stale-goal",
                vec!["band-tension".to_string()],
                60,
            )],
        };
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("core-candidate", vec!["core-tension".to_string()]);
        let verdict = verdict(vec![contradiction("core-candidate", "stale-goal")]);

        let (resolution, events) = resolve_admission(&candidate, &verdict, &state, &fixture, 4);

        assert_eq!(
            resolution,
            AdmissionResolution::Judged {
                admitted: true,
                rejected_by: vec![],
                cancelled_goal_ids: vec!["stale-goal".to_string()],
                suppressed_cancellations_due_to_rejection: vec![],
            }
        );
        assert_eq!(
            events,
            vec![
                VolitionEvent::GoalRetired {
                    goal_id: "stale-goal".to_string(),
                    tick: 4
                },
                VolitionEvent::GoalCandidateAccepted {
                    goal_id: "core-candidate".to_string(),
                    acceptance_evidence: EvidenceRef::try_new(
                        "coherence-check: admitted at tick 4 with no more-fundamental contradiction"
                    )
                    .unwrap(),
                    tick: 4,
                },
            ],
            "GoalRetired for the cancellation target must precede GoalCandidateAccepted"
        );
    }

    #[test]
    fn resolve_admission_reject_dominates_and_suppresses_cancellation() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("core-tension", 4),
                make_tension("mid-tension", 5),
                make_tension("band-tension", 7),
            ],
            goals: vec![
                make_goal("core-goal", vec!["core-tension".to_string()], 90),
                make_goal("stale-goal", vec!["band-tension".to_string()], 60),
            ],
        };
        let state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("mixed-candidate", vec!["mid-tension".to_string()]);
        let verdict = verdict(vec![
            contradiction("mixed-candidate", "core-goal"),
            contradiction("mixed-candidate", "stale-goal"),
        ]);

        let (resolution, events) = resolve_admission(&candidate, &verdict, &state, &fixture, 2);

        assert_eq!(
            resolution,
            AdmissionResolution::Judged {
                admitted: false,
                rejected_by: vec!["core-goal".to_string()],
                cancelled_goal_ids: vec![],
                suppressed_cancellations_due_to_rejection: vec!["stale-goal".to_string()],
            }
        );
        assert_eq!(
            events,
            vec![VolitionEvent::GoalCandidateRejected {
                goal_id: "mixed-candidate".to_string(),
                reason: "contradicts more-or-equally fundamental goal(s): core-goal".to_string(),
                coherence_decline: Some(CoherenceDecline {
                    conflict: DeclineReason::ConflictingGoal {
                        goal_id: "core-goal".to_string(),
                    },
                    rationale: "mixed-candidate vs core-goal".to_string(),
                }),
                tick: 2,
            }],
            "reject dominates: no GoalRetired must be emitted"
        );
    }

    // ── Sweep ────────────────────────────────────────────────────────────────

    #[test]
    fn resolve_sweep_cancels_higher_tier_number_goal_only() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("core-tension", 4),
                make_tension("band-tension", 7),
            ],
            goals: vec![
                make_goal("core-goal", vec!["core-tension".to_string()], 90),
                make_goal("band-goal", vec!["band-tension".to_string()], 60),
            ],
        };
        let state = VolitionState::from_fixture(&fixture);
        let verdict = verdict(vec![contradiction("core-goal", "band-goal")]);

        let (resolution, events) = resolve_sweep(&verdict, &state, &fixture, 9);

        assert_eq!(
            resolution.cancellations,
            vec![SweepCancellation {
                cancelled_goal_id: "band-goal".to_string(),
                conflicting_goal_id: "core-goal".to_string(),
                tie_break: None,
            }]
        );
        assert!(resolution.flagged.is_empty());
        assert_eq!(
            events,
            vec![VolitionEvent::GoalRetired {
                goal_id: "band-goal".to_string(),
                tick: 9
            }]
        );
    }

    #[test]
    fn resolve_sweep_equal_tier_tie_break_cancels_greater_activation_tick() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("shared-tension", 6)],
            goals: vec![
                make_goal("goal-a", vec!["shared-tension".to_string()], 80),
                make_goal("goal-b", vec!["shared-tension".to_string()], 80),
            ],
        };
        let mut state = VolitionState::from_fixture(&fixture);
        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "goal-a".to_string(),
                tick: 3,
            },
        );
        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "goal-b".to_string(),
                tick: 7,
            },
        );
        let verdict = verdict(vec![contradiction("goal-a", "goal-b")]);

        let (resolution, events) = resolve_sweep(&verdict, &state, &fixture, 10);

        assert_eq!(
            resolution.cancellations,
            vec![SweepCancellation {
                cancelled_goal_id: "goal-b".to_string(),
                conflicting_goal_id: "goal-a".to_string(),
                tie_break: Some(SweepTieBreak::GreaterActivationTick),
            }]
        );
        assert_eq!(
            events,
            vec![VolitionEvent::GoalRetired {
                goal_id: "goal-b".to_string(),
                tick: 10
            }]
        );
    }

    #[test]
    fn resolve_sweep_equal_tier_and_tick_tie_break_cancels_greater_goal_id() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("shared-tension", 6)],
            goals: vec![
                make_goal("goal-alpha", vec!["shared-tension".to_string()], 80),
                make_goal("goal-beta", vec!["shared-tension".to_string()], 80),
            ],
        };
        // Neither goal is activated, so both default to last_activated_tick=0 — a tick tie.
        let state = VolitionState::from_fixture(&fixture);
        let verdict = verdict(vec![contradiction("goal-alpha", "goal-beta")]);

        let (resolution, _events) = resolve_sweep(&verdict, &state, &fixture, 5);

        assert_eq!(
            resolution.cancellations,
            vec![SweepCancellation {
                cancelled_goal_id: "goal-beta".to_string(),
                conflicting_goal_id: "goal-alpha".to_string(),
                tie_break: Some(SweepTieBreak::GreaterGoalId),
            }]
        );
    }

    #[test]
    fn resolve_sweep_flags_floor_vs_floor_contradiction_without_cancelling() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("floor-tension-a", 1),
                make_tension("floor-tension-b", 2),
            ],
            goals: vec![
                make_goal("floor-goal-a", vec!["floor-tension-a".to_string()], 90),
                make_goal("floor-goal-b", vec!["floor-tension-b".to_string()], 90),
            ],
        };
        let state = VolitionState::from_fixture(&fixture);
        let verdict = verdict(vec![contradiction("floor-goal-a", "floor-goal-b")]);

        let (resolution, events) = resolve_sweep(&verdict, &state, &fixture, 1);

        assert!(resolution.cancellations.is_empty());
        assert_eq!(
            resolution.flagged,
            vec![FlaggedFloorContradiction {
                goal_a: "floor-goal-a".to_string(),
                goal_b: "floor-goal-b".to_string(),
            }]
        );
        assert!(events.is_empty());
    }

    #[test]
    fn resolve_sweep_tiers_accepted_candidate_from_its_own_tension_ids() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("core-tension", 4),
                make_tension("band-tension", 7),
            ],
            goals: vec![make_goal("core-goal", vec!["core-tension".to_string()], 90)],
        };
        let mut state = VolitionState::from_fixture(&fixture);
        let candidate = make_candidate("accepted-band-candidate", vec!["band-tension".to_string()]);
        state = apply(
            state,
            VolitionEvent::GoalCandidateAdded { candidate, tick: 1 },
        );
        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: "accepted-band-candidate".to_string(),
                acceptance_evidence: EvidenceRef::try_new("test-accept").unwrap(),
                tick: 2,
            },
        );
        let verdict = verdict(vec![contradiction("core-goal", "accepted-band-candidate")]);

        let (resolution, events) = resolve_sweep(&verdict, &state, &fixture, 5);

        assert_eq!(
            resolution.cancellations,
            vec![SweepCancellation {
                cancelled_goal_id: "accepted-band-candidate".to_string(),
                conflicting_goal_id: "core-goal".to_string(),
                tie_break: None,
            }],
            "accepted candidate absent from fixture.goals must still be tiered from its own tension_ids"
        );
        assert_eq!(
            events,
            vec![VolitionEvent::GoalRetired {
                goal_id: "accepted-band-candidate".to_string(),
                tick: 5
            }]
        );
    }
}
