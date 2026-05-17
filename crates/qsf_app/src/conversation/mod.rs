//! Conversation prompt assembly for cache-stable multi-turn sessions.

pub mod prompt;

pub use prompt::{
    ContentHash, PromptAssembly, SESSION_SYSTEM_PROMPT, assemble_prompt, canonical_hash,
    format_new_turn, prior_request_prefix_hash,
};
