use std::fs;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    EvidenceRef, GoalCandidateProposalResult, ProposedGoalCandidate, VolitionEvent, VolitionState,
    apply, propose_goal_candidates, static_fixture,
};

struct ReviewAccept<'a> {
    id: &'a str,
    acceptance_evidence: &'a str,
}

struct ReviewReject<'a> {
    id: &'a str,
    reason: &'a str,
}

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct VolitionReflectionGoalCandidatesExperiment;

impl Experiment for VolitionReflectionGoalCandidatesExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionReflectionGoalCandidates
    }

    fn description(&self) -> &'static str {
        "Replay a scripted propose/accept/reject/inert sequence to exercise reflection-generated \
         goal candidates — no effect is executed; validated pending/accepted storage before \
         selector wiring was introduced in the next experiment"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        // Four scripted open questions: three match tensions, one matches none.
        let open_questions: Vec<String> = vec![
            "Is continuity preserved across sessions?".to_string(),
            "Are there coherence issues when speculative ideas are blended with current facts?"
                .to_string(),
            "What research questions remain about the memory system design?".to_string(),
            "What time is it in milliseconds?".to_string(),
        ];

        // ── Turn 1: Propose ───────────────────────────────────────────────────
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 1, "phase": "propose", "question_count": open_questions.len() }),
            None,
        )?;

        let proposal = propose_goal_candidates(&open_questions, &fixture);

        for candidate in &proposal.candidates {
            state = apply(
                state,
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick: 1,
                },
            );
        }

        let turn1_trace = turn_trace(
            context,
            1,
            "propose",
            &state,
            &proposal,
            None::<ReviewAccept<'_>>,
            None::<ReviewReject<'_>>,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 1,
                "phase": "propose",
                "trace_id": turn1_trace,
                "matched_candidates": proposal.candidates.len(),
                "unmatched_questions": proposal.unmatched_questions.len(),
                "pending_candidates": state.pending_candidates.len(),
                "accepted_candidates": state.accepted_candidates.len(),
                "executed_effects": 0,
            }),
            Some(turn1_trace),
        )?;

        // ── Turn 2: Accept ─────────────────────────────────────────────────────
        // Accept the first matched candidate (continuity-preservation).
        let accept_id = proposal.candidates[0].id().to_string();
        let acceptance_evidence_str = "trace: continuity-preservation question confirmed relevant";
        let acceptance_evidence = EvidenceRef::try_new(acceptance_evidence_str).unwrap();

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 2, "phase": "accept", "goal_id": accept_id }),
            None,
        )?;

        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: accept_id.clone(),
                acceptance_evidence,
                tick: 2,
            },
        );

        let turn2_trace = turn_trace(
            context,
            2,
            "accept",
            &state,
            &proposal,
            Some(ReviewAccept {
                id: &accept_id,
                acceptance_evidence: acceptance_evidence_str,
            }),
            None,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 2,
                "phase": "accept",
                "trace_id": turn2_trace,
                "accepted_goal_id": accept_id,
                "pending_candidates": state.pending_candidates.len(),
                "accepted_candidates": state.accepted_candidates.len(),
                "executed_effects": 0,
            }),
            Some(turn2_trace),
        )?;

        // ── Turn 3: Reject ─────────────────────────────────────────────────────
        // Reject the second matched candidate (coherence-maintenance).
        let reject_id = proposal.candidates[1].id().to_string();
        let rejection_reason = "Coherence question already addressed by existing fixture goal.";

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 3, "phase": "reject", "goal_id": reject_id }),
            None,
        )?;

        state = apply(
            state,
            VolitionEvent::GoalCandidateRejected {
                goal_id: reject_id.clone(),
                reason: rejection_reason.to_string(),
                coherence_decline: None,
                tick: 3,
            },
        );

        let turn3_trace = turn_trace(
            context,
            3,
            "reject",
            &state,
            &proposal,
            None,
            Some(ReviewReject {
                id: &reject_id,
                reason: rejection_reason,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 3,
                "phase": "reject",
                "trace_id": turn3_trace,
                "rejected_goal_id": reject_id,
                "pending_candidates": state.pending_candidates.len(),
                "accepted_candidates": state.accepted_candidates.len(),
                "executed_effects": 0,
            }),
            Some(turn3_trace),
        )?;

        // ── Turn 4: Inert ──────────────────────────────────────────────────────
        // No review events. The remaining candidate (research-curiosity) stays pending.
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 4, "phase": "inert" }),
            None,
        )?;

        state = apply(state, VolitionEvent::TickAdvanced { tick: 4 });

        let turn4_trace = turn_trace(
            context,
            4,
            "inert",
            &state,
            &proposal,
            None::<ReviewAccept<'_>>,
            None::<ReviewReject<'_>>,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 4,
                "phase": "inert",
                "trace_id": turn4_trace,
                "pending_candidates": state.pending_candidates.len(),
                "accepted_candidates": state.accepted_candidates.len(),
                "executed_effects": 0,
            }),
            Some(turn4_trace),
        )?;

        let remaining_pending = state.pending_candidates.len();
        let total_accepted = state.accepted_candidates.len();

        write_reflection_report(context, &fixture, &proposal, &state)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Replayed 4 scripted turns through the reflection-generated candidate pipeline. \
                 propose_goal_candidates matched {} of {} questions. \
                 After accept/reject/inert turns: {} pending, {} accepted. \
                 Executed effects: 0.",
                proposal.candidates.len(),
                open_questions.len(),
                remaining_pending,
                total_accepted,
            ),
            observations: vec![
                format!(
                    "{} questions matched tensions and produced candidates; {} matched no tension.",
                    proposal.candidates.len(),
                    proposal.unmatched_questions.len()
                ),
                "GoalCandidateAdded appends to pending_candidates without auto-accepting.".to_string(),
                "GoalCandidateAccepted moves candidate from pending_candidates to accepted_candidates."
                    .to_string(),
                "GoalCandidateRejected removes the candidate from pending_candidates (no durable rejected state).".to_string(),
                "Remaining candidate stays in pending_candidates unchanged across the inert turn.".to_string(),
                "accepted_candidates held pending/accepted storage in this phase; selector wiring via activation_keywords was validated in VolitionBoundedInitiativeExecution.".to_string(),
                "executed_effects=0 on every turn (explicit no-execution marker).".to_string(),
            ],
            failure_modes: vec![
                "propose_goal_candidates uses keyword matching against tension ids and summaries; questions using synonyms or domain-specific phrasing may not match any tension.".to_string(),
                "Candidate ids are derived from question content (slug); duplicate questions would produce duplicate ids.".to_string(),
            ],
            follow_up_questions: vec![
                "Selector integration: how should accepted_candidates wire into select_goals_with_salience — merged into fixture goals or as a parallel selector layer?".to_string(),
                "Should accepted candidates inherit activation_keywords from their matched tensions rather than defaulting to empty?".to_string(),
                "Should a cap on pending_candidates length be introduced once the experiment reveals accumulation risks?".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the pending/accepted storage boundary (separate from fixture goals) as the durable design once selector integration for accepted candidates is validated.".to_string(),
            ],
            extra_artifacts: vec!["volition-reflection-goal-candidates-report.md".to_string()],
        })
    }
}

fn turn_trace(
    context: &mut RunContext,
    turn: u64,
    phase: &str,
    state: &VolitionState,
    proposal: &GoalCandidateProposalResult,
    accepted: Option<ReviewAccept<'_>>,
    rejected: Option<ReviewReject<'_>>,
) -> anyhow::Result<uuid::Uuid> {
    let pending_snapshot: Vec<CandidateSnapshot> = state
        .pending_candidates
        .iter()
        .map(CandidateSnapshot::from_pending)
        .collect();
    let accepted_snapshot: Vec<AcceptedSnapshot> = state
        .accepted_candidates
        .values()
        .map(AcceptedSnapshot::from_goal)
        .collect();

    let accepted_this_turn = accepted.map(|a| {
        let candidate = proposal.candidates.iter().find(|c| c.id() == a.id);
        json!({
            "id": a.id,
            "acceptance_evidence": a.acceptance_evidence,
            "proposal_evidence": candidate
                .map(|c| c.proposal_evidence().iter().map(|e| e.to_string()).collect::<Vec<_>>())
                .unwrap_or_default(),
            "source_question": candidate.map(|c| c.title()).unwrap_or(""),
        })
    });
    let rejected_this_turn = rejected.map(|r| {
        let candidate = proposal.candidates.iter().find(|c| c.id() == r.id);
        json!({
            "id": r.id,
            "reason": r.reason,
            "proposal_evidence": candidate
                .map(|c| c.proposal_evidence().iter().map(|e| e.to_string()).collect::<Vec<_>>())
                .unwrap_or_default(),
            "source_question": candidate.map(|c| c.title()).unwrap_or(""),
        })
    });

    let detail = json!({
        "turn": turn,
        "phase": phase,
        "tick": state.tick,
        "unmatched_questions": proposal.unmatched_questions,
        "pending_candidates": pending_snapshot,
        "accepted_candidates": accepted_snapshot,
        "accepted_this_turn": accepted_this_turn,
        "rejected_this_turn": rejected_this_turn,
        "executed_effects": 0,
    });

    let trace_record = TraceRecord::new(
        context.experiment_id(),
        "volition-reflection-candidate-turn",
        format!("turn={turn} phase={phase}"),
        format!(
            "pending={} accepted={} executed_effects=0",
            state.pending_candidates.len(),
            state.accepted_candidates.len(),
        ),
    )
    .with_details(detail);

    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;
    Ok(trace_id)
}

#[derive(Clone, Debug, Serialize)]
struct CandidateSnapshot {
    id: String,
    tension_ids: Vec<String>,
    proposal_evidence: Vec<String>,
    status: &'static str,
}

impl CandidateSnapshot {
    fn from_pending(candidate: &ProposedGoalCandidate) -> Self {
        Self {
            id: candidate.id().to_string(),
            tension_ids: candidate.tension_ids().to_vec(),
            proposal_evidence: candidate
                .proposal_evidence()
                .iter()
                .map(|e| e.to_string())
                .collect(),
            status: "pending_review",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct AcceptedSnapshot {
    id: String,
    title: String,
    tension_ids: Vec<String>,
    evidence_refs: Vec<String>,
    status: &'static str,
}

impl AcceptedSnapshot {
    fn from_goal(goal: &crate::volition::Goal) -> Self {
        Self {
            id: goal.id.clone(),
            title: goal.title.clone(),
            tension_ids: goal.tension_ids.clone(),
            evidence_refs: goal.evidence_refs.clone(),
            status: "accepted_data_record",
        }
    }
}

fn write_reflection_report(
    context: &RunContext,
    fixture: &crate::volition::VolitionFixture,
    proposal: &GoalCandidateProposalResult,
    final_state: &VolitionState,
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Reflection-Generated Goal Candidates\n\n");
    md.push_str(
        "Scripted 4-turn sequence: propose → accept → reject → inert. \
        No effect was executed. This phase validated pending/accepted storage; \
        selector wiring was introduced in VolitionBoundedInitiativeExecution.\n\n",
    );

    md.push_str("## Fixture Tensions\n\n");
    md.push_str("| Tension | Tier | Summary |\n");
    md.push_str("|---|---:|---|\n");
    for tension in &fixture.tensions {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            tension.id, tension.arbitration_tier, tension.summary
        ));
    }

    md.push_str("\n## Proposal Results\n\n");
    md.push_str(&format!(
        "Matched {} of {} questions.\n\n",
        proposal.candidates.len(),
        proposal.candidates.len() + proposal.unmatched_questions.len(),
    ));
    md.push_str("| Candidate id | Tension ids | Evidence ref |\n");
    md.push_str("|---|---|---|\n");
    for candidate in &proposal.candidates {
        let evidence: Vec<String> = candidate
            .proposal_evidence()
            .iter()
            .map(|e| e.to_string())
            .collect();
        let tensions = candidate
            .tension_ids()
            .iter()
            .map(|t| format!("`{t}`"))
            .collect::<Vec<_>>()
            .join(", ");
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            candidate.id(),
            tensions,
            evidence.join(", ")
        ));
    }
    if !proposal.unmatched_questions.is_empty() {
        md.push_str("\n**Unmatched questions:**\n");
        for q in &proposal.unmatched_questions {
            md.push_str(&format!("- {q}\n"));
        }
    }

    md.push_str("\n## Final State After 4 Turns\n\n");
    md.push_str(&format!(
        "- Pending candidates: {}\n",
        final_state.pending_candidates.len()
    ));
    md.push_str(&format!(
        "- Accepted candidates: {}\n",
        final_state.accepted_candidates.len()
    ));
    for id in final_state.accepted_candidates.keys() {
        md.push_str(&format!(
            "  - `{id}` (accepted data record, not in fixture goals)\n"
        ));
    }

    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str(
        "- [ ] Proposed candidates are clearly distinct from fixture-seeded accepted goals.\n",
    );
    md.push_str("- [ ] The accept/reject trace answers \"why was this accepted or rejected?\" from the evidence ref and reason fields alone.\n");
    md.push_str("- [ ] Nothing in the output implies a candidate was active or influenced behavior before acceptance.\n");
    md.push_str("- [ ] `executed_effects=0` on every turn.\n");
    md.push_str("- [ ] The remaining pending candidate is still in `pending_candidates` after the inert turn — unchanged.\n");

    md.push_str("\n## Notes\n\n");
    md.push_str("- `propose_goal_candidates` is pure and deterministic: same questions + same fixture → identical output.\n");
    md.push_str("- `accepted_candidates` is keyed by goal id and holds the static `Goal` struct; `VolitionState::goals` holds dynamic state for both fixture goals and (after `GoalCandidateAccepted`) accepted candidates.\n");
    md.push_str(
        "- Selector wiring via activation_keywords derived from tension id parts was validated in `VolitionBoundedInitiativeExecution`.\n",
    );

    fs::write(
        context
            .run_dir()
            .join("volition-reflection-goal-candidates-report.md"),
        md,
    )
    .with_context(|| {
        format!(
            "failed to write reflection report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::registry::run_experiment_in;
    use crate::volition::{
        EvidenceRef, VolitionEvent, VolitionState, apply, propose_goal_candidates, static_fixture,
    };

    #[test]
    fn propose_three_matched_one_unmatched_from_scripted_questions() {
        let fixture = static_fixture();
        let questions: Vec<String> = vec![
            "Is continuity preserved across sessions?".to_string(),
            "Are there coherence issues when speculative ideas are blended with current facts?"
                .to_string(),
            "What research questions remain about the memory system design?".to_string(),
            "What time is it in milliseconds?".to_string(),
        ];
        let result = propose_goal_candidates(&questions, &fixture);
        assert_eq!(
            result.candidates.len(),
            3,
            "three questions should match tensions"
        );
        assert_eq!(
            result.unmatched_questions.len(),
            1,
            "one question should match no tension"
        );
        assert!(
            result
                .unmatched_questions
                .iter()
                .any(|q| q.contains("time")),
            "the time question should be unmatched"
        );
    }

    #[test]
    fn accept_moves_candidate_to_accepted_candidates_map() {
        let fixture = static_fixture();
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let mut state = VolitionState::from_fixture(&fixture);

        for candidate in &proposal.candidates {
            state = apply(
                state,
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick: 1,
                },
            );
        }

        let accept_id = proposal.candidates[0].id().to_string();
        let acceptance_evidence = EvidenceRef::try_new("trace: confirmed relevant").unwrap();
        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: accept_id.clone(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            state.accepted_candidates.contains_key(&accept_id),
            "goal data must be in accepted_candidates"
        );
        assert!(
            !state.pending_candidates.iter().any(|c| c.id() == accept_id),
            "accepted candidate must not remain in pending_candidates"
        );
        assert!(
            state.goals.contains_key(&accept_id),
            "dynamic state must be in state.goals for selector and lifecycle wiring"
        );
    }

    #[test]
    fn reject_removes_candidate_from_pending_no_durable_state() {
        let fixture = static_fixture();
        let questions = vec![
            "Is continuity preserved across sessions?".to_string(),
            "Are there coherence issues when speculative ideas are blended with current facts?"
                .to_string(),
        ];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let mut state = VolitionState::from_fixture(&fixture);

        for candidate in &proposal.candidates {
            state = apply(
                state,
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick: 1,
                },
            );
        }

        let reject_id = proposal.candidates[1].id().to_string();
        state = apply(
            state,
            VolitionEvent::GoalCandidateRejected {
                goal_id: reject_id.clone(),
                reason: "Already covered by fixture goal.".to_string(),
                coherence_decline: None,
                tick: 2,
            },
        );

        assert!(
            !state.pending_candidates.iter().any(|c| c.id() == reject_id),
            "rejected candidate must not remain in pending_candidates"
        );
        assert!(
            !state.accepted_candidates.contains_key(&reject_id),
            "rejected candidate must not appear in accepted_candidates"
        );
    }

    #[test]
    fn inert_turn_does_not_modify_pending_or_accepted() {
        let fixture = static_fixture();
        let questions = vec!["Is continuity preserved across sessions?".to_string()];
        let proposal = propose_goal_candidates(&questions, &fixture);
        let mut state = VolitionState::from_fixture(&fixture);

        for candidate in &proposal.candidates {
            state = apply(
                state,
                VolitionEvent::GoalCandidateAdded {
                    candidate: candidate.clone(),
                    tick: 1,
                },
            );
        }

        let pending_before = state.pending_candidates.len();
        let accepted_before = state.accepted_candidates.len();
        state = apply(state, VolitionEvent::TickAdvanced { tick: 2 });

        assert_eq!(
            state.pending_candidates.len(),
            pending_before,
            "inert turn must not change pending_candidates count"
        );
        assert_eq!(
            state.accepted_candidates.len(),
            accepted_before,
            "inert turn must not change accepted_candidates count"
        );
    }

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-reflection-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(
            &base_dir,
            super::super::registry::ExperimentName::VolitionReflectionGoalCandidates,
        )
        .unwrap();

        assert_eq!(summary.experiment_id, "volition-reflection-goal-candidates");
        assert_eq!(summary.status, "completed");

        let report = std::fs::read_to_string(summary.run_dir.join("Report.md")).unwrap();
        assert!(report.contains("volition-reflection-goal-candidates"));

        let artifact = std::fs::read_to_string(
            summary
                .run_dir
                .join("volition-reflection-goal-candidates-report.md"),
        )
        .unwrap();
        assert!(artifact.contains("Human Verification Checklist"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn traces_include_acceptance_evidence_and_rejection_reason() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-reflection-trace-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(
            &base_dir,
            super::super::registry::ExperimentName::VolitionReflectionGoalCandidates,
        )
        .unwrap();

        let traces = std::fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();
        assert!(
            traces.contains("continuity-preservation question confirmed relevant"),
            "traces must include the acceptance evidence string"
        );
        assert!(
            traces.contains("Coherence question already addressed"),
            "traces must include the rejection reason"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn scripted_questions_map_to_expected_tension_ids() {
        let fixture = static_fixture();
        let questions: Vec<String> = vec![
            "Is continuity preserved across sessions?".to_string(),
            "Are there coherence issues when speculative ideas are blended with current facts?"
                .to_string(),
            "What research questions remain about the memory system design?".to_string(),
        ];
        let result = propose_goal_candidates(&questions, &fixture);
        assert_eq!(
            result.candidates.len(),
            3,
            "all three questions should match a tension"
        );

        let tension_ids_for = |fragment: &str| -> Vec<String> {
            result
                .candidates
                .iter()
                .find(|c| c.title().contains(fragment))
                .map(|c| c.tension_ids().to_vec())
                .unwrap_or_default()
        };

        assert!(
            tension_ids_for("continuity").contains(&"continuity-preservation".to_string()),
            "continuity question must map to continuity-preservation tension"
        );
        assert!(
            tension_ids_for("coherence").contains(&"coherence-maintenance".to_string()),
            "coherence question must map to coherence-maintenance tension"
        );
        assert!(
            tension_ids_for("research").contains(&"research-curiosity".to_string()),
            "research question must map to research-curiosity tension"
        );
    }
}
