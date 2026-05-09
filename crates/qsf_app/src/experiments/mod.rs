use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;

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
    engine_logging::initialize();
    engine_logging::engine_info!("starting placeholder experiment: {}", name);

    println!("Experiment `{}` is registered as a placeholder.", name);
    println!("Phase 1 verifies workspace, CLI dispatch, and engine_logging integration.");

    engine_logging::engine_info!("completed placeholder experiment: {}", name);
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
