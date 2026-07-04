use std::fs;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::volition::{
    DeltaAssessment, GoalSelectionResult, PreInitiativeTrace, VolitionFixture,
    build_pre_initiative_traces, select_goals, static_fixture,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 2,
    max_estimated_tokens: 80,
};

const INPUT_A: &str = "Is the goal system implemented yet?";
const INPUT_B: &str = "We never settled how voice memory affects continuity.";
const INPUT_C: &str = "Give me the build command.";
const INPUT_D: &str = "Should we turn the volition note into a tiny experiment?";

pub struct VolitionTraceBackedInitiativeExperiment;

impl Experiment for VolitionTraceBackedInitiativeExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionTraceBackedInitiative
    }

    fn description(&self) -> &'static str {
        "Record pre-initiative traces for selected volition goals before any effect could change behavior"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        write_fixture_snapshot(context, &fixture)?;

        let scripted_inputs = [
            ("input-a", INPUT_A),
            ("input-b", INPUT_B),
            ("input-c", INPUT_C),
            ("input-d", INPUT_D),
        ];

        let mut runs = Vec::new();
        let mut all_traces = Vec::new();
        for (scenario, input) in scripted_inputs {
            context.record_event(
                EventType::InputReceived,
                json!({
                    "scenario": scenario,
                    "input": input,
                    "budget": BUDGET,
                }),
                None,
            )?;

            let (summary, traces) = record_traces(context, scenario, input, &fixture, BUDGET)?;
            runs.push(summary);
            all_traces.extend(traces);
        }

        let executed_effects = all_traces.iter().filter(|trace| trace.executed).count();
        write_traces_artifact(context, &all_traces)?;
        write_trace_report(context, &fixture, &runs)?;

        Ok(ExperimentOutcome {
            summary: "Recorded deterministic pre-initiative traces for each scripted input, connecting selected goal to tension provenance, detected delta, candidate initiatives, and the proposed bounded effect while executing nothing.".to_string(),
            observations: vec![
                "Every selected goal produced a trace linking its detected delta and tension provenance to a proposed bounded effect; losing candidates were recorded with deterministic precedence reasons.".to_string(),
                "The direct-task baseline produced a single trace with an explicit no-delta reason, no candidate effects, and no proposed effect.".to_string(),
                format!("No trace executed an effect (executed_effects={executed_effects})."),
                "Tension provenance is recorded for legibility only; the trace explicitly notes that tension priority did not determine selection.".to_string(),
            ],
            failure_modes: vec![
                "The detected delta still leans on matched keywords plus the goal's own concern summary, so its evidential strength is bounded by the static fixture.".to_string(),
                "Losing-candidate reasons are precedence-based, not semantic; richer rejection reasons are deferred to the arbitration slice.".to_string(),
            ],
            follow_up_questions: vec![
                "Is tension provenance useful in traces before any salience or priority mechanism affects selection?".to_string(),
                "What is the smallest delta representation that stays more informative than a keyword match once event-driven salience exists?".to_string(),
                "Should precedence-based candidate-choice reasons become structured enums when full multi-goal arbitration is introduced?".to_string(),
            ],
            decision_candidates: vec![
                "Require a serialized pre-initiative trace before any future initiative is allowed to influence behavior.".to_string(),
                "Keep candidate choice local to a single goal (first allowed effect wins) until arbitration is built, recording losers as precedence-ordered candidates.".to_string(),
            ],
            extra_artifacts: vec![
                "volition-fixture.json".to_string(),
                "pre-initiative-traces.jsonl".to_string(),
                "volition-trace-backed-initiative.md".to_string(),
            ],
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct TraceRunSummary {
    scenario: String,
    input: String,
    selected_goal_ids: Vec<String>,
    omitted_goals: Vec<String>,
    proposed_effects: Vec<String>,
    losing_candidate_count: usize,
    delta_count: usize,
    no_delta_count: usize,
}

/// Compact projection of the selector result so the trace artifacts preserve why
/// non-selected goals lost, not just the winning goals.
#[derive(Clone, Debug, Serialize)]
struct SelectorSnapshot {
    selected: Vec<SelectedGoalSnapshot>,
    omitted: Vec<OmittedGoalSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
struct SelectedGoalSnapshot {
    goal_id: String,
    relevance_score: f64,
    matched_terms: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct OmittedGoalSnapshot {
    goal_id: String,
    relevance_score: f64,
    matched_terms: Vec<String>,
    reason: String,
}

fn selector_snapshot(selection: &GoalSelectionResult) -> SelectorSnapshot {
    SelectorSnapshot {
        selected: selection
            .selected
            .iter()
            .map(|selected| SelectedGoalSnapshot {
                goal_id: selected.goal.id.clone(),
                relevance_score: selected.relevance_score,
                matched_terms: selected.matched_terms(),
            })
            .collect(),
        omitted: selection
            .omitted
            .iter()
            .map(|omitted| OmittedGoalSnapshot {
                goal_id: omitted.goal.id.clone(),
                relevance_score: omitted.relevance_score,
                matched_terms: omitted.matched_terms.clone(),
                reason: omitted.reason.clone(),
            })
            .collect(),
    }
}

fn record_traces(
    context: &mut RunContext,
    scenario: &str,
    input: &str,
    fixture: &VolitionFixture,
    budget: ContextBudget,
) -> anyhow::Result<(TraceRunSummary, Vec<PreInitiativeTrace>)> {
    let started_at = Instant::now();
    let selection: GoalSelectionResult = select_goals(input, fixture, budget);
    let traces = build_pre_initiative_traces(&selection, fixture);
    let snapshot = selector_snapshot(&selection);
    let elapsed_ns = elapsed_ns(started_at);

    let proposed_effects: Vec<String> = traces
        .iter()
        .filter_map(|trace| trace.choice.as_ref())
        .map(|choice| format!("{} -> {}", choice.proposed.goal_id, choice.proposed.effect))
        .collect();
    let losing_candidate_count: usize = traces
        .iter()
        .filter_map(|trace| trace.choice.as_ref())
        .map(|choice| choice.losing.len())
        .sum();
    let delta_count = traces
        .iter()
        .filter(|trace| matches!(trace.delta, DeltaAssessment::Delta(_)))
        .count();
    let no_delta_count = traces
        .iter()
        .filter(|trace| matches!(trace.delta, DeltaAssessment::NoDelta { .. }))
        .count();

    let trace_record = TraceRecord::new(
        context.experiment_id(),
        "volition-pre-initiative-trace",
        format!("scenario={scenario} input={input}"),
        format!(
            "traces={} proposed={} losing={} delta={} no_delta={}",
            traces.len(),
            proposed_effects.len(),
            losing_candidate_count,
            delta_count,
            no_delta_count
        ),
    )
    .with_details(json!({
        "scenario": scenario,
        "input": input,
        "budget": budget,
        "selector_snapshot": snapshot,
        "pre_initiative_traces": traces,
    }))
    .with_latency_ns(elapsed_ns);
    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;

    context.record_event(
        EventType::TraceRecorded,
        json!({
            "scenario": scenario,
            "operation": "volition-pre-initiative-trace",
            "trace_id": trace_id,
            "selected_goal_ids": selection
                .selected
                .iter()
                .map(|selected| &selected.goal.id)
                .collect::<Vec<_>>(),
            "omitted_goals": selection
                .omitted
                .iter()
                .map(|omitted| json!({ "goal_id": omitted.goal.id, "reason": omitted.reason }))
                .collect::<Vec<_>>(),
            "proposed_effects": proposed_effects,
            "losing_candidate_count": losing_candidate_count,
            "executed_effects": 0,
        }),
        Some(trace_id),
    )?;

    let summary = TraceRunSummary {
        scenario: scenario.to_string(),
        input: input.to_string(),
        selected_goal_ids: selection
            .selected
            .iter()
            .map(|selected| selected.goal.id.clone())
            .collect(),
        omitted_goals: selection
            .omitted
            .iter()
            .map(|omitted| format!("{} ({})", omitted.goal.id, omitted.reason))
            .collect(),
        proposed_effects,
        losing_candidate_count,
        delta_count,
        no_delta_count,
    };

    Ok((summary, traces))
}

fn write_fixture_snapshot(context: &RunContext, fixture: &VolitionFixture) -> anyhow::Result<()> {
    let path = context.run_dir().join("volition-fixture.json");
    let contents = serde_json::to_string_pretty(fixture)?;

    fs::write(&path, contents).with_context(|| {
        format!(
            "failed to write volition fixture snapshot for run {}",
            context.run_id()
        )
    })?;

    Ok(())
}

fn write_traces_artifact(
    context: &RunContext,
    traces: &[PreInitiativeTrace],
) -> anyhow::Result<()> {
    let mut contents = String::new();
    for trace in traces {
        contents.push_str(&serde_json::to_string(trace)?);
        contents.push('\n');
    }

    fs::write(
        context.run_dir().join("pre-initiative-traces.jsonl"),
        contents,
    )
    .with_context(|| {
        format!(
            "failed to write pre-initiative traces for run {}",
            context.run_id()
        )
    })?;

    Ok(())
}

fn write_trace_report(
    context: &RunContext,
    fixture: &VolitionFixture,
    runs: &[TraceRunSummary],
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Volition Trace-Backed Initiative\n\n");
    markdown.push_str(
        "Each scripted input is run through the static fixture selector, then a pre-initiative \
trace is recorded for every selected goal (or a single no-delta trace when nothing is selected). \
No effect is executed.\n\n",
    );

    markdown.push_str("## Fixture Snapshot\n\n");
    markdown.push_str("### Tensions\n\n");
    for tension in &fixture.tensions {
        markdown.push_str(&format!(
            "- {} ({}) - {}\n",
            tension.title, tension.priority_bias, tension.summary
        ));
    }
    markdown.push_str("\n### Goals\n\n");
    for goal in &fixture.goals {
        markdown.push_str(&format!(
            "- {} [{}] - {}\n",
            goal.title, goal.status, goal.summary
        ));
    }

    markdown.push_str("\n## Trace Runs\n\n");
    markdown.push_str(
        "| Scenario | Input | Selected goals | Proposed effects | Omitted goals | Losing candidates | Deltas | No-delta |\n",
    );
    markdown.push_str("|---|---|---|---|---|---:|---:|---:|\n");
    for run in runs {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            run.scenario,
            run.input,
            join_or_dash(&run.selected_goal_ids),
            join_or_dash(&run.proposed_effects),
            join_or_dash(&run.omitted_goals),
            run.losing_candidate_count,
            run.delta_count,
            run.no_delta_count
        ));
    }

    markdown.push_str(
        "\n## Notes\n\n- Tension provenance is recorded for legibility only; tension priority did \
not determine selection.\n- Losing candidates carry deterministic precedence reasons, not full \
arbitration.\n- No trace executes an effect.\n",
    );

    fs::write(
        context
            .run_dir()
            .join("volition-trace-backed-initiative.md"),
        markdown,
    )?;

    Ok(())
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
    use super::{BUDGET, selector_snapshot};
    use crate::volition::{select_goals, static_fixture};

    #[test]
    fn snapshot_preserves_omitted_state_for_selected_goal_inputs() {
        let fixture = static_fixture();
        let selection = select_goals(
            "We never settled how voice memory affects continuity.",
            &fixture,
            BUDGET,
        );
        let snapshot = selector_snapshot(&selection);

        assert!(
            !snapshot.selected.is_empty(),
            "input should select at least one goal"
        );
        assert!(
            snapshot
                .omitted
                .iter()
                .any(|omitted| omitted.goal_id == "avoid-overstating-impl-status"),
            "omitted selector state should be preserved alongside selected goals"
        );
        assert!(
            snapshot
                .omitted
                .iter()
                .all(|omitted| !omitted.reason.is_empty()),
            "each omitted goal should carry an omission reason"
        );
    }
}
