//! Experiment definitions, registration, and runner dispatch.

mod accept_reviewed_memory;
mod audio_preparation_layer;
mod failure;
mod memory_and_context;
mod model_role_smoke;
mod placeholder;
mod realtime_voice_session;
mod registry;
mod reviewed_memory_draft;
mod sleep_phase_session_summary;
mod streaming_transcription_mvp;
mod text_owned_voice_loop;
mod tool_as_perception_calculator;

pub use registry::{
    Experiment, ExperimentInfo, ExperimentName, ExperimentOutcome, ExperimentRunSummary,
    available_experiments, run_experiment, run_experiment_in,
};
