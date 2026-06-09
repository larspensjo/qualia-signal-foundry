use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::context::ContextAssembly;
use crate::models::{ModelMessage, ModelMessageRole, ModelToolCall};
use qsf_session::ContentHash;

/// Constant system-prompt prefix; warm-tier summaries are appended at assembly time.
pub const SESSION_SYSTEM_PROMPT: &str = "You are a concise conversational responder. Treat this as one continuous human-driven text session. Use retrieved memory as context, keep prior turns stable, and never initiate a turn without user input. If an older summarized turn needs exact details, request recall_turn with its turn_id; only summarized turns can be recalled. If the user asks for arithmetic or exact numeric calculation, request calculator with the expression instead of estimating mentally.";

pub const PROJECT_DOC_INTROSPECTION_PROMPT: &str = "\
You can consult the project's own documents to ground questions about Qualia Signal Foundry. Use search_project_docs to find relevant material, then read_project_doc to pull a focused excerpt or a bounded slice from the most promising one.\n\
\n\
Every result carries a kind (Frame, Concept, Research, Plan, Idea, Design, Architecture, ExperimentSpec, ExperimentReport, Decision, Diary, or Unknown) and, where applicable, a maturity tag (Brainstorm, Sketch, Candidate, Accepted, Implemented, Deprecated, or Unknown).\n\
\n\
Attribute lightly in your reply, using kind and maturity to hedge:\n\
  - \"The project's accepted framing says...\"         (Frame, or Accepted Concept)\n\
  - \"An accepted decision records that...\"           (DecisionLog entry)\n\
  - \"There's a candidate architecture sketch for...\" (Candidate Architecture)\n\
  - \"A brainstorm idea explores...\"                  (Idea, or Brainstorm Concept)\n\
  - \"I found a document but couldn't classify it...\" (Unknown kind or maturity)\n\
\n\
Do not claim current behavior from a Plan, Idea, or Concept; those describe intent. Source code is the only authority for what runs today, and is not available to this channel. If a read was truncated or limited to a single section, mention that. When nothing relevant comes back, or when the metadata is Unknown, say so plainly rather than improvising.";

#[derive(Clone, Debug, PartialEq)]
pub struct PromptTurn<'a> {
    pub user_input: &'a str,
    pub retrieved_memory_block: &'a str,
    pub recalled_tool_messages: Vec<PromptToolMessage>,
    pub assistant_response: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptToolMessage {
    pub tool_name: String,
    pub call_id: String,
    pub arguments: Value,
    pub content: String,
}

impl PromptToolMessage {
    pub fn model_tool_call(&self) -> ModelToolCall {
        ModelToolCall::new(
            self.call_id.clone(),
            self.tool_name.clone(),
            self.arguments.clone(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptTurnSummary<'a> {
    pub turn_index: usize,
    pub summary: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
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
    assemble_prompt_with_summaries_and_project_doc_channel(
        &[],
        prior_turns,
        input,
        retrieved,
        false,
    )
}

pub fn assemble_prompt_with_summaries(
    summarized_turns: &[PromptTurnSummary<'_>],
    prior_turns: &[PromptTurn<'_>],
    input: &str,
    retrieved: &str,
) -> PromptAssembly {
    assemble_prompt_with_summaries_and_project_doc_channel(
        summarized_turns,
        prior_turns,
        input,
        retrieved,
        false,
    )
}

pub fn assemble_prompt_with_summaries_and_project_doc_channel(
    summarized_turns: &[PromptTurnSummary<'_>],
    prior_turns: &[PromptTurn<'_>],
    input: &str,
    retrieved: &str,
    project_doc_channel_enabled: bool,
) -> PromptAssembly {
    let mut messages = vec![ModelMessage::system(format_system_prompt(summarized_turns))];

    for turn in prior_turns {
        messages.push(ModelMessage::user(format_new_turn(
            turn.user_input,
            turn.retrieved_memory_block,
        )));
        for tool_message in &turn.recalled_tool_messages {
            messages.push(ModelMessage::assistant_tool_calls(
                "",
                vec![tool_message.model_tool_call()],
            ));
            messages.push(ModelMessage::tool_result(
                &tool_message.call_id,
                format_tool_message(tool_message),
            ));
        }
        messages.push(ModelMessage::assistant(turn.assistant_response));
    }

    messages.push(ModelMessage::user(format_new_turn(input, retrieved)));
    let mut assembly = prompt_assembly_from_messages(messages);
    if project_doc_channel_enabled {
        assembly.messages[0].content = format!(
            "{}\n\n{}",
            assembly.messages[0].content, PROJECT_DOC_INTROSPECTION_PROMPT
        );
        assembly = prompt_assembly_from_messages(assembly.messages);
    }
    assembly
}

pub fn prompt_assembly_from_messages(messages: Vec<ModelMessage>) -> PromptAssembly {
    let full_request_hash = canonical_hash(&messages);
    let message_count = messages.len();
    let total_bytes = messages
        .iter()
        .map(|message| {
            message_role_name(message.role).len()
                + message.content.len()
                + message
                    .tool_call_id
                    .as_ref()
                    .map(|id| id.len())
                    .unwrap_or(0)
                + tool_calls_byte_len(&message.tool_calls)
        })
        .sum();

    PromptAssembly {
        messages,
        full_request_hash,
        message_count,
        total_bytes,
    }
}

fn format_system_prompt(summarized_turns: &[PromptTurnSummary<'_>]) -> String {
    if summarized_turns.is_empty() {
        return SESSION_SYSTEM_PROMPT.to_string();
    }

    let mut prompt = SESSION_SYSTEM_PROMPT.to_string();
    prompt.push_str("\n\n[Earlier in this session]\n");
    for summary in summarized_turns {
        prompt.push_str(&format!(
            "- Turn {}: {}\n",
            summary.turn_index,
            summary.summary.trim()
        ));
    }
    let trimmed_len = prompt.trim_end().len();
    prompt.truncate(trimmed_len);
    prompt
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

pub fn format_tool_message(tool_message: &PromptToolMessage) -> String {
    format!(
        "[Tool]\nname: {}\ncall_id: {}\n{}",
        tool_message.tool_name,
        tool_message.call_id,
        tool_message.content.trim()
    )
}

pub fn retrieved_memory_block(assembly: &ContextAssembly) -> String {
    let mut directs = Vec::new();
    let mut hints = Vec::new();

    for selection in &assembly.selected {
        let line = format!(
            "- {}: {}",
            selection.fragment.fragment_id, selection.fragment.summary
        );
        match selection.fragment.source_kind {
            crate::context::ContextSourceKind::Memory => directs.push(line),
            crate::context::ContextSourceKind::MemoryHint => hints.push(line),
            _ => {}
        }
    }

    let mut out = String::new();
    if !directs.is_empty() {
        out.push_str("=== Memories retrieved for this turn ===\n");
        out.push_str(&directs.join("\n"));
    }
    if !hints.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        // Keep this prompt label ASCII-only for stable terminal rendering on Windows.
        out.push_str("=== Associated memories (hints - may or may not be relevant) ===\n");
        out.push_str(&hints.join("\n"));
    }
    out
}

pub fn canonical_hash(messages: &[ModelMessage]) -> ContentHash {
    let mut hasher = Sha256::new();

    for message in messages {
        let role = message_role_name(message.role).as_bytes();
        let content = message.content.as_bytes();
        let tool_call_id = message.tool_call_id.as_deref().unwrap_or("").as_bytes();
        hasher.update((role.len() as u32).to_le_bytes());
        hasher.update(role);
        hasher.update((content.len() as u32).to_le_bytes());
        hasher.update(content);
        hasher.update((tool_call_id.len() as u32).to_le_bytes());
        hasher.update(tool_call_id);
        hasher.update((message.tool_calls.len() as u32).to_le_bytes());
        for tool_call in &message.tool_calls {
            update_hash_with_len_prefixed_bytes(&mut hasher, tool_call.call_id.as_bytes());
            update_hash_with_len_prefixed_bytes(&mut hasher, tool_call.name.as_bytes());
            let arguments = tool_call.arguments.to_string();
            update_hash_with_len_prefixed_bytes(&mut hasher, arguments.as_bytes());
        }
    }

    ContentHash(hasher.finalize().into())
}

fn tool_calls_byte_len(tool_calls: &[ModelToolCall]) -> usize {
    tool_calls
        .iter()
        .map(|tool_call| {
            tool_call.call_id.len() + tool_call.name.len() + tool_call.arguments.to_string().len()
        })
        .sum()
}

fn update_hash_with_len_prefixed_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u32).to_le_bytes());
    hasher.update(bytes);
}

fn message_role_name(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
        ModelMessageRole::Tool => "tool",
    }
}

#[cfg(test)]
mod tests {
    use crate::context::{
        ContextAssembly, ContextBudget, ContextFragment, ContextSelection, ContextSourceKind,
    };

    use super::{
        PROJECT_DOC_INTROSPECTION_PROMPT, PromptTurn, PromptTurnSummary, SESSION_SYSTEM_PROMPT,
        assemble_prompt, assemble_prompt_with_summaries,
        assemble_prompt_with_summaries_and_project_doc_channel, canonical_hash, format_new_turn,
        prior_request_prefix_hash, retrieved_memory_block,
    };
    use crate::models::{ModelMessage, ModelMessageRole};

    #[test]
    fn prior_request_hash_is_stable_when_new_retrieval_changes() {
        let first = assemble_prompt(&[], "first input", "- memory.a: First memory");
        let turn = PromptTurn {
            user_input: "first input",
            retrieved_memory_block: "- memory.a: First memory",
            recalled_tool_messages: vec![],
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
            "=== Memories retrieved for this turn ===\n- memory.a: A remembered fact."
        );
    }

    #[test]
    fn block_renderer_emits_two_labeled_sections_when_hints_present() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(8, 1024),
            used_estimated_tokens: 40,
            omitted: vec![],
            selected: vec![
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory.foo".to_string(),
                        source_kind: ContextSourceKind::Memory,
                        summary: "Foo summary".to_string(),
                        tags: vec![],
                        score: 1.0,
                        estimated_tokens: 20,
                        source_reference: "x".to_string(),
                        selection_reason: "matched: decay".to_string(),
                    },
                    cumulative_estimated_tokens: 20,
                },
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory.baz".to_string(),
                        source_kind: ContextSourceKind::MemoryHint,
                        summary: "Baz summary".to_string(),
                        tags: vec![],
                        score: 0.4,
                        estimated_tokens: 20,
                        source_reference: "x".to_string(),
                        selection_reason: "via memory.foo: co-retrieved".to_string(),
                    },
                    cumulative_estimated_tokens: 40,
                },
            ],
        };

        let block = retrieved_memory_block(&assembly);

        assert!(block.contains("=== Memories retrieved for this turn ==="));
        assert!(block.contains("- memory.foo: Foo summary"));
        assert!(block.contains("=== Associated memories (hints - may or may not be relevant) ==="));
        assert!(block.contains("- memory.baz: Baz summary"));
        let direct_pos = block.find("memory.foo").unwrap();
        let hint_pos = block.find("memory.baz").unwrap();
        assert!(direct_pos < hint_pos);
    }

    #[test]
    fn block_renderer_omits_hint_section_when_no_hints() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(8, 1024),
            used_estimated_tokens: 20,
            omitted: vec![],
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: "memory.foo".to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: "Foo".to_string(),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 20,
                    source_reference: "x".to_string(),
                    selection_reason: "matched".to_string(),
                },
                cumulative_estimated_tokens: 20,
            }],
        };

        let block = retrieved_memory_block(&assembly);

        assert!(block.contains("=== Memories retrieved for this turn ==="));
        assert!(!block.contains("=== Associated memories"));
    }

    #[test]
    fn summaries_render_inside_system_message() {
        let prompt = assemble_prompt_with_summaries(
            &[PromptTurnSummary {
                turn_index: 0,
                summary: "The opening turn established continuity.",
            }],
            &[],
            "next",
            "",
        );

        assert!(
            prompt.messages[0]
                .content
                .starts_with(SESSION_SYSTEM_PROMPT)
        );
        assert!(prompt.messages[0].content.contains(
            "[Earlier in this session]\n- Turn 0: The opening turn established continuity."
        ));
    }

    #[test]
    fn project_doc_prompt_appends_when_channel_enabled() {
        let prompt =
            assemble_prompt_with_summaries_and_project_doc_channel(&[], &[], "next", "", true);

        assert!(
            prompt.messages[0]
                .content
                .contains(PROJECT_DOC_INTROSPECTION_PROMPT)
        );
        assert!(prompt.messages[0].content.contains("search_project_docs"));
        assert!(prompt.messages[0].content.contains("kind and maturity"));
    }

    #[test]
    fn project_doc_prompt_is_omitted_when_channel_disabled() {
        let prompt =
            assemble_prompt_with_summaries_and_project_doc_channel(&[], &[], "next", "", false);

        assert_eq!(prompt.messages[0].content, SESSION_SYSTEM_PROMPT);
    }

    #[test]
    fn canonical_hash_changes_when_tool_call_id_changes() {
        let with_call_id = vec![ModelMessage::tool_result("call-1", "tool output")];
        let other_call_id = vec![ModelMessage::tool_result("call-2", "tool output")];

        assert_eq!(with_call_id[0].role, ModelMessageRole::Tool);
        assert_ne!(
            canonical_hash(&with_call_id),
            canonical_hash(&other_call_id)
        );
    }

    #[test]
    fn canonical_hash_changes_when_assistant_tool_calls_change() {
        let first = vec![ModelMessage::assistant_tool_calls(
            "",
            vec![crate::models::ModelToolCall::new(
                "call-1",
                "recall_turn",
                serde_json::json!({ "turn_id": 0 }),
            )],
        )];
        let second = vec![ModelMessage::assistant_tool_calls(
            "",
            vec![crate::models::ModelToolCall::new(
                "call-2",
                "recall_turn",
                serde_json::json!({ "turn_id": 0 }),
            )],
        )];

        assert_ne!(canonical_hash(&first), canonical_hash(&second));
    }
}
