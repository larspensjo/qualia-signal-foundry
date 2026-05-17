use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::context::ContextAssembly;
use crate::models::{ModelMessage, ModelMessageRole};

pub const SESSION_SYSTEM_PROMPT: &str = "You are a concise conversational responder. Treat this as one continuous human-driven text session. Use retrieved memory as context, keep prior turns stable, and never initiate a turn without user input.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
        }
        output
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.hex())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptTurn<'a> {
    pub user_input: &'a str,
    pub retrieved_memory_block: &'a str,
    pub assistant_response: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptAssembly {
    pub messages: Vec<ModelMessage>,
    pub full_request_hash: ContentHash,
    pub message_count: usize,
    pub total_bytes: usize,
}

pub fn assemble_prompt(
    prior_turns: &[PromptTurn<'_>],
    input: &str,
    retrieved: &str,
) -> PromptAssembly {
    let mut messages = vec![ModelMessage::system(SESSION_SYSTEM_PROMPT)];

    for turn in prior_turns {
        messages.push(ModelMessage::user(format_new_turn(
            turn.user_input,
            turn.retrieved_memory_block,
        )));
        messages.push(ModelMessage::assistant(turn.assistant_response));
    }

    messages.push(ModelMessage::user(format_new_turn(input, retrieved)));
    let full_request_hash = canonical_hash(&messages);
    let message_count = messages.len();
    let total_bytes = messages
        .iter()
        .map(|message| message_role_name(message.role).len() + message.content.len())
        .sum();

    PromptAssembly {
        messages,
        full_request_hash,
        message_count,
        total_bytes,
    }
}

pub fn prior_request_prefix_hash(
    messages: &[ModelMessage],
    previous_message_count: usize,
) -> Option<ContentHash> {
    if messages.len() < previous_message_count {
        return None;
    }

    Some(canonical_hash(&messages[..previous_message_count]))
}

/// Renders both frozen prior turns and the new turn. Keeping one renderer is the
/// byte-stability contract for prompt-cache prefix reuse.
pub fn format_new_turn(user_input: &str, retrieved_memory_block: &str) -> String {
    let trimmed_memory = retrieved_memory_block.trim();
    if trimmed_memory.is_empty() {
        format!("[User]\n{user_input}")
    } else {
        format!("[Retrieved memory]\n{trimmed_memory}\n\n[User]\n{user_input}")
    }
}

pub fn retrieved_memory_block(assembly: &ContextAssembly) -> String {
    assembly
        .selected
        .iter()
        .filter(|selection| {
            matches!(
                selection.fragment.source_kind,
                crate::context::ContextSourceKind::Memory
            )
        })
        .map(|selection| {
            format!(
                "- {}: {}",
                selection.fragment.fragment_id, selection.fragment.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn canonical_hash(messages: &[ModelMessage]) -> ContentHash {
    let mut hasher = Sha256::new();

    for message in messages {
        let role = message_role_name(message.role).as_bytes();
        let content = message.content.as_bytes();
        hasher.update((role.len() as u32).to_le_bytes());
        hasher.update(role);
        hasher.update((content.len() as u32).to_le_bytes());
        hasher.update(content);
    }

    ContentHash(hasher.finalize().into())
}

fn message_role_name(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests {
    use crate::context::{
        ContextAssembly, ContextBudget, ContextFragment, ContextSelection, ContextSourceKind,
    };

    use super::{
        PromptTurn, SESSION_SYSTEM_PROMPT, assemble_prompt, format_new_turn,
        prior_request_prefix_hash, retrieved_memory_block,
    };

    #[test]
    fn prior_request_hash_is_stable_when_new_retrieval_changes() {
        let first = assemble_prompt(&[], "first input", "- memory.a: First memory");
        let turn = PromptTurn {
            user_input: "first input",
            retrieved_memory_block: "- memory.a: First memory",
            assistant_response: "first answer",
        };
        let second = assemble_prompt(&[turn], "second input", "- memory.b: Different memory");

        assert_eq!(
            prior_request_prefix_hash(&second.messages, first.message_count),
            Some(first.full_request_hash)
        );
    }

    #[test]
    fn system_prompt_is_constant_at_the_front() {
        let first = assemble_prompt(&[], "hello", "");
        let second = assemble_prompt(&[], "different", "- memory.a: A");

        assert_eq!(first.messages[0].content, SESSION_SYSTEM_PROMPT);
        assert_eq!(second.messages[0].content, SESSION_SYSTEM_PROMPT);
    }

    #[test]
    fn empty_retrieval_omits_memory_block() {
        assert_eq!(format_new_turn("hello", ""), "[User]\nhello");
    }

    #[test]
    fn selected_memory_serializes_as_bullets() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(4, 600),
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: "memory.a".to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: "A remembered fact.".to_string(),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 10,
                    source_reference: "test".to_string(),
                    selection_reason: "test".to_string(),
                },
                cumulative_estimated_tokens: 10,
            }],
            omitted: vec![],
            used_estimated_tokens: 10,
        };

        assert_eq!(
            retrieved_memory_block(&assembly),
            "- memory.a: A remembered fact."
        );
    }
}
