use serde_json::json;

use crate::audio::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, SpeechOutputProviderError,
    SpeechOutputRequest, SpeechOutputSession, TranscriptProviderError, TranscriptProviderSession,
    transcript_provider_to_input_boundary,
};
use crate::context::ContextAssembly;
use crate::memory::{RetrievalResult, retrieve_memories, retrieved_memory_ids};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use qsf_models::{ModelRequest, ModelResponse};

use super::super::failure::{SanitizedFailure, record_sanitized_failure};
use super::memory_source::VoiceMemorySourceSnapshot;
use super::{
    VOICE_CONTEXT_ASSEMBLY_LATENCY_MS, VOICE_MEMORY_RETRIEVAL_LIMIT,
    VOICE_MEMORY_RETRIEVAL_STRATEGY,
};

pub(super) fn record_transcript_provider_trace(
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

pub(super) fn record_input_bridge_trace(
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

pub(super) fn retrieve_voice_memories(
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

pub(super) fn record_context_assembly(
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

pub(super) fn record_context_events(
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

pub(super) fn record_voice_model_response(
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

pub(super) fn record_speech_output(
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

pub(super) fn record_speech_events(
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

pub(super) fn record_voice_loop_latency(
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

pub(super) fn record_latency_event(
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

pub(super) fn record_transcript_failure(
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

pub(super) fn record_speech_failure(
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

pub(super) fn transcript_to_input_boundary() -> AudioRuntimeBoundary {
    transcript_provider_to_input_boundary(
        "Final transcript text becomes QSF-owned input only after AudioFinalTranscript.",
    )
}

pub(super) fn speech_output_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::RuntimeOutput,
        producer_event: EventType::OutputProduced,
        runtime_event: EventType::SpeechPlaybackRequested,
        description:
            "QSF-owned OutputProduced text is handed unchanged to the speech output provider."
                .to_string(),
    }
}

pub(super) fn voice_loop_total_latency_ms(
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
