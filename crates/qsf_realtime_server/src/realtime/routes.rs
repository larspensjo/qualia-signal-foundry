use std::time::SystemTime;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::diagnostics::DiagnosticRecord;
use crate::state::{AppState, CallBinding, SessionRuntime, SidebandStatus};
use qsf_session::{
    Exchange, ExchangeOutput, LiveSessionEvent, LiveSessionState, PartialTranscript,
    ProviderEventKind, ProviderEventRecord, SessionEndReason, UtteranceRecord, reduce_live_session,
};
use tokio::sync::watch;

const MAX_RELAY_PAYLOAD_BYTES: usize = 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/realtime/session", post(create_session))
        .route("/api/realtime/sdp", post(exchange_sdp))
        .route("/api/realtime/events", get(events_socket))
        .route("/api/realtime/stop", post(stop_session))
}

#[derive(Debug, Deserialize)]
pub struct SdpExchangeRequest {
    pub qsf_session_id: String,
    pub offer_sdp: String,
}

#[derive(Debug, Serialize)]
pub struct SdpExchangeResponse {
    pub qsf_session_id: String,
    pub answer_sdp: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RelayEnvelope {
    pub qsf_session_id: String,
    pub event_id: String,
    pub kind: RelayEventKind,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub previous_item_id: Option<String>,
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub transcript: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub audio_marker: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayEventKind {
    UserTurnStarted,
    PartialTranscript,
    FinalTranscript,
    ResponseStarted,
    ResponseCompleted,
    SpeechPlaybackStarted,
    SpeechPlaybackCompleted,
    SessionStopped,
}

#[derive(Debug, Serialize)]
pub struct StopResponse {
    pub qsf_session_id: String,
    pub stopped: bool,
    pub completed_exchanges: usize,
}

async fn create_session(State(state): State<AppState>) -> Response {
    match state.create_session().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::from_error(error)),
        )
            .into_response(),
    }
}

async fn exchange_sdp(
    State(state): State<AppState>,
    Json(request): Json<SdpExchangeRequest>,
) -> Response {
    match exchange_sdp_impl(&state, request).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse::from_error(error)),
        )
            .into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct EventsQuery {
    /// Optional session id so the socket can subscribe to server-side status
    /// before the browser relays its first event.
    session: Option<String>,
}

async fn events_socket(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_events_socket(state, socket, query.session).await {
            log::warn!("realtime events socket ended with error: {error}");
        }
    })
}

async fn stop_session(State(state): State<AppState>, Json(request): Json<StopRequest>) -> Response {
    match stop_session_impl(&state, request.qsf_session_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::from_error(error)),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct StopRequest {
    qsf_session_id: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

impl ErrorResponse {
    fn from_error(error: impl std::fmt::Display) -> Self {
        Self {
            error: error.to_string(),
        }
    }
}

async fn exchange_sdp_impl(
    state: &AppState,
    request: SdpExchangeRequest,
) -> anyhow::Result<SdpExchangeResponse> {
    let session = state
        .session_runtime(&request.qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{}`", request.qsf_session_id))?;
    let config = {
        let guard = session.lock().await;
        guard.config.clone()
    };
    let openai_session = config.openai_session_request();
    let form = reqwest::multipart::Form::new()
        .text("sdp", request.offer_sdp.clone())
        .text("session", serde_json::to_string(&openai_session)?);

    let started_at = OffsetDateTime::now_utc();
    let response = state
        .http_client()
        .post(state.openai_calls_url())
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", state.openai_api_key()),
        )
        .header(
            "OpenAI-Safety-Identifier",
            hash_session_id(&request.qsf_session_id),
        )
        .multipart(form)
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to POST SDP to OpenAI realtime calls endpoint for session `{}`",
                request.qsf_session_id
            )
        })?;
    let finished_at = OffsetDateTime::now_utc();
    let status = response.status();
    let headers = response.headers().clone();
    let answer_sdp = response.text().await?;
    if !status.is_success() {
        // The error body is OpenAI's diagnostic (e.g. which field was rejected),
        // never a credential. Cap the length defensively on a char boundary.
        let detail: String = answer_sdp.trim().chars().take(1000).collect();
        log::warn!(
            "OpenAI realtime calls returned {} for session `{}`: {}",
            status,
            request.qsf_session_id,
            detail
        );
        anyhow::bail!(
            "OpenAI realtime calls returned {} for session `{}`: {}",
            status,
            request.qsf_session_id,
            detail
        );
    }
    let call_id = extract_call_id(headers.get(header::LOCATION))
        .ok_or_else(|| anyhow::anyhow!("OpenAI realtime calls response missing Location header"))?;

    {
        let mut guard = session.lock().await;
        guard.call_binding = Some(CallBinding {
            call_id: call_id.clone(),
            bound_at: finished_at,
            invalidated_at: None,
            reason: None,
        });
        guard.diagnostics.write(&DiagnosticRecord::CallBound {
            qsf_session_id: request.qsf_session_id.clone(),
            call_id: call_id.clone(),
            bound_at: finished_at,
        })?;
        guard
            .diagnostics
            .write(&DiagnosticRecord::LatencyObservation {
                qsf_session_id: request.qsf_session_id.clone(),
                label: "sdp_rendezvous".to_string(),
                started_at,
                finished_at,
                latency_ms: (finished_at - started_at).whole_milliseconds() as i64,
            })?;
        guard
            .diagnostics
            .write(&DiagnosticRecord::NoSecretEvidence {
                qsf_session_id: request.qsf_session_id.clone(),
                at: finished_at,
                note: "API key used server-side only; the browser never receives it".to_string(),
            })?;
        guard.sideband = Some(crate::realtime::sideband::SidebandHandle::spawn(
            state.clone(),
            request.qsf_session_id.clone(),
            call_id.clone(),
        ));
    }

    Ok(SdpExchangeResponse {
        qsf_session_id: request.qsf_session_id,
        answer_sdp,
    })
}

async fn stop_session_impl(
    state: &AppState,
    qsf_session_id: String,
) -> anyhow::Result<StopResponse> {
    let session = state
        .remove_session(&qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{}`", qsf_session_id))?;
    let (sideband, stop_session_id, completed) = {
        let mut guard = session.lock().await;
        let before = guard.relay_state.completed_exchanges.len();
        finalize_open_exchange(&mut guard);
        persist_completed_diagnostic_exchanges(&mut guard)?;
        let completed = guard
            .relay_state
            .completed_exchanges
            .len()
            .saturating_sub(before);
        let stop_session_id = guard.qsf_session_id.clone();
        let call_invalidated = guard.call_binding.as_mut().map(|binding| {
            let invalidated_at = OffsetDateTime::now_utc();
            let call_id = binding.call_id.clone();
            binding.invalidated_at = Some(invalidated_at);
            binding.reason = Some("stop".to_string());
            (call_id, invalidated_at)
        });
        if let Some((call_id, invalidated_at)) = call_invalidated {
            guard
                .diagnostics
                .write(&DiagnosticRecord::CallInvalidated {
                    qsf_session_id: stop_session_id.clone(),
                    call_id,
                    invalidated_at,
                    reason: "stop".to_string(),
                })?;
        }
        guard.call_binding = None;
        let sideband = guard.sideband.take();
        (sideband, stop_session_id, completed)
    };

    if let Some(sideband) = sideband {
        sideband.stop().await;
    }

    Ok(StopResponse {
        qsf_session_id: stop_session_id,
        stopped: true,
        completed_exchanges: completed,
    })
}

async fn handle_events_socket(
    state: AppState,
    mut socket: WebSocket,
    session_hint: Option<String>,
) -> anyhow::Result<()> {
    // Subscribing to the session's sideband status lets the server push
    // health updates (e.g. a sideband that failed to attach) to the browser,
    // rather than the UI sitting silently in "Listening". The socket binds to a
    // single session — from the connect query or, failing that, the first
    // relayed envelope — so it cannot relay one session while reporting another
    // session's health.
    let mut status_rx: Option<watch::Receiver<SidebandStatus>> = None;
    let mut bound_session: Option<String> = session_hint;
    if let Some(id) = bound_session.clone() {
        if let Some(rx) = subscribe_status(&state, &id).await {
            let status = rx.borrow().clone();
            push_status(&mut socket, &id, &status).await;
            status_rx = Some(rx);
        }
    }

    loop {
        let status_changed = async {
            match status_rx.as_mut() {
                Some(rx) => rx.changed().await,
                None => std::future::pending::<Result<(), watch::error::RecvError>>().await,
            }
        };

        tokio::select! {
            changed = status_changed => {
                match changed {
                    Ok(()) => {
                        let status = status_rx
                            .as_ref()
                            .expect("status receiver present when change observed")
                            .borrow()
                            .clone();
                        let id = bound_session.as_deref().unwrap_or_default();
                        push_status(&mut socket, id, &status).await;
                    }
                    // Sender dropped (session removed): stop watching, keep relaying.
                    Err(_) => status_rx = None,
                }
            }
            incoming = socket.next() => {
                let Some(message) = incoming else { break; };
                let message = message?;
                match message {
                    Message::Text(text) => {
                        if relay_payload_is_oversized(&text) {
                            socket.send(Message::Close(None)).await.ok();
                            break;
                        }
                        let envelope: RelayEnvelope = match serde_json::from_str(&text) {
                            Ok(envelope) => envelope,
                            Err(error) => {
                                log::warn!("rejected malformed realtime relay payload: {error}");
                                continue;
                            }
                        };
                        let qsf_session_id = envelope.qsf_session_id.clone();
                        let event_id = envelope.event_id.clone();

                        // Reject envelopes for a different session than the one
                        // this socket is bound to; otherwise relay acks and
                        // sideband status could describe two different sessions.
                        if let Some(bound) = &bound_session {
                            if bound != &qsf_session_id {
                                log::warn!(
                                    "rejected relay event `{event_id}` for session `{qsf_session_id}` on socket bound to `{bound}`"
                                );
                                let ack = RelayAck {
                                    qsf_session_id,
                                    event_id,
                                    accepted: false,
                                };
                                socket
                                    .send(Message::Text(serde_json::to_string(&ack)?.into()))
                                    .await
                                    .ok();
                                continue;
                            }
                        } else {
                            bound_session = Some(qsf_session_id.clone());
                        }

                        let outcome = match process_relay_envelope(&state, envelope).await {
                            Ok(outcome) => outcome,
                            Err(error) => {
                                log::warn!(
                                    "rejected realtime relay event `{event_id}` for session `{qsf_session_id}`: {error}"
                                );
                                RelayAck {
                                    qsf_session_id: qsf_session_id.clone(),
                                    event_id,
                                    accepted: false,
                                }
                            }
                        };
                        socket
                            .send(Message::Text(serde_json::to_string(&outcome)?.into()))
                            .await
                            .ok();

                        if status_rx.is_none() {
                            if let Some(rx) = subscribe_status(&state, &qsf_session_id).await {
                                let status = rx.borrow().clone();
                                push_status(&mut socket, &qsf_session_id, &status).await;
                                status_rx = Some(rx);
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }
    Ok(())
}

async fn subscribe_status(
    state: &AppState,
    qsf_session_id: &str,
) -> Option<watch::Receiver<SidebandStatus>> {
    let session = state.session_runtime(qsf_session_id).await?;
    let guard = session.lock().await;
    Some(guard.subscribe_status())
}

/// Push a sideband health update to the browser. The `kind` discriminator lets
/// the UI distinguish these server-originated messages from relay acks on the
/// same socket.
async fn push_status(socket: &mut WebSocket, qsf_session_id: &str, status: &SidebandStatus) {
    let message = SidebandStatusMessage {
        kind: "sideband_status",
        qsf_session_id: qsf_session_id.to_string(),
        degraded: status.degraded,
        detail: status.detail.clone(),
    };
    if let Ok(text) = serde_json::to_string(&message) {
        socket.send(Message::Text(text.into())).await.ok();
    }
}

#[derive(Debug, Serialize)]
struct SidebandStatusMessage {
    kind: &'static str,
    qsf_session_id: String,
    degraded: bool,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct RelayAck {
    qsf_session_id: String,
    event_id: String,
    accepted: bool,
}

fn apply_relay_live_session_event(state: &mut LiveSessionState, event: LiveSessionEvent) {
    *state = reduce_live_session(std::mem::take(state), event);
}

async fn process_relay_envelope(
    state: &AppState,
    envelope: RelayEnvelope,
) -> anyhow::Result<RelayAck> {
    let session = state
        .session_runtime(&envelope.qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{}`", envelope.qsf_session_id))?;

    let mut guard = session.lock().await;
    if !guard.seen_event_ids.insert(envelope.event_id.clone()) {
        return Ok(RelayAck {
            qsf_session_id: envelope.qsf_session_id,
            event_id: envelope.event_id,
            accepted: false,
        });
    }

    guard
        .diagnostics
        .write(&DiagnosticRecord::RelayEventReceived {
            qsf_session_id: guard.qsf_session_id.clone(),
            event_id: envelope.event_id.clone(),
            event_kind: format!("{:?}", envelope.kind),
            at: OffsetDateTime::now_utc(),
        })?;
    let call_id = guard
        .call_binding
        .as_ref()
        .map(|binding| binding.call_id.clone());
    let call_id_text = call_id
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "unknown-call".to_string());

    let _exchange_index = match envelope.kind {
        RelayEventKind::UserTurnStarted => {
            let exchange =
                Exchange::new_voice_pending(guard.new_exchange_index(), SystemTime::now());
            let index = exchange.index;
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::ExchangeStarted(Box::new(exchange)),
            );
            index
        }
        RelayEventKind::PartialTranscript => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::AudioPartialTranscriptRecorded(PartialTranscript {
                    exchange_index,
                    utterance_id: envelope
                        .item_id
                        .clone()
                        .unwrap_or_else(|| format!("utterance-{exchange_index}")),
                    revision_index: 0,
                    transcript: envelope.transcript.unwrap_or_default(),
                    received_at: SystemTime::now(),
                    provider_id: Some(call_id_text.clone()),
                    source_chunk_index: None,
                }),
            );
            exchange_index
        }
        RelayEventKind::FinalTranscript => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::AudioFinalTranscriptCommitted {
                    exchange_index,
                    utterance: UtteranceRecord {
                        utterance_id: envelope
                            .item_id
                            .clone()
                            .unwrap_or_else(|| format!("utterance-{exchange_index}")),
                        revision_index: 0,
                        transcript: envelope.transcript.clone().unwrap_or_default(),
                        received_at: SystemTime::now(),
                        provider_id: Some(call_id_text.clone()),
                        source_chunk_index: None,
                    },
                    final_transcript: envelope.transcript.unwrap_or_default(),
                },
            );
            exchange_index
        }
        RelayEventKind::ResponseStarted => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::ResponseStarted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: call_id.clone(),
                    event_id: Some(envelope.event_id.clone()),
                    item_id: envelope.item_id.clone(),
                    previous_item_id: envelope.previous_item_id.clone(),
                    response_id: envelope.response_id.clone(),
                    text: envelope.text.clone(),
                    status: envelope.status.clone(),
                    audio_marker: envelope.audio_marker.clone(),
                }),
            );
            exchange_index
        }
        RelayEventKind::ResponseCompleted => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::ResponseCompleted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: call_id.clone(),
                    event_id: Some(envelope.event_id.clone()),
                    item_id: envelope.item_id.clone(),
                    previous_item_id: envelope.previous_item_id.clone(),
                    response_id: envelope.response_id.clone(),
                    text: envelope.text.clone(),
                    status: envelope.status.clone(),
                    audio_marker: envelope.audio_marker.clone(),
                }),
            );
            if let Some(text) = envelope.text.clone() {
                apply_relay_live_session_event(
                    &mut guard.relay_state,
                    LiveSessionEvent::OutputProduced(ExchangeOutput {
                        response_id: envelope.response_id.clone(),
                        text,
                        produced_at: SystemTime::now(),
                        provider_name: Some("openai_realtime".to_string()),
                        target: Some("speech".to_string()),
                        audio_marker: envelope.audio_marker.clone(),
                    }),
                );
            }
            exchange_index
        }
        RelayEventKind::SpeechPlaybackStarted => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::SpeechPlaybackStarted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: call_id.clone(),
                    event_id: Some(envelope.event_id.clone()),
                    item_id: envelope.item_id.clone(),
                    previous_item_id: envelope.previous_item_id.clone(),
                    response_id: envelope.response_id.clone(),
                    text: envelope.text.clone(),
                    status: envelope.status.clone(),
                    audio_marker: envelope.audio_marker.clone(),
                }),
            );
            exchange_index
        }
        RelayEventKind::SpeechPlaybackCompleted => {
            let exchange_index = ensure_active_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::SpeechPlaybackCompleted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: SystemTime::now(),
                    call_id: call_id.clone(),
                    event_id: Some(envelope.event_id.clone()),
                    item_id: envelope.item_id.clone(),
                    previous_item_id: envelope.previous_item_id.clone(),
                    response_id: envelope.response_id.clone(),
                    text: envelope.text.clone(),
                    status: envelope.status.clone(),
                    audio_marker: envelope.audio_marker.clone(),
                }),
            );
            finalize_open_exchange(&mut guard);
            exchange_index
        }
        RelayEventKind::SessionStopped => {
            finalize_open_exchange(&mut guard);
            apply_relay_live_session_event(
                &mut guard.relay_state,
                LiveSessionEvent::SessionEnded {
                    reason: SessionEndReason::Eof,
                },
            );
            let qsf_session_id = guard.qsf_session_id.clone();
            let call_invalidated = guard.call_binding.as_mut().map(|binding| {
                let invalidated_at = OffsetDateTime::now_utc();
                let call_id = binding.call_id.clone();
                binding.invalidated_at = Some(invalidated_at);
                binding.reason = Some("stop".to_string());
                (call_id, invalidated_at)
            });
            if let Some((call_id, invalidated_at)) = call_invalidated {
                guard
                    .diagnostics
                    .write(&DiagnosticRecord::CallInvalidated {
                        qsf_session_id,
                        call_id,
                        invalidated_at,
                        reason: "stop".to_string(),
                    })?;
            }
            guard.call_binding = None;
            0
        }
    };

    persist_completed_diagnostic_exchanges(&mut guard)?;
    Ok(RelayAck {
        qsf_session_id: envelope.qsf_session_id,
        event_id: envelope.event_id,
        accepted: true,
    })
}

fn finalize_open_exchange(runtime: &mut SessionRuntime) {
    if let Some(active_exchange) = runtime.relay_state.active_exchange.as_ref() {
        let exchange_index = active_exchange.index;
        apply_relay_live_session_event(
            &mut runtime.relay_state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index,
                completed_at: SystemTime::now(),
            },
        );
    }
}

fn ensure_active_exchange(runtime: &mut SessionRuntime) -> usize {
    if let Some(exchange) = runtime.relay_state.active_exchange.as_ref() {
        return exchange.index;
    }
    let exchange = Exchange::new_voice_pending(runtime.new_exchange_index(), SystemTime::now());
    let exchange_index = exchange.index;
    apply_relay_live_session_event(
        &mut runtime.relay_state,
        LiveSessionEvent::ExchangeStarted(Box::new(exchange)),
    );
    exchange_index
}

fn persist_completed_diagnostic_exchanges(runtime: &mut SessionRuntime) -> anyhow::Result<()> {
    while runtime.persisted_exchange_count < runtime.relay_state.completed_exchanges.len() {
        let exchange =
            runtime.relay_state.completed_exchanges[runtime.persisted_exchange_count].clone();
        runtime
            .diagnostics
            .write(&DiagnosticRecord::DiagnosticExchangeRecorded {
                qsf_session_id: runtime.qsf_session_id.clone(),
                source: "browser_relay".to_string(),
                trust: runtime.trust,
                recorded_at: OffsetDateTime::now_utc(),
                exchange,
            })?;
        runtime.persisted_exchange_count += 1;
    }
    Ok(())
}

fn relay_payload_is_oversized(payload: &str) -> bool {
    payload.len() > MAX_RELAY_PAYLOAD_BYTES
}

fn extract_call_id(location: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let location = location?.to_str().ok()?;
    let mut location_parts = location.splitn(2, '?');
    let path = location_parts.next().unwrap_or(location);
    if let Some(query) = location_parts.next() {
        for part in query.split('&') {
            if let Some((key, value)) = part.split_once('=') {
                if key == "call_id" && !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    path.rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

fn hash_session_id(session_id: &str) -> String {
    let mut hash = sha2::Sha256::new();
    use sha2::Digest;
    hash.update(session_id.as_bytes());
    let digest = hash.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    use std::fmt::Write as _;
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use axum::body::{Body, Bytes, to_bytes};
    use axum::http::{HeaderMap, Request};
    use axum::routing::post;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;

    #[derive(Debug)]
    struct CapturedOpenAiRequest {
        authorization: Option<String>,
        safety_identifier: Option<String>,
        body: String,
    }

    #[tokio::test]
    async fn session_route_returns_default_config_without_secret() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state");
        let app = router().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/realtime/session")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let body_text = String::from_utf8(body.to_vec()).expect("utf8");

        assert_eq!(json["session"]["model"], "gpt-realtime-2");
        assert_eq!(json["session"]["voice"], "marin");
        assert_eq!(json["session"]["reasoning_effort"], "medium");
        assert_eq!(
            json["session"]["audio"]["input"]["turn_detection"]["type"],
            "server_vad"
        );
        assert_eq!(
            json["session"]["audio"]["input"]["turn_detection"]["create_response"],
            false
        );
        assert_eq!(
            json["session"]["audio"]["input"]["turn_detection"]["interrupt_response"],
            true
        );
        // Input transcription must be enabled by default: the sideband gates
        // `response.create` on the transcription-completed event, so without a
        // model the conversation never produces a response.
        assert_eq!(
            json["session"]["input_transcription_model"],
            "gpt-4o-mini-transcribe"
        );
        assert!(json.get("api_key").is_none());
        assert!(!body_text.contains("test-api-key"));
    }

    #[tokio::test]
    async fn sdp_proxy_captures_call_binding_and_keeps_secret_out_of_response() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let captured = Arc::new(StdMutex::new(None));
        let openai_base_url = start_mock_openai(
            "answer-sdp",
            "/v1/realtime/calls/call_mock",
            captured.clone(),
        )
        .await;
        let state = AppState::new(
            "test-api-key",
            openai_base_url,
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state");
        let allocation = state.create_session().await.expect("session");
        let app = router().with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/realtime/sdp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "qsf_session_id": allocation.qsf_session_id,
                            "offer_sdp": "offer-sdp",
                        }))
                        .expect("request json"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let body_text = String::from_utf8(body.to_vec()).expect("utf8");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["answer_sdp"], "answer-sdp");
        assert!(!body_text.contains("test-api-key"));

        let request = captured
            .lock()
            .expect("captured request mutex")
            .take()
            .expect("mock OpenAI request");
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer test-api-key")
        );
        assert!(request.safety_identifier.is_some());
        assert!(request.body.contains("offer-sdp"));
        // `reasoning_effort` is QSF session metadata only; the OpenAI realtime
        // calls session object rejects it, so it must not be forwarded.
        assert!(!request.body.contains("reasoning_effort"));
        assert!(!request.body.contains("test-api-key"));

        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        let guard = runtime.lock().await;
        assert_eq!(
            guard
                .call_binding
                .as_ref()
                .map(|binding| binding.call_id.as_str()),
            Some("call_mock")
        );
        drop(guard);

        let diagnostics = std::fs::read_to_string(
            state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .expect("diagnostics");
        assert!(diagnostics.contains("\"kind\":\"call_bound\""));
        assert!(diagnostics.contains("\"kind\":\"no_secret_evidence\""));
        assert!(!diagnostics.contains("test-api-key"));
    }

    #[tokio::test]
    async fn stop_route_persists_final_exchange_and_invalidates_binding() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state");
        let allocation = state.create_session().await.expect("session");
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            let mut guard = runtime.lock().await;
            guard.call_binding = Some(CallBinding {
                call_id: "call-stop".to_string(),
                bound_at: OffsetDateTime::now_utc(),
                invalidated_at: None,
                reason: None,
            });
        }

        process_relay_envelope(
            &state,
            RelayEnvelope {
                transcript: Some("final words".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-final-transcript",
                    RelayEventKind::FinalTranscript,
                )
            },
        )
        .await
        .expect("relay event");

        let app = router().with_state(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/realtime/stop")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "qsf_session_id": allocation.qsf_session_id,
                        }))
                        .expect("request json"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["completed_exchanges"], 1);
        assert!(
            state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .is_none()
        );

        let diagnostics = std::fs::read_to_string(
            state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .expect("diagnostics");
        assert!(diagnostics.contains("\"kind\":\"diagnostic_exchange_recorded\""));
        assert!(diagnostics.contains("\"source\":\"browser_relay\""));
        assert!(diagnostics.contains("\"trust\":\"untrusted\""));
        assert!(diagnostics.contains("final words"));
        assert!(diagnostics.contains("\"kind\":\"call_invalidated\""));
        assert!(diagnostics.contains("\"call_id\":\"call-stop\""));
    }

    #[tokio::test]
    async fn relay_dedupes_event_ids_before_reducing() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state");
        let allocation = state.create_session().await.expect("session");

        let first = process_relay_envelope(
            &state,
            RelayEnvelope {
                transcript: Some("first transcript".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-duplicate",
                    RelayEventKind::FinalTranscript,
                )
            },
        )
        .await
        .expect("first relay");
        let duplicate = process_relay_envelope(
            &state,
            RelayEnvelope {
                transcript: Some("second transcript".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-duplicate",
                    RelayEventKind::FinalTranscript,
                )
            },
        )
        .await
        .expect("duplicate relay");

        assert!(first.accepted);
        assert!(!duplicate.accepted);
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        let guard = runtime.lock().await;
        let exchange = guard
            .relay_state
            .active_exchange
            .as_ref()
            .expect("active exchange");
        assert_eq!(exchange.final_user_input(), "first transcript");
        assert_eq!(guard.seen_event_ids.len(), 1);
    }

    #[tokio::test]
    async fn relay_persists_completed_diagnostic_exchange_with_identity_fields() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state");
        let allocation = state.create_session().await.expect("session");
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            let mut guard = runtime.lock().await;
            guard.call_binding = Some(CallBinding {
                call_id: "call-relay".to_string(),
                bound_at: OffsetDateTime::now_utc(),
                invalidated_at: None,
                reason: None,
            });
        }

        process_relay_envelope(
            &state,
            RelayEnvelope {
                item_id: Some("item-user".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-start",
                    RelayEventKind::UserTurnStarted,
                )
            },
        )
        .await
        .expect("start relay");
        process_relay_envelope(
            &state,
            RelayEnvelope {
                item_id: Some("item-user".to_string()),
                transcript: Some("hello".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-final",
                    RelayEventKind::FinalTranscript,
                )
            },
        )
        .await
        .expect("final relay");
        process_relay_envelope(
            &state,
            RelayEnvelope {
                item_id: Some("item-response".to_string()),
                previous_item_id: Some("item-user".to_string()),
                response_id: Some("response-relay".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-response-start",
                    RelayEventKind::ResponseStarted,
                )
            },
        )
        .await
        .expect("response start relay");
        process_relay_envelope(
            &state,
            RelayEnvelope {
                item_id: Some("item-response".to_string()),
                previous_item_id: Some("item-user".to_string()),
                response_id: Some("response-relay".to_string()),
                text: Some("hi".to_string()),
                status: Some("completed".to_string()),
                audio_marker: Some("audio-relay".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-response-done",
                    RelayEventKind::ResponseCompleted,
                )
            },
        )
        .await
        .expect("response done relay");
        process_relay_envelope(
            &state,
            RelayEnvelope {
                item_id: Some("item-response".to_string()),
                previous_item_id: Some("item-user".to_string()),
                response_id: Some("response-relay".to_string()),
                audio_marker: Some("audio-relay".to_string()),
                ..relay_envelope(
                    &allocation.qsf_session_id,
                    "evt-audio-done",
                    RelayEventKind::SpeechPlaybackCompleted,
                )
            },
        )
        .await
        .expect("audio done relay");

        let diagnostics = std::fs::read_to_string(
            state
                .diagnostics_dir()
                .join(format!("{}.jsonl", allocation.qsf_session_id)),
        )
        .expect("diagnostics");
        assert!(diagnostics.contains("\"kind\":\"diagnostic_exchange_recorded\""));
        assert!(diagnostics.contains("\"source\":\"browser_relay\""));
        assert!(diagnostics.contains("\"trust\":\"untrusted\""));
        assert!(diagnostics.contains("\"call_id\":\"call-relay\""));
        assert!(diagnostics.contains("\"event_id\":\"evt-response-start\""));
        assert!(diagnostics.contains("\"item_id\":\"item-response\""));
        assert!(diagnostics.contains("\"previous_item_id\":\"item-user\""));
        assert!(diagnostics.contains("\"response_id\":\"response-relay\""));
    }

    #[test]
    fn malformed_relay_payloads_do_not_parse_as_envelopes() {
        assert!(serde_json::from_str::<RelayEnvelope>("{not-json").is_err());
        assert!(
            serde_json::from_str::<RelayEnvelope>(
                r#"{"qsf_session_id":"session","event_id":"event","kind":"not_a_kind"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn relay_payload_size_limit_is_exclusive() {
        assert!(!relay_payload_is_oversized(
            &"x".repeat(MAX_RELAY_PAYLOAD_BYTES)
        ));
        assert!(relay_payload_is_oversized(
            &"x".repeat(MAX_RELAY_PAYLOAD_BYTES + 1)
        ));
    }

    #[test]
    fn extract_call_id_ignores_malformed_query_parts_and_falls_back_to_path() {
        let location =
            reqwest::header::HeaderValue::from_static("/v1/realtime/calls/call_path?flag&x=1");
        assert_eq!(
            extract_call_id(Some(&location)).as_deref(),
            Some("call_path")
        );

        let location = reqwest::header::HeaderValue::from_static(
            "/v1/realtime/calls/ignored?flag&call_id=call_query",
        );
        assert_eq!(
            extract_call_id(Some(&location)).as_deref(),
            Some("call_query")
        );
    }

    async fn start_mock_openai(
        answer_sdp: &'static str,
        location: &'static str,
        captured: Arc<StdMutex<Option<CapturedOpenAiRequest>>>,
    ) -> String {
        let app = axum::Router::new().route(
            "/v1/realtime/calls",
            post(move |headers: HeaderMap, body: Bytes| {
                let captured = captured.clone();
                async move {
                    *captured.lock().expect("captured request mutex") =
                        Some(CapturedOpenAiRequest {
                            authorization: headers
                                .get(header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            safety_identifier: headers
                                .get("OpenAI-Safety-Identifier")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string),
                            body: String::from_utf8_lossy(&body).into_owned(),
                        });
                    (StatusCode::OK, [(header::LOCATION, location)], answer_sdp)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock OpenAI listener");
        let addr = listener.local_addr().expect("mock OpenAI addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("mock OpenAI server");
        });
        format!("http://{addr}")
    }

    fn relay_envelope(qsf_session_id: &str, event_id: &str, kind: RelayEventKind) -> RelayEnvelope {
        RelayEnvelope {
            qsf_session_id: qsf_session_id.to_string(),
            event_id: event_id.to_string(),
            kind,
            item_id: None,
            previous_item_id: None,
            response_id: None,
            transcript: None,
            text: None,
            status: None,
            audio_marker: None,
            payload: Value::Null,
        }
    }
}
