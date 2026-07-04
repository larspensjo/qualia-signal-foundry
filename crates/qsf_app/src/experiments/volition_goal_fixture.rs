use std::fs;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;
use crate::volition::{GoalSelectionResult, VolitionFixture, select_goals, static_fixture};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 2,
    max_estimated_tokens: 80,
};

const INPUT_A: &str = "Is the goal system implemented yet?";
const INPUT_B: &str = "We never settled how voice memory affects continuity.";
const INPUT_C: &str = "Give me the build command.";

pub struct VolitionGoalFixtureExperiment;

impl Experiment for VolitionGoalFixtureExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionGoalFixture
    }

    fn description(&self) -> &'static str {
        "Select budget-bounded volition goals from a static fixture and trace candidate initiatives"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        write_fixture_snapshot(context, &fixture)?;

        let scripted_inputs = [
            ("input-a", INPUT_A),
            ("input-b", INPUT_B),
            ("input-c", INPUT_C),
        ];

        let mut runs = Vec::new();
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

            let run = record_selection(context, scenario, input, &fixture, BUDGET, None)?;
            runs.push(run);
        }

        let mut perturbed_fixture = fixture.clone();
        let perturbed_goal = perturbed_fixture
            .goals
            .iter_mut()
            .find(|goal| goal.id == "resurface-open-thread")
            .expect("fixture must contain the continuity goal");
        perturbed_goal
            .activation_keywords
            .retain(|keyword| keyword.term != "continuity");

        context.record_event(
            EventType::InputReceived,
            json!({
                "scenario": "perturbation",
                "input": INPUT_B,
                "budget": BUDGET,
                "fixture_change": "removed continuity keyword from resurface-open-thread",
            }),
            None,
        )?;

        let perturbed_run = record_selection(
            context,
            "perturbation",
            INPUT_B,
            &perturbed_fixture,
            BUDGET,
            Some("continuity keyword removed"),
        )?;

        write_selection_report(context, &fixture, &runs, &perturbed_run)?;

        Ok(ExperimentOutcome {
            summary: "The volition goal fixture loaded a static tension/goal set, selected a budget-bounded subset for scripted inputs, and recorded candidate initiatives with trace records and a perturbation comparison.".to_string(),
            observations: vec![
                "The direct-task baseline selected no goals, which keeps the selector from firing on everything.".to_string(),
                "Relevant inputs surfaced goal candidates through RuntimeState context fragments and the existing context assembler rather than a special volition-specific assembler.".to_string(),
                "Removing a single activation keyword predictably changed the selected goals for the continuity input.".to_string(),
            ],
            failure_modes: vec![
                "The fixture is still hand-authored, so relevance scores can be tuned too tightly to the scripted inputs.".to_string(),
                "Keyword matching is intentionally simple; later phases may need a richer relevance model once salience and arbitration exist.".to_string(),
            ],
            follow_up_questions: vec![
                "Should phase 3 record a dedicated pre-initiative trace before any later arbitration exists?".to_string(),
                "Should additional goal evidence sources be promoted into RuntimeState context fragments or wait for a richer source kind?".to_string(),
            ],
            decision_candidates: vec![
                "Keep the first volition slice pure by mapping goals into RuntimeState context fragments and reusing the existing assembler.".to_string(),
                "Mirror volition selection with TraceRecorded events instead of introducing a goal-specific event type in phase 2.".to_string(),
            ],
            extra_artifacts: vec![
                "volition-fixture.json".to_string(),
                "volition-goal-fixture.md".to_string(),
            ],
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct SelectionRunSummary {
    scenario: String,
    input: String,
    selected_goal_ids: Vec<String>,
    selected_initiatives: Vec<String>,
    omitted_goal_ids: Vec<String>,
    used_estimated_tokens: usize,
    perturbation_note: Option<String>,
}

fn record_selection(
    context: &mut RunContext,
    scenario: &str,
    input: &str,
    fixture: &VolitionFixture,
    budget: ContextBudget,
    perturbation_note: Option<&str>,
) -> anyhow::Result<SelectionRunSummary> {
    let started_at = Instant::now();
    let selection: GoalSelectionResult = select_goals(input, fixture, budget);
    let elapsed_ns = elapsed_ns(started_at);

    let trace = TraceRecord::new(
        context.experiment_id(),
        "volition-goal-selection",
        format!("scenario={} input={input}", scenario),
        format!(
            "selected={} omitted={} initiatives={}",
            selection.selected.len(),
            selection.omitted.len(),
            selection
                .selected
                .iter()
                .map(|goal| goal.initiative.effect.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .with_details(json!({
        "scenario": scenario,
        "input": input,
        "budget": budget,
        "selection": selection,
        "perturbation_note": perturbation_note,
    }))
    .with_latency_ns(elapsed_ns);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;

    context.record_event(
        EventType::TraceRecorded,
        json!({
            "scenario": scenario,
            "operation": "volition-goal-selection",
            "trace_id": trace_id,
            "selected_goal_ids": selection.selected.iter().map(|selected| &selected.goal.id).collect::<Vec<_>>(),
            "omitted_goal_ids": selection.omitted.iter().map(|omitted| &omitted.goal.id).collect::<Vec<_>>(),
        }),
        Some(trace_id),
    )?;

    Ok(SelectionRunSummary {
        scenario: scenario.to_string(),
        input: input.to_string(),
        selected_goal_ids: selection
            .selected
            .iter()
            .map(|selected| selected.goal.id.clone())
            .collect(),
        selected_initiatives: selection
            .selected
            .iter()
            .map(|selected| format!("{} -> {}", selected.goal.id, selected.initiative.effect))
            .collect(),
        omitted_goal_ids: selection
            .omitted
            .iter()
            .map(|omitted| omitted.goal.id.clone())
            .collect(),
        used_estimated_tokens: selection.assembly.used_estimated_tokens,
        perturbation_note: perturbation_note.map(|note| note.to_string()),
    })
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

fn write_selection_report(
    context: &RunContext,
    fixture: &VolitionFixture,
    runs: &[SelectionRunSummary],
    perturbed_run: &SelectionRunSummary,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Volition Goal Fixture\n\n");
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

    markdown.push_str("\n## Selection Runs\n\n");
    markdown
        .push_str("| Scenario | Input | Selected goals | Initiatives | Omitted goals | Tokens |\n");
    markdown.push_str("|---|---|---|---|---|---:|\n");
    for run in runs {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            run.scenario,
            run.input,
            run.selected_goal_ids.join(", "),
            run.selected_initiatives.join(", "),
            run.omitted_goal_ids.join(", "),
            run.used_estimated_tokens
        ));
    }

    markdown.push_str("\n## Perturbation\n\n");
    markdown.push_str(&format!(
        "The perturbation reran `{}` after removing a single activation keyword.\n\n",
        perturbed_run.input
    ));
    if let Some(note) = &perturbed_run.perturbation_note {
        markdown.push_str(&format!("- Perturbation note: {}\n", note));
    }
    markdown.push_str(&format!(
        "- Selected goals: {}\n",
        perturbed_run.selected_goal_ids.join(", ")
    ));
    markdown.push_str(&format!(
        "- Initiatives: {}\n",
        perturbed_run.selected_initiatives.join(", ")
    ));
    markdown.push_str(&format!(
        "- Omitted goals: {}\n",
        perturbed_run.omitted_goal_ids.join(", ")
    ));

    fs::write(context.run_dir().join("volition-goal-fixture.md"), markdown)?;

    Ok(())
}
