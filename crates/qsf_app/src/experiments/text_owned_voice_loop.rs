use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::audio::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, SpeechOutputProvider,
    SpeechOutputProviderError, SpeechOutputRequest, SpeechOutputSession, TranscriptEventEmission,
    TranscriptEventTraceIds, TranscriptProvider, TranscriptProviderError,
    TranscriptProviderRequest, TranscriptProviderSession, build_speech_output_provider,
    build_transcript_provider, record_transcript_runtime_events,
    requested_speech_output_provider_from_env, requested_transcript_provider_from_env,
    transcript_provider_to_input_boundary,
};
use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSourceKind, assemble_context,
};
use crate::memory::{
    Association, MemoryFixture, MemoryRecord, RetrievalResult, RetrievalStrategy, RetrievedMemory,
    phase_four_fixture, retrieve_memories, retrieved_memory_ids,
};
use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId, build_client,
    invoke_model_role, requested_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ms};
use crate::runtime::run_context::RunContext;

use super::failure::{SanitizedFailure, record_sanitized_failure};
use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const VOICE_CONTEXT_ASSEMBLY_LATENCY_MS: u64 = 6;
const VOICE_MEMORY_RETRIEVAL_LIMIT: usize = 1;
const VOICE_MEMORY_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::AssociationWeighted;
const VOICE_MEMORY_SOURCE_ENV_VAR: &str = "QSF_VOICE_MEMORY_SOURCE";
const VOICE_MEMORY_FILE_ENV_VAR: &str = "QSF_VOICE_MEMORY_FILE";

pub struct TextOwnedVoiceLoopExperiment;

impl Experiment for TextOwnedVoiceLoopExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::TextOwnedVoiceLoop
    }

    fn description(&self) -> &'static str {
        "Capture or simulate speech, route finalized text through QSF-owned model behavior, then synthesize speech output from the QSF text response"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let transcript_provider =
            build_transcript_provider(requested_transcript_provider_from_env());
        let model_client = build_client(requested_provider_from_env())?;
        let speech_provider =
            build_speech_output_provider(requested_speech_output_provider_from_env());
        let memory_source = build_voice_memory_source_from_env()?;

        self.run_with_components_and_memory_source(
            context,
            transcript_provider.as_ref(),
            model_client.as_ref(),
            speech_provider.as_ref(),
            memory_source.as_ref(),
        )
    }
}

impl TextOwnedVoiceLoopExperiment {
    #[cfg(test)]
    fn run_with_components(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        self.run_with_components_and_memory_source(
            context,
            transcript_provider,
            model_client,
            speech_provider,
            &PhaseFourVoiceMemorySource,
        )
    }

    fn run_with_components_and_memory_source(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
        memory_source: &dyn VoiceLoopMemorySource,
    ) -> anyhow::Result<ExperimentOutcome> {
        let session_id = format!("{}-text-owned-voice-loop", context.run_id());
        let transcript_request = TranscriptProviderRequest::from_env(&session_id);
        let transcript_session = match transcript_provider.transcribe(&transcript_request) {
            Ok(session) => session,
            Err(error) => {
                record_transcript_failure(context, &error)?;
                return Err(error).context("transcript provider failed");
            }
        };
        if transcript_session
            .final_transcript
            .transcript
            .trim()
            .is_empty()
        {
            let error = TranscriptProviderError::TranscriptionFailed {
                provider: transcript_session.provider_name.clone(),
                message: "transcript provider returned an empty final transcript".to_string(),
            };
            record_transcript_failure(context, &error)?;
            return Err(error).context("transcript provider failed");
        }

        let provider_trace_id = record_transcript_provider_trace(context, &transcript_session)?;
        let input_bridge_trace_id = record_input_bridge_trace(context, &transcript_session)?;
        record_transcript_runtime_events(
            context,
            &transcript_session,
            TranscriptEventTraceIds {
                provider_trace_id,
                latency_trace_id: provider_trace_id,
                runtime_bridge_trace_id: input_bridge_trace_id,
            },
            TranscriptEventEmission::new(transcript_to_input_boundary(), "transcription"),
        )?;

        let memory_snapshot = memory_source.load()?;
        write_voice_memory_source_snapshot(context, &memory_snapshot)?;
        let memory_retrieval = retrieve_voice_memories(
            context,
            &transcript_session.session_id,
            &transcript_session.final_transcript.transcript,
            &memory_snapshot,
        )?;

        let context_assembly = assemble_voice_context(
            &transcript_session.final_transcript.transcript,
            &memory_retrieval.selected,
        );
        let context_trace_id =
            record_context_assembly(context, &transcript_session.session_id, &context_assembly)?;
        record_context_events(
            context,
            &transcript_session.session_id,
            &context_assembly,
            context_trace_id,
        )?;

        let model_request = build_conversational_request(
            &transcript_session.session_id,
            &transcript_session.final_transcript.transcript,
            &context_assembly,
        );
        let model_started_at = Instant::now();
        let model_response = invoke_model_role(context, model_client, &model_request)?;
        let model_latency_ms = elapsed_ms(model_started_at);
        let model_trace_id = record_voice_model_response(
            context,
            &transcript_session.session_id,
            &model_request,
            &model_response,
            model_latency_ms,
        )?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "session_id": &transcript_session.session_id,
                "source": "qsf_model_role",
                "role_id": model_response.role_id,
                "provider_name": &model_response.provider_name,
                "model_name": &model_response.model_name,
                "message": &model_response.output_text,
                "target": "speech-output-provider",
                "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
            }),
            Some(model_trace_id),
        )?;
        engine_logging::engine_info!(
            "text-owned voice response produced: experiment_id={} run_id={} session_id={}",
            context.experiment_id(),
            context.run_id(),
            transcript_session.session_id
        );

        let speech_request = SpeechOutputRequest::from_env(
            &transcript_session.session_id,
            &model_response.output_text,
        );
        context.record_event(
            EventType::SpeechPlaybackRequested,
            json!({
                "session_id": &transcript_session.session_id,
                "provider": speech_provider.provider_name(),
                "model": &speech_request.model,
                "voice": &speech_request.voice,
                "output_mode": speech_request.output_mode,
                "text": &speech_request.text,
                "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
                "entry_point_description": AudioRuntimeEntryPoint::RuntimeOutput.description(),
                "boundary": speech_output_boundary(),
                "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            }),
            Some(model_trace_id),
        )?;

        let speech_session = match speech_provider.synthesize(&speech_request) {
            Ok(session) => session,
            Err(error) => {
                record_speech_failure(context, &transcript_session.session_id, &error)?;
                return Err(error).context("speech output provider failed");
            }
        };
        let speech_trace_id = record_speech_output(context, &speech_request, &speech_session)?;
        record_speech_events(context, &speech_session, speech_trace_id)?;

        let latency_trace_id = record_voice_loop_latency(
            context,
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            context_assembly.used_estimated_tokens,
            model_latency_ms,
        )?;
        record_latency_event(
            context,
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            model_latency_ms,
            latency_trace_id,
        )?;

        let report_timing = VoiceLoopReportTiming::new(
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            model_latency_ms,
        );
        write_text_owned_voice_loop_report(
            context,
            VoiceLoopReport {
                transcript_session: &transcript_session,
                context_assembly: &context_assembly,
                memory_snapshot: &memory_snapshot,
                model_response: &model_response,
                speech_request: &speech_request,
                speech_session: &speech_session,
                timing: report_timing,
            },
        )?;

        Ok(ExperimentOutcome {
            summary: "The deterministic text-owned voice loop now turns simulated speech into AudioFinalTranscript, bridges it to InputReceived, assembles QSF context, invokes the ConversationalResponder model role, emits OutputProduced, and hands that exact text to a simulated speech output provider.".to_string(),
            observations: vec![
                "The answer text is produced by the QSF model-role path before speech output receives it.".to_string(),
                "One retrieved memory fragment now participates in the selected context passed to ConversationalResponder.".to_string(),
                "The default speech output provider records metadata-only playback lifecycle events without persisting raw audio.".to_string(),
                "One voice-loop session id correlates transcript, context, model, speech output, and latency records.".to_string(),
            ],
            failure_modes: vec![
                "OpenAI speech output remains unavailable by design until the simulated exact-text boundary is proven.".to_string(),
                "Live microphone input remains an explicit transcript-provider configuration path and is not the default.".to_string(),
            ],
            follow_up_questions: vec![
                "Should the AudioFinalTranscript to InputReceived bridge become a shared dispatcher before more audio experiments reuse it?".to_string(),
                "What latency budget should ConversationalResponder target for live microphone turns?".to_string(),
            ],
            decision_candidates: vec![
                "Voice interfaces should adapt around QSF-owned text turns unless an experiment is explicitly provider-owned.".to_string(),
                "Speech output providers receive exactly OutputProduced text and do not alter response ownership.".to_string(),
            ],
            extra_artifacts: vec![
                "text-owned-voice-loop.md".to_string(),
                "voice-memory-source.json".to_string(),
            ],
        })
    }
}

trait VoiceLoopMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot>;
}

struct PhaseFourVoiceMemorySource;

impl VoiceLoopMemorySource for PhaseFourVoiceMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot> {
        Ok(VoiceMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "crate::memory::phase_four_fixture",
            phase_four_fixture(),
        ))
    }
}

struct FileVoiceMemorySource {
    path: PathBuf,
}

impl FileVoiceMemorySource {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl VoiceLoopMemorySource for FileVoiceMemorySource {
    fn load(&self) -> anyhow::Result<VoiceMemorySourceSnapshot> {
        let contents = fs::read_to_string(&self.path).with_context(|| {
            format!(
                "failed to read voice memory source file `{}`",
                self.path.display()
            )
        })?;
        let fixture: MemoryFixture = serde_json::from_str(&contents).with_context(|| {
            format!(
                "failed to parse voice memory source file `{}`",
                self.path.display()
            )
        })?;

        Ok(VoiceMemorySourceSnapshot::from_fixture(
            "file",
            self.path.display().to_string(),
            fixture,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct VoiceMemorySourceSnapshot {
    source_name: String,
    source_reference: String,
    records: Vec<MemoryRecord>,
    associations: Vec<Association>,
}

impl VoiceMemorySourceSnapshot {
    fn from_fixture(
        source_name: impl Into<String>,
        source_reference: impl Into<String>,
        fixture: MemoryFixture,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            source_reference: source_reference.into(),
            records: fixture.records,
            associations: fixture.associations,
        }
    }

    fn record_count(&self) -> usize {
        self.records.len()
    }

    fn association_count(&self) -> usize {
        self.associations.len()
    }
}

fn build_voice_memory_source_from_env() -> anyhow::Result<Box<dyn VoiceLoopMemorySource>> {
    match std::env::var(VOICE_MEMORY_SOURCE_ENV_VAR)
        .unwrap_or_else(|_| "phase_four_fixture".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "phase_four_fixture" | "fixture" => Ok(Box::new(PhaseFourVoiceMemorySource)),
        "file" => {
            let path = std::env::var(VOICE_MEMORY_FILE_ENV_VAR).with_context(|| {
                format!(
                    "`{VOICE_MEMORY_FILE_ENV_VAR}` must be set when `{VOICE_MEMORY_SOURCE_ENV_VAR}=file`"
                )
            })?;
            Ok(Box::new(FileVoiceMemorySource::new(path)))
        }
        value => anyhow::bail!(
            "unsupported voice memory source `{}`; expected `phase_four_fixture` or `file`",
            value
        ),
    }
}

fn record_transcript_provider_trace(
    context: &mut RunContext,
    session: &TranscriptProviderSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "transcript-provider-session",
        format!(
            "provider={} input_source={}",
            session.provider_name,
            session.input_source.label()
        ),
        format!(
            "emitted {} partial revisions and one final transcript",
            session.partial_revision_count()
        ),
    )
    .with_details(json!({
        "session_id": &session.session_id,
        "provider": &session.provider_name,
        "model": &session.model,
        "input_source": &session.input_source,
        "partial_count": session.partial_revision_count(),
        "final_transcript_received_at_ms": session.final_transcript.received_at_ms,
        "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
    }))
    .with_latency_context("audio", "transcript-provider-session")
    .with_latency_ms(session.final_transcript_latency_ms());
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_input_bridge_trace(
    context: &mut RunContext,
    session: &TranscriptProviderSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-runtime-input-bridge",
        "AudioFinalTranscript emitted by transcript provider",
        "InputReceived emitted as the QSF-owned runtime input",
    )
    .with_details(json!({
        "session_id": &session.session_id,
        "boundary": transcript_to_input_boundary(),
        "final_transcript_length": session.final_transcript_text_length(),
    }))
    .with_latency_context("runtime", "voice-runtime-input-bridge")
    .with_latency_ms(
        session
            .completed_at_ms
            .saturating_sub(session.final_transcript.received_at_ms),
    );
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn retrieve_voice_memories(
    context: &mut RunContext,
    session_id: &str,
    query: &str,
    memory_snapshot: &VoiceMemorySourceSnapshot,
) -> anyhow::Result<RetrievalResult> {
    context.record_event(
        EventType::MemoryRetrievalRequested,
        json!({
            "session_id": session_id,
            "query": query,
            "strategy": VOICE_MEMORY_RETRIEVAL_STRATEGY,
            "retrieval_limit": VOICE_MEMORY_RETRIEVAL_LIMIT,
            "memory_source": &memory_snapshot.source_name,
            "memory_source_reference": &memory_snapshot.source_reference,
            "memory_records": memory_snapshot.record_count(),
            "associations": memory_snapshot.association_count(),
        }),
        None,
    )?;

    let retrieval = retrieve_memories(
        &memory_snapshot.records,
        &memory_snapshot.associations,
        query,
        VOICE_MEMORY_RETRIEVAL_STRATEGY,
        VOICE_MEMORY_RETRIEVAL_LIMIT,
    )?;
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-memory-retrieval",
        format!("strategy={} query={}", retrieval.strategy, retrieval.query),
        format!(
            "selected {} memory fragments and omitted {}",
            retrieval.selected.len(),
            retrieval.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": session_id,
        "memory_source": &memory_snapshot.source_name,
        "memory_source_reference": &memory_snapshot.source_reference,
        "query": &retrieval.query,
        "strategy": retrieval.strategy,
        "selected": &retrieval.selected,
        "omitted": &retrieval.omitted,
    }))
    .with_latency_context("runtime", "voice-memory-retrieval")
    .with_latency_ns(retrieval.latency_ns);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;

    context.record_event(
        EventType::MemoryRetrieved,
        json!({
            "session_id": session_id,
            "memory_source": &memory_snapshot.source_name,
            "strategy": retrieval.strategy,
            "selected": retrieved_memory_ids(&retrieval.selected),
            "omitted": retrieved_memory_ids(&retrieval.omitted),
            "latency_ms": retrieval.latency_ms,
            "latency_ns": retrieval.latency_ns,
        }),
        Some(trace_id),
    )?;

    Ok(retrieval)
}

fn assemble_voice_context(
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

fn record_context_assembly(
    context: &mut RunContext,
    session_id: &str,
    assembly: &ContextAssembly,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-context-assembly",
        "final transcript plus QSF voice-loop context candidates",
        format!(
            "selected {} fragments and omitted {}",
            assembly.selected.len(),
            assembly.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": session_id,
        "assembly": assembly,
    }))
    .with_latency_context("runtime", "voice-context-assembly")
    .with_latency_ms(VOICE_CONTEXT_ASSEMBLY_LATENCY_MS);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_context_events(
    context: &mut RunContext,
    session_id: &str,
    assembly: &ContextAssembly,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::ContextAssemblyRequested,
        json!({
            "session_id": session_id,
            "source_event": EventType::InputReceived,
            "budget": &assembly.budget,
        }),
        Some(trace_id),
    )?;

    context.record_event(
        EventType::ContextAssembled,
        json!({
            "session_id": session_id,
            "selected_count": assembly.selected.len(),
            "omitted_count": assembly.omitted.len(),
            "used_estimated_tokens": assembly.used_estimated_tokens,
            "selected": &assembly.selected,
            "omitted": &assembly.omitted,
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn build_conversational_request(
    session_id: &str,
    final_transcript: &str,
    assembly: &ContextAssembly,
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
                "Final transcript:\n{final_transcript}\n\nSelected context:\n{context_summary}"
            )),
        ],
    )
    .with_session_id(session_id)
    .with_temperature(0.0)
    .with_max_output_tokens(120)
}

fn record_voice_model_response(
    context: &mut RunContext,
    session_id: &str,
    request: &ModelRequest,
    response: &ModelResponse,
    model_latency_ms: u64,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-model-response-summary",
        format!(
            "role={} provider={}",
            request.role.role_id, response.provider_name
        ),
        response.output_summary(),
    )
    .with_details(json!({
        "session_id": session_id,
        "role_id": request.role.role_id,
        "provider": &response.provider_name,
        "model": &response.model_name,
        "response_length": response.output_text.chars().count(),
        "usage": &response.usage,
        "finish_reason": &response.finish_reason,
        "model_latency_ms": model_latency_ms,
    }))
    .with_latency_context("runtime", "voice-model-response-summary")
    .with_latency_ms(model_latency_ms);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_speech_output(
    context: &mut RunContext,
    request: &SpeechOutputRequest,
    session: &SpeechOutputSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "speech-output-provider",
        format!(
            "provider={} voice={} output_mode={}",
            session.provider_name, session.voice, session.output_mode
        ),
        format!(
            "metadata recorded for {} chars of QSF-owned text",
            session.text_length
        ),
    )
    .with_details(json!({
        "session_id": &session.session_id,
        "speech_provider": &session.provider_name,
        "model": &session.model,
        "voice": &session.voice,
        "output_mode": session.output_mode,
        "audio_byte_count": session.audio_output_bytes,
        "playback_adapter": &session.playback_adapter,
        "playback_timing": session,
        "exact_text_received": request.text,
    }))
    .with_latency_context("audio", "speech-output-provider")
    .with_latency_ms(session.total_latency_ms());
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_speech_events(
    context: &mut RunContext,
    session: &SpeechOutputSession,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::SpeechPlaybackStarted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "adapter": &session.playback_adapter,
            "voice": &session.voice,
            "output_mode": session.output_mode,
            "started_at_ms": session.first_audio_at_ms,
            "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    context.record_event(
        EventType::SpeechPlaybackCompleted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "adapter": &session.playback_adapter,
            "voice": &session.voice,
            "output_mode": session.output_mode,
            "completed_at_ms": session.completed_at_ms,
            "audio_output_bytes": session.audio_output_bytes,
            "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn record_voice_loop_latency(
    context: &mut RunContext,
    transcript_session: &TranscriptProviderSession,
    memory_retrieval: &RetrievalResult,
    speech_session: &SpeechOutputSession,
    context_tokens: usize,
    model_latency_ms: u64,
) -> anyhow::Result<uuid::Uuid> {
    let capture_completed_ms = transcript_session
        .chunks
        .last()
        .map(|chunk| chunk.captured_at_ms + chunk.duration_ms)
        .unwrap_or(transcript_session.completed_at_ms);
    let input_received_ms = transcript_session.completed_at_ms;
    let memory_started_ms = input_received_ms;
    let memory_completed_ms = memory_started_ms.saturating_add(memory_retrieval.latency_ms);
    let context_started_ms = memory_completed_ms;
    let context_completed_ms = context_started_ms + VOICE_CONTEXT_ASSEMBLY_LATENCY_MS;
    let model_started_ms = context_completed_ms;
    let model_completed_ms = model_started_ms.saturating_add(model_latency_ms);
    let output_produced_ms = model_completed_ms;
    let speech_requested_ms = output_produced_ms;
    let speech_provider_first_audio_offset_ms = speech_session
        .first_audio_at_ms
        .unwrap_or(speech_session.completed_at_ms)
        .saturating_sub(speech_session.started_at_ms);
    let speech_started_ms = speech_requested_ms + speech_provider_first_audio_offset_ms;
    let speech_completed_ms = speech_requested_ms + speech_session.total_latency_ms();
    let total_turn_ms = voice_loop_total_latency_ms(
        transcript_session,
        memory_retrieval,
        model_latency_ms,
        speech_session,
    );

    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-loop-latency",
        "text-owned voice turn timing",
        "capture, transcription, runtime, model, and speech output timings recorded",
    )
    .with_details(json!({
        "session_id": &transcript_session.session_id,
        "capture_started_ms": transcript_session.started_at_ms,
        "capture_completed_ms": capture_completed_ms,
        "first_partial_transcript_ms": transcript_session.partials.first().map(|partial| partial.received_at_ms),
        "final_transcript_ms": transcript_session.final_transcript.received_at_ms,
        "input_received_ms": input_received_ms,
        "memory_started_ms": memory_started_ms,
        "memory_completed_ms": memory_completed_ms,
        "memory_retrieval_latency_ms": memory_retrieval.latency_ms,
        "context_started_ms": context_started_ms,
        "context_completed_ms": context_completed_ms,
        "context_assembly_latency_ms": VOICE_CONTEXT_ASSEMBLY_LATENCY_MS,
        "model_started_ms": model_started_ms,
        "model_completed_ms": model_completed_ms,
        "model_role_latency_ms": model_latency_ms,
        "output_produced_ms": output_produced_ms,
        "speech_requested_ms": speech_requested_ms,
        "speech_started_ms": speech_started_ms,
        "speech_completed_ms": speech_completed_ms,
        "timeline_source": "derived_from_provider_and_stage_latency_reports",
        "context_used_estimated_tokens": context_tokens,
        "transcript_measurements": transcript_session.latency_measurements(),
        "speech_measurements": speech_session.latency_measurements(),
        "total_turn_ms": total_turn_ms,
    }))
    .with_latency_context("audio", "text-owned-voice-loop")
    .with_latency_ms(total_turn_ms);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_latency_event(
    context: &mut RunContext,
    transcript_session: &TranscriptProviderSession,
    memory_retrieval: &RetrievalResult,
    speech_session: &SpeechOutputSession,
    model_latency_ms: u64,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    let total_turn_ms = voice_loop_total_latency_ms(
        transcript_session,
        memory_retrieval,
        model_latency_ms,
        speech_session,
    );

    context.record_event(
        EventType::LatencyMeasurementRecorded,
        json!({
            "session_id": &transcript_session.session_id,
            "domain": "audio",
            "stage": "text-owned-voice-loop",
            "transcript_measurements": transcript_session.latency_measurements(),
            "speech_measurements": speech_session.latency_measurements(),
            "first_partial_latency_ms": transcript_session.first_partial_latency_ms(),
            "final_transcript_latency_ms": transcript_session.final_transcript_latency_ms(),
            "memory_retrieval_latency_ms": memory_retrieval.latency_ms,
            "context_assembly_latency_ms": VOICE_CONTEXT_ASSEMBLY_LATENCY_MS,
            "model_role_latency_ms": model_latency_ms,
            "speech_output_latency_ms": speech_session.total_latency_ms(),
            "total_turn_ms": total_turn_ms,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn record_transcript_failure(
    context: &mut RunContext,
    error: &TranscriptProviderError,
) -> anyhow::Result<()> {
    engine_logging::engine_error!(
        "transcript provider failed: experiment_id={} run_id={} provider={} error={}",
        context.experiment_id(),
        context.run_id(),
        error.provider(),
        error.sanitized_message()
    );
    let sanitized_message = error.sanitized_message();
    record_sanitized_failure(
        context,
        SanitizedFailure {
            event_type: EventType::AudioTranscriptionFailed,
            operation: "transcript-provider-session",
            output_summary: "transcription failed before text-owned voice loop input",
            latency_stage: "text-owned-voice-loop",
            provider: error.provider(),
            error_category: error.category(),
            sanitized_error: &sanitized_message,
            extra_payload: serde_json::Map::new(),
        },
    )
}

fn record_speech_failure(
    context: &mut RunContext,
    session_id: &str,
    error: &SpeechOutputProviderError,
) -> anyhow::Result<()> {
    engine_logging::engine_error!(
        "speech output provider failed: experiment_id={} run_id={} session_id={} provider={} error={}",
        context.experiment_id(),
        context.run_id(),
        session_id,
        error.provider(),
        error.sanitized_message()
    );
    let sanitized_message = error.sanitized_message();
    let mut extra_payload = serde_json::Map::new();
    extra_payload.insert("session_id".to_string(), json!(session_id));
    extra_payload.insert("operation".to_string(), json!("speech-output-provider"));

    record_sanitized_failure(
        context,
        SanitizedFailure {
            event_type: EventType::ErrorOccurred,
            operation: "speech-output-provider",
            output_summary: "speech output failed after OutputProduced",
            latency_stage: "speech-output-provider",
            provider: error.provider(),
            error_category: error.category(),
            sanitized_error: &sanitized_message,
            extra_payload,
        },
    )
}

fn transcript_to_input_boundary() -> AudioRuntimeBoundary {
    transcript_provider_to_input_boundary(
        "Final transcript text becomes QSF-owned input only after AudioFinalTranscript.",
    )
}

fn speech_output_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::RuntimeOutput,
        producer_event: EventType::OutputProduced,
        runtime_event: EventType::SpeechPlaybackRequested,
        description:
            "QSF-owned OutputProduced text is handed unchanged to the speech output provider."
                .to_string(),
    }
}

struct VoiceLoopReport<'a> {
    transcript_session: &'a TranscriptProviderSession,
    context_assembly: &'a ContextAssembly,
    memory_snapshot: &'a VoiceMemorySourceSnapshot,
    model_response: &'a ModelResponse,
    speech_request: &'a SpeechOutputRequest,
    speech_session: &'a SpeechOutputSession,
    timing: VoiceLoopReportTiming,
}

fn write_text_owned_voice_loop_report(
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
struct VoiceLoopReportTiming {
    memory_retrieval_latency_ms: u64,
    context_assembly_latency_ms: u64,
    model_role_latency_ms: u64,
    speech_output_latency_ms: u64,
    total_observed_turn_latency_ms: u64,
}

impl VoiceLoopReportTiming {
    fn new(
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

fn write_voice_memory_source_snapshot(
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

fn voice_loop_total_latency_ms(
    transcript_session: &TranscriptProviderSession,
    memory_retrieval: &RetrievalResult,
    model_latency_ms: u64,
    speech_session: &SpeechOutputSession,
) -> u64 {
    transcript_session
        .completed_at_ms
        .saturating_add(memory_retrieval.latency_ms)
        .saturating_add(VOICE_CONTEXT_ASSEMBLY_LATENCY_MS)
        .saturating_add(model_latency_ms)
        .saturating_add(speech_session.total_latency_ms())
        .saturating_sub(transcript_session.started_at_ms)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use super::{
        FileVoiceMemorySource, TextOwnedVoiceLoopExperiment, VOICE_CONTEXT_ASSEMBLY_LATENCY_MS,
    };
    use crate::audio::test_support::{
        assert_events_have_safety_markers, assert_payloads_do_not_contain_raw_audio_fields,
        is_audio_or_speech_event, parse_event_records,
    };
    use crate::audio::{
        FinalTranscript, SimulatedSpeechOutputProvider, SimulatedTranscriptProvider,
        SpeechOutputProvider, SpeechOutputProviderError, SpeechOutputRequest, SpeechOutputSession,
        TranscriptProvider, TranscriptProviderError, TranscriptProviderRequest,
        TranscriptProviderSession,
    };
    use crate::experiments::registry::{Experiment, ExperimentName};
    use crate::memory::{MemoryFixture, MemoryRecord, MemoryRecordKind};
    use crate::models::{MockModelClient, ModelClient, ModelRequest, ModelResponse};
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use anyhow::anyhow;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;
    use uuid::Uuid;

    struct FailingModelClient;

    impl ModelClient for FailingModelClient {
        fn client_name(&self) -> &str {
            "failing-model"
        }

        fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            Err(anyhow!("model boom"))
        }
    }

    struct SlowModelClient {
        delay: Duration,
    }

    impl ModelClient for SlowModelClient {
        fn client_name(&self) -> &str {
            "slow-mock-model"
        }

        fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            std::thread::sleep(self.delay);
            MockModelClient::default().complete(request)
        }
    }

    struct FailingSpeechOutputProvider;

    struct EmptyFinalTranscriptProvider;

    impl TranscriptProvider for EmptyFinalTranscriptProvider {
        fn provider_name(&self) -> &str {
            "empty-final-transcript-provider"
        }

        fn transcribe(
            &self,
            request: &TranscriptProviderRequest,
        ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
            let mut session = SimulatedTranscriptProvider.transcribe(request)?;
            session.provider_name = self.provider_name().to_string();
            session.final_transcript = FinalTranscript {
                utterance_index: 0,
                received_at_ms: 86,
                transcript: "   ".to_string(),
            };
            Ok(session)
        }
    }

    impl SpeechOutputProvider for FailingSpeechOutputProvider {
        fn provider_name(&self) -> &str {
            "failing-speech-output-provider"
        }

        fn synthesize(
            &self,
            _request: &SpeechOutputRequest,
        ) -> Result<SpeechOutputSession, SpeechOutputProviderError> {
            Err(SpeechOutputProviderError::SynthesisFailed {
                provider: self.provider_name().to_string(),
                message: "upstream returned Authorization: Bearer sk-test-secret".to_string(),
            })
        }
    }

    #[test]
    fn text_owned_voice_loop_writes_deterministic_turn_events_and_traces() {
        let base_dir = std::env::temp_dir().join(format!("qsf-text-owned-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &MockModelClient::default(),
                &SimulatedSpeechOutputProvider,
            )
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
        let event_records = parse_event_records(&events);

        assert_event_order(
            &event_records,
            &[
                EventType::AudioInputStarted,
                EventType::AudioInputChunkCaptured,
                EventType::AudioPartialTranscript,
                EventType::AudioFinalTranscript,
                EventType::AudioInputEnded,
                EventType::LatencyMeasurementRecorded,
                EventType::InputReceived,
                EventType::MemoryRetrievalRequested,
                EventType::MemoryRetrieved,
                EventType::ContextAssemblyRequested,
                EventType::ContextAssembled,
                EventType::ModelRoleRequested,
                EventType::ModelRoleCompleted,
                EventType::OutputProduced,
                EventType::SpeechPlaybackRequested,
                EventType::SpeechPlaybackStarted,
                EventType::SpeechPlaybackCompleted,
                EventType::LatencyMeasurementRecorded,
            ],
        );
        assert_only_final_transcript_commits_runtime_input(&event_records);
        assert_memory_context_participates_in_model_path(&event_records);
        assert_one_session_id_links_voice_loop_events(&event_records);
        assert_output_text_is_handed_exactly_to_speech_provider(&event_records);
        assert_events_have_safety_markers(&event_records, is_audio_or_speech_event);
        assert_payloads_do_not_contain_raw_audio_fields(&event_records);
        assert!(traces.contains("transcript-provider-session"));
        assert!(traces.contains("voice-runtime-input-bridge"));
        assert!(traces.contains("voice-memory-retrieval"));
        assert!(traces.contains("voice-context-assembly"));
        assert!(traces.contains("voice-model-response"));
        assert!(traces.contains("speech-output-provider"));
        assert!(traces.contains("voice-loop-latency"));
        assert!(report.contains("Text-Owned Voice Loop"));
        assert!(report.contains("Selected memory context: `memory."));
        assert!(report.contains("Model role latency:"));
        assert!(report.contains("Total observed turn latency:"));
        assert!(report.contains("## Diagnostics"));
        assert!(report.contains("Response owner: `qsf_model_role`"));
        assert!(report.contains("Memory source: `phase_four_fixture`"));
        assert!(report.contains("Memory records: `10`"));
        assert!(report.contains("Retrieval strategy: `association-weighted`"));
        assert!(report.contains("Exact speech handoff: `true`"));
        assert!(report.contains("Raw audio logged: `false`"));
        assert!(context.run_dir().join("voice-memory-source.json").exists());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn file_memory_source_drives_voice_retrieval_and_diagnostics() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-memory-file-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_dir).unwrap();
        let source_path = base_dir.join("voice-memory.json");
        let fixture = MemoryFixture {
            records: vec![MemoryRecord::new(
                "memory.voice-session-input",
                MemoryRecordKind::Observation,
                "Voice session input",
                "Streaming transcription enters QSF as finalized input events.",
                vec!["streaming", "transcription", "events", "runtime"],
                timestamp("2026-05-15T06:00:00Z"),
                0.9,
                1,
                "tests/voice-memory.json",
                28,
            )],
            associations: vec![],
        };
        fs::write(
            &source_path,
            serde_json::to_string_pretty(&fixture).unwrap(),
        )
        .unwrap();
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        experiment
            .run_with_components_and_memory_source(
                &mut context,
                &SimulatedTranscriptProvider,
                &MockModelClient::default(),
                &SimulatedSpeechOutputProvider,
                &FileVoiceMemorySource::new(&source_path),
            )
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
        let source_snapshot =
            fs::read_to_string(context.run_dir().join("voice-memory-source.json")).unwrap();
        let event_records = parse_event_records(&events);

        assert!(report.contains("Selected memory context: `memory.voice-session-input`"));
        assert!(report.contains("Memory source: `file`"));
        assert!(report.contains("Memory records: `1`"));
        assert!(source_snapshot.contains("memory.voice-session-input"));
        assert!(source_snapshot.contains("voice-memory.json"));
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::MemoryRetrievalRequested
                && record.payload["memory_source"] == "file"
                && record.payload["memory_records"] == 1
        }));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn latency_summary_includes_model_runtime() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-latency-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &SlowModelClient {
                    delay: Duration::from_millis(25),
                },
                &SimulatedSpeechOutputProvider,
            )
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
        let event_records = parse_event_records(&events);
        let latency = text_owned_loop_latency_event(&event_records);

        let final_transcript_latency_ms = latency.payload["final_transcript_latency_ms"]
            .as_u64()
            .unwrap();
        let memory_retrieval_latency_ms = latency.payload["memory_retrieval_latency_ms"]
            .as_u64()
            .unwrap();
        let context_assembly_latency_ms = latency.payload["context_assembly_latency_ms"]
            .as_u64()
            .unwrap();
        let model_role_latency_ms = latency.payload["model_role_latency_ms"].as_u64().unwrap();
        let speech_output_latency_ms = latency.payload["speech_output_latency_ms"]
            .as_u64()
            .unwrap();
        let total_turn_ms = latency.payload["total_turn_ms"].as_u64().unwrap();

        assert_eq!(
            context_assembly_latency_ms,
            VOICE_CONTEXT_ASSEMBLY_LATENCY_MS
        );
        assert!(model_role_latency_ms >= 10);
        assert!(
            total_turn_ms
                >= final_transcript_latency_ms
                    + memory_retrieval_latency_ms
                    + context_assembly_latency_ms
                    + model_role_latency_ms
                    + speech_output_latency_ms
        );
        assert!(report.contains(&format!("Model role latency: {model_role_latency_ms} ms")));
        assert!(report.contains(&format!("Total observed turn latency: {total_turn_ms} ms")));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn model_failure_records_failure_and_does_not_emit_output() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-model-fail-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        let error = experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &FailingModelClient,
                &SimulatedSpeechOutputProvider,
            )
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert!(error.to_string().contains("conversational_responder"));
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::ModelRoleFailed)
        );
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::OutputProduced)
        );
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::SpeechPlaybackRequested)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn empty_final_transcript_does_not_commit_runtime_input() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-empty-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        let error = experiment
            .run_with_components(
                &mut context,
                &EmptyFinalTranscriptProvider,
                &MockModelClient::default(),
                &SimulatedSpeechOutputProvider,
            )
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert_eq!(error.to_string(), "transcript provider failed");
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::AudioTranscriptionFailed
                && record.payload["error_category"] == "transcription_failed"
        }));
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::InputReceived)
        );
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::OutputProduced)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn speech_failure_is_sanitized_after_output_produced() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-speech-fail-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        let error = experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &MockModelClient::default(),
                &FailingSpeechOutputProvider,
            )
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert_eq!(error.to_string(), "speech output provider failed");
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::OutputProduced)
        );
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::ErrorOccurred
                && record.payload["error_category"] == "synthesis_failed"
                && record.payload["sanitized_error"]
                    == "provider error redacted because it may contain credential-like content"
        }));
        assert!(!events.contains("sk-test-secret"));
        assert!(!traces.contains("sk-test-secret"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn experiment_name_matches_registry_id() {
        let experiment = TextOwnedVoiceLoopExperiment;

        assert_eq!(experiment.name(), ExperimentName::TextOwnedVoiceLoop);
        assert_eq!(experiment.id(), "text-owned-voice-loop");
    }

    fn assert_event_order(records: &[EventRecord], expected_order: &[EventType]) {
        let mut next_index = 0usize;

        for record in records {
            if next_index < expected_order.len() && record.event_type == expected_order[next_index]
            {
                next_index += 1;
            }
        }

        assert_eq!(next_index, expected_order.len());
    }

    fn assert_only_final_transcript_commits_runtime_input(records: &[EventRecord]) {
        assert!(records.iter().any(|record| {
            record.event_type == EventType::AudioPartialTranscript
                && record.payload["committed_to_runtime"] == false
        }));
        let input_count = records
            .iter()
            .filter(|record| record.event_type == EventType::InputReceived)
            .count();
        assert_eq!(input_count, 1);
    }

    fn assert_memory_context_participates_in_model_path(records: &[EventRecord]) {
        let retrieval = records
            .iter()
            .find(|record| record.event_type == EventType::MemoryRetrieved)
            .unwrap();
        assert_eq!(retrieval.payload["strategy"], "association_weighted");
        let selected_memories = retrieval.payload["selected"].as_array().unwrap();
        assert_eq!(selected_memories.len(), 1);
        assert!(
            selected_memories[0]
                .as_str()
                .unwrap()
                .starts_with("memory.")
        );

        let context = records
            .iter()
            .find(|record| record.event_type == EventType::ContextAssembled)
            .unwrap();
        let selected_context = context.payload["selected"].as_array().unwrap();
        assert!(selected_context.iter().any(|selection| {
            selection["fragment"]["source_kind"] == "memory"
                && selection["fragment"]["fragment_id"]
                    .as_str()
                    .unwrap()
                    .starts_with("memory.")
        }));
    }

    fn assert_one_session_id_links_voice_loop_events(records: &[EventRecord]) {
        let session_ids = records
            .iter()
            .filter(|record| {
                matches!(
                    record.event_type,
                    EventType::AudioFinalTranscript
                        | EventType::InputReceived
                        | EventType::MemoryRetrievalRequested
                        | EventType::MemoryRetrieved
                        | EventType::ModelRoleRequested
                        | EventType::ModelRoleCompleted
                        | EventType::OutputProduced
                        | EventType::SpeechPlaybackRequested
                        | EventType::SpeechPlaybackCompleted
                )
            })
            .map(|record| record.payload["session_id"].as_str().unwrap().to_string())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(session_ids.len(), 1);
    }

    fn assert_output_text_is_handed_exactly_to_speech_provider(records: &[EventRecord]) {
        let output_text = records
            .iter()
            .find(|record| record.event_type == EventType::OutputProduced)
            .unwrap()
            .payload["message"]
            .as_str()
            .unwrap();
        let speech_text = records
            .iter()
            .find(|record| record.event_type == EventType::SpeechPlaybackRequested)
            .unwrap()
            .payload["text"]
            .as_str()
            .unwrap();

        assert_eq!(output_text, speech_text);
    }

    fn text_owned_loop_latency_event(records: &[EventRecord]) -> &EventRecord {
        records
            .iter()
            .rev()
            .find(|record| {
                record.event_type == EventType::LatencyMeasurementRecorded
                    && record.payload["stage"] == "text-owned-voice-loop"
            })
            .unwrap()
    }

    fn timestamp(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).unwrap()
    }
}
