use crate::audio::TranscriptProviderSession;
use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSourceKind, assemble_context,
};
use crate::conversation::prompt;
use crate::memory::RetrievedMemory;
use qsf_models::{ModelMessage, ModelRequest, ModelRole, ModelRoleId};

pub(super) fn assemble_voice_context(
    final_transcript: &str,
    retrieved_memories: &[RetrievedMemory],
) -> ContextAssembly {
    let mut fragments = vec![
        ContextFragment {
            fragment_id: "voice-loop-runtime-boundary".to_string(),
            source_kind: ContextSourceKind::RuntimeState,
            summary: "AudioFinalTranscript is the commit point; only finalized speech becomes InputReceived.".to_string(),
            tags: vec!["audio".to_string(), "runtime".to_string()],
            score: 1.0,
            estimated_tokens: 70,
            source_reference: "runtime/audio-loop".to_string(),
            selection_reason: "required to answer through the QSF-owned runtime boundary"
                .to_string(),
        },
        ContextFragment {
            fragment_id: "voice-loop-output-boundary".to_string(),
            source_kind: ContextSourceKind::RuntimeState,
            summary: "OutputProduced must exist before speech output providers receive text.".to_string(),
            tags: vec!["audio".to_string(), "speech-output".to_string()],
            score: 0.94,
            estimated_tokens: 58,
            source_reference: "runtime/audio-loop".to_string(),
            selection_reason: "keeps response ownership separate from speech rendering"
                .to_string(),
        },
        ContextFragment {
            fragment_id: "voice-loop-user-turn".to_string(),
            source_kind: ContextSourceKind::ProjectFrame,
            summary: format!("Current finalized spoken input: {final_transcript}"),
            tags: vec!["current-turn".to_string()],
            score: 0.9,
            estimated_tokens: 52,
            source_reference: "audio-final-transcript".to_string(),
            selection_reason: "current turn input anchors the spoken response".to_string(),
        },
    ];
    fragments.extend(retrieved_memories.iter().map(ContextFragment::from));

    assemble_context(fragments, ContextBudget::new(4, 600))
}

pub(super) fn retrieved_memory_block_with_boot_brief(
    assembly: &ContextAssembly,
    boot_brief_fragment: Option<&str>,
) -> String {
    let retrieved_memory_block = prompt::retrieved_memory_block(assembly);
    match (boot_brief_fragment, retrieved_memory_block.is_empty()) {
        (Some(brief), true) => brief.to_string(),
        (Some(brief), false) => format!("{brief}\n\n{retrieved_memory_block}"),
        (None, _) => retrieved_memory_block,
    }
}

pub(super) fn voice_utterance_id(session: &TranscriptProviderSession) -> String {
    format!(
        "{}-utterance-{}",
        session.session_id, session.final_transcript.utterance_index
    )
}

pub(super) fn build_conversational_request(
    session_id: &str,
    final_transcript: &str,
    assembly: &ContextAssembly,
    retrieved_memory_block: &str,
) -> ModelRequest {
    let context_summary = assembly
        .selected
        .iter()
        .map(|selection| {
            format!(
                "- {}: {}",
                selection.fragment.fragment_id, selection.fragment.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    ModelRequest::new(
        ModelRole::predefined(ModelRoleId::ConversationalResponder),
        vec![
            ModelMessage::system(
                "Answer as a short spoken QSF-owned response. Do not claim that the speech provider generated the answer.",
            ),
            ModelMessage::user(format!(
                "Final transcript:\n{final_transcript}\n\nSelected context:\n{context_summary}\n\nRetrieved memory and session brief:\n{retrieved_memory_block}"
            )),
        ],
    )
    .with_session_id(session_id)
    .with_temperature(0.0)
    .with_max_output_tokens(120)
}
