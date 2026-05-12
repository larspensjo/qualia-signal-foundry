//! Experiment definitions, registration, and runner dispatch.

mod memory_and_context;
mod model_role_smoke;
mod placeholder;
mod registry;
mod sleep_phase_session_summary;
mod tool_as_perception_calculator;

pub use registry::{
    Experiment, ExperimentInfo, ExperimentName, ExperimentOutcome, ExperimentRunSummary,
    available_experiments, run_experiment, run_experiment_in,
};
