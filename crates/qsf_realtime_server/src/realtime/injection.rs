use qsf_context::{ContextAssembly, ContextBudget, ContextFragment, assemble_context};
use qsf_memory::RetrievedMemory;
use qsf_realtime_protocol::{
    build_openai_realtime_conversation_item_create,
    build_openai_realtime_conversation_session_update,
};

pub(crate) const DEFAULT_PCM_RATE_HZ: u32 = 24_000;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MemoryInjectionPacket {
    pub session_update: serde_json::Value,
    pub conversation_item_create: serde_json::Value,
    pub context_assembly: ContextAssembly,
    pub memory_block: String,
}

#[derive(Clone, Debug)]
pub struct MemoryInjectionRequest<'a> {
    pub model: &'a str,
    pub voice: &'a str,
    pub base_instructions: &'a str,
    pub output_modalities: &'a [String],
    pub session_identity: &'a str,
    pub tone: &'a str,
    pub user_transcript: &'a str,
    pub retrieved_memories: &'a [RetrievedMemory],
    pub budget: ContextBudget,
    pub pcm_rate_hz: u32,
}

pub fn build_memory_injection_packet(
    request: &MemoryInjectionRequest<'_>,
) -> Option<MemoryInjectionPacket> {
    let context_assembly = assemble_memory_context(request);
    if context_assembly.selected.is_empty() {
        return None;
    }

    let memory_block = build_memory_block(request, &context_assembly);
    let session_instructions = build_session_instructions(request);
    let session_update = build_openai_realtime_conversation_session_update(
        request.model,
        request.voice,
        &session_instructions,
        request.output_modalities,
        request.pcm_rate_hz,
        false,
    );
    let conversation_item_create =
        build_openai_realtime_conversation_item_create("system", &memory_block);

    Some(MemoryInjectionPacket {
        session_update,
        conversation_item_create,
        context_assembly,
        memory_block,
    })
}

pub fn assemble_memory_context(request: &MemoryInjectionRequest<'_>) -> ContextAssembly {
    let fragments = request
        .retrieved_memories
        .iter()
        .map(ContextFragment::from)
        .collect::<Vec<_>>();
    assemble_context(fragments, request.budget)
}

fn build_memory_block(request: &MemoryInjectionRequest<'_>, assembly: &ContextAssembly) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Session identity: {}", request.session_identity));
    lines.push(format!("Tone: {}", request.tone));
    if !request.user_transcript.trim().is_empty() {
        lines.push(format!(
            "Current user turn: {}",
            request.user_transcript.trim()
        ));
    }
    lines.push("Relevant memory:".to_string());
    for selection in &assembly.selected {
        lines.push(format!(
            "- {} [{}]",
            selection.fragment.summary, selection.fragment.source_reference
        ));
    }
    lines.join("\n")
}

fn build_session_instructions(request: &MemoryInjectionRequest<'_>) -> String {
    format!(
        "{base}\n\nSession identity: {session_identity}\nTone: {tone}\n\nUse injected working memory only when it is relevant and keep the spoken answer brief.",
        base = request.base_instructions.trim(),
        session_identity = request.session_identity,
        tone = request.tone,
    )
}

#[cfg(test)]
mod tests {
    use qsf_memory::retrieval::{
        AssociationPath, RetrievalScore, RetrievalStrategy, RetrievedMemory,
    };
    use qsf_memory::{MemoryRecord, MemoryRecordKind};
    use time::OffsetDateTime;

    use super::*;

    fn retrieved_memory(id: &str, summary: &str, estimated_tokens: usize) -> RetrievedMemory {
        RetrievedMemory {
            memory: MemoryRecord::new(
                id,
                MemoryRecordKind::Concept,
                format!("Title {id}"),
                summary,
                vec!["test"],
                OffsetDateTime::UNIX_EPOCH,
                0.8,
                0,
                "tests",
                estimated_tokens,
            ),
            strategy: RetrievalStrategy::AssociationWeighted,
            score: RetrievalScore {
                total: 1.0,
                recency: 0.2,
                keyword: 0.2,
                tag: 0.2,
                association: 0.2,
                importance: 0.1,
                reinforcement: 0.1,
            },
            matched_terms: vec!["test".to_string()],
            association_paths: vec![AssociationPath {
                from_memory_id: "seed".to_string(),
                to_memory_id: id.to_string(),
                weight: 0.5,
                reason: "fixture".to_string(),
            }],
            skip_reason: None,
        }
    }

    #[test]
    fn empty_retrieval_returns_no_injection() {
        let output_modalities = vec!["audio".to_string()];
        let request = MemoryInjectionRequest {
            model: "gpt-realtime-2",
            voice: "marin",
            base_instructions: "Speak briefly.",
            output_modalities: &output_modalities,
            session_identity: "session-1",
            tone: "calm",
            user_transcript: "hello",
            retrieved_memories: &[],
            budget: ContextBudget::new(2, 40),
            pcm_rate_hz: DEFAULT_PCM_RATE_HZ,
        };

        assert!(build_memory_injection_packet(&request).is_none());
        let assembly = assemble_memory_context(&request);
        assert!(assembly.selected.is_empty());
        assert_eq!(assembly.budget, ContextBudget::new(2, 40));
    }

    #[test]
    fn builder_caps_fragments_under_budget() {
        let memories = vec![
            retrieved_memory("memory-a", "First memory", 30),
            retrieved_memory("memory-b", "Second memory", 30),
            retrieved_memory("memory-c", "Third memory", 30),
        ];
        let output_modalities = vec!["audio".to_string()];
        let request = MemoryInjectionRequest {
            model: "gpt-realtime-2",
            voice: "marin",
            base_instructions: "Speak briefly.",
            output_modalities: &output_modalities,
            session_identity: "session-1",
            tone: "focused",
            user_transcript: "remember the earlier bit",
            retrieved_memories: &memories,
            budget: ContextBudget::new(1, 35),
            pcm_rate_hz: DEFAULT_PCM_RATE_HZ,
        };

        let packet = build_memory_injection_packet(&request).expect("packet");
        assert_eq!(packet.context_assembly.selected.len(), 1);
        assert_eq!(packet.context_assembly.omitted.len(), 2);
        assert_eq!(
            packet.session_update["session"]["audio"]["input"]["turn_detection"]["create_response"],
            false
        );
        assert_eq!(packet.conversation_item_create["item"]["role"], "system");
        assert!(packet.memory_block.contains("Session identity: session-1"));
        assert!(packet.memory_block.contains("First memory"));
        assert!(!packet.memory_block.contains("Second memory"));
        assert!(
            !packet.session_update["session"]["instructions"]
                .as_str()
                .expect("instructions")
                .contains("First memory")
        );
    }

    #[test]
    fn returned_context_assembly_matches_injected_content() {
        let memories = vec![retrieved_memory("memory-a", "Only memory", 18)];
        let output_modalities = vec!["audio".to_string()];
        let request = MemoryInjectionRequest {
            model: "gpt-realtime-2",
            voice: "marin",
            base_instructions: "Speak briefly.",
            output_modalities: &output_modalities,
            session_identity: "session-1",
            tone: "warm",
            user_transcript: "what do you remember?",
            retrieved_memories: &memories,
            budget: ContextBudget::new(2, 40),
            pcm_rate_hz: DEFAULT_PCM_RATE_HZ,
        };

        let packet = build_memory_injection_packet(&request).expect("packet");
        let selected_ids = packet.context_assembly.retrieved_memory_ids();
        assert_eq!(selected_ids, vec!["memory-a".to_string()]);
        assert!(
            packet.conversation_item_create["item"]["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("Only memory")
        );
    }
}
