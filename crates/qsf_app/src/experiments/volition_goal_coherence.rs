use std::collections::BTreeMap;
use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ActivationKeyword, AllowedEffect, Goal, GoalScope, GoalStatus, ProposedGoalCandidate, Tension,
    TensionPriority,
    VolitionEvent, VolitionFixture, VolitionState, apply, candidate_hard_tier_floor_rejected,
    effective_tier_from_tension_ids, propose_goal_candidates, resolve_admission,
    resolve_protected_floor_rejection, resolve_sweep,
};
use qsf_models::{CoherenceJudge, CoherenceJudgeGoalRef, ScriptedCoherenceJudge};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};
use super::volition_trace_support::{goal_status_diff, write_coherence_trace};

pub struct VolitionGoalCoherenceExperiment;

impl Experiment for VolitionGoalCoherenceExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionGoalCoherence
    }

    fn description(&self) -> &'static str {
        "Admit, reject, and cancel goal candidates and sweep the whole goal set for \
         contradictions under an immutable protected floor — offline, no live wiring"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = coherence_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        let questions = vec![
            "Research curiosity question one?".to_string(),
            "Research curiosity question two?".to_string(),
            "Coherence maintenance question three?".to_string(),
            "Boundary integrity question four?".to_string(),
            "Continuity preservation question five?".to_string(),
        ];
        let proposal = propose_goal_candidates(&questions, &fixture);
        anyhow::ensure!(
            proposal.candidates.len() == 5,
            "expected all 5 scripted questions to match exactly one tension each, got {}",
            proposal.candidates.len()
        );

        context.record_event(
            EventType::InputReceived,
            json!({
                "phase": "propose",
                "question_count": questions.len(),
                "matched_candidates": proposal.candidates.len(),
            }),
            None,
        )?;
        for candidate in &proposal.candidates {
            state = apply(
                state,
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick: 1,
                },
            );
        }

        let admit_no_conflict = proposal.candidates[0].clone();
        let reject_by_tier = proposal.candidates[1].clone();
        let admit_and_cancel = proposal.candidates[2].clone();
        let floor_gate = proposal.candidates[3].clone();
        let reject_dominates = proposal.candidates[4].clone();

        // Admit: no contradiction.
        state = run_admission_check(context, state, &fixture, &admit_no_conflict, vec![], 2)?;

        // Reject: candidate contradicts a more-fundamental existing goal.
        state = run_admission_check(
            context,
            state,
            &fixture,
            &reject_by_tier,
            vec![(
                reject_by_tier.id().to_string(),
                "maintain-coherence".to_string(),
                "raising unresolved curiosity here would relitigate ground coherence-maintenance already governs".to_string(),
            )],
            3,
        )?;

        // Admit and cancel: candidate is more fundamental than a contradicting existing goal.
        state = run_admission_check(
            context,
            state,
            &fixture,
            &admit_and_cancel,
            vec![(
                admit_and_cancel.id().to_string(),
                "track-open-tangent".to_string(),
                "refocusing on coherence-maintenance supersedes the open research tangent"
                    .to_string(),
            )],
            4,
        )?;

        // Hard tier-floor gate: candidate's own tier is at or below the protected floor —
        // rejected before any model call, regardless of any scripted verdict.
        state = run_admission_check(context, state, &fixture, &floor_gate, vec![], 5)?;

        // Reject dominates: one rejecting conflict and one cancelling conflict — not admitted,
        // and the cancelling target is only suppressed, never retired.
        state = run_admission_check(
            context,
            state,
            &fixture,
            &reject_dominates,
            vec![
                (
                    reject_dominates.id().to_string(),
                    "maintain-coherence".to_string(),
                    "continuity here would relitigate ground coherence-maintenance already governs"
                        .to_string(),
                ),
                (
                    reject_dominates.id().to_string(),
                    "pursue-curiosity-topic".to_string(),
                    "continuity would supersede the still-open curiosity topic".to_string(),
                ),
            ],
            6,
        )?;

        // Set up the sweep's equal-tier tie-break pair (greater activation tick loses).
        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "align-current-focus".to_string(),
                tick: 7,
            },
        );
        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "sustain-current-focus".to_string(),
                tick: 8,
            },
        );

        // Sweep: cancel-less-fundamental (including tiering an accepted candidate absent from
        // fixture.goals), an activation-tick tie-break, a goal-id tie-break, and a
        // floor-vs-floor pair that must only be flagged, never cancelled.
        state = run_sweep_check(
            context,
            state,
            &fixture,
            vec![
                (
                    "maintain-coherence".to_string(),
                    admit_no_conflict.id().to_string(),
                    "the accepted curiosity candidate now relitigates ground coherence-maintenance already governs".to_string(),
                ),
                (
                    "align-current-focus".to_string(),
                    "sustain-current-focus".to_string(),
                    "both goals now pull continuity toward incompatible current-focus framings".to_string(),
                ),
                (
                    "note-emerging-pattern-alpha".to_string(),
                    "note-emerging-pattern-beta".to_string(),
                    "both goals now name incompatible readings of the same emerging pattern".to_string(),
                ),
                (
                    "protect-boundaries".to_string(),
                    "maintain-safety-check".to_string(),
                    "fixture curation drift: two protected-floor goals now read as contradictory".to_string(),
                ),
            ],
            9,
        )?;

        write_coherence_report(context, &fixture, &state)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Ran 5 admission checks (admit, reject-by-tier, admit-and-cancel, \
                 hard-tier-floor-gate, reject-dominates) and one whole-set sweep (cancel, \
                 activation-tick tie-break, goal-id tie-break, floor-vs-floor flag) over a \
                 {}-tension coherence fixture. Every decision is recorded as a \
                 goal-coherence-check trace record linked to its emitted lifecycle events.",
                fixture.tensions.len()
            ),
            observations: vec![
                "The hard tier-floor gate rejects a candidate before any model call — its trace record carries judge_ref: null and contradictions: [].".to_string(),
                "Admission resolution reuses the existing GoalCandidateAccepted / GoalCandidateRejected / GoalRetired events; no new event types were introduced.".to_string(),
                "Reject dominates: a candidate with one rejecting and one cancelling conflict is not admitted, and the cancelling target is recorded as suppressed rather than retired.".to_string(),
                "The sweep tiers an accepted candidate absent from fixture.goals from its own tension_ids, not from a fixture.goals lookup.".to_string(),
                "A floor-vs-floor contradiction is flagged for human review and never auto-resolved.".to_string(),
            ],
            failure_modes: vec![
                "The mock CoherenceJudge is scripted per check; a real model judge is non-deterministic and may detect contradictions this fixture does not anticipate.".to_string(),
                "propose_goal_candidates matches by keyword overlap; a poorly chosen fixture summary could cause a question to match more than one tension.".to_string(),
            ],
            follow_up_questions: vec![
                "Phase 2: how should live discussion-formed candidates be queued into this same admission engine off the hot path?".to_string(),
                "Should the sleep-phase sweep run on a fixed cadence or be triggered by candidate-admission volume?".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the goal-coherence-check trace shape (this experiment's contract) as the durable trace format before Phase 2 live wiring.".to_string(),
            ],
            extra_artifacts: vec!["goal-coherence-report.md".to_string()],
        })
    }
}

/// A malleable, non-floor tension/goal fixture purpose-built to exercise every coherence
/// resolution branch. Tension id-terms are disjoint so `propose_goal_candidates` matches each
/// scripted question to exactly one tension.
fn coherence_fixture() -> VolitionFixture {
    let tensions = vec![
        Tension {
            id: "boundary-integrity".to_string(),
            title: "Boundary integrity".to_string(),
            summary: "Protects boundary integrity of the system.".to_string(),
            priority_bias: TensionPriority::Highest,
            arbitration_tier: 1,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "operational-safety".to_string(),
            title: "Operational safety".to_string(),
            summary: "Protects operational safety guarantees.".to_string(),
            priority_bias: TensionPriority::Highest,
            arbitration_tier: 3,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "coherence-maintenance".to_string(),
            title: "Coherence maintenance".to_string(),
            summary: "Maintains coherence between claims.".to_string(),
            priority_bias: TensionPriority::High,
            arbitration_tier: 4,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "continuity-preservation".to_string(),
            title: "Continuity preservation".to_string(),
            summary: "Preserves continuity across turns.".to_string(),
            priority_bias: TensionPriority::High,
            arbitration_tier: 5,
            focused_bias: 0,
            exploratory_bias: 0,
        },
        Tension {
            id: "research-curiosity".to_string(),
            title: "Research curiosity".to_string(),
            summary: "Surfaces open research curiosity.".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
            focused_bias: 0,
            exploratory_bias: 0,
        },
    ];

    let goals = vec![
        goal("protect-boundaries", "boundary-integrity", 100),
        goal("maintain-safety-check", "operational-safety", 99),
        goal("maintain-coherence", "coherence-maintenance", 90),
        goal("track-open-tangent", "research-curiosity", 60),
        goal("pursue-curiosity-topic", "research-curiosity", 60),
        goal("align-current-focus", "continuity-preservation", 80),
        goal("sustain-current-focus", "continuity-preservation", 80),
        goal("note-emerging-pattern-alpha", "continuity-preservation", 80),
        goal("note-emerging-pattern-beta", "continuity-preservation", 80),
    ];

    VolitionFixture { tensions, goals }
}

fn goal(id: &str, tension_id: &str, base_priority: u8) -> Goal {
    Goal {
        id: id.to_string(),
        title: id.to_string(),
        summary: id.to_string(),
        tension_ids: vec![tension_id.to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority,
        activation_keywords: vec![ActivationKeyword::normal("test")],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: id.to_string(),
        evidence_refs: vec![
            "docs/Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md".to_string(),
        ],
        estimated_tokens: 10,
        source_reference: "docs/Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md"
            .to_string(),
    }
}

/// One goal or accepted candidate currently under coherence evaluation. Retired goals are
/// excluded — a sweep only re-litigates goals still live in the system.
struct EvaluatedGoal<'a> {
    goal: &'a Goal,
    effective_tier: u8,
    status: GoalStatus,
    last_activated_tick: Option<u64>,
}

fn evaluated_goal_set<'a>(
    state: &'a VolitionState,
    fixture: &'a VolitionFixture,
) -> Vec<EvaluatedGoal<'a>> {
    let mut merged: BTreeMap<&str, &Goal> = BTreeMap::new();
    for goal in &fixture.goals {
        merged.insert(goal.id.as_str(), goal);
    }
    for (id, goal) in &state.accepted_candidates {
        merged.insert(id.as_str(), goal);
    }
    merged
        .into_iter()
        .filter_map(|(id, goal)| {
            let dynamic = state.goals.get(id)?;
            if dynamic.status == GoalStatus::Retired {
                return None;
            }
            Some(EvaluatedGoal {
                goal,
                effective_tier: effective_tier_from_tension_ids(&goal.tension_ids, fixture),
                status: dynamic.status,
                last_activated_tick: dynamic.last_activated_tick,
            })
        })
        .collect()
}

/// Builds the goal set sent to the judge for an admission check: every evaluated goal plus
/// the candidate itself, each carrying its real title and summary (not a title/title
/// substitution) so a non-scripted judge has the same information a human reviewer would.
fn admission_goal_set(
    evaluated: &[EvaluatedGoal<'_>],
    candidate: &ProposedGoalCandidate,
) -> Vec<CoherenceJudgeGoalRef> {
    let mut goal_set: Vec<CoherenceJudgeGoalRef> = evaluated
        .iter()
        .map(|e| {
            CoherenceJudgeGoalRef::new(
                e.goal.id.clone(),
                e.goal.title.clone(),
                e.goal.summary.clone(),
            )
        })
        .collect();
    goal_set.push(CoherenceJudgeGoalRef::new(
        candidate.id(),
        candidate.title(),
        candidate.summary(),
    ));
    goal_set
}

fn goal_set_snapshot_json(evaluated: &[EvaluatedGoal<'_>]) -> serde_json::Value {
    json!(
        evaluated
            .iter()
            .map(|e| json!({
                "goal_id": e.goal.id,
                "effective_tier": e.effective_tier,
                "status": e.status,
                "last_activated_tick": e.last_activated_tick,
            }))
            .collect::<Vec<_>>()
    )
}

fn run_admission_check(
    context: &mut RunContext,
    state: VolitionState,
    fixture: &VolitionFixture,
    candidate: &ProposedGoalCandidate,
    scripted_pairs: Vec<(String, String, String)>,
    tick: u64,
) -> anyhow::Result<VolitionState> {
    let candidate_tier = effective_tier_from_tension_ids(candidate.tension_ids(), fixture);
    let evaluated = evaluated_goal_set(&state, fixture);

    let (resolution_json, judge_ref_json, contradictions_json, hard_tier_floor_rejected, events) =
        if candidate_hard_tier_floor_rejected(candidate, fixture) {
            let (resolution, events) = resolve_protected_floor_rejection(candidate, tick);
            (json!(resolution), json!(null), json!([]), true, events)
        } else {
            let judge = ScriptedCoherenceJudge::new(scripted_pairs);
            let goal_set = admission_goal_set(&evaluated, candidate);
            let verdict = judge.judge(context, &goal_set)?;
            let (resolution, events) =
                resolve_admission(candidate, &verdict, &state, fixture, tick);
            (
                json!(resolution),
                json!(verdict.judge_ref),
                json!(verdict.contradictions),
                false,
                events,
            )
        };

    let mut next_state = state.clone();
    for event in events.clone() {
        next_state = apply(next_state, event);
    }
    let (status_before, status_after) = goal_status_diff(&events, &state, &next_state);

    let details = json!({
        "trigger": "admission",
        "tick": tick,
        "candidate": { "goal_id": candidate.id(), "effective_tier": candidate_tier },
        "goal_set_snapshot": goal_set_snapshot_json(&evaluated),
        "judge_ref": judge_ref_json,
        "contradictions": contradictions_json,
        "hard_tier_floor_rejected": hard_tier_floor_rejected,
        "resolution": resolution_json,
        "events_emitted": events,
        "goal_status_before": status_before,
        "goal_status_after": status_after,
        "artifact_or_record_reference": format!("goal-coherence-check#trigger=admission&tick={tick}"),
    });
    let trace_id = write_coherence_trace(context, "admission", tick, &events, details)?;

    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": "admission",
            "tick": tick,
            "candidate_id": candidate.id(),
            "hard_tier_floor_rejected": hard_tier_floor_rejected,
            "events_emitted": events.len(),
        }),
        Some(trace_id),
    )?;

    Ok(next_state)
}

fn run_sweep_check(
    context: &mut RunContext,
    state: VolitionState,
    fixture: &VolitionFixture,
    scripted_pairs: Vec<(String, String, String)>,
    tick: u64,
) -> anyhow::Result<VolitionState> {
    let evaluated = evaluated_goal_set(&state, fixture);
    let goal_set: Vec<CoherenceJudgeGoalRef> = evaluated
        .iter()
        .map(|e| {
            CoherenceJudgeGoalRef::new(
                e.goal.id.clone(),
                e.goal.title.clone(),
                e.goal.summary.clone(),
            )
        })
        .collect();
    let judge = ScriptedCoherenceJudge::new(scripted_pairs);
    let verdict = judge.judge(context, &goal_set)?;
    let (resolution, events) = resolve_sweep(&verdict, &state, fixture, tick);

    let mut next_state = state.clone();
    for event in events.clone() {
        next_state = apply(next_state, event);
    }
    let (status_before, status_after) = goal_status_diff(&events, &state, &next_state);

    let details = json!({
        "trigger": "sweep",
        "tick": tick,
        "candidate": null,
        "goal_set_snapshot": goal_set_snapshot_json(&evaluated),
        "judge_ref": verdict.judge_ref,
        "contradictions": verdict.contradictions,
        "hard_tier_floor_rejected": null,
        "resolution": resolution,
        "events_emitted": events,
        "goal_status_before": status_before,
        "goal_status_after": status_after,
        "artifact_or_record_reference": format!("goal-coherence-check#trigger=sweep&tick={tick}"),
    });
    let trace_id = write_coherence_trace(context, "sweep", tick, &events, details)?;

    context.record_event(
        EventType::TraceRecorded,
        json!({
            "trigger": "sweep",
            "tick": tick,
            "cancelled_goal_ids": resolution.cancellations.iter().map(|c| c.cancelled_goal_id.clone()).collect::<Vec<_>>(),
            "flagged_pairs": resolution.flagged.len(),
            "events_emitted": events.len(),
        }),
        Some(trace_id),
    )?;

    Ok(next_state)
}

fn write_coherence_report(
    context: &RunContext,
    fixture: &VolitionFixture,
    final_state: &VolitionState,
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Goal Coherence Under a Protected Floor\n\n");
    md.push_str(
        "Offline engine: a model judge detects contradictions; pure resolution functions \
         (`resolve_admission`, `resolve_sweep`) decide admit / reject / cancel deterministically \
         and reuse the existing goal-lifecycle events. See \
         `docs/Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md`.\n\n",
    );

    md.push_str("## Final Goal Status\n\n");
    md.push_str("| Goal id | Status | Effective tier |\n");
    md.push_str("|---|---|---:|\n");
    for evaluated in evaluated_all_including_retired(final_state, fixture) {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            evaluated.0, evaluated.1, evaluated.2
        ));
    }

    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str("- [ ] Every admit/reject/cancel decision is explained by its `goal-coherence-check` trace record alone.\n");
    md.push_str("- [ ] The hard-tier-floor rejection trace has `judge_ref: null` and `contradictions: []`.\n");
    md.push_str("- [ ] No protected-floor goal was cancelled or newly admitted into the floor.\n");
    md.push_str("- [ ] The floor-vs-floor contradiction is flagged, not cancelled.\n");

    fs::write(context.run_dir().join("goal-coherence-report.md"), md).with_context(|| {
        format!(
            "failed to write goal coherence report for run {}",
            context.run_id()
        )
    })
}

fn evaluated_all_including_retired(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> Vec<(String, GoalStatus, u8)> {
    let mut merged: BTreeMap<&str, &Goal> = BTreeMap::new();
    for goal in &fixture.goals {
        merged.insert(goal.id.as_str(), goal);
    }
    for (id, goal) in &state.accepted_candidates {
        merged.insert(id.as_str(), goal);
    }
    merged
        .into_iter()
        .filter_map(|(id, goal)| {
            let dynamic = state.goals.get(id)?;
            Some((
                id.to_string(),
                dynamic.status,
                effective_tier_from_tension_ids(&goal.tension_ids, fixture),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    use serde_json::Value;

    use super::super::registry::{ExperimentName, run_experiment_in};

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-goal-coherence-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionGoalCoherence).unwrap();

        assert_eq!(summary.experiment_id, "volition-goal-coherence");
        assert_eq!(summary.status, "completed");

        let report =
            std::fs::read_to_string(summary.run_dir.join("goal-coherence-report.md")).unwrap();
        assert!(report.contains("Human Verification Checklist"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn admission_goal_set_carries_the_candidate_summary_not_its_title() {
        let candidate = super::ProposedGoalCandidate::try_new(
            "distinct-summary-candidate".to_string(),
            "A short title".to_string(),
            "A longer summary that is not the same text as the title.".to_string(),
            vec![],
            crate::volition::GoalScope::Session,
            70,
            vec![crate::volition::AllowedEffect::Reflect],
            "Satisfied when resolved.".to_string(),
            vec![crate::volition::EvidenceRef::try_new("test-evidence").unwrap()],
            "test".to_string(),
            vec![],
        )
        .unwrap();

        let goal_set = super::admission_goal_set(&[], &candidate);

        let candidate_ref = goal_set
            .iter()
            .find(|goal_ref| goal_ref.id == "distinct-summary-candidate")
            .expect("candidate must be present in the queried goal set");
        assert_eq!(candidate_ref.title, "A short title");
        assert_eq!(
            candidate_ref.summary,
            "A longer summary that is not the same text as the title."
        );
        assert_ne!(
            candidate_ref.summary, candidate_ref.title,
            "summary must not be a copy of the title"
        );
    }

    // ── Trace-completeness contract: parsed from traces.jsonl / events.jsonl on disk,
    // never from the in-memory VolitionState/resolution structs the experiment built. ──

    fn parse_jsonl(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn goal_coherence_run() -> (Vec<Value>, Vec<Value>, PathBuf) {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-goal-coherence-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionGoalCoherence).unwrap();
        let traces = parse_jsonl(&summary.run_dir.join("traces.jsonl"));
        let events = parse_jsonl(&summary.run_dir.join("events.jsonl"));
        (traces, events, base_dir)
    }

    fn coherence_checks(traces: &[Value]) -> Vec<&Value> {
        traces
            .iter()
            .filter(|trace| trace["operation"] == "goal-coherence-check")
            .collect()
    }

    fn check_at_tick<'a>(checks: &[&'a Value], tick: u64) -> &'a Value {
        checks
            .iter()
            .copied()
            .find(|trace| trace["details"]["tick"].as_u64() == Some(tick))
            .unwrap_or_else(|| panic!("no goal-coherence-check trace at tick {tick}"))
    }

    #[test]
    fn admission_resolution_is_recomputable_from_contradictions_and_effective_tiers() {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);

        // Ticks 3 (reject-by-tier), 4 (admit-and-cancel), 6 (reject-dominates): replay the
        // deterministic resolution rule purely from the trace's own recorded contradictions
        // and effective tiers, independent of the model verdict that produced them.
        for tick in [3u64, 4, 6] {
            let record = check_at_tick(&checks, tick);
            let details = &record["details"];
            let candidate_id = details["candidate"]["goal_id"].as_str().unwrap();
            let candidate_tier = details["candidate"]["effective_tier"].as_u64().unwrap();
            let snapshot_tier: HashMap<String, u64> = details["goal_set_snapshot"]
                .as_array()
                .unwrap()
                .iter()
                .map(|goal| {
                    (
                        goal["goal_id"].as_str().unwrap().to_string(),
                        goal["effective_tier"].as_u64().unwrap(),
                    )
                })
                .collect();

            let mut expected_rejected_by = Vec::new();
            let mut expected_cancelled = Vec::new();
            for contradiction in details["contradictions"].as_array().unwrap() {
                let a = contradiction["goal_a"].as_str().unwrap();
                let b = contradiction["goal_b"].as_str().unwrap();
                let other = if a == candidate_id { b } else { a };
                let other_tier = *snapshot_tier.get(other).unwrap_or_else(|| {
                    panic!("tick {tick}: {other} missing from goal_set_snapshot")
                });
                if candidate_tier >= other_tier {
                    expected_rejected_by.push(other.to_string());
                } else {
                    expected_cancelled.push(other.to_string());
                }
            }
            expected_rejected_by.sort();
            expected_cancelled.sort();

            let resolution = &details["resolution"];
            if !expected_rejected_by.is_empty() {
                assert_eq!(resolution["admitted"], false, "tick {tick}");
                let mut actual_rejected: Vec<String> = resolution["rejected_by"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                actual_rejected.sort();
                assert_eq!(actual_rejected, expected_rejected_by, "tick {tick}");
                let mut actual_suppressed: Vec<String> =
                    resolution["suppressed_cancellations_due_to_rejection"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect();
                actual_suppressed.sort();
                assert_eq!(actual_suppressed, expected_cancelled, "tick {tick}");
            } else {
                assert_eq!(resolution["admitted"], true, "tick {tick}");
                let mut actual_cancelled: Vec<String> = resolution["cancelled_goal_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                actual_cancelled.sort();
                assert_eq!(actual_cancelled, expected_cancelled, "tick {tick}");
            }
        }

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn rejected_candidate_has_no_accept_event_and_no_extra_goal_set_change() {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let record = check_at_tick(&checks, 3); // reject-by-tier

        let events_emitted = record["details"]["events_emitted"].as_array().unwrap();
        assert_eq!(
            events_emitted.len(),
            1,
            "rejection must change nothing else"
        );
        assert_eq!(events_emitted[0]["kind"], "goal_candidate_rejected");
        assert!(
            !events_emitted
                .iter()
                .any(|event| event["kind"] == "goal_candidate_accepted")
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn admit_and_cancel_retires_before_accepting() {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let record = check_at_tick(&checks, 4); // admit-and-cancel

        let events_emitted = record["details"]["events_emitted"].as_array().unwrap();
        assert_eq!(events_emitted.len(), 2);
        assert_eq!(events_emitted[0]["kind"], "goal_retired");
        assert_eq!(events_emitted[0]["goal_id"], "track-open-tangent");
        assert_eq!(events_emitted[1]["kind"], "goal_candidate_accepted");

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn sweep_tie_break_follows_none_as_zero_greater_tick_then_greater_id_rule() {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let record = check_at_tick(&checks, 9); // sweep

        let cancellations = record["details"]["resolution"]["cancellations"]
            .as_array()
            .unwrap();

        let tick_tie_break = cancellations
            .iter()
            .find(|c| c["cancelled_goal_id"] == "sustain-current-focus")
            .expect("greater-activation-tick tie-break cancellation must be present");
        assert_eq!(tick_tie_break["tie_break"], "greater_activation_tick");
        assert_eq!(tick_tie_break["conflicting_goal_id"], "align-current-focus");

        let id_tie_break = cancellations
            .iter()
            .find(|c| c["cancelled_goal_id"] == "note-emerging-pattern-beta")
            .expect("greater-goal-id tie-break cancellation must be present");
        assert_eq!(id_tie_break["tie_break"], "greater_goal_id");
        assert_eq!(
            id_tie_break["conflicting_goal_id"],
            "note-emerging-pattern-alpha"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn floor_vs_floor_contradiction_is_flagged_and_not_retired() {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let record = check_at_tick(&checks, 9); // sweep

        let flagged = record["details"]["resolution"]["flagged"]
            .as_array()
            .unwrap();
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0]["goal_a"], "protect-boundaries");
        assert_eq!(flagged[0]["goal_b"], "maintain-safety-check");

        let events_emitted = record["details"]["events_emitted"].as_array().unwrap();
        assert!(!events_emitted.iter().any(|event| {
            event["kind"] == "goal_retired"
                && (event["goal_id"] == "protect-boundaries"
                    || event["goal_id"] == "maintain-safety-check")
        }));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn below_floor_candidate_record_has_null_judge_ref_and_empty_contradictions_regardless_of_script()
     {
        let (traces, _events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let record = check_at_tick(&checks, 5); // hard tier-floor gate

        let details = &record["details"];
        assert_eq!(details["hard_tier_floor_rejected"], true);
        assert!(details["judge_ref"].is_null());
        assert_eq!(details["contradictions"].as_array().unwrap().len(), 0);
        assert_eq!(details["resolution"]["kind"], "reject_protected_floor");

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn events_jsonl_links_one_trace_recorded_entry_per_goal_coherence_check() {
        let (traces, events, base_dir) = goal_coherence_run();
        let checks = coherence_checks(&traces);
        let check_trace_ids: HashSet<String> = checks
            .iter()
            .map(|trace| trace["trace_id"].as_str().unwrap().to_string())
            .collect();

        let trace_recorded: Vec<&Value> = events
            .iter()
            .filter(|event| event["event_type"] == "TraceRecorded")
            .collect();
        assert_eq!(
            trace_recorded.len(),
            checks.len(),
            "one TraceRecorded event per goal-coherence-check trace"
        );
        for event in &trace_recorded {
            let linked_trace_id = event["trace_id"].as_str().expect("trace_id must be linked");
            assert!(
                check_trace_ids.contains(linked_trace_id),
                "TraceRecorded event must link to a real goal-coherence-check trace_id"
            );
        }

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
