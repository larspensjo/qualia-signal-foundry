use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use anyhow::Context;
use clap::ValueEnum;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::reports::markdown_report::{ExperimentReport, write_report};
use crate::runtime::run_context::RunContext;

use super::memory_and_context::{
    AssociativeMemoryToyModelExperiment, ContextBudgetRetrievalTestExperiment,
};
use super::model_role_smoke::ModelRoleSmokeExperiment;
use super::placeholder::PlaceholderExperiment;
use super::tool_as_perception_calculator::ToolAsPerceptionCalculatorExperiment;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ExperimentName {
    FrameworkSkeletonMvp,
    AssociativeMemoryToyModel,
    ContextBudgetRetrievalTest,
    ModelRoleSmokeTest,
    SleepPhaseSessionSummary,
    ToolAsPerceptionCalculator,
}

impl ExperimentName {
    pub fn id(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "framework-skeleton-mvp",
            Self::AssociativeMemoryToyModel => "associative-memory-toy-model",
            Self::ContextBudgetRetrievalTest => "context-budget-retrieval-test",
            Self::ModelRoleSmokeTest => "model-role-smoke-test",
            Self::SleepPhaseSessionSummary => "sleep-phase-session-summary",
            Self::ToolAsPerceptionCalculator => "tool-as-perception-calculator",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "Placeholder framework skeleton smoke experiment",
            Self::AssociativeMemoryToyModel => {
                "Compare recency, keyword/tag, and association-weighted memory retrieval"
            }
            Self::ContextBudgetRetrievalTest => {
                "Compare selected and omitted memory context under a deliberately small budget"
            }
            Self::ModelRoleSmokeTest => {
                "Invoke a model role through a deterministic mock client or the optional OpenAI adapter"
            }
            Self::SleepPhaseSessionSummary => "Placeholder sleep phase session summary experiment",
            Self::ToolAsPerceptionCalculator => {
                "Execute a compute-only calculator tool and treat the result as a context candidate"
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentOutcome {
    pub summary: String,
    pub observations: Vec<String>,
    pub failure_modes: Vec<String>,
    pub follow_up_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
    pub extra_artifacts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentRunSummary {
    pub experiment_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub status: String,
    pub event_count: usize,
    pub trace_count: usize,
}

pub trait Experiment {
    fn name(&self) -> ExperimentName;

    fn description(&self) -> &'static str;

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome>;

    fn id(&self) -> &'static str {
        self.name().id()
    }
}

pub fn available_experiments() -> Vec<ExperimentInfo> {
    experiment_names()
        .into_iter()
        .map(|experiment| ExperimentInfo {
            id: experiment.id(),
            description: experiment.description(),
        })
        .collect()
}

pub fn run_experiment(name: ExperimentName) -> anyhow::Result<ExperimentRunSummary> {
    run_experiment_in("runs", name)
}

pub fn run_experiment_in(
    base_dir: impl AsRef<Path>,
    name: ExperimentName,
) -> anyhow::Result<ExperimentRunSummary> {
    let experiment = experiment_for(name);
    let started_at = Instant::now();
    let mut context = RunContext::create_in(base_dir, experiment.id())?;
    context.initialize_engine_logging();

    engine_logging::engine_info!(
        "starting experiment: experiment_id={} run_id={}",
        experiment.id(),
        context.run_id()
    );

    context.record_event(
        EventType::ExperimentStarted,
        json!({
            "run_id": context.run_id(),
            "description": experiment.description(),
        }),
        None,
    )?;

    let outcome = match experiment.run(&mut context) {
        Ok(outcome) => outcome,
        Err(error) => {
            let error_message = error.to_string();
            engine_logging::engine_error!(
                "experiment failed: experiment_id={} run_id={} error={}",
                experiment.id(),
                context.run_id(),
                error_message
            );
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "error": error_message,
                }),
                None,
            )?;
            context.record_event(
                EventType::ExperimentCompleted,
                json!({
                    "status": "failed",
                    "elapsed_ms": started_at.elapsed().as_millis(),
                }),
                None,
            )?;
            write_report(
                context.report_path(),
                &ExperimentReport {
                    experiment_id: context.experiment_id().to_string(),
                    run_id: context.run_id().to_string(),
                    status: "failed".to_string(),
                    summary: "Experiment failed before producing a completed outcome.".to_string(),
                    configuration: report_configuration(),
                    event_count: context.event_count(),
                    trace_count: context.trace_count(),
                    observations: vec![],
                    failure_modes: vec![error_message],
                    follow_up_questions: vec![],
                    decision_candidates: vec![],
                    extra_artifacts: vec![],
                },
            )?;

            return Err(error).with_context(|| {
                format!(
                    "experiment `{}` failed; run artifacts: {}",
                    experiment.id(),
                    context.run_dir().display()
                )
            });
        }
    };

    context.record_event(
        EventType::ExperimentCompleted,
        json!({
            "status": "completed",
            "elapsed_ms": started_at.elapsed().as_millis(),
        }),
        None,
    )?;

    write_report(
        context.report_path(),
        &ExperimentReport {
            experiment_id: context.experiment_id().to_string(),
            run_id: context.run_id().to_string(),
            status: "completed".to_string(),
            summary: outcome.summary,
            configuration: report_configuration(),
            event_count: context.event_count(),
            trace_count: context.trace_count(),
            observations: outcome.observations,
            failure_modes: outcome.failure_modes,
            follow_up_questions: outcome.follow_up_questions,
            decision_candidates: outcome.decision_candidates,
            extra_artifacts: outcome.extra_artifacts,
        },
    )?;

    engine_logging::engine_info!(
        "completed experiment: experiment_id={} run_id={}",
        experiment.id(),
        context.run_id()
    );

    Ok(ExperimentRunSummary {
        experiment_id: context.experiment_id().to_string(),
        run_id: context.run_id().to_string(),
        run_dir: context.run_dir().to_path_buf(),
        status: "completed".to_string(),
        event_count: context.event_count(),
        trace_count: context.trace_count(),
    })
}

fn experiment_names() -> [ExperimentName; 6] {
    [
        ExperimentName::FrameworkSkeletonMvp,
        ExperimentName::AssociativeMemoryToyModel,
        ExperimentName::ContextBudgetRetrievalTest,
        ExperimentName::ModelRoleSmokeTest,
        ExperimentName::SleepPhaseSessionSummary,
        ExperimentName::ToolAsPerceptionCalculator,
    ]
}

fn experiment_for(name: ExperimentName) -> Box<dyn Experiment> {
    match name {
        ExperimentName::AssociativeMemoryToyModel => Box::new(AssociativeMemoryToyModelExperiment),
        ExperimentName::ContextBudgetRetrievalTest => {
            Box::new(ContextBudgetRetrievalTestExperiment)
        }
        ExperimentName::ModelRoleSmokeTest => Box::new(ModelRoleSmokeExperiment),
        ExperimentName::ToolAsPerceptionCalculator => {
            Box::new(ToolAsPerceptionCalculatorExperiment)
        }
        _ => Box::new(PlaceholderExperiment::new(name)),
    }
}

fn report_configuration() -> Vec<String> {
    vec![
        "Runner: first-class Phase 3 experiment runner".to_string(),
        "Event log: `events.jsonl`".to_string(),
        "Trace log: `traces.jsonl`".to_string(),
        "Developer log: `engine.log`".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ExperimentName, available_experiments, run_experiment_in};

    #[test]
    fn placeholder_experiments_are_registered() {
        let experiments = available_experiments();

        assert!(
            experiments
                .iter()
                .any(|experiment| { experiment.id == ExperimentName::FrameworkSkeletonMvp.id() })
        );
        let associative = experiments
            .iter()
            .find(|experiment| experiment.id == ExperimentName::AssociativeMemoryToyModel.id())
            .unwrap();
        assert!(!associative.description.contains("Placeholder"));
    }

    #[test]
    fn experiment_id_round_trips() {
        let parsed: ExperimentName = "framework-skeleton-mvp".parse().unwrap();

        assert_eq!(parsed, ExperimentName::FrameworkSkeletonMvp);
    }

    #[test]
    fn named_experiment_dispatch_writes_run_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-runner-{}", uuid::Uuid::new_v4()));
        let summary =
            run_experiment_in(&base_dir, ExperimentName::AssociativeMemoryToyModel).unwrap();

        assert_eq!(summary.experiment_id, "associative-memory-toy-model");
        assert_eq!(summary.status, "completed");
        assert_eq!(summary.event_count, 15);
        assert_eq!(summary.trace_count, 6);

        let events = fs::read_to_string(summary.run_dir.join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();
        let report = fs::read_to_string(summary.run_dir.join("Report.md")).unwrap();

        assert!(events.contains("ExperimentStarted"));
        assert!(events.contains("InputReceived"));
        assert!(events.contains("MemoryRetrieved"));
        assert!(events.contains("ContextAssembled"));
        assert!(events.contains("ExperimentCompleted"));
        assert!(traces.contains("memory-retrieval"));
        assert!(traces.contains("context-assembly"));
        assert!(report.contains("Phase 4 associative memory toy model"));
        assert!(report.contains("Require memory retrieval traces"));
        assert!(report.contains("memory-fixture.json"));
        assert!(report.contains("retrieval-comparison.md"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn logging_smoke_path_uses_engine_logging_test_initializer() {
        engine_logging::initialize_for_tests();
        engine_logging::engine_info!("qsf_app logging smoke test");
    }
}
