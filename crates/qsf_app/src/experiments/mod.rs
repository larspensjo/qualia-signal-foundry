use std::fmt;
use std::str::FromStr;
use std::time::Instant;

use clap::ValueEnum;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::reports::markdown_report::{ExperimentReport, write_report};
use crate::runtime::run_context::RunContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ExperimentName {
    FrameworkSkeletonMvp,
    AssociativeMemoryToyModel,
    ContextBudgetRetrievalTest,
    SleepPhaseSessionSummary,
    ToolAsPerceptionCalculator,
}

impl ExperimentName {
    pub fn id(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "framework-skeleton-mvp",
            Self::AssociativeMemoryToyModel => "associative-memory-toy-model",
            Self::ContextBudgetRetrievalTest => "context-budget-retrieval-test",
            Self::SleepPhaseSessionSummary => "sleep-phase-session-summary",
            Self::ToolAsPerceptionCalculator => "tool-as-perception-calculator",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "Placeholder framework skeleton smoke experiment",
            Self::AssociativeMemoryToyModel => {
                "Placeholder associative memory toy model experiment"
            }
            Self::ContextBudgetRetrievalTest => "Placeholder context budget retrieval experiment",
            Self::SleepPhaseSessionSummary => "Placeholder sleep phase session summary experiment",
            Self::ToolAsPerceptionCalculator => {
                "Placeholder tool-as-perception calculator experiment"
            }
        }
    }
}

impl fmt::Display for ExperimentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

impl FromStr for ExperimentName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <Self as ValueEnum>::from_str(value, true).map_err(|_| {
            format!(
                "unknown experiment `{value}`; run `qsf_app list-experiments` for available names"
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExperimentInfo {
    pub id: &'static str,
    pub description: &'static str,
}

pub fn available_experiments() -> Vec<ExperimentInfo> {
    [
        ExperimentName::FrameworkSkeletonMvp,
        ExperimentName::AssociativeMemoryToyModel,
        ExperimentName::ContextBudgetRetrievalTest,
        ExperimentName::SleepPhaseSessionSummary,
        ExperimentName::ToolAsPerceptionCalculator,
    ]
    .into_iter()
    .map(|experiment| ExperimentInfo {
        id: experiment.id(),
        description: experiment.description(),
    })
    .collect()
}

pub fn run_placeholder(name: ExperimentName) -> anyhow::Result<()> {
    let started_at = Instant::now();
    let mut context = RunContext::create(name.id())?;
    context.initialize_engine_logging();

    engine_logging::engine_info!(
        "starting placeholder experiment: experiment_id={} run_id={}",
        name,
        context.run_id()
    );

    context.record_event(
        EventType::ExperimentStarted,
        json!({
            "run_id": context.run_id(),
            "description": name.description(),
        }),
        None,
    )?;

    let trace = TraceRecord::new(
        context.experiment_id(),
        "placeholder-experiment-run",
        "named placeholder experiment",
        "runner emitted initial observability artifacts",
    )
    .with_details(json!({
        "phase": "2",
        "artifacts": ["engine.log", "events.jsonl", "traces.jsonl", "Report.md"],
    }))
    .with_latency_ms(
        started_at
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    let trace_id = trace.trace_id;

    context.record_trace(trace)?;
    context.record_event(
        EventType::TraceRecorded,
        json!({
            "operation": "placeholder-experiment-run",
        }),
        Some(trace_id),
    )?;

    let output_message = format!("Experiment `{}` completed placeholder Phase 2 run.", name);
    context.record_event(
        EventType::OutputProduced,
        json!({
            "message": output_message,
        }),
        Some(trace_id),
    )?;

    context.record_event(
        EventType::ExperimentCompleted,
        json!({
            "status": "completed",
            "elapsed_ms": started_at.elapsed().as_millis(),
        }),
        Some(trace_id),
    )?;

    write_report(
        context.report_path(),
        &ExperimentReport {
            experiment_id: context.experiment_id().to_string(),
            run_id: context.run_id().to_string(),
            status: "completed".to_string(),
            summary: "Phase 2 placeholder run verified per-run developer logging, structured event logging, structured trace logging, and Markdown report generation.".to_string(),
            event_count: 4,
            trace_count: 1,
            observations: vec![
                "Run artifacts are separated into developer log, event log, trace log, and report files.".to_string(),
                "The placeholder runner now has the artifact shape expected by future experiments.".to_string(),
            ],
            follow_up_questions: vec![
                "Which event payload fields should become strongly typed first?".to_string(),
            ],
        },
    )?;

    engine_logging::engine_info!(
        "completed placeholder experiment: experiment_id={} run_id={}",
        name,
        context.run_id()
    );

    println!(
        "Experiment `{}` completed. Run artifacts: {}",
        name,
        context.run_dir().display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ExperimentName, available_experiments};

    #[test]
    fn placeholder_experiments_are_registered() {
        let experiments = available_experiments();

        assert!(
            experiments
                .iter()
                .any(|experiment| { experiment.id == ExperimentName::FrameworkSkeletonMvp.id() })
        );
        assert!(
            experiments.iter().any(|experiment| {
                experiment.id == ExperimentName::AssociativeMemoryToyModel.id()
            })
        );
    }

    #[test]
    fn experiment_id_round_trips() {
        let parsed: ExperimentName = "framework-skeleton-mvp".parse().unwrap();

        assert_eq!(parsed, ExperimentName::FrameworkSkeletonMvp);
    }

    #[test]
    fn logging_smoke_path_uses_engine_logging_test_initializer() {
        engine_logging::initialize_for_tests();
        engine_logging::engine_info!("qsf_app logging smoke test");
    }
}
