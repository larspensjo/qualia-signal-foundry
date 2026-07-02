use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::audio::{SimulatedSpeechOutputProvider, SimulatedTranscriptProvider};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    DEFAULT_SESSION_MODEL, SessionMemorySource, prompt_prefix_status_for_report, run_one_turn,
    run_with_io_and_components, run_with_io_and_components_at_state_dir,
    run_with_io_and_components_at_state_resolution,
};
use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSelection, ContextSourceKind,
};
use crate::conversation::ContentHash;
use crate::conversation::prompt::{
    PromptTurn, PromptTurnSummary, assemble_prompt_with_summaries_and_project_doc_channel,
    prior_request_prefix_hash,
};
use crate::experiments::text_owned_voice_loop::SharedVoiceMemorySource;
use crate::memory::{
    Association, LiveCaptureInput, MemoryFixture, MemoryRecord, MemoryRecordKind, MemoryStore,
    RetrievalStrategy, capture_live_memory_candidates, phase_four_fixture, retrieve_memories,
};
use crate::observability::event_log::{EventRecord, EventType};
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::session::ageing;
use crate::session::{
    MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent, SessionState,
    StateDirectoryResolution, Turn, TurnRange, TurnSummary, reduce_session,
    resume_breaking_config_changed,
};
use crate::tools::{
    CALCULATOR_TOOL_NAME, READ_PROJECT_DOC_TOOL_NAME, RECALL_TURN_TOOL_NAME,
    SEARCH_PROJECT_DOCS_TOOL_NAME,
};
use qsf_models::{
    MockModelClient, ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRoleId,
    ModelToolCall, ModelUsage,
};

include!("tests/basics.rs");
include!("tests/reducers_and_ageing.rs");
include!("tests/runtime_resume.rs");
include!("tests/warm_tools_and_self_questions.rs");
include!("tests/report_and_config.rs");
include!("tests/support.rs");
