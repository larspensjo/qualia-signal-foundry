use std::fs;

use anyhow::Context;

use crate::audio::{SpeechOutputRequest, SpeechOutputSession, TranscriptProviderSession};
use crate::context::{ContextAssembly, ContextSourceKind};
use crate::memory::RetrievalResult;
use crate::runtime::run_context::RunContext;
use qsf_models::ModelResponse;

use super::memory_source::VoiceMemorySourceSnapshot;
use super::{
    VOICE_CONTEXT_ASSEMBLY_LATENCY_MS, VOICE_MEMORY_RETRIEVAL_STRATEGY, voice_loop_total_latency_ms,
};

pub(crate) struct VoiceLoopReport<'a> {
    pub(crate) transcript_session: &'a TranscriptProviderSession,
    pub(crate) context_assembly: &'a ContextAssembly,
    pub(crate) memory_snapshot: &'a VoiceMemorySourceSnapshot,
    pub(crate) model_response: &'a ModelResponse,
    pub(crate) speech_request: &'a SpeechOutputRequest,
    pub(crate) speech_session: &'a SpeechOutputSession,
    pub(crate) timing: VoiceLoopReportTiming,
}

pub(crate) fn write_text_owned_voice_loop_report(
    context: &RunContext,
    report: VoiceLoopReport<'_>,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Text-Owned Voice Loop\n\n");
    markdown.push_str("## Turn\n\n");
    markdown.push_str(&format!(
        "- Session id: `{}`\n",
        report.transcript_session.session_id
    ));
    markdown.push_str(&format!(
        "- Transcript provider: `{}`\n",
        report.transcript_session.provider_name
    ));
    markdown.push_str(&format!(
        "- Final transcript: {}\n",
        report.transcript_session.final_transcript.transcript
    ));
    markdown.push_str(&format!(
        "- Context fragments selected: `{}`\n",
        report.context_assembly.selected.len()
    ));
    markdown.push_str(&format!(
        "- Selected context: `{}`\n",
        selected_context_ids(report.context_assembly).join(", ")
    ));
    markdown.push_str(&format!(
        "- Selected memory context: `{}`\n",
        selected_memory_context_ids(report.context_assembly).join(", ")
    ));
    markdown.push_str(&format!(
        "- Memory source: `{}`\n",
        report.memory_snapshot.source_name
    ));
    markdown.push_str(&format!(
        "- Memory source reference: `{}`\n",
        report.memory_snapshot.source_reference
    ));
    markdown.push_str(&format!(
        "- Model role: `{}` via `{}`\n",
        report.model_response.role_id, report.model_response.provider_name
    ));
    markdown.push_str(&format!(
        "- OutputProduced text: {}\n",
        report.model_response.output_text
    ));
    markdown.push_str(&format!(
        "- Speech output provider: `{}`\n",
        report.speech_session.provider_name
    ));
    markdown.push_str(&format!(
        "- Speech output mode: `{}`\n",
        report.speech_session.output_mode
    ));
    markdown.push_str("- Raw audio logged: `false`\n\n");

    markdown.push_str("## Latency\n\n");
    markdown.push_str(&format!(
        "- Final transcript latency: {} ms\n",
        report.transcript_session.final_transcript_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Memory retrieval latency: {} ms\n",
        report.timing.memory_retrieval_latency_ms
    ));
    markdown.push_str(&format!(
        "- Context assembly latency: {} ms\n",
        report.timing.context_assembly_latency_ms
    ));
    markdown.push_str(&format!(
        "- Model role latency: {} ms\n",
        report.timing.model_role_latency_ms
    ));
    markdown.push_str(&format!(
        "- Speech output latency: {} ms\n",
        report.timing.speech_output_latency_ms
    ));
    markdown.push_str(&format!(
        "- Total observed turn latency: {} ms\n",
        report.timing.total_observed_turn_latency_ms
    ));
    markdown.push('\n');

    push_diagnostics_section(&mut markdown, &report);

    fs::write(context.run_dir().join("text-owned-voice-loop.md"), markdown).with_context(|| {
        format!(
            "failed to write text-owned voice loop report for run {}",
            context.run_id()
        )
    })
}

fn push_diagnostics_section(markdown: &mut String, report: &VoiceLoopReport<'_>) {
    markdown.push_str("## Diagnostics\n\n");
    markdown.push_str("- Response owner: `qsf_model_role`\n");
    markdown.push_str(&format!(
        "- Selected memory context: `{}`\n",
        selected_memory_context_ids(report.context_assembly).join(", ")
    ));
    markdown.push_str(&format!(
        "- Memory source: `{}`\n",
        report.memory_snapshot.source_name
    ));
    markdown.push_str(&format!(
        "- Memory records: `{}`\n",
        report.memory_snapshot.record_count()
    ));
    markdown.push_str(&format!(
        "- Retrieval strategy: `{}`\n",
        VOICE_MEMORY_RETRIEVAL_STRATEGY
    ));
    markdown.push_str(&format!(
        "- Model provider: `{}`\n",
        report.model_response.provider_name
    ));
    markdown.push_str(&format!(
        "- Model: `{}`\n",
        report.model_response.model_name
    ));
    markdown.push_str(&format!(
        "- Model role latency: {} ms\n",
        report.timing.model_role_latency_ms
    ));
    markdown.push_str(&format!(
        "- Exact speech handoff: `{}`\n",
        report.speech_request.text == report.model_response.output_text
    ));
    markdown.push_str(&format!(
        "- Speech output provider: `{}`\n",
        report.speech_session.provider_name
    ));
    markdown.push_str(&format!(
        "- Total observed turn latency: {} ms\n",
        report.timing.total_observed_turn_latency_ms
    ));
    markdown.push_str("- Raw audio logged: `false`\n");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VoiceLoopReportTiming {
    memory_retrieval_latency_ms: u64,
    context_assembly_latency_ms: u64,
    model_role_latency_ms: u64,
    speech_output_latency_ms: u64,
    total_observed_turn_latency_ms: u64,
}

impl VoiceLoopReportTiming {
    pub(crate) fn new(
        transcript_session: &TranscriptProviderSession,
        memory_retrieval: &RetrievalResult,
        speech_session: &SpeechOutputSession,
        model_latency_ms: u64,
    ) -> Self {
        Self {
            memory_retrieval_latency_ms: memory_retrieval.latency_ms,
            context_assembly_latency_ms: VOICE_CONTEXT_ASSEMBLY_LATENCY_MS,
            model_role_latency_ms: model_latency_ms,
            speech_output_latency_ms: speech_session.total_latency_ms(),
            total_observed_turn_latency_ms: voice_loop_total_latency_ms(
                transcript_session,
                memory_retrieval,
                model_latency_ms,
                speech_session,
            ),
        }
    }
}

pub(crate) fn write_voice_memory_source_snapshot(
    context: &RunContext,
    snapshot: &VoiceMemorySourceSnapshot,
) -> anyhow::Result<()> {
    let contents = serde_json::to_string_pretty(snapshot)?;
    fs::write(context.run_dir().join("voice-memory-source.json"), contents).with_context(|| {
        format!(
            "failed to write voice memory source snapshot for run {}",
            context.run_id()
        )
    })
}

fn selected_context_ids(assembly: &ContextAssembly) -> Vec<String> {
    assembly
        .selected
        .iter()
        .map(|selection| selection.fragment.fragment_id.clone())
        .collect()
}

fn selected_memory_context_ids(assembly: &ContextAssembly) -> Vec<String> {
    let ids = assembly
        .selected
        .iter()
        .filter(|selection| selection.fragment.source_kind == ContextSourceKind::Memory)
        .map(|selection| selection.fragment.fragment_id.clone())
        .collect::<Vec<_>>();

    if ids.is_empty() {
        vec!["none".to_string()]
    } else {
        ids
    }
}
