use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::ContextAssembly;
use crate::conversation::ContentHash;
use crate::tools::{ToolCategory, ToolSideEffectLevel};

pub const SESSION_STATE_SCHEMA_VERSION: u32 = 2;
const LEGACY_SESSION_STATE_SCHEMA_VERSION: u32 = 1;

fn legacy_session_state_schema_version() -> u32 {
    LEGACY_SESSION_STATE_SCHEMA_VERSION
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionState {
    #[serde(default = "legacy_session_state_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub session_id: String,
    /// Previous session id is set only when a new session is bootstrapped from a consolidated brief.
    /// Awake continuation keeps the same `session_id` and leaves this empty.
    #[serde(default)]
    pub previous_session_id: Option<String>,
    pub started_at: SystemTime,
    pub config: SessionConfig,
    pub turns: Vec<Turn>,
    pub summarized_turns: Vec<TurnSummary>,
    #[serde(default)]
    pub exchanges: Vec<crate::exchange::Exchange>,
    #[serde(default)]
    pub live: crate::live_state::LiveSessionState,
    pub ended_reason: Option<SessionEndReason>,
    pub last_input: Option<String>,
    pub last_prompt_hash: Option<ContentHash>,
    pub prefix_invalidated_since_last_prompt: bool,
    #[serde(default)]
    pub prompt_prefix_invalidations: Vec<PromptPrefixInvalidation>,
    pub last_model_error: Option<String>,
    pub limit_reached: Option<SessionLimit>,
}

impl SessionState {
    pub fn new(config: SessionConfig) -> Self {
        Self::new_with_id(uuid::Uuid::new_v4().to_string(), config)
    }

    pub fn new_with_id(session_id: String, config: SessionConfig) -> Self {
        Self {
            schema_version: SESSION_STATE_SCHEMA_VERSION,
            session_id,
            previous_session_id: None,
            started_at: SystemTime::now(),
            config,
            turns: vec![],
            summarized_turns: vec![],
            exchanges: vec![],
            live: crate::live_state::LiveSessionState::default(),
            ended_reason: None,
            last_input: None,
            last_prompt_hash: None,
            prefix_invalidated_since_last_prompt: false,
            prompt_prefix_invalidations: vec![],
            last_model_error: None,
            limit_reached: None,
        }
    }

    pub fn upgrade_schema_version(&mut self) {
        if self.schema_version < SESSION_STATE_SCHEMA_VERSION {
            self.schema_version = SESSION_STATE_SCHEMA_VERSION;
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
pub struct PromptPrefixInvalidation {
    pub after_turn_index: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecallRecord {
    pub call_id: String,
    pub turn_id: usize,
    pub tool_name: String,
    pub category: ToolCategory,
    pub side_effect_level: ToolSideEffectLevel,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnRange {
    pub first_index: usize,
    pub last_index: usize,
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
    PromptPrefixInvalidated {
        after_turn_index: usize,
        reason: String,
    },
    ExchangeRecorded {
        session_id: String,
        exchange: Box<crate::exchange::Exchange>,
    },
    TurnSummarized(TurnSummary),
    TurnsAgedAndCoRetrieved {
        range: TurnRange,
        new_associations: usize,
        strengthened_associations: usize,
        persisted_at: SystemTime,
        summaries: Vec<TurnSummary>,
    },
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

#[cfg(test)]
pub(crate) mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget};
    use crate::conversation::ContentHash;

    #[test]
    fn new_session_state_carries_session_id_and_no_previous() {
        let state = SessionState::new_with_id("session-abc".to_string(), config());

        assert_eq!(state.schema_version, SESSION_STATE_SCHEMA_VERSION);
        assert_eq!(state.session_id, "session-abc");
        assert_eq!(state.previous_session_id, None);
    }

    #[test]
    fn session_state_serde_roundtrips_with_new_fields() {
        let state = SessionState::new_with_id("session-roundtrip".to_string(), config());

        let json = serde_json::to_string(&state).unwrap();
        let parsed: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema_version, SESSION_STATE_SCHEMA_VERSION);
        assert_eq!(parsed.session_id, "session-roundtrip");
        assert_eq!(parsed.previous_session_id, None);
        assert_eq!(parsed.live, crate::live_state::LiveSessionState::default());
    }

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

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }
}
