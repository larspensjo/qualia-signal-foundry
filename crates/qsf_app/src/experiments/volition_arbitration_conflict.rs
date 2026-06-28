use std::fs;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    ArbitrationResult, SalienceGoalSelectionResult, VolitionEvent, VolitionState, apply, arbitrate,
    select_goals_with_salience, static_fixture, tick_events,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

// Budget sized to allow 3+ goals in the same turn for conflict_resolved turns.
const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 4,
    max_estimated_tokens: 100,
};

pub struct VolitionArbitrationConflictExperiment;

impl Experiment for VolitionArbitrationConflictExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionArbitrationConflict
    }

    fn description(&self) -> &'static str {
        "Replay a scripted multi-turn sequence exercising no_selection, single_selection, and conflict_resolved arbitration outcomes — no effect is executed"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        let mut turn_summaries: Vec<TurnSummary> = Vec::new();
        let executed_effects = 0usize;

        // Scripted turns covering all three required arbitration_status outcomes.
        //
        // Turn 1 — no_selection: generic direct-task input; no goal keywords match.
        // Turn 2 — single_selection: "memory" matches clarify-weak-evidence-topic only
        //           (research-curiosity, tier 7).
        // Turn 3 — conflict_resolved: "complete"/"status" match avoid-overstating-impl-status
        //           (boundary-preservation, tier 1); "continuity"/"thread" match
        //           resurface-open-thread (continuity-preservation, tier 5);
        //           "evidence" matches clarify-weak-evidence-topic (research-curiosity, tier 7).
        //           Three goals compete; avoid-overstating-impl-status wins at tier 1.
        let scripted_turns: &[&str] = &[
            "What is two plus two?",
            "What is the current research direction for the memory system?",
            "Is the continuity thread complete enough to be confident in the evidence?",
        ];

        for (turn_index, &input) in scripted_turns.iter().enumerate() {
            let turn_number = (turn_index + 1) as u64;

            context.record_event(
                EventType::InputReceived,
                json!({ "turn": turn_number, "input": input }),
                None,
            )?;

            // Advance the tick and apply any tick-driven lifecycle events.
            // TickAdvanced is always applied last so state.tick increases even
            // when no lifecycle events are emitted (e.g. all goals at zero salience).
            let new_tick = state.tick + 1;
            for event in tick_events(&state, &fixture, new_tick) {
                state = apply(state, event);
            }
            state = apply(state, VolitionEvent::TickAdvanced { tick: new_tick });

            let selection = select_goals_with_salience(input, &fixture, &state, BUDGET);
            let arbitration_result = arbitrate(selection.selected.clone(), &fixture);
            let arbitration_status = arbitration_status_label(&arbitration_result);

            let turn_detail = json!({
                "turn": turn_number,
                "input": input,
                "tick": state.tick,
                "arbitration_status": arbitration_status,
                "selection": selection_snapshot(&selection),
                "arbitration_result": arbitration_snapshot(&arbitration_result),
                "executed_effects": 0,
            });

            let trace_record = TraceRecord::new(
                context.experiment_id(),
                "volition-arbitration-turn",
                format!("turn={turn_number} input={input}"),
                format!(
                    "arbitration_status={arbitration_status} selected={} executed_effects=0",
                    selection.selected.len(),
                ),
            )
            .with_details(turn_detail);

            let trace_id = trace_record.trace_id;
            context.record_trace(trace_record)?;

            context.record_event(
                EventType::TraceRecorded,
                json!({
                    "turn": turn_number,
                    "operation": "volition-arbitration-turn",
                    "trace_id": trace_id,
                    "tick": state.tick,
                    "arbitration_status": arbitration_status,
                    "selected": selection.selected.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
                    "winner": arbitration_result.as_ref().map(|r| r.winner.goal.id.as_str()),
                    "loser_count": arbitration_result.as_ref().map(|r| r.losers.len()).unwrap_or(0),
                    "executed_effects": 0,
                }),
                Some(trace_id),
            )?;

            turn_summaries.push(TurnSummary {
                turn: turn_number,
                input: input.to_string(),
                tick: state.tick,
                arbitration_status: arbitration_status.to_string(),
                selected_ids: selection
                    .selected
                    .iter()
                    .map(|s| s.goal.id.clone())
                    .collect(),
                winner_id: arbitration_result
                    .as_ref()
                    .map(|r| r.winner.goal.id.clone()),
                winner_tier: arbitration_result.as_ref().map(|r| r.winner_effective_tier),
                winner_tension_id: arbitration_result
                    .as_ref()
                    .map(|r| r.winner_effective_tension_id.clone()),
                loser_ids: arbitration_result
                    .as_ref()
                    .map(|r| {
                        r.losers
                            .iter()
                            .map(|l| l.selection.goal.id.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }

        let conflict_turns = turn_summaries
            .iter()
            .filter(|t| t.arbitration_status == "conflict_resolved")
            .count();

        write_arbitration_report(context, &fixture, &turn_summaries)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Replayed {} scripted turns through the arbitration pipeline. \
                 Arbitration status coverage: no_selection={} single_selection={} conflict_resolved={}. \
                 Executed effects: {executed_effects}.",
                turn_summaries.len(),
                turn_summaries.iter().filter(|t| t.arbitration_status == "no_selection").count(),
                turn_summaries.iter().filter(|t| t.arbitration_status == "single_selection").count(),
                conflict_turns,
            ),
            observations: vec![
                "arbitration_status is recorded on every turn: no_selection, single_selection, and conflict_resolved each appear at least once.".to_string(),
                "executed_effects=0 on every turn (explicit no-execution marker).".to_string(),
                "In conflict_resolved turns, boundary-preservation (tier 1) outranks continuity-preservation (tier 5) and research-curiosity (tier 7).".to_string(),
                "arbitrate() is a pure function: given the same fixture and selections, output is deterministic across runs.".to_string(),
            ],
            failure_modes: vec![
                "arbitration_tier values are hand-assigned constants; a new tension added without assigning a tier defaults to u8::MAX and loses all conflict.".to_string(),
                "Conflict detection relies on keyword-based selection; inputs that do not contain activation keywords from multiple tensions will not produce conflict turns.".to_string(),
            ],
            follow_up_questions: vec![
                "Should the arbitration result be wired into the pre-initiative trace so the full select→arbitrate→trace chain is visible in one record?".to_string(),
                "Are the fixture tier assignments (1, 4, 5, 7) well-calibrated for the actual tensions, or should any be adjusted?".to_string(),
                "Should tiers 2, 3, 6, 8 be covered by placeholder tensions now, or only when a concrete experiment needs them?".to_string(),
            ],
            decision_candidates: vec![
                "Promote arbitration_tier assignments to the decision log once the tier hierarchy is validated across multiple experiments.".to_string(),
                "Consider adding an explicit no-execution marker field to ArbitrationResult for stronger contract enforcement.".to_string(),
            ],
            extra_artifacts: vec!["volition-arbitration-report.md".to_string()],
        })
    }
}

fn arbitration_status_label(result: &Option<ArbitrationResult>) -> &'static str {
    match result {
        None => "no_selection",
        Some(r) if r.losers.is_empty() => "single_selection",
        Some(_) => "conflict_resolved",
    }
}

#[derive(Clone, Debug, Serialize)]
struct SelectionSnapshot {
    selected: Vec<String>,
    omitted: Vec<String>,
    suppressed_cooldown: Vec<String>,
    visible_blocked: Vec<String>,
}

fn selection_snapshot(result: &SalienceGoalSelectionResult) -> SelectionSnapshot {
    SelectionSnapshot {
        selected: result.selected.iter().map(|s| s.goal.id.clone()).collect(),
        omitted: result.omitted.iter().map(|s| s.goal.id.clone()).collect(),
        suppressed_cooldown: result
            .suppressed_cooldown
            .iter()
            .map(|s| s.goal.id.clone())
            .collect(),
        visible_blocked: result
            .visible_blocked
            .iter()
            .map(|s| s.goal.id.clone())
            .collect(),
    }
}

#[derive(Clone, Debug, Serialize)]
struct ArbitrationSnapshot {
    winner_goal_id: Option<String>,
    winner_effective_tier: Option<u8>,
    winner_effective_tension_id: Option<String>,
    losers: Vec<LoserSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
struct LoserSnapshot {
    goal_id: String,
    effective_tier: u8,
    effective_tension_id: String,
    reason: String,
}

fn arbitration_snapshot(result: &Option<ArbitrationResult>) -> ArbitrationSnapshot {
    match result {
        None => ArbitrationSnapshot {
            winner_goal_id: None,
            winner_effective_tier: None,
            winner_effective_tension_id: None,
            losers: vec![],
        },
        Some(r) => ArbitrationSnapshot {
            winner_goal_id: Some(r.winner.goal.id.clone()),
            winner_effective_tier: Some(r.winner_effective_tier),
            winner_effective_tension_id: Some(r.winner_effective_tension_id.clone()),
            losers: r
                .losers
                .iter()
                .map(|l| LoserSnapshot {
                    goal_id: l.selection.goal.id.clone(),
                    effective_tier: l.effective_tier,
                    effective_tension_id: l.effective_tension_id.clone(),
                    reason: l.reason.clone(),
                })
                .collect(),
        },
    }
}

#[derive(Clone, Debug)]
struct TurnSummary {
    turn: u64,
    input: String,
    tick: u64,
    arbitration_status: String,
    selected_ids: Vec<String>,
    winner_id: Option<String>,
    winner_tier: Option<u8>,
    winner_tension_id: Option<String>,
    loser_ids: Vec<String>,
}

fn write_arbitration_report(
    context: &RunContext,
    fixture: &crate::volition::VolitionFixture,
    turns: &[TurnSummary],
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Arbitration and Conflict Resolution\n\n");
    md.push_str(
        "Scripted multi-turn replay exercising `no_selection`, `single_selection`, and \
        `conflict_resolved` arbitration outcomes. No effect was executed.\n\n",
    );

    md.push_str("## Fixture Tensions and Tiers\n\n");
    md.push_str("| Tension | Tier | Priority bias |\n");
    md.push_str("|---|---:|---|\n");
    for tension in &fixture.tensions {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            tension.id, tension.arbitration_tier, tension.priority_bias
        ));
    }

    md.push_str("\n## Per-Turn Arbitration Trace\n\n");
    md.push_str("| Turn | Tick | Input | Status | Selected | Winner | Winner tier | Winner tension | Losers |\n");
    md.push_str("|---:|---:|---|---|---|---|---:|---|---|\n");
    for turn in turns {
        md.push_str(&format!(
            "| {} | {} | {} | `{}` | {} | {} | {} | {} | {} |\n",
            turn.turn,
            turn.tick,
            turn.input,
            turn.arbitration_status,
            join_or_dash(&turn.selected_ids),
            turn.winner_id.as_deref().unwrap_or("-"),
            turn.winner_tier
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".to_string()),
            turn.winner_tension_id.as_deref().unwrap_or("-"),
            join_or_dash(&turn.loser_ids),
        ));
    }

    md.push_str("\n## Human Verification Checklist\n\n");
    md.push_str("- [ ] `boundary-preservation` (tier 1) consistently outranks `continuity-preservation` (tier 5) and `research-curiosity` (tier 7) in conflict turns.\n");
    md.push_str("- [ ] The `no_selection` and `single_selection` turns are clearly distinguishable from `conflict_resolved` — absent output does not look like a silent failure.\n");
    md.push_str("- [ ] Every loser's `reason` field answers \"why did X lose to Y?\" from tier numbers and tension names alone.\n");
    md.push_str("- [ ] `executed_effects=0` on every turn.\n");

    md.push_str("\n## Notes\n\n");
    md.push_str("- Arbitration is a pure, composable layer over `select_goals_with_salience`; no existing selector or reducer was modified.\n");
    md.push_str("- Tiers 2 (user intent), 3 (task completion), 6 (experiment mode), and 8 (optional exploration) are not yet covered by any fixture tension.\n");
    md.push_str("- A goal with no parent tensions in the fixture defaults to effective tier `u8::MAX` and will always lose arbitration.\n");

    fs::write(context.run_dir().join("volition-arbitration-report.md"), md).with_context(|| {
        format!(
            "failed to write arbitration report for run {}",
            context.run_id()
        )
    })
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::BUDGET;
    use crate::volition::{
        VolitionEvent, VolitionState, apply, arbitrate, select_goals_with_salience, static_fixture,
        tick_events,
    };

    #[test]
    fn ticks_increase_monotonically_across_scripted_turns() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        let scripted_turns: &[&str] = &[
            "What is two plus two?",
            "What is the current research direction for the memory system?",
            "Is the continuity thread complete enough to be confident in the evidence?",
        ];

        let mut prev_tick = state.tick;
        for (turn_index, &input) in scripted_turns.iter().enumerate() {
            let new_tick = state.tick + 1;
            for event in tick_events(&state, &fixture, new_tick) {
                state = apply(state, event);
            }
            state = apply(state, VolitionEvent::TickAdvanced { tick: new_tick });

            let _ = select_goals_with_salience(input, &fixture, &state, BUDGET);

            assert!(
                state.tick > prev_tick,
                "tick must increase on turn {}; was {prev_tick}, now {}",
                turn_index + 1,
                state.tick,
            );
            prev_tick = state.tick;
        }

        assert_eq!(state.tick, scripted_turns.len() as u64);
    }

    #[test]
    fn turn1_produces_no_selection() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = select_goals_with_salience("What is two plus two?", &fixture, &state, BUDGET);
        assert!(result.selected.is_empty(), "turn 1 must select nothing");
        let arb = arbitrate(result.selected, &fixture);
        assert!(arb.is_none());
    }

    #[test]
    fn turn2_produces_single_selection() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = select_goals_with_salience(
            "What is the current research direction for the memory system?",
            &fixture,
            &state,
            BUDGET,
        );
        assert_eq!(
            result.selected.len(),
            1,
            "turn 2 must select exactly one goal"
        );
        let arb = arbitrate(result.selected, &fixture).unwrap();
        assert!(arb.losers.is_empty(), "single selection has no losers");
        assert_eq!(arb.winner.goal.id, "clarify-weak-evidence-topic");
    }

    #[test]
    fn turn3_produces_conflict_resolved_with_boundary_preservation_winner() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = select_goals_with_salience(
            "Is the continuity thread complete enough to be confident in the evidence?",
            &fixture,
            &state,
            BUDGET,
        );
        assert!(
            result.selected.len() >= 2,
            "turn 3 must select at least 2 goals for conflict; got {}",
            result.selected.len()
        );
        let arb = arbitrate(result.selected, &fixture).unwrap();
        assert!(!arb.losers.is_empty(), "conflict turn must have losers");
        assert_eq!(arb.winner.goal.id, "avoid-overstating-impl-status");
        assert_eq!(arb.winner_effective_tier, 1);
        assert_eq!(arb.winner_effective_tension_id, "boundary-preservation");
    }

    #[test]
    fn experiment_output_is_deterministic_across_runs() {
        let run = || {
            let fixture = static_fixture();
            let state = VolitionState::from_fixture(&fixture);
            let inputs = [
                "What is two plus two?",
                "What is the current research direction for the memory system?",
                "Is the continuity thread complete enough to be confident in the evidence?",
            ];
            inputs
                .iter()
                .map(|input| {
                    let result = select_goals_with_salience(input, &fixture, &state, BUDGET);
                    serde_json::to_string(&arbitrate(result.selected, &fixture)).unwrap()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            run(),
            run(),
            "arbitration output must be identical across runs"
        );
    }
}
