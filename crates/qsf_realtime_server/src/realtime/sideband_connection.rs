use std::time::Duration;

use anyhow::Context;
use axum::http::header;
use futures_util::{SinkExt, StreamExt};
use qsf_realtime_protocol::{
    build_openai_realtime_conversation_session_update, parse_realtime_server_event,
    realtime_event_type,
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

use crate::realtime::injection::DEFAULT_PCM_RATE_HZ;
use crate::realtime::sideband::{
    SidebandCommand, SidebandRuntimeState, handle_text_turn, send_json,
};
use crate::realtime::sideband_provider_event::handle_provider_event;
use crate::state::AppState;

const INITIAL_BACKOFF_MS: u64 = 100;
const MAX_BACKOFF_MS: u64 = 2_000;

#[derive(Debug)]
enum SidebandExit {
    Stopped,
    Disconnected { reason: String },
}

pub(super) async fn run_sideband(
    state: AppState,
    qsf_session_id: String,
    call_id: String,
    mut stop_rx: watch::Receiver<bool>,
    mut command_rx: mpsc::UnboundedReceiver<SidebandCommand>,
) {
    let mut backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
    loop {
        if *stop_rx.borrow() {
            break;
        }

        match connect_and_run_once(
            &state,
            &qsf_session_id,
            &call_id,
            &mut stop_rx,
            &mut command_rx,
        )
        .await
        {
            Ok(SidebandExit::Stopped) => break,
            Ok(SidebandExit::Disconnected { reason }) => {
                mark_session_degraded(&state, &qsf_session_id, &call_id, &reason).await;
                if *stop_rx.borrow() {
                    break;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_millis(MAX_BACKOFF_MS));
            }
            Err(error) => {
                // Surface the full anyhow chain (`{error:#}`), not just the
                // outermost context, so the failing operation is identifiable.
                let reason = format!("{error:#}");
                mark_session_degraded(&state, &qsf_session_id, &call_id, &reason).await;
                log::warn!(
                    "sideband task failed for session `{qsf_session_id}` call `{call_id}`: {reason}"
                );
                if *stop_rx.borrow() {
                    break;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_millis(MAX_BACKOFF_MS));
            }
        }
    }
}

async fn connect_and_run_once(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    stop_rx: &mut watch::Receiver<bool>,
    command_rx: &mut mpsc::UnboundedReceiver<SidebandCommand>,
) -> anyhow::Result<SidebandExit> {
    let mut runtime_state = SidebandRuntimeState::default();
    // Build the request through `into_client_request` so tungstenite generates
    // the websocket handshake headers (`Sec-WebSocket-Key`, `Upgrade`,
    // `Connection`, `Sec-WebSocket-Version`, `Host`); a hand-built `Request`
    // omits them and fails the handshake client-side. Only the Authorization
    // header is layered on afterwards.
    let mut request = state
        .openai_realtime_ws_url(call_id)
        .into_client_request()
        .context("failed to build sideband websocket request")?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {}", state.openai_api_key())
            .parse()
            .context("failed to build sideband authorization header")?,
    );
    // A failed handshake is treated as a (retryable) disconnect rather than a
    // fatal error: the call_id is often not yet joinable at the instant the
    // server binds it (the browser has not finished the WebRTC handshake), so
    // OpenAI returns `404 No session found for the provided call_id` until the
    // call goes live. Routing through `Disconnected` lets `run_sideband` retry
    // with backoff and attach once the call is up.
    let (websocket, _response) = match connect_async(request).await {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(SidebandExit::Disconnected {
                reason: format_connect_error(qsf_session_id, &error),
            });
        }
    };

    log::info!("sideband attached to call `{call_id}` for session `{qsf_session_id}`");

    let (mut sink, mut stream) = websocket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if let Err(error) = sink.send(message).await {
                return Err::<(), anyhow::Error>(anyhow::anyhow!(error));
            }
        }
        let _ = sink.close().await;
        Ok::<(), anyhow::Error>(())
    });

    let session_config = session_config(state, qsf_session_id).await?;
    send_json(
        &outbound_tx,
        build_openai_realtime_conversation_session_update(
            &session_config.model,
            &session_config.voice,
            &session_config.instructions,
            &session_config.output_modalities,
            DEFAULT_PCM_RATE_HZ,
            false,
            false,
            &session_config.tools,
            Some("auto"),
            session_config.input_transcription_model.as_deref(),
        ),
    )?;

    loop {
        let message = tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                if *stop_rx.borrow() {
                    let _ = outbound_tx.send(Message::Close(None));
                    break;
                }
                continue;
            }
            command = command_rx.recv() => {
                match command {
                    Some(SidebandCommand::TextTurn { text }) => {
                        handle_text_turn(
                            state,
                            qsf_session_id,
                            call_id,
                            &text,
                            &mut runtime_state,
                            &outbound_tx,
                        )
                        .await?;
                    }
                    None => {}
                }
                continue;
            }
            message = stream.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                drop(outbound_tx);
                let _ = writer.await;
                return Ok(SidebandExit::Disconnected {
                    reason: format!("websocket read error: {error}"),
                });
            }
        };

        match message {
            Message::Text(text) => {
                let Some(event) = parse_realtime_server_event("openai_realtime", &text) else {
                    continue;
                };
                if let Some(event_type) = realtime_event_type(&event) {
                    handle_provider_event(
                        state,
                        qsf_session_id,
                        call_id,
                        event_type,
                        &event,
                        &mut runtime_state,
                        &outbound_tx,
                    )
                    .await?;
                }
            }
            Message::Close(_) => {
                drop(outbound_tx);
                let _ = writer.await;
                return Ok(SidebandExit::Disconnected {
                    reason: "provider closed websocket".to_string(),
                });
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }

    drop(outbound_tx);
    let _ = writer.await;
    Ok(SidebandExit::Stopped)
}

async fn mark_session_degraded(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    reason: &str,
) {
    if let Some(session) = state.session_runtime(qsf_session_id).await {
        let mut guard = session.lock().await;
        if let Some(exchange_index) = guard
            .session_state
            .live
            .active_exchange
            .as_ref()
            .map(|exchange| exchange.index)
        {
            guard.non_promotable_exchange_indices.insert(exchange_index);
            log::warn!(
                "trusted exchange `{exchange_index}` for session `{qsf_session_id}` marked non-promotable because of sideband gap"
            );
        }
        guard.set_sideband_status(true, Some(reason.to_string()));
        log::warn!(
            "sideband gap/degradation for session `{qsf_session_id}` call `{call_id}`: {reason}"
        );
    }
}

async fn session_config(
    state: &AppState,
    qsf_session_id: &str,
) -> anyhow::Result<crate::state::BrowserSessionConfig> {
    let session = state
        .session_runtime(qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{qsf_session_id}`"))?;
    let guard = session.lock().await;
    Ok(guard.config.clone())
}

/// Render a websocket handshake failure with enough detail to diagnose it.
///
/// For an HTTP rejection this includes the status and (bounded) response body,
/// which carries OpenAI's machine-readable error (e.g. an unknown call_id),
/// rather than the opaque "HTTP error" the default `Display` would emit.
fn format_connect_error(
    qsf_session_id: &str,
    error: &tokio_tungstenite::tungstenite::Error,
) -> String {
    use tokio_tungstenite::tungstenite::Error as WsError;
    match error {
        WsError::Http(response) => {
            let status = response.status();
            let body = response
                .body()
                .as_ref()
                .map(|bytes| {
                    String::from_utf8_lossy(bytes)
                        .chars()
                        .take(1000)
                        .collect::<String>()
                })
                .unwrap_or_default();
            format!(
                "failed to connect sideband websocket for session `{qsf_session_id}`: HTTP {status}: {body}"
            )
        }
        other => {
            format!("failed to connect sideband websocket for session `{qsf_session_id}`: {other}")
        }
    }
}
