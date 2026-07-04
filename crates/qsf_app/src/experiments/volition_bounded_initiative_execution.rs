use std::fs;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ArbitrationResult, EvidenceRef, GoalDynamicState, InitiativeOutput,
    SalienceGoalSelectionResult, VolitionEvent, VolitionState, apply, arbitrate,
    execute_initiative, propose_goal_candidates, select_goals_with_salience, static_fixture,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 4,
    max_estimated_tokens: 200,
};

pub struct VolitionBoundedInitiativeExecutionExperiment;

impl Experiment for VolitionBoundedInitiativeExecutionExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionBoundedInitiativeExecution
    }

    fn description(&self) -> &'static str {
        "Replay a 5-turn scripted sequence exercising accepted-candidate selector wiring, \
         arbitration, and bounded initiative execution — executed_effects=0 on every turn"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        // ── Turn 1: Proposal ──────────────────────────────────────────────────
        // propose_goal_candidates on one matched question; verify activation_keywords.
        let question = "Is continuity preserved across sessions?".to_string();
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 1, "phase": "proposal", "question": question }),
            None,
        )?;

        let proposal = propose_goal_candidates(std::slice::from_ref(&question), &fixture);
        assert_eq!(
            proposal.candidates.len(),
            1,
            "continuity question must match exactly one tension"
        );
        let candidate = &proposal.candidates[0];
        let candidate_id = candidate.id().to_string();

        let keywords = candidate.activation_keywords().to_vec();
        assert!(
            !keywords.is_empty(),
            "candidate must carry non-empty activation_keywords derived from tension id parts"
        );
        assert!(
            keywords.contains(&"continuity".to_string()),
            "activation_keywords must include 'continuity' from 'continuity-preservation'"
        );
        assert!(
            keywords.contains(&"preservation".to_string()),
            "activation_keywords must include 'preservation' from 'continuity-preservation'"
        );

        state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );

        let turn1_id = record_turn(
            context,
            1,
            "proposal",
            &state,
            json!({
                "candidate_id": candidate_id,
                "activation_keywords": keywords,
                "events_applied": [{"event": "GoalCandidateAdded", "candidate_id": candidate_id, "tick": 1}],
                "executed_effects": 0,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 1,
                "phase": "proposal",
                "trace_id": turn1_id,
                "candidate_id": candidate_id,
                "activation_keywords": keywords,
                "executed_effects": 0,
            }),
            Some(turn1_id),
        )?;

        // ── Turn 2: Accept ────────────────────────────────────────────────────
        // GoalCandidateAccepted inserts GoalDynamicState; verify selector wiring.
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 2, "phase": "accept", "goal_id": candidate_id }),
            None,
        )?;

        let acceptance_evidence =
            EvidenceRef::try_new("trace: continuity question confirmed relevant").unwrap();
        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence,
                tick: 2,
            },
        );

        assert!(
            state.goals.contains_key(&candidate_id),
            "GoalDynamicState entry must exist in state.goals after acceptance"
        );

        // Selector wiring: input matching derived keywords returns the accepted goal.
        let selector_result =
            select_goals_with_salience("Is continuity still preserved?", &fixture, &state, BUDGET);
        assert!(
            selector_result
                .selected
                .iter()
                .any(|s| s.goal.id == candidate_id),
            "accepted candidate must appear in select_goals_with_salience when keywords match"
        );

        let turn2_id = record_turn(
            context,
            2,
            "accept",
            &state,
            json!({
                "goal_id": candidate_id,
                "dynamic_state": dynamic_state_snapshot(state.goals.get(&candidate_id)),
                "selector": selector_output_detail(&selector_result),
                "executed_effects": 0,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 2,
                "phase": "accept",
                "trace_id": turn2_id,
                "goal_id": candidate_id,
                "executed_effects": 0,
            }),
            Some(turn2_id),
        )?;

        // ── Turn 3: Arbitration ───────────────────────────────────────────────
        // Input matches both a fixture goal and the accepted candidate; verify tier ordering.
        // "complete"/"status" → avoid-overstating-impl-status (boundary-preservation, tier 1)
        // "continuity" → accepted candidate (continuity-preservation, tier 5)
        let arbitration_input =
            "Is the implementation status complete and is continuity still preserved?";
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 3, "phase": "arbitration", "input": arbitration_input }),
            None,
        )?;

        let arb_selection = select_goals_with_salience(arbitration_input, &fixture, &state, BUDGET);
        let arbitration =
            arbitrate(arb_selection.selected.clone(), &fixture).and_then(|o| o.qualified);
        assert!(
            arbitration.is_some(),
            "arbitration must produce a qualified winner with multiple selections"
        );
        let arb_result = arbitration.as_ref().unwrap();

        // boundary-preservation (tier 1) must beat continuity-preservation (tier 5).
        assert_eq!(
            arb_result.winner.goal.id, "avoid-overstating-impl-status",
            "tier-1 fixture goal must win over tier-5 accepted candidate"
        );

        let turn3_id = record_turn(
            context,
            3,
            "arbitration",
            &state,
            json!({
                "input": arbitration_input,
                "selector": selector_output_detail(&arb_selection),
                "arbitration_result": arbitration_result_detail(arb_result),
                "executed_effects": 0,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 3,
                "phase": "arbitration",
                "trace_id": turn3_id,
                "winner_goal_id": arb_result.winner.goal.id,
                "executed_effects": 0,
            }),
            Some(turn3_id),
        )?;

        // ── Turn 4: Execution ─────────────────────────────────────────────────
        // execute_initiative on the arbitration winner; apply InitiativeExecuted.
        let winner_selection = &arb_result.winner;
        let initiative_output =
            execute_initiative(&winner_selection.initiative, &winner_selection.goal);

        let rationale = format!(
            "goal '{}' won arbitration at tier {} via '{}'",
            winner_selection.goal.id,
            arb_result.winner_effective_tier,
            arb_result.winner_effective_tension_id,
        );

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 4, "phase": "execution", "goal_id": winner_selection.goal.id }),
            None,
        )?;

        state = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: winner_selection.goal.id.clone(),
                effect: winner_selection.initiative.effect,
                output: initiative_output.clone(),
                rationale: rationale.clone(),
                tick: 4,
            },
        );

        let dynamic_after_exec = state.goals.get(&winner_selection.goal.id).unwrap();
        assert!(
            dynamic_after_exec.last_initiative_output.is_some(),
            "GoalDynamicState::last_initiative_output must be set after InitiativeExecuted"
        );

        let turn4_id = record_turn(
            context,
            4,
            "execution",
            &state,
            json!({
                "goal_id": winner_selection.goal.id,
                "effect": winner_selection.initiative.effect.to_string(),
                "initiative_output": serde_json::to_value(&initiative_output).unwrap_or_default(),
                "rationale": rationale,
                "arbitration_result": arbitration_result_detail(arb_result),
                "dynamic_state": dynamic_state_snapshot(state.goals.get(&winner_selection.goal.id)),
                "executed_effects": 0,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 4,
                "phase": "execution",
                "trace_id": turn4_id,
                "goal_id": winner_selection.goal.id,
                "executed_effects": 0,
            }),
            Some(turn4_id),
        )?;

        // ── Turn 5: Outcome ───────────────────────────────────────────────────
        // Apply GoalProgressObserved; verify lifecycle advances and evidence is preserved.
        let progress_evidence =
            EvidenceRef::try_new("trace: turn-5 progress observed on winner").unwrap();
        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 5, "phase": "outcome", "goal_id": winner_selection.goal.id }),
            None,
        )?;

        state = apply(
            state,
            VolitionEvent::GoalProgressObserved {
                goal_id: winner_selection.goal.id.clone(),
                evidence: progress_evidence.clone(),
                tick: 5,
            },
        );

        let dynamic_after_progress = state.goals.get(&winner_selection.goal.id).unwrap();
        assert_eq!(
            dynamic_after_progress.reinforcement_count, 1,
            "reinforcement_count must increment on GoalProgressObserved"
        );
        assert!(
            dynamic_after_progress
                .progress_evidence_refs
                .contains(&progress_evidence),
            "evidence ref must be recorded in progress_evidence_refs"
        );

        let turn5_id = record_turn(
            context,
            5,
            "outcome",
            &state,
            json!({
                "goal_id": winner_selection.goal.id,
                "arbitration_result": arbitration_result_detail(arb_result),
                "dynamic_state": dynamic_state_snapshot(state.goals.get(&winner_selection.goal.id)),
                "executed_effects": 0,
            }),
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 5,
                "phase": "outcome",
                "trace_id": turn5_id,
                "goal_id": winner_selection.goal.id,
                "executed_effects": 0,
            }),
            Some(turn5_id),
        )?;

        write_execution_report(context, &fixture, &state, &candidate_id, &initiative_output)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Replayed 5 scripted turns through the bounded initiative execution pipeline. \
                 Candidate '{}' proposed, accepted, and wired into selector. \
                 Arbitration winner: '{}' (tier {}). \
                 InitiativeExecuted applied with output variant '{}'. \
                 GoalProgressObserved confirmed lifecycle advance. \
                 executed_effects=0 on every turn.",
                candidate_id,
                arb_result.winner.goal.id,
                arb_result.winner_effective_tier,
                initiative_output_kind(&initiative_output),
            ),
            observations: vec![
                format!(
                    "propose_goal_candidates derived activation_keywords {:?} from tension id parts.",
                    proposal.candidates[0].activation_keywords()
                ),
                "GoalCandidateAccepted inserts GoalDynamicState::initial() into state.goals — same map as fixture goals.".to_string(),
                "select_goals_with_salience returns accepted candidate when input matches its derived keywords.".to_string(),
                "arbitrate() respects fixture tension tiers for accepted candidates; tier ordering is preserved.".to_string(),
                "execute_initiative is deterministic: same initiative + goal → same InitiativeOutput.".to_string(),
                "InitiativeExecuted sets goal to Active and records last_initiative_output.".to_string(),
                "GoalProgressObserved advances lifecycle; evidence ref preserved in progress_evidence_refs.".to_string(),
                "executed_effects=0 on every turn (no external write-capable action).".to_string(),
            ],
            failure_modes: vec![
                "Accepted candidate keywords are derived from tension id parts; a tension id with unusual segmentation could produce unexpected keywords.".to_string(),
                "execute_initiative produces structural output only — the runtime must still decide whether to act on it.".to_string(),
            ],
            follow_up_questions: vec![
                "Should InitiativeOutput be surfaced in the session report or only in traces?".to_string(),
                "When should last_initiative_output be cleared — on next InitiativeExecuted, or on GoalSatisfied?".to_string(),
                "Should accepted candidates with many matched tensions get deduplicated activation_keywords? (currently using BTreeSet)".to_string(),
            ],
            decision_candidates: vec![
                "Confirm the two-map pattern (accepted_candidates for goal data, state.goals for dynamic state) as the durable design for accepted candidate lifecycle management.".to_string(),
            ],
            extra_artifacts: vec!["volition-bounded-initiative-execution-report.md".to_string()],
        })
    }
}

fn record_turn(
    context: &mut RunContext,
    turn: u64,
    phase: &str,
    state: &VolitionState,
    detail_extra: serde_json::Value,
) -> anyhow::Result<uuid::Uuid> {
    let detail = json!({
        "turn": turn,
        "phase": phase,
        "tick": state.tick,
        "pending_candidates": state.pending_candidates.len(),
        "accepted_candidates": state.accepted_candidates.len(),
        "goals_with_dynamic_state": state.goals.len(),
        "extra": detail_extra,
    });

    let trace_record = TraceRecord::new(
        context.experiment_id(),
        "volition-bounded-initiative-turn",
        format!("turn={turn} phase={phase}"),
        format!(
            "tick={} pending={} accepted={} goal_states={}",
            state.tick,
            state.pending_candidates.len(),
            state.accepted_candidates.len(),
            state.goals.len(),
        ),
    )
    .with_details(detail);

    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;
    Ok(trace_id)
}

#[derive(Serialize)]
struct DynamicStateSnapshot {
    status: String,
    salience: i32,
    reinforcement_count: u32,
    last_activated_tick: Option<u64>,
    has_initiative_output: bool,
}

fn dynamic_state_snapshot(dynamic: Option<&GoalDynamicState>) -> serde_json::Value {
    match dynamic {
        Some(d) => serde_json::to_value(DynamicStateSnapshot {
            status: d.status.to_string(),
            salience: d.salience,
            reinforcement_count: d.reinforcement_count,
            last_activated_tick: d.last_activated_tick,
            has_initiative_output: d.last_initiative_output.is_some(),
        })
        .unwrap_or_default(),
        None => json!(null),
    }
}

fn selector_output_detail(result: &SalienceGoalSelectionResult) -> serde_json::Value {
    json!({
        "selected_ids": result.selected.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
        "omitted_ids": result.omitted.iter().map(|o| &o.goal.id).collect::<Vec<_>>(),
        "suppressed_cooldown_ids": result.suppressed_cooldown.iter().map(|o| &o.goal.id).collect::<Vec<_>>(),
        "visible_blocked_ids": result.visible_blocked.iter().map(|o| &o.goal.id).collect::<Vec<_>>(),
    })
}

fn arbitration_result_detail(result: &ArbitrationResult) -> serde_json::Value {
    json!({
        "winner_goal_id": result.winner.goal.id,
        "winner_effective_tier": result.winner_effective_tier,
        "winner_effective_tension_id": result.winner_effective_tension_id,
        "losers": result.losers.iter().map(|l| json!({
            "goal_id": l.selection.goal.id,
            "effective_tier": l.effective_tier,
            "effective_tension_id": l.effective_tension_id,
            "reason": l.reason,
        })).collect::<Vec<_>>(),
    })
}

fn initiative_output_kind(output: &InitiativeOutput) -> &'static str {
    match output {
        InitiativeOutput::ReflectionRequested { .. } => "ReflectionRequested",
        InitiativeOutput::ContextRetrievalRequested { .. } => "ContextRetrievalRequested",
        InitiativeOutput::ExperimentProposed { .. } => "ExperimentProposed",
        InitiativeOutput::OpenThreadSurfaced { .. } => "OpenThreadSurfaced",
    }
}

fn write_execution_report(
    context: &RunContext,
    fixture: &crate::volition::VolitionFixture,
    final_state: &VolitionState,
    candidate_id: &str,
    initiative_output: &InitiativeOutput,
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Bounded Initiative Execution\n\n");
    md.push_str(
        "Scripted 5-turn sequence: proposal → accept → arbitration → execution → outcome. \
        executed_effects=0 on every turn; InitiativeExecuted records structural output only.\n\n",
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

    md.push_str("\n## Accepted Candidate\n\n");
    if let Some(goal) = final_state.accepted_candidates.get(candidate_id) {
        md.push_str(&format!("- **id**: `{}`\n", goal.id));
        md.push_str(&format!("- **title**: {}\n", goal.title));
        md.push_str(&format!(
            "- **tension_ids**: {}\n",
            goal.tension_ids
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        md.push_str(&format!(
            "- **activation_keywords**: {}\n",
            goal.activation_keywords
                .iter()
                .map(|k| format!("`{}`", k.term))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    md.push_str("\n## Initiative Output (Turn 4)\n\n");
    md.push_str(&format!(
        "```json\n{}\n```\n",
        serde_json::to_string_pretty(initiative_output).unwrap_or_default()
    ));

    md.push_str("\n## Final Dynamic State Snapshot\n\n");
    md.push_str("| Goal id | Status | Salience | Reinforcements | Has initiative output |\n");
    md.push_str("|---|---|---:|---:|:---:|\n");
    for (id, dynamic) in &final_state.goals {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            id,
            dynamic.status,
            dynamic.salience,
            dynamic.reinforcement_count,
            if dynamic.last_initiative_output.is_some() {
                "yes"
            } else {
                "no"
            },
        ));
    }

    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str("- [ ] The accepted candidate is visually distinct from fixture goals (identifiable from `source_reference` / `evidence_refs`, not position).\n");
    md.push_str("- [ ] The `InitiativeOutput` for each `AllowedEffect` reads as a plausible internal action.\n");
    md.push_str("- [ ] The trace chain (goal → tension provenance → delta → arbitration → execute_initiative output) answers 'why did this initiative execute?' without external context.\n");
    md.push_str(
        "- [ ] Nothing in the output implies a real external write-capable action occurred.\n",
    );
    md.push_str("- [ ] `executed_effects=0` on every turn.\n");

    fs::write(
        context
            .run_dir()
            .join("volition-bounded-initiative-execution-report.md"),
        md,
    )
    .with_context(|| {
        format!(
            "failed to write execution report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::registry::{ExperimentName, run_experiment_in};
    use crate::context::ContextBudget;
    use crate::volition::{
        AllowedEffect, EvidenceRef, InitiativeOutput, VolitionEvent, VolitionState, apply,
        arbitrate, execute_initiative, propose_goal_candidates, select_goals_with_salience,
        static_fixture,
    };

    #[test]
    fn proposal_turn_candidate_has_non_empty_activation_keywords() {
        let fixture = static_fixture();
        let result = propose_goal_candidates(
            &["Is continuity preserved across sessions?".to_string()],
            &fixture,
        );
        assert_eq!(result.candidates.len(), 1);
        let candidate = &result.candidates[0];
        assert!(
            !candidate.activation_keywords().is_empty(),
            "candidate must carry non-empty activation_keywords"
        );
        assert!(
            candidate
                .activation_keywords()
                .contains(&"continuity".to_string()),
            "'continuity' must be derived from tension id 'continuity-preservation'"
        );
    }

    #[test]
    fn accept_turn_inserts_dynamic_state_and_wires_selector() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        let result = propose_goal_candidates(
            &["Is continuity preserved across sessions?".to_string()],
            &fixture,
        );
        let candidate = &result.candidates[0];
        let candidate_id = candidate.id().to_string();

        state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: candidate.clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: accepted").unwrap();
        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );

        assert!(state.goals.contains_key(&candidate_id));

        let sel = select_goals_with_salience(
            "Is continuity still preserved?",
            &fixture,
            &state,
            ContextBudget::new(4, 200),
        );
        assert!(
            sel.selected.iter().any(|s| s.goal.id == candidate_id),
            "accepted candidate must appear in selector output"
        );
    }

    #[test]
    fn arbitration_turn_tier_ordering_respected_for_accepted_candidate() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        let result = propose_goal_candidates(
            &["Is continuity preserved across sessions?".to_string()],
            &fixture,
        );
        let candidate_id = result.candidates[0].id().to_string();

        state = apply(
            state,
            VolitionEvent::GoalCandidateAdded {
                candidate: result.candidates[0].clone(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: accepted").unwrap();
        state = apply(
            state,
            VolitionEvent::GoalCandidateAccepted {
                goal_id: candidate_id.clone(),
                acceptance_evidence: evidence,
                tick: 2,
            },
        );

        // "complete"/"status" → avoid-overstating-impl-status (tier 1)
        // "continuity" → accepted candidate (tier 5)
        let sel = select_goals_with_salience(
            "Is the status complete and is continuity preserved?",
            &fixture,
            &state,
            ContextBudget::new(4, 200),
        );
        let arb = arbitrate(sel.selected, &fixture)
            .unwrap()
            .qualified
            .unwrap();
        assert_eq!(
            arb.winner.goal.id, "avoid-overstating-impl-status",
            "tier-1 fixture goal must win over tier-5 accepted candidate"
        );
        assert_eq!(arb.winner_effective_tier, 1);
    }

    #[test]
    fn execution_turn_initiative_executed_stores_output() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";
        let goal = fixture.goals.iter().find(|g| g.id == goal_id).unwrap();

        let initiative = crate::volition::InitiativeProposal {
            goal_id: goal_id.to_string(),
            goal_title: goal.title.clone(),
            effect: AllowedEffect::Reflect,
            rationale: "test".to_string(),
            matched_terms: vec!["memory".to_string()],
            scope: goal.scope,
        };
        let output = execute_initiative(&initiative, goal);

        state = apply(
            state,
            VolitionEvent::InitiativeExecuted {
                goal_id: goal_id.to_string(),
                effect: AllowedEffect::Reflect,
                output: output.clone(),
                rationale: "test".to_string(),
                tick: 4,
            },
        );

        let dynamic = state.goals.get(goal_id).unwrap();
        assert_eq!(dynamic.last_initiative_output.as_ref(), Some(&output));
        assert_eq!(dynamic.last_activated_tick, Some(4));
    }

    #[test]
    fn outcome_turn_goal_progress_advances_lifecycle() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        let goal_id = "clarify-weak-evidence-topic";

        state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let evidence = EvidenceRef::try_new("trace: progress").unwrap();
        state = apply(
            state,
            VolitionEvent::GoalProgressObserved {
                goal_id: goal_id.to_string(),
                evidence: evidence.clone(),
                tick: 2,
            },
        );

        let dynamic = state.goals.get(goal_id).unwrap();
        assert_eq!(dynamic.reinforcement_count, 1);
        assert!(dynamic.progress_evidence_refs.contains(&evidence));
    }

    #[test]
    fn execute_initiative_all_effects_produce_correct_output_variants() {
        let fixture = static_fixture();
        let effects_and_goals: &[(AllowedEffect, &str)] = &[
            (AllowedEffect::Reflect, "clarify-weak-evidence-topic"),
            (AllowedEffect::RetrieveContext, "resurface-open-thread"),
            (
                AllowedEffect::ProposeExperiment,
                "propose-followup-experiment",
            ),
            (AllowedEffect::SurfaceOpenThread, "resurface-open-thread"),
        ];

        for (effect, goal_id) in effects_and_goals {
            let goal = fixture.goals.iter().find(|g| g.id == *goal_id).unwrap();
            let initiative = crate::volition::InitiativeProposal {
                goal_id: goal_id.to_string(),
                goal_title: goal.title.clone(),
                effect: *effect,
                rationale: "test".to_string(),
                matched_terms: vec!["test".to_string()],
                scope: goal.scope,
            };
            let output = execute_initiative(&initiative, goal);
            match (effect, &output) {
                (AllowedEffect::Reflect, InitiativeOutput::ReflectionRequested { .. }) => {}
                (
                    AllowedEffect::RetrieveContext,
                    InitiativeOutput::ContextRetrievalRequested { .. },
                ) => {}
                (AllowedEffect::ProposeExperiment, InitiativeOutput::ExperimentProposed { .. }) => {
                }
                (AllowedEffect::SurfaceOpenThread, InitiativeOutput::OpenThreadSurfaced { .. }) => {
                }
                _ => panic!("wrong output variant for effect {effect:?}: {output:?}"),
            }
        }
    }

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-bounded-initiative-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(
            &base_dir,
            ExperimentName::VolitionBoundedInitiativeExecution,
        )
        .unwrap();

        assert_eq!(
            summary.experiment_id,
            "volition-bounded-initiative-execution"
        );
        assert_eq!(summary.status, "completed");

        let report = std::fs::read_to_string(summary.run_dir.join("Report.md")).unwrap();
        assert!(report.contains("volition-bounded-initiative-execution"));

        let artifact = std::fs::read_to_string(
            summary
                .run_dir
                .join("volition-bounded-initiative-execution-report.md"),
        )
        .unwrap();
        assert!(artifact.contains("Human Verification Checklist"));
        assert!(artifact.contains("executed_effects=0"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn all_turns_record_executed_effects_zero() {
        let base_dir = std::env::temp_dir().join(format!(
            "qsf-bounded-initiative-traces-{}",
            uuid::Uuid::new_v4()
        ));
        let summary = run_experiment_in(
            &base_dir,
            ExperimentName::VolitionBoundedInitiativeExecution,
        )
        .unwrap();

        let events = std::fs::read_to_string(summary.run_dir.join("events.jsonl")).unwrap();
        // Parse each JSON line and check the payload.executed_effects field specifically,
        // so description strings containing "executed_effects=0" don't trigger false failures.
        let nonzero_effects_count = events
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|record| {
                record
                    .get("payload")
                    .and_then(|p| p.get("executed_effects"))
                    .and_then(|e| e.as_u64())
                    .map(|e| e != 0)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            nonzero_effects_count, 0,
            "every event payload with executed_effects must record 0"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn replay_produces_identical_turn3_arbitration_trace_detail() {
        let base1 = std::env::temp_dir().join(format!("qsf-bounded-det1-{}", uuid::Uuid::new_v4()));
        let base2 = std::env::temp_dir().join(format!("qsf-bounded-det2-{}", uuid::Uuid::new_v4()));

        let summary1 =
            run_experiment_in(&base1, ExperimentName::VolitionBoundedInitiativeExecution).unwrap();
        let summary2 =
            run_experiment_in(&base2, ExperimentName::VolitionBoundedInitiativeExecution).unwrap();

        let turn3_detail = |run_dir: &std::path::Path| -> serde_json::Value {
            let traces = std::fs::read_to_string(run_dir.join("traces.jsonl")).unwrap();
            traces
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find(|v| {
                    v.get("details")
                        .and_then(|d| d.get("turn"))
                        .and_then(|t| t.as_u64())
                        == Some(3)
                })
                .and_then(|v| v.get("details").cloned())
                .unwrap_or_default()
        };

        let detail1 = turn3_detail(&summary1.run_dir);
        let detail2 = turn3_detail(&summary2.run_dir);

        let winner_goal_id = |d: &serde_json::Value| {
            d.get("arbitration_result")
                .and_then(|a| a.get("winner_goal_id"))
                .cloned()
        };
        let winner_tier = |d: &serde_json::Value| {
            d.get("arbitration_result")
                .and_then(|a| a.get("winner_effective_tier"))
                .cloned()
        };
        let loser_count = |d: &serde_json::Value| {
            d.get("arbitration_result")
                .and_then(|a| a.get("losers"))
                .and_then(|l| l.as_array())
                .map(|l| l.len())
        };

        assert_eq!(
            winner_goal_id(&detail1),
            winner_goal_id(&detail2),
            "turn 3 arbitration winner_goal_id must be identical across replays"
        );
        assert_eq!(
            winner_tier(&detail1),
            winner_tier(&detail2),
            "turn 3 winner_effective_tier must be identical across replays"
        );
        assert_eq!(
            loser_count(&detail1),
            loser_count(&detail2),
            "turn 3 loser count must be identical across replays"
        );
        assert_eq!(
            detail1.get("selector").and_then(|s| s.get("selected_ids")),
            detail2.get("selector").and_then(|s| s.get("selected_ids")),
            "turn 3 selector selected_ids must be identical across replays"
        );

        std::fs::remove_dir_all(base1).unwrap();
        std::fs::remove_dir_all(base2).unwrap();
    }
}
