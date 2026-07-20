//! Experiment definitions, registration, and runner dispatch.

mod accept_reviewed_memory;
mod accept_reviewed_volition_seed;
mod audio_preparation_layer;
mod failure;
mod live_goal_formation_and_coherence;
mod live_memory_extraction;
mod memory_and_context;
mod model_role_smoke;
mod multi_turn_text_loop;
mod placeholder;
mod realtime_voice_session;
mod registry;
mod reviewed_memory_draft;
mod sleep_phase_session_summary;
mod streaming_transcription_mvp;
mod text_owned_voice_loop;
mod tool_as_perception_calculator;
mod transcript_format;
mod voice_loop;
mod volition_arbitration_conflict;
mod volition_bounded_initiative_execution;
pub(crate) mod volition_continuity;
mod volition_emotion_signals;
mod volition_goal_coherence;
mod volition_goal_fixture;
mod volition_goal_visibility;
mod volition_mode_bias;
mod volition_reflection_goal_candidates;
mod volition_salience_and_satisfaction;
mod volition_trace_backed_initiative;
pub(crate) mod volition_trace_support;

pub use registry::{
    Experiment, ExperimentInfo, ExperimentName, ExperimentOutcome, ExperimentRunSummary,
    available_experiments, run_experiment, run_experiment_in, run_experiment_with_workspace_root,
};

pub(crate) use volition_continuity::{
    consolidate_session_volition,
    render_markdown_report as render_volition_continuity_markdown_report,
    run_sleep_volition_goal_maintenance,
};
