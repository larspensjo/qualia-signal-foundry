//! Experiment definitions, registration, and runner dispatch.

mod placeholder;
mod registry;

pub use registry::{
    Experiment, ExperimentInfo, ExperimentName, ExperimentOutcome, ExperimentRunSummary,
    available_experiments, run_experiment, run_experiment_in,
};
