//! Conversation prompt assembly for cache-stable multi-turn sessions.

pub mod prompt;

pub use prompt::{
    PromptAssembly, PromptTurnSummary, SESSION_SYSTEM_PROMPT, assemble_prompt,
    assemble_prompt_with_summaries, canonical_hash, format_new_turn, prior_request_prefix_hash,
};
pub use qsf_session::ContentHash;
