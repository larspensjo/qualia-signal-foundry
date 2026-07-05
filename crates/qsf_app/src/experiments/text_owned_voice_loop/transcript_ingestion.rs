use std::time::SystemTime;

use anyhow::Context;

use crate::audio::{
    TranscriptEventEmission, TranscriptEventTraceIds, TranscriptProvider, TranscriptProviderError,
    TranscriptProviderRequest, TranscriptProviderSession, record_transcript_runtime_events,
    relative_audio_event_time,
};
use crate::runtime::run_context::RunContext;
use crate::session::{
    Exchange, LiveSessionEvent, SessionEndReason, SessionEvent, SessionState, UtteranceRecord,
};

use super::request_building::voice_utterance_id;
use super::trace_recording::{
    record_input_bridge_trace, record_transcript_failure, record_transcript_provider_trace,
    transcript_to_input_boundary,
};

pub(super) struct TranscriptIngestionOutcome {
    pub(super) transcript_session: TranscriptProviderSession,
    pub(super) final_transcript: String,
    pub(super) exchange_index: usize,
}

pub(super) fn ingest_transcript_to_input(
    context: &mut RunContext,
    state: &mut SessionState,
    transcript_provider: &dyn TranscriptProvider,
) -> anyhow::Result<TranscriptIngestionOutcome> {
    let session_id = state.session_id.clone();
    let transcript_request = TranscriptProviderRequest::from_env(&session_id);
    let transcript_session = match transcript_provider.transcribe(&transcript_request) {
        Ok(session) => session,
        Err(error) => {
            record_transcript_failure(context, &error)?;
            crate::session::apply_live_session_event(
                state,
                LiveSessionEvent::SessionEnded {
                    reason: SessionEndReason::Error,
                },
            );
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
        crate::session::apply_live_session_event(
            state,
            LiveSessionEvent::SessionEnded {
                reason: SessionEndReason::Error,
            },
        );
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

    let exchange_index = state.turns.len();
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            exchange_index,
            SystemTime::now(),
        ))),
    );
    for partial in &transcript_session.partials {
        crate::session::apply_live_session_event(
            state,
            LiveSessionEvent::AudioPartialTranscriptRecorded(crate::session::PartialTranscript {
                exchange_index,
                utterance_id: voice_utterance_id(&transcript_session),
                revision_index: partial.revision_index,
                transcript: partial.transcript.clone(),
                received_at: relative_audio_event_time(partial.received_at_ms),
                provider_id: Some(transcript_session.provider_name.clone()),
                source_chunk_index: Some(partial.source_chunk_index as usize),
            }),
        );
    }
    let final_transcript = transcript_session.final_transcript.transcript.clone();
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index,
            utterance: UtteranceRecord {
                utterance_id: voice_utterance_id(&transcript_session),
                revision_index: transcript_session
                    .partials
                    .iter()
                    .map(|partial| partial.revision_index)
                    .max()
                    .unwrap_or(0),
                transcript: final_transcript.clone(),
                received_at: relative_audio_event_time(
                    transcript_session.final_transcript.received_at_ms,
                ),
                provider_id: Some(transcript_session.provider_name.clone()),
                source_chunk_index: None,
            },
            final_transcript: final_transcript.clone(),
        },
    );
    crate::session::reduce_session_in_place(
        state,
        SessionEvent::InputReceived {
            input: final_transcript.clone(),
        },
    );

    Ok(TranscriptIngestionOutcome {
        transcript_session,
        final_transcript,
        exchange_index,
    })
}
