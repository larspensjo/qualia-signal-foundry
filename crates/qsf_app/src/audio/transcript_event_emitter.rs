use serde_json::json;
use uuid::Uuid;

use crate::observability::event_log::EventType;
use crate::runtime::run_context::RunContext;

use super::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, TranscriptProviderSession,
};

#[derive(Clone, Copy, Debug)]
pub struct TranscriptEventTraceIds {
    pub provider_trace_id: Uuid,
    pub latency_trace_id: Uuid,
    pub runtime_bridge_trace_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct TranscriptEventEmission {
    pub boundary: AudioRuntimeBoundary,
    pub latency_stage: String,
}

impl TranscriptEventEmission {
    pub fn new(boundary: AudioRuntimeBoundary, latency_stage: impl Into<String>) -> Self {
        Self {
            boundary,
            latency_stage: latency_stage.into(),
        }
    }
}

pub fn record_transcript_runtime_events(
    context: &mut RunContext,
    session: &TranscriptProviderSession,
    trace_ids: TranscriptEventTraceIds,
    emission: TranscriptEventEmission,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::AudioInputStarted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "model": &session.model,
            "input_source": &session.input_source,
            "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_ids.provider_trace_id),
    )?;

    for chunk in &session.chunks {
        context.record_event(
            EventType::AudioInputChunkCaptured,
            json!({
                "session_id": &session.session_id,
                "provider": &session.provider_name,
                "chunk_index": chunk.chunk_index,
                "captured_chunk_count": session.chunks.len(),
                "captured_at_ms": chunk.captured_at_ms,
                "duration_ms": chunk.duration_ms,
                "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
                "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            }),
            Some(trace_ids.provider_trace_id),
        )?;
    }

    for partial in &session.partials {
        context.record_event(
            EventType::AudioPartialTranscript,
            json!({
                "session_id": &session.session_id,
                "provider": &session.provider_name,
                "utterance_index": partial.utterance_index,
                "revision_index": partial.revision_index,
                "source_chunk_index": partial.source_chunk_index,
                "received_at_ms": partial.received_at_ms,
                "transcript": &partial.transcript,
                "committed_to_runtime": false,
                "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            }),
            Some(trace_ids.provider_trace_id),
        )?;
    }

    context.record_event(
        EventType::AudioFinalTranscript,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "utterance_index": session.final_transcript.utterance_index,
            "received_at_ms": session.final_transcript.received_at_ms,
            "transcript": &session.final_transcript.transcript,
            "boundary": emission.boundary,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_ids.runtime_bridge_trace_id),
    )?;

    context.record_event(
        EventType::AudioInputEnded,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "captured_chunk_count": session.chunks.len(),
            "completed_at_ms": session.completed_at_ms,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_ids.provider_trace_id),
    )?;

    context.record_event(
        EventType::LatencyMeasurementRecorded,
        json!({
            "session_id": &session.session_id,
            "domain": "audio",
            "stage": emission.latency_stage,
            "measurements": session.latency_measurements(),
            "first_partial_latency_ms": session.first_partial_latency_ms(),
            "final_transcript_latency_ms": session.final_transcript_latency_ms(),
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_ids.latency_trace_id),
    )?;

    context.record_event(
        EventType::InputReceived,
        json!({
            "session_id": &session.session_id,
            "source_event": EventType::AudioFinalTranscript,
            "input_text": &session.final_transcript.transcript,
            "entry_point": AudioRuntimeEntryPoint::RuntimeInput,
            "entry_point_description": AudioRuntimeEntryPoint::RuntimeInput.description(),
        }),
        Some(trace_ids.runtime_bridge_trace_id),
    )?;

    Ok(())
}
