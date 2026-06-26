use std::fs;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    COOLDOWN_SPAN_TICKS, EvidenceRef, RETIREMENT_INACTIVITY_TICKS, SalienceGoalSelectionResult,
    VolitionEvent, VolitionState, apply, select_goals_with_salience, static_fixture, tick_events,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 2,
    max_estimated_tokens: 80,
};

pub struct VolitionSalienceAndSatisfactionExperiment;

impl Experiment for VolitionSalienceAndSatisfactionExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionSalienceAndSatisfaction
    }

    fn description(&self) -> &'static str {
        "Replay a scripted multi-turn input/event sequence to exercise salience rise/decay, satisfaction cooldown, blocked visibility, and retirement"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        let mut turn_summaries: Vec<TurnSummary> = Vec::new();
        let executed_effects = 0usize;
        let mut total_evidence_backed_events = 0usize;
        let mut cooldown_suppressed_count = 0usize;
        let mut blocked_visible_count = 0usize;
        let mut retired_count = 0usize;

        // Scripted turns designed so that in one standard run:
        //   - clarify-weak-evidence-topic is activated, progressed, satisfied → cooldown → returned
        //   - resurface-open-thread is activated then blocked
        //   - avoid-overstating-impl-status stays inert (no matching input) → retires
        //   - propose-followup-experiment is activated, decays
        // Each event in the scripted turns carries a unique, strictly increasing tick so
        // that tick advancement is unambiguous across the full sequence.
        // Turn 1 scripted events: ticks 2, 3 (tick-driven fires at 1)
        // Turn 2 scripted events: tick 5  (tick-driven fires at 4)
        // Turn 3 scripted events: tick 7  (tick-driven fires at 6)
        //   → cooldown_until = 7 + COOLDOWN_SPAN_TICKS
        // Turn 4 scripted events: ticks 9, 10 (tick-driven fires at 8)
        // Turn 5 scripted events: none; tick-driven fires GoalCooldownElapsed at 11
        //   (state.tick=10 after Turn 4, new_tick=11 ≥ cooldown_until=10)
        let scripted_turns: Vec<(&str, Vec<VolitionEvent>)> = vec![
            // Turn 1: activate clarify-weak-evidence-topic and propose-followup-experiment
            (
                "We never settled how voice memory affects the experiment continuity.",
                vec![
                    VolitionEvent::GoalActivated {
                        goal_id: "clarify-weak-evidence-topic".to_string(),
                        tick: 2,
                    },
                    VolitionEvent::GoalActivated {
                        goal_id: "propose-followup-experiment".to_string(),
                        tick: 3,
                    },
                ],
            ),
            // Turn 2: more evidence, progress clarify goal
            (
                "The voice memory evidence question is still unresolved.",
                vec![VolitionEvent::GoalProgressObserved {
                    goal_id: "clarify-weak-evidence-topic".to_string(),
                    evidence: EvidenceRef::try_new(
                        "docs/Experiments/Experiment.VolitionGoalFixture.md",
                    )
                    .expect("valid evidence ref"),
                    tick: 5,
                }],
            ),
            // Turn 3: satisfy clarify goal (enters cooldown)
            (
                "We now have a clear narrow question about voice memory evidence.",
                vec![VolitionEvent::GoalSatisfied {
                    goal_id: "clarify-weak-evidence-topic".to_string(),
                    evidence: EvidenceRef::try_new(
                        "docs/Experiments/Experiment.VolitionTraceBackedInitiative.md",
                    )
                    .expect("valid evidence ref"),
                    tick: 7,
                }],
            ),
            // Turn 4: activate resurface-open-thread, then block it
            (
                "There is an open thread about continuity that revisit is blocked on.",
                vec![
                    VolitionEvent::GoalActivated {
                        goal_id: "resurface-open-thread".to_string(),
                        tick: 9,
                    },
                    VolitionEvent::GoalBlocked {
                        goal_id: "resurface-open-thread".to_string(),
                        tick: 10,
                    },
                ],
            ),
            // Turn 5: no scripted events; tick-driven fires GoalCooldownElapsed because
            // new_tick (11) >= cooldown_until (7 + COOLDOWN_SPAN_TICKS = 10).
            ("Let us revisit the experiment plan.", vec![]),
        ];

        for (turn_index, (input, turn_events)) in scripted_turns.iter().enumerate() {
            let turn_number = (turn_index + 1) as u64;

            context.record_event(
                EventType::InputReceived,
                json!({
                    "turn": turn_number,
                    "input": input,
                }),
                None,
            )?;

            // Advance tick and apply any tick-driven events first.
            let new_tick = state.tick + 1;
            let driven_events = tick_events(&state, new_tick);
            for event in driven_events {
                state = apply(state, event);
            }

            // Apply the scripted turn events.
            for event in turn_events {
                match event {
                    VolitionEvent::GoalProgressObserved { .. }
                    | VolitionEvent::GoalSatisfied { .. } => {
                        total_evidence_backed_events += 1;
                    }
                    _ => {}
                }
                state = apply(state, event.clone());
            }

            // Run the salience-aware selector.
            let selection = select_goals_with_salience(input, &fixture, &state, BUDGET);
            cooldown_suppressed_count += selection.suppressed_cooldown.len();
            blocked_visible_count += selection.visible_blocked.len();

            let state_snapshot: Vec<GoalStateSnapshot> = state
                .goals
                .iter()
                .map(|(id, dynamic)| GoalStateSnapshot {
                    goal_id: id.clone(),
                    status: dynamic.status.to_string(),
                    salience: dynamic.salience,
                    reinforcement_count: dynamic.reinforcement_count,
                    evidence_ref_count: dynamic.progress_evidence_refs.len(),
                    last_activated_tick: dynamic.last_activated_tick,
                    last_satisfied_tick: dynamic.last_satisfied_tick,
                    cooldown_until_tick: dynamic.cooldown_until_tick,
                })
                .collect();

            retired_count += state_snapshot
                .iter()
                .filter(|s| s.status == "retired")
                .count();

            let trace_record = TraceRecord::new(
                context.experiment_id(),
                "volition-salience-turn",
                format!("turn={turn_number} input={input}"),
                format!(
                    "selected={} suppressed={} blocked={} tick={}",
                    selection.selected.len(),
                    selection.suppressed_cooldown.len(),
                    selection.visible_blocked.len(),
                    state.tick,
                ),
            )
            .with_details(json!({
                "turn": turn_number,
                "input": input,
                "tick": state.tick,
                "selection": selection_snapshot(&selection),
                "state_snapshot": state_snapshot,
                "executed_effects": 0,
            }));

            let trace_id = trace_record.trace_id;
            context.record_trace(trace_record)?;

            context.record_event(
                EventType::TraceRecorded,
                json!({
                    "turn": turn_number,
                    "operation": "volition-salience-turn",
                    "trace_id": trace_id,
                    "tick": state.tick,
                    "selected": selection.selected.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
                    "suppressed_cooldown": selection.suppressed_cooldown.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
                    "visible_blocked": selection.visible_blocked.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
                    "executed_effects": 0,
                }),
                Some(trace_id),
            )?;

            turn_summaries.push(TurnSummary {
                turn: turn_number,
                input: input.to_string(),
                tick: state.tick,
                selected_ids: selection
                    .selected
                    .iter()
                    .map(|s| s.goal.id.clone())
                    .collect(),
                suppressed_cooldown_ids: selection
                    .suppressed_cooldown
                    .iter()
                    .map(|s| s.goal.id.clone())
                    .collect(),
                visible_blocked_ids: selection
                    .visible_blocked
                    .iter()
                    .map(|s| s.goal.id.clone())
                    .collect(),
                state_snapshot,
            });
        }

        // Advance ticks until retirement threshold to trigger retirement of inert goals.
        let retire_tick = RETIREMENT_INACTIVITY_TICKS + state.tick + 1;
        let driven_events = tick_events(&state, retire_tick);
        let mut retirement_event_ids: Vec<String> = Vec::new();
        for event in &driven_events {
            if let VolitionEvent::GoalRetired { goal_id, .. } = event {
                retired_count += 1;
                retirement_event_ids.push(goal_id.clone());
            }
            state = apply(state, event.clone());
        }

        // Record the retirement phase as a final turn summary so the report shows the
        // lifecycle transition rather than leaving it invisible after the scripted turns.
        let retirement_state_snapshot: Vec<GoalStateSnapshot> = state
            .goals
            .iter()
            .map(|(id, dynamic)| GoalStateSnapshot {
                goal_id: id.clone(),
                status: dynamic.status.to_string(),
                salience: dynamic.salience,
                reinforcement_count: dynamic.reinforcement_count,
                evidence_ref_count: dynamic.progress_evidence_refs.len(),
                last_activated_tick: dynamic.last_activated_tick,
                last_satisfied_tick: dynamic.last_satisfied_tick,
                cooldown_until_tick: dynamic.cooldown_until_tick,
            })
            .collect();

        let retirement_trace = TraceRecord::new(
            context.experiment_id(),
            "volition-salience-turn",
            format!("turn=retirement tick={retire_tick} input=(maintenance)"),
            format!("retired={} tick={}", retirement_event_ids.len(), state.tick,),
        )
        .with_details(json!({
            "turn": "retirement",
            "input": "(maintenance)",
            "tick": state.tick,
            "selection": serde_json::Value::Null,
            "state_snapshot": retirement_state_snapshot,
            "executed_effects": 0,
        }));
        let retirement_trace_id = retirement_trace.trace_id;
        context.record_trace(retirement_trace)?;

        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": "retirement",
                "operation": "volition-salience-turn",
                "trace_id": retirement_trace_id,
                "tick": state.tick,
                "retired": retirement_event_ids,
            }),
            Some(retirement_trace_id),
        )?;

        turn_summaries.push(TurnSummary {
            turn: (turn_summaries.len() as u64) + 1,
            input: "(maintenance)".to_string(),
            tick: state.tick,
            selected_ids: Vec::new(),
            suppressed_cooldown_ids: Vec::new(),
            visible_blocked_ids: Vec::new(),
            state_snapshot: retirement_state_snapshot,
        });

        write_salience_report(context, &fixture, &turn_summaries)?;
        write_state_snapshot(context, &state)?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Replayed {turns} scripted turns through the salience/satisfaction/blocking/cooldown lifecycle. \
                 Evidence-backed events: {total_evidence_backed_events}. \
                 Cooldown suppressions: {cooldown_suppressed_count}. \
                 Blocked-but-visible: {blocked_visible_count}. \
                 Retired goals: {retired_count}. \
                 Executed effects: {executed_effects}.",
                turns = turn_summaries.len(),
            ),
            observations: vec![
                "Salience rose on GoalActivated and GoalProgressObserved; GoalDecayed lowered it deterministically.".to_string(),
                "GoalSatisfied reset salience to zero and entered cooldown; GoalCooldownElapsed returned the goal to Accepted.".to_string(),
                "Blocked goals remained visible in visible_blocked and were not selected.".to_string(),
                "Inert goals with zero salience and no reinforcement retired after the inactivity threshold.".to_string(),
                format!("No initiative effect was executed (executed_effects={executed_effects})."),
            ],
            failure_modes: vec![
                "Delta detection remains keyword-based; salience rises on any keyword match regardless of semantic depth.".to_string(),
                "Retirement threshold and cooldown span are fixed constants; adaptive thresholds are deferred to a later slice.".to_string(),
            ],
            follow_up_questions: vec![
                "Does rising salience materially change which goals are selected over a realistic multi-turn session?".to_string(),
                format!("Are the default retirement ({RETIREMENT_INACTIVITY_TICKS} ticks) and cooldown ({COOLDOWN_SPAN_TICKS} ticks) spans well-calibrated?"),
                "Should blocked-goal visibility surface to the bounded-initiative layer, or only feed the trace?".to_string(),
            ],
            decision_candidates: vec![
                "Record lifecycle snapshots in the pre-initiative trace so arbitration can see per-goal status.".to_string(),
                "Keep EvidenceRef as the gating primitive for progress/satisfaction; do not accept free-form strings.".to_string(),
            ],
            extra_artifacts: vec![
                "volition-salience-report.md".to_string(),
                "volition-state-snapshot.json".to_string(),
            ],
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct GoalStateSnapshot {
    goal_id: String,
    status: String,
    salience: i32,
    reinforcement_count: u32,
    evidence_ref_count: usize,
    last_activated_tick: Option<u64>,
    last_satisfied_tick: Option<u64>,
    cooldown_until_tick: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct TurnSummary {
    turn: u64,
    input: String,
    tick: u64,
    selected_ids: Vec<String>,
    suppressed_cooldown_ids: Vec<String>,
    visible_blocked_ids: Vec<String>,
    state_snapshot: Vec<GoalStateSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
struct SelectionSnapshot {
    selected: Vec<String>,
    suppressed_cooldown: Vec<String>,
    visible_blocked: Vec<String>,
    omitted: Vec<String>,
}

fn selection_snapshot(result: &SalienceGoalSelectionResult) -> SelectionSnapshot {
    SelectionSnapshot {
        selected: result.selected.iter().map(|s| s.goal.id.clone()).collect(),
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
        omitted: result.omitted.iter().map(|s| s.goal.id.clone()).collect(),
    }
}

fn write_state_snapshot(context: &RunContext, state: &VolitionState) -> anyhow::Result<()> {
    let path = context.run_dir().join("volition-state-snapshot.json");
    let contents = serde_json::to_string_pretty(state)?;
    fs::write(&path, contents).with_context(|| {
        format!(
            "failed to write state snapshot for run {}",
            context.run_id()
        )
    })
}

fn write_salience_report(
    context: &RunContext,
    fixture: &crate::volition::VolitionFixture,
    turns: &[TurnSummary],
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Salience and Satisfaction\n\n");
    md.push_str(
        "Scripted multi-turn replay exercising salience, satisfaction, cooldown, blocking, \
        and retirement. No effect was executed.\n\n",
    );

    md.push_str("## Fixture Goals\n\n");
    for goal in &fixture.goals {
        md.push_str(&format!("- `{}` — {}\n", goal.id, goal.summary));
    }

    md.push_str("\n## Per-Turn Salience/Lifecycle Trace\n\n");
    md.push_str("| Turn | Tick | Input | Selected | Suppressed (cooldown) | Visible (blocked) |\n");
    md.push_str("|---:|---:|---|---|---|---|\n");
    for turn in turns {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            turn.turn,
            turn.tick,
            turn.input,
            join_or_dash(&turn.selected_ids),
            join_or_dash(&turn.suppressed_cooldown_ids),
            join_or_dash(&turn.visible_blocked_ids),
        ));
    }

    md.push_str("\n## Per-Turn Goal State\n\n");
    for turn in turns {
        md.push_str(&format!("### Turn {} (tick {})\n\n", turn.turn, turn.tick));
        md.push_str("| Goal | Status | Salience | Reinforcements | Evidence refs |\n");
        md.push_str("|---|---|---:|---:|---:|\n");
        for snapshot in &turn.state_snapshot {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                snapshot.goal_id,
                snapshot.status,
                snapshot.salience,
                snapshot.reinforcement_count,
                snapshot.evidence_ref_count,
            ));
        }
        md.push('\n');
    }

    md.push_str("## Notes\n\n");
    md.push_str("- Cooldown and retirement thresholds are constants, not adaptive.\n");
    md.push_str("- No tick-driven event changes status inside the selector.\n");
    md.push_str("- Blocked goals stay visible as unresolved tensions.\n");

    fs::write(context.run_dir().join("volition-salience-report.md"), md).with_context(|| {
        format!(
            "failed to write salience report for run {}",
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
        EvidenceRef, VolitionEvent, VolitionState, apply, select_goals_with_salience,
        static_fixture, tick_events,
    };

    #[test]
    fn experiment_exercises_cooldown_suppression() {
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

        let input = "We never settled how voice memory affects continuity.";
        let result = select_goals_with_salience(input, &fixture, &state, BUDGET);

        assert!(
            result
                .suppressed_cooldown
                .iter()
                .any(|s| s.goal.id == goal_id),
            "satisfied goal should be in suppressed_cooldown"
        );
        assert!(
            result.selected.iter().all(|s| s.goal.id != goal_id),
            "satisfied goal must not appear in selected"
        );
    }

    #[test]
    fn scripted_run_serializes_identically_across_two_fresh_runs() {
        let run = || {
            let fixture = static_fixture();
            let mut state = VolitionState::from_fixture(&fixture);
            let evidence1 =
                EvidenceRef::try_new("docs/Experiments/Experiment.VolitionGoalFixture.md").unwrap();
            let evidence2 = EvidenceRef::try_new(
                "docs/Experiments/Experiment.VolitionTraceBackedInitiative.md",
            )
            .unwrap();

            let events: Vec<VolitionEvent> = vec![
                VolitionEvent::GoalActivated {
                    goal_id: "clarify-weak-evidence-topic".to_string(),
                    tick: 2,
                },
                VolitionEvent::GoalActivated {
                    goal_id: "propose-followup-experiment".to_string(),
                    tick: 3,
                },
                VolitionEvent::GoalProgressObserved {
                    goal_id: "clarify-weak-evidence-topic".to_string(),
                    evidence: evidence1,
                    tick: 5,
                },
                VolitionEvent::GoalSatisfied {
                    goal_id: "clarify-weak-evidence-topic".to_string(),
                    evidence: evidence2,
                    tick: 7,
                },
                VolitionEvent::GoalActivated {
                    goal_id: "resurface-open-thread".to_string(),
                    tick: 9,
                },
                VolitionEvent::GoalBlocked {
                    goal_id: "resurface-open-thread".to_string(),
                    tick: 10,
                },
            ];
            for event in events {
                let driven = tick_events(&state, state.tick + 1);
                for e in driven {
                    state = apply(state, e);
                }
                state = apply(state, event);
            }
            let retire_tick = crate::volition::RETIREMENT_INACTIVITY_TICKS + state.tick + 1;
            for e in tick_events(&state, retire_tick) {
                state = apply(state, e);
            }
            serde_json::to_string(&state).unwrap()
        };

        assert_eq!(
            run(),
            run(),
            "serialized state must be byte-for-byte identical across runs"
        );
    }

    #[test]
    fn experiment_exercises_blocked_visibility() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let goal_id = "resurface-open-thread";

        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: goal_id.to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: goal_id.to_string(),
                tick: 2,
            },
        );

        let input = "There is an open thread that revisit is blocked on.";
        let result = select_goals_with_salience(input, &fixture, &state, BUDGET);

        assert!(
            result.visible_blocked.iter().any(|s| s.goal.id == goal_id),
            "blocked goal must stay visible"
        );
    }
}
