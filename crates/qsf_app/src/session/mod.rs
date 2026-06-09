pub mod ageing;
pub mod config;
pub mod continuation;
pub mod exchange;
pub mod live_memory;
pub mod live_state;
pub mod manifest;
pub mod persistence;
pub mod resume;
pub mod runtime;
pub mod sleep_records;
pub mod state_directory;

pub use qsf_session::resume::{
    LoadResumeInputsResult, ResumeInputs, SchemaUpgrade, classify_resume_mode,
    load_resume_inputs_with_upgrade,
};
pub use qsf_session::{
    ActiveResponseState, AgedCoRetrievalRecord, CONTINUITY_MANIFEST_SCHEMA_VERSION,
    ContinuityManifest, Exchange, ExchangeInput, ExchangeModelUse, ExchangeOutput, ExchangeRange,
    ExchangeStatus, ExchangeTurnConversionError, InterruptionAction, InterruptionRecord,
    InterruptionStopOutcome, LiveCaptureContext, LiveSessionEvent, LiveSessionState,
    MemorySourceConfig, PartialTranscript, PromptPrefixInvalidation, ProviderEventKind,
    ProviderEventRecord, RecallRecord, ResponseStatus, ResumeMode, RuntimePhase,
    SESSION_STATE_SCHEMA_VERSION, SessionConfig, SessionEndReason, SessionEvent, SessionLimit,
    SessionState, SleepRecord, SleepRecordKind, ToolCategory, ToolRequestRecord,
    ToolSideEffectLevel, Turn, TurnRange, TurnSummary, UtteranceRecord, apply_live_session_event,
    is_turn_summarized, reduce_live_session, reduce_session, reduce_session_in_place,
    resume_breaking_config_changed,
};
pub use runtime::{
    BootedSession, SessionBootRequest, apply_session_event, boot_session,
    format_boot_brief_for_context, persist_continuity_state, persist_continuity_state_from_dirs,
};
pub use state_directory::{StateDirectoryResolution, resolve_shared_state_directory_from_env};

pub(crate) use live_memory::{apply_live_memory_capture, apply_live_memory_reinforcement};

#[cfg(test)]
pub(crate) mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget};
    use crate::conversation::ContentHash;

    pub(crate) fn fake_turn(index: usize) -> Turn {
        Turn {
            index,
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: SystemTime::UNIX_EPOCH,
            user_input: format!("turn-{index}-input"),
            context_assembly: ContextAssembly {
                budget: ContextBudget::new(4, 600),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            },
            retrieved_memory_block: String::new(),
            assistant_response: format!("turn-{index}-response"),
            recalled_turns: vec![],
            model_id: "mock".to_string(),
            model_latency_ms: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            full_request_hash: ContentHash([index as u8; 32]),
            message_count: 0,
        }
    }
}
