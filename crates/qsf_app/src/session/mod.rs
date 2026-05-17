use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::ContextAssembly;
use crate::conversation::ContentHash;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionState {
    pub started_at: SystemTime,
    pub config: SessionConfig,
    pub turns: Vec<Turn>,
    pub summarized_turns: Vec<TurnSummary>,
    pub ended_reason: Option<SessionEndReason>,
    pub last_input: Option<String>,
    pub last_prompt_hash: Option<ContentHash>,
    pub prefix_invalidated_since_last_prompt: bool,
    pub last_model_error: Option<String>,
    pub limit_reached: Option<SessionLimit>,
}

impl SessionState {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            started_at: SystemTime::now(),
            config,
            turns: vec![],
            summarized_turns: vec![],
            ended_reason: None,
            last_input: None,
            last_prompt_hash: None,
            prefix_invalidated_since_last_prompt: false,
            last_model_error: None,
            limit_reached: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionConfig {
    pub model_id: String,
    pub max_turns: usize,
    pub warm_threshold: usize,
    pub allow_over_limit: bool,
    pub memory_source: MemorySourceConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemorySourceConfig {
    pub source: String,
    pub file: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Turn {
    pub index: usize,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
    pub user_input: String,
    pub context_assembly: ContextAssembly,
    pub retrieved_memory_block: String,
    pub assistant_response: String,
    pub recalled_turns: Vec<RecallRecord>,
    pub model_id: String,
    pub model_latency_ms: u64,
    pub input_tokens: u32,
    pub cached_input_tokens: u32,
    pub output_tokens: u32,
    pub full_request_hash: ContentHash,
    pub message_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecallRecord {
    pub call_id: String,
    pub turn_id: usize,
    pub tool_name: String,
    pub category: crate::tools::ToolCategory,
    pub side_effect_level: crate::tools::ToolSideEffectLevel,
    pub verbatim_text: String,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TurnSummary {
    pub turn_index: usize,
    pub summarized_after_turn_index: usize,
    pub summary: String,
    pub model_id: String,
    pub model_latency_ms: u64,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionLimit {
    pub current: usize,
    pub max: usize,
    pub override_active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    Eof,
    QuitCommand,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    SessionStarted(SessionConfig),
    InputReceived {
        input: String,
    },
    MemoryRetrieved,
    ContextAssembled(ContextAssembly),
    PromptAssembled {
        full_request_hash: ContentHash,
        message_count: usize,
        total_bytes: usize,
    },
    ModelRoleCompleted {
        response: String,
        latency_ms: u64,
        input_tokens: u32,
        cached_input_tokens: u32,
        output_tokens: u32,
    },
    ModelRoleFailed {
        error_summary: String,
    },
    TurnCompleted(Turn),
    TurnSummarized(TurnSummary),
    ToolCompleted(RecallRecord),
    SessionLimitReached {
        current: usize,
        max: usize,
        override_active: bool,
    },
    SessionEnded {
        reason: SessionEndReason,
    },
}

pub fn is_turn_summarized(state: &SessionState, turn_index: usize) -> bool {
    // Summaries are append-only and always cover the oldest unsummarized turns.
    // Completed Turn records stay in `turns`; prompt assembly skips this prefix.
    turn_index < state.summarized_turns.len()
}
