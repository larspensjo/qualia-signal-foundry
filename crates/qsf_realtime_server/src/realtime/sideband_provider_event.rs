use std::time::{Instant, SystemTime};

use qsf_realtime_protocol::{
    build_openai_realtime_response_cancel, realtime_event_delta_text, realtime_event_response_id,
    realtime_event_response_status, realtime_event_text, realtime_event_transcript,
};
use qsf_session::{
    Exchange, LiveSessionEvent, ProviderEventKind, ProviderEventRecord, SessionEndReason,
    apply_live_session_event,
};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};
use crate::realtime::sideband::{
    SidebandRuntimeState, apply_trusted_transcript_to_volition, ensure_authoritative_exchange,
    hash_text, record_latency_observation_if_ready, send_json,
};
use crate::realtime::sideband_response_done::handle_response_done_event;
use crate::realtime::sideband_turn_injection::inject_trusted_turn_context_and_response;
use crate::realtime::token_usage::{
    INPUT_TRANSCRIPTION_ROLE, TokenClassCounts, transcription_token_counts,
};
use crate::realtime::tools::VolitionStateSnapshot;
use crate::realtime::turn_integrity::{
    TranscriptDisposition, TurnPhase, classify_final_transcript,
};
use crate::state::AppState;

pub(crate) async fn handle_provider_event(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    event_type: &str,
    event: &serde_json::Value,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
) -> anyhow::Result<()> {
    let session = state
        .session_runtime(qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{qsf_session_id}`"))?;
    let mut guard = session.lock().await;
    let config = guard.config.clone();

    match event_type {
        "session.created" | "session.updated" => {
            if event_type == "session.updated" && guard.degraded {
                guard.set_sideband_status(false, None);
                log::info!(
                    "sideband recovery verified for session `{qsf_session_id}` call `{call_id}` after session.updated"
                );
            }
            guard
                .diagnostics
                .write(&crate::diagnostics::DiagnosticRecord::RelayEventReceived {
                    qsf_session_id: qsf_session_id.to_string(),
                    event_id: event
                        .get("event_id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("openai-realtime")
                        .to_string(),
                    event_kind: event_type.to_string(),
                    at: time::OffsetDateTime::now_utc(),
                })?;
        }
        "conversation.item.input_audio_transcription.completed" => {
            // Transcription spend is billed against the transcription model and reported
            // here rather than in `response.done`. Record it before any disposition check
            // so noise-classified turns still account for what the provider charged. The
            // model id comes from session config: the event does not carry one.
            if let Some(transcription_model) = config.input_transcription_model.as_deref() {
                let transcription_counts = transcription_token_counts(event);
                if transcription_counts != TokenClassCounts::default() {
                    guard.record_token_usage(
                        INPUT_TRANSCRIPTION_ROLE,
                        transcription_model,
                        transcription_counts,
                    );
                }
            }
            let transcript = realtime_event_transcript(event)
                .or_else(|| event.get("transcript").and_then(serde_json::Value::as_str))
                .unwrap_or_default()
                .to_string();
            if transcript.trim().is_empty() {
                guard
                    .diagnostics
                    .write(&DiagnosticRecord::IgnoredContinuationTranscript {
                        qsf_session_id: qsf_session_id.to_string(),
                        transcript,
                        turn_phase: runtime_state.turn_phase,
                        response_id: runtime_state.response_id.clone(),
                        at: time::OffsetDateTime::now_utc(),
                    })?;
                log::warn!(
                    "ignored empty final transcript for session `{qsf_session_id}` during {:?}",
                    runtime_state.turn_phase
                );
                return Ok(());
            }
            let volition_tick_before;
            let volition_events_applied;
            let volition_snapshot;
            match classify_final_transcript(runtime_state.turn_phase, &transcript) {
                TranscriptDisposition::IgnoreAsNoise => {
                    guard
                        .diagnostics
                        .write(&DiagnosticRecord::IgnoredContinuationTranscript {
                            qsf_session_id: qsf_session_id.to_string(),
                            transcript: transcript.clone(),
                            turn_phase: runtime_state.turn_phase,
                            response_id: runtime_state.response_id.clone(),
                            at: time::OffsetDateTime::now_utc(),
                        })?;
                    log::info!(
                        "ignored continuation transcript for session `{qsf_session_id}` during {:?}: `{transcript}`",
                        runtime_state.turn_phase
                    );
                    return Ok(());
                }
                TranscriptDisposition::Interrupt => {
                    let Some(current_exchange_index) =
                        runtime_state.active_exchange_index.or_else(|| {
                            guard
                                .session_state
                                .live
                                .active_exchange
                                .as_ref()
                                .map(|exchange| exchange.index)
                        })
                    else {
                        log::warn!(
                            "could not interrupt continuation transcript for session `{qsf_session_id}` because no active exchange was available"
                        );
                        return Ok(());
                    };

                    guard
                        .non_promotable_exchange_indices
                        .insert(current_exchange_index);
                    let interruption = qsf_session::InterruptionRecord {
                        exchange_index: current_exchange_index,
                        response_id: runtime_state.response_id.clone(),
                        detected_at: SystemTime::now(),
                        source: "sideband_final_transcript".to_string(),
                        action: qsf_session::InterruptionAction::MarkInterrupted,
                        stop_outcome: qsf_session::InterruptionStopOutcome::Stopped,
                        partial_response_text: guard
                            .session_state
                            .live
                            .active_response
                            .as_ref()
                            .map(|response| response.partial_text.clone())
                            .filter(|text| !text.is_empty()),
                    };
                    apply_live_session_event(
                        &mut guard.session_state,
                        LiveSessionEvent::UserInterrupted(interruption),
                    );
                    let interrupted_exchange =
                        guard.session_state.live.active_exchange.as_ref().cloned();
                    if let Some(response_id) = runtime_state.response_id.clone() {
                        runtime_state.stale_response_ids.insert(response_id);
                    } else {
                        log::warn!(
                            "continuation interruption for session `{qsf_session_id}` landed before response.created; old response id is unknown"
                        );
                    }
                    send_json(outbound_tx, build_openai_realtime_response_cancel())?;

                    let new_exchange_index = guard.new_trusted_exchange_index();
                    apply_live_session_event(
                        &mut guard.session_state,
                        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                            new_exchange_index,
                            SystemTime::now(),
                        ))),
                    );
                    apply_live_session_event(
                        &mut guard.session_state,
                        LiveSessionEvent::AudioFinalTranscriptCommitted {
                            exchange_index: new_exchange_index,
                            utterance: qsf_session::UtteranceRecord {
                                utterance_id: event
                                    .get("item_id")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| format!("utterance-{new_exchange_index}")),
                                revision_index: 0,
                                transcript: transcript.clone(),
                                received_at: SystemTime::now(),
                                provider_id: Some(call_id.to_string()),
                                source_chunk_index: None,
                            },
                            final_transcript: transcript.clone(),
                        },
                    );
                    runtime_state.clear_in_flight_response_state();
                    runtime_state.final_transcript_received_at = Some(OffsetDateTime::now_utc());
                    runtime_state.active_exchange_index = Some(new_exchange_index);
                    runtime_state.pending_response_exchange = None;
                    runtime_state.turn_phase = TurnPhase::Idle;
                    if let Some(exchange) = interrupted_exchange {
                        record_interrupted_exchange_diagnostic(
                            &guard.diagnostics,
                            qsf_session_id,
                            &exchange,
                        )?;
                    }
                    volition_tick_before = guard.volition.state.tick;
                    volition_events_applied =
                        apply_trusted_transcript_to_volition(&mut guard, &transcript);
                    volition_snapshot = VolitionStateSnapshot {
                        state: guard.volition.state.clone(),
                        fixture: guard.volition.fixture.clone(),
                    };
                }
                TranscriptDisposition::StartTurn => {
                    let exchange_index = ensure_authoritative_exchange(&mut guard);
                    apply_live_session_event(
                        &mut guard.session_state,
                        LiveSessionEvent::AudioFinalTranscriptCommitted {
                            exchange_index,
                            utterance: qsf_session::UtteranceRecord {
                                utterance_id: event
                                    .get("item_id")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| format!("utterance-{exchange_index}")),
                                revision_index: 0,
                                transcript: transcript.clone(),
                                received_at: SystemTime::now(),
                                provider_id: Some(call_id.to_string()),
                                source_chunk_index: None,
                            },
                            final_transcript: transcript.clone(),
                        },
                    );
                    runtime_state.final_transcript_received_at = Some(OffsetDateTime::now_utc());
                    runtime_state.active_exchange_index = Some(exchange_index);
                    runtime_state.pending_response_exchange = None;
                    runtime_state.turn_phase = TurnPhase::Idle;
                    volition_tick_before = guard.volition.state.tick;
                    volition_events_applied =
                        apply_trusted_transcript_to_volition(&mut guard, &transcript);
                    volition_snapshot = VolitionStateSnapshot {
                        state: guard.volition.state.clone(),
                        fixture: guard.volition.fixture.clone(),
                    };
                }
            }
            drop(guard);
            let input_transcript_ref = format!(
                "exchange:{}/transcript:{}",
                runtime_state.active_exchange_index.unwrap_or_default(),
                hash_text(&transcript)
            );
            inject_trusted_turn_context_and_response(
                state,
                session,
                qsf_session_id,
                &transcript,
                &config,
                runtime_state,
                outbound_tx,
                None,
                runtime_state.active_exchange_index.unwrap_or_default(),
                &volition_snapshot,
                volition_events_applied,
                volition_tick_before,
                input_transcript_ref,
            )
            .await?;
        }
        "response.created" => {
            let response_id = realtime_event_response_id(event)
                .map(str::to_string)
                .or_else(|| {
                    event
                        .get("response")
                        .and_then(|response| response.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                });
            let current_exchange_index = runtime_state.active_exchange_index.or_else(|| {
                guard
                    .session_state
                    .live
                    .active_exchange
                    .as_ref()
                    .map(|exchange| exchange.index)
            });
            let response_is_stale = response_id
                .as_ref()
                .map(|response_id| runtime_state.stale_response_ids.contains(response_id))
                .unwrap_or(false);
            let exchange_is_stale = match (
                runtime_state.pending_response_exchange,
                current_exchange_index,
            ) {
                (Some(pending_exchange), Some(active_exchange)) => {
                    pending_exchange != active_exchange
                }
                _ => true,
            };
            if response_is_stale || exchange_is_stale {
                log::warn!(
                    "ignored stale response.created for session `{qsf_session_id}` with response id `{}`",
                    response_id.as_deref().unwrap_or("<unknown>")
                );
                return Ok(());
            }
            runtime_state.response_id = response_id.clone();
            runtime_state.response_started_at = Some(Instant::now());
            runtime_state.response_created_at = Some(OffsetDateTime::now_utc());

            let exchange_index = ensure_authoritative_exchange(&mut guard);
            let diagnostics = guard.diagnostics.clone();
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::ResponseStarted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: Some(call_id.to_string()),
                    event_id: Some(
                        event
                            .get("event_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("response.created")
                            .to_string(),
                    ),
                    item_id: event
                        .get("item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    previous_item_id: event
                        .get("previous_item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    response_id,
                    text: realtime_event_text(event).map(str::to_string),
                    status: realtime_event_response_status(event).map(str::to_string),
                    audio_marker: event
                        .get("audio_marker")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
            );
            let response_create_sent_at = runtime_state.response_create_sent_at;
            let response_created_at = runtime_state.response_created_at;
            let first_audio_received_at = runtime_state.first_audio_received_at;
            record_latency_observation_if_ready(
                runtime_state,
                &diagnostics,
                qsf_session_id,
                "response_create_sent_to_response_created",
                response_create_sent_at,
                response_created_at,
            )?;
            record_latency_observation_if_ready(
                runtime_state,
                &diagnostics,
                qsf_session_id,
                "response_created_to_first_audio",
                response_created_at,
                first_audio_received_at,
            )?;
        }
        "response.output_audio.delta"
        | "response.audio.delta"
        | "response.output_audio_transcript.delta"
        | "response.output_audio_transcript.done" => {
            let exchange_index = ensure_authoritative_exchange(&mut guard);
            let diagnostics = guard.diagnostics.clone();
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::SpeechPlaybackStarted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: Some(call_id.to_string()),
                    event_id: Some(
                        event
                            .get("event_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(event_type)
                            .to_string(),
                    ),
                    item_id: event
                        .get("item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    previous_item_id: event
                        .get("previous_item_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    response_id: runtime_state.response_id.clone(),
                    text: realtime_event_delta_text(event)
                        .or_else(|| realtime_event_text(event))
                        .map(str::to_string),
                    status: realtime_event_response_status(event).map(str::to_string),
                    audio_marker: event
                        .get("audio_marker")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
            );
            if runtime_state.first_audio_received_at.is_none() {
                runtime_state.first_audio_received_at = Some(OffsetDateTime::now_utc());
            }
            let response_created_at = runtime_state.response_created_at;
            let first_audio_received_at = runtime_state.first_audio_received_at;
            let final_transcript_received_at = runtime_state.final_transcript_received_at;
            record_latency_observation_if_ready(
                runtime_state,
                &diagnostics,
                qsf_session_id,
                "response_created_to_first_audio",
                response_created_at,
                first_audio_received_at,
            )?;
            record_latency_observation_if_ready(
                runtime_state,
                &diagnostics,
                qsf_session_id,
                "final_transcript_received_to_first_audio",
                final_transcript_received_at,
                first_audio_received_at,
            )?;
        }
        "response.done" => {
            handle_response_done_event(
                state,
                qsf_session_id,
                call_id,
                event,
                session.clone(),
                guard,
                runtime_state,
                outbound_tx,
            )
            .await?;
        }
        "session.closed" => {
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::SessionEnded {
                    reason: SessionEndReason::Eof,
                },
            );
            guard.set_sideband_status(
                true,
                Some("provider closed the realtime session".to_string()),
            );
            runtime_state.clear_in_flight_response_state();
            runtime_state.active_exchange_index = None;
            runtime_state.pending_response_exchange = None;
            runtime_state.turn_phase = TurnPhase::Idle;
            runtime_state.stale_response_ids.clear();
        }
        _ => {
            if let Some(exchange_index) = runtime_state.active_exchange_index {
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                        exchange_index,
                        event_kind: ProviderEventKind::Preamble,
                        provider_id: "openai_realtime".to_string(),
                        received_at: SystemTime::now(),
                        call_id: Some(call_id.to_string()),
                        event_id: event
                            .get("event_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                        item_id: event
                            .get("item_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                        previous_item_id: event
                            .get("previous_item_id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                        response_id: runtime_state.response_id.clone(),
                        text: realtime_event_text(event).map(str::to_string),
                        status: realtime_event_response_status(event).map(str::to_string),
                        audio_marker: event
                            .get("audio_marker")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                    }),
                );
            }
        }
    }

    Ok(())
}

fn record_interrupted_exchange_diagnostic(
    diagnostics: &crate::diagnostics::DiagnosticWriter,
    qsf_session_id: &str,
    exchange: &Exchange,
) -> anyhow::Result<()> {
    diagnostics.write(&DiagnosticRecord::DiagnosticExchangeRecorded {
        qsf_session_id: qsf_session_id.to_string(),
        source: "sideband_interruption".to_string(),
        trust: DiagnosticTrust::Trusted,
        recorded_at: OffsetDateTime::now_utc(),
        exchange: exchange.clone(),
    })
}
