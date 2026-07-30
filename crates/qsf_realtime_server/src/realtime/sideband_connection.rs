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
use crate::realtime::safety_identifier::{OPENAI_SAFETY_IDENTIFIER_HEADER, hash_session_id};
use crate::realtime::sideband::{
    SidebandCommand, SidebandRuntimeState, handle_text_turn, send_json,
};
use crate::realtime::sideband_attachment::{ReconnectPolicy, SidebandAttachment};
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
    attachment: SidebandAttachment,
    mut stop_rx: watch::Receiver<bool>,
    mut command_rx: mpsc::UnboundedReceiver<SidebandCommand>,
) {
    let mut backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
    let mut has_successfully_attached = false;
    loop {
        if *stop_rx.borrow() {
            break;
        }

        match connect_and_run_once(
            &state,
            &qsf_session_id,
            &attachment,
            &mut has_successfully_attached,
            &mut stop_rx,
            &mut command_rx,
        )
        .await
        {
            Ok(SidebandExit::Stopped) => break,
            Ok(SidebandExit::Disconnected { reason }) => {
                if *stop_rx.borrow() {
                    break;
                }
                mark_session_degraded(&state, &qsf_session_id, &attachment, &reason).await;
                if *stop_rx.borrow() {
                    break;
                }
                if has_successfully_attached
                    && attachment.reconnect_policy() == ReconnectPolicy::FailClosedAfterFirstAttach
                {
                    terminate_sideband(&state, &qsf_session_id, &attachment, &reason).await;
                    break;
                }
                engine_logging::engine_info!(
                    "sideband reconnecting for session `{qsf_session_id}` attachment `{attachment}` policy `{}` after disconnect: {reason}",
                    attachment.reconnect_policy()
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_millis(MAX_BACKOFF_MS));
            }
            Err(error) => {
                if *stop_rx.borrow() {
                    break;
                }
                // Surface the full anyhow chain (`{error:#}`), not just the
                // outermost context, so the failing operation is identifiable.
                let reason = format!("{error:#}");
                mark_session_degraded(&state, &qsf_session_id, &attachment, &reason).await;
                engine_logging::engine_warn!(
                    "sideband task failed for session `{qsf_session_id}` attachment `{attachment}` policy `{}`: {reason}",
                    attachment.reconnect_policy()
                );
                if *stop_rx.borrow() {
                    break;
                }
                if has_successfully_attached
                    && attachment.reconnect_policy() == ReconnectPolicy::FailClosedAfterFirstAttach
                {
                    terminate_sideband(&state, &qsf_session_id, &attachment, &reason).await;
                    break;
                }
                engine_logging::engine_info!(
                    "sideband reconnecting for session `{qsf_session_id}` attachment `{attachment}` policy `{}` after error",
                    attachment.reconnect_policy()
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_millis(MAX_BACKOFF_MS));
            }
        }
    }
    set_sideband_attached(&state, &qsf_session_id, false).await;
}

async fn connect_and_run_once(
    state: &AppState,
    qsf_session_id: &str,
    attachment: &SidebandAttachment,
    has_successfully_attached: &mut bool,
    stop_rx: &mut watch::Receiver<bool>,
    command_rx: &mut mpsc::UnboundedReceiver<SidebandCommand>,
) -> anyhow::Result<SidebandExit> {
    let mut runtime_state = SidebandRuntimeState::default();
    // Build the request through `into_client_request` so tungstenite generates
    // the websocket handshake headers (`Sec-WebSocket-Key`, `Upgrade`,
    // `Connection`, `Sec-WebSocket-Version`, `Host`); a hand-built `Request`
    // omits them and fails the handshake client-side. Provider authentication
    // and the safety identifier are layered on afterwards.
    let mut request = attachment
        .websocket_url(state.openai_realtime_ws_base_url())
        .into_client_request()
        .context("failed to build sideband websocket request")?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {}", state.openai_api_key())
            .parse()
            .context("failed to build sideband authorization header")?,
    );
    request.headers_mut().insert(
        OPENAI_SAFETY_IDENTIFIER_HEADER,
        hash_session_id(qsf_session_id)
            .parse()
            .context("failed to build sideband safety identifier header")?,
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

    engine_logging::engine_info!(
        "sideband attached for session `{qsf_session_id}` attachment `{attachment}` policy `{}`",
        attachment.reconnect_policy()
    );

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
                            attachment,
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
            drop(outbound_tx);
            let _ = writer.await;
            return Ok(if *stop_rx.borrow() {
                SidebandExit::Stopped
            } else {
                SidebandExit::Disconnected {
                    reason: "provider ended the websocket stream".to_string(),
                }
            });
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
                    if event_type == "session.updated" {
                        *has_successfully_attached = true;
                    }
                    handle_provider_event(
                        state,
                        qsf_session_id,
                        attachment,
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
    attachment: &SidebandAttachment,
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
            engine_logging::engine_warn!(
                "trusted exchange `{exchange_index}` for session `{qsf_session_id}` attachment `{attachment}` policy `{}` marked non-promotable because of sideband gap",
                attachment.reconnect_policy()
            );
        }
        guard.set_sideband_attached(false);
        guard.set_sideband_status(true, Some(reason.to_string()));
        engine_logging::engine_warn!(
            "sideband gap/degradation for session `{qsf_session_id}` attachment `{attachment}` policy `{}`: {reason}",
            attachment.reconnect_policy()
        );
    }
}

async fn set_sideband_attached(state: &AppState, qsf_session_id: &str, attached: bool) {
    if let Some(session) = state.session_runtime(qsf_session_id).await {
        session.lock().await.set_sideband_attached(attached);
    }
}

async fn terminate_sideband(
    state: &AppState,
    qsf_session_id: &str,
    attachment: &SidebandAttachment,
    reason: &str,
) {
    if let Some(session) = state.session_runtime(qsf_session_id).await {
        let mut guard = session.lock().await;
        let terminal_reason = format!(
            "sideband terminated after attachment `{attachment}` with policy `{}`: {reason}",
            attachment.reconnect_policy()
        );
        guard.terminate_sideband(terminal_reason.clone());
        engine_logging::engine_warn!(
            "sideband terminal failure for session `{qsf_session_id}` attachment `{attachment}`: {terminal_reason}"
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use futures_util::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio::sync::watch;
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{Request, Response},
        },
    };

    use super::*;
    use crate::realtime::sideband::SidebandHandle;
    use crate::realtime::sideband_attachment::SidebandAttachment;
    use crate::state::SessionIdMode;

    #[allow(clippy::result_large_err)]
    async fn spawn_stub(
        expect_reconnect: bool,
    ) -> (
        String,
        Arc<AtomicUsize>,
        Arc<Mutex<Vec<String>>>,
        watch::Receiver<usize>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("stub listener");
        let address = listener.local_addr().expect("stub address");
        let connection_count = Arc::new(AtomicUsize::new(0));
        let safety_identifiers = Arc::new(Mutex::new(Vec::new()));
        let (count_tx, count_rx) = watch::channel(0usize);
        let count_for_task = connection_count.clone();
        let safety_identifiers_for_task = safety_identifiers.clone();
        let server = tokio::spawn(async move {
            let expected_connections = if expect_reconnect { 2 } else { 1 };
            for connection_number in 1..=expected_connections {
                let (stream, _) = listener.accept().await.expect("stub connection");
                let safety_identifiers_for_handshake = safety_identifiers_for_task.clone();
                let mut websocket =
                    accept_hdr_async(stream, move |request: &Request, response: Response| {
                        let safety_identifier = request
                            .headers()
                            .get(OPENAI_SAFETY_IDENTIFIER_HEADER)
                            .expect("safety identifier header")
                            .to_str()
                            .expect("safety identifier text")
                            .to_string();
                        safety_identifiers_for_handshake
                            .lock()
                            .expect("safety identifier capture")
                            .push(safety_identifier);
                        Ok(response)
                    })
                    .await
                    .expect("stub websocket");
                count_for_task.store(connection_number, Ordering::SeqCst);
                count_tx.send_replace(connection_number);
                websocket
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "session.updated",
                            "event_id": format!("stub-session-updated-{connection_number}")
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("session.updated");

                if connection_number == 1 {
                    while let Some(message) = websocket.next().await {
                        let Ok(Message::Text(text)) = message else {
                            continue;
                        };
                        let event: serde_json::Value =
                            serde_json::from_str(&text).expect("sideband command JSON");
                        if event.get("type").and_then(serde_json::Value::as_str)
                            == Some("response.create")
                        {
                            websocket
                                .send(Message::Text(
                                    serde_json::json!({
                                        "type": "response.done",
                                        "event_id": "stub-response-done",
                                        "response": {
                                            "id": "stub-response",
                                            "status": "completed",
                                            "output": [{
                                                "content": [{
                                                    "type": "output_text",
                                                    "text": "stub response"
                                                }]
                                            }],
                                            "usage": {
                                                "input_tokens": 1,
                                                "output_tokens": 1
                                            }
                                        }
                                    })
                                    .to_string()
                                    .into(),
                                ))
                                .await
                                .expect("response.done");
                            websocket
                                .send(Message::Close(None))
                                .await
                                .expect("close stub websocket");
                            break;
                        }
                    }
                }
            }
        });

        (
            format!("ws://{address}/v1/realtime"),
            connection_count,
            safety_identifiers,
            count_rx,
            server,
        )
    }

    async fn test_state(tempdir: &TempDir, websocket_base_url: &str) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            websocket_base_url,
            tempdir.path(),
            SessionIdMode::Default,
        )
        .expect("state")
    }

    async fn wait_for_attachment(
        runtime: &Arc<tokio::sync::Mutex<crate::state::SessionRuntime>>,
    ) -> watch::Receiver<crate::state::SidebandStatus> {
        let mut status_rx = runtime.lock().await.subscribe_status();
        tokio::time::timeout(
            Duration::from_secs(5),
            status_rx.wait_for(|status| status.attached),
        )
        .await
        .expect("sideband attach timeout")
        .expect("sideband status channel");
        status_rx
    }

    #[tokio::test]
    async fn model_session_disconnect_fails_closed_without_processing_later_turns() {
        let tempdir = TempDir::new().expect("tempdir");
        let (websocket_url, connection_count, safety_identifiers, _count_rx, server) =
            spawn_stub(false).await;
        let state = test_state(&tempdir, &websocket_url).await;
        let allocation = state.create_session().await.expect("session");
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        let attachment = SidebandAttachment::ServerModelSession {
            model: "gpt-realtime-test".to_string(),
        };
        // The sideband is owned by the session, so spawn it after the runtime is allocated.
        let handle =
            SidebandHandle::spawn(state.clone(), allocation.qsf_session_id.clone(), attachment);
        let mut status_rx = wait_for_attachment(&runtime).await;
        handle
            .submit_text_turn("first scripted phrase".to_string())
            .expect("first phrase");
        tokio::time::timeout(
            Duration::from_secs(5),
            status_rx.wait_for(|status| status.terminated.is_some()),
        )
        .await
        .expect("terminal sideband timeout")
        .expect("sideband status channel");

        assert_eq!(connection_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            safety_identifiers
                .lock()
                .expect("safety identifiers")
                .as_slice(),
            &[hash_session_id(&allocation.qsf_session_id)]
        );
        let (promoted, turns, next_trusted_exchange_index) = {
            let guard = runtime.lock().await;
            (
                guard.trusted_promoted_exchange_count,
                guard.session_state.turns.len(),
                guard.next_trusted_exchange_index,
            )
        };
        assert_eq!(promoted, 1);
        assert_eq!(turns, 1);
        let _ = handle.submit_text_turn("phrase after terminal failure".to_string());
        tokio::time::sleep(Duration::from_millis(150)).await;
        let guard = runtime.lock().await;
        assert_eq!(guard.trusted_promoted_exchange_count, promoted);
        assert_eq!(guard.session_state.turns.len(), turns);
        assert_eq!(
            guard.next_trusted_exchange_index,
            next_trusted_exchange_index
        );
        assert!(!guard.sideband_attached);
        drop(guard);
        handle.stop().await;
        server.await.expect("stub task");
    }

    #[tokio::test]
    async fn browser_call_disconnect_keeps_the_reattach_loop() {
        let tempdir = TempDir::new().expect("tempdir");
        let (websocket_url, connection_count, safety_identifiers, mut count_rx, server) =
            spawn_stub(true).await;
        let state = test_state(&tempdir, &websocket_url).await;
        let allocation = state.create_session().await.expect("session");
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        let attachment = SidebandAttachment::BrowserCall {
            call_id: "browser-call-123".to_string(),
        };
        let handle = SidebandHandle::spawn(state, allocation.qsf_session_id.clone(), attachment);

        let _status_rx = wait_for_attachment(&runtime).await;
        handle
            .submit_text_turn("browser scripted phrase".to_string())
            .expect("phrase");
        tokio::time::timeout(
            Duration::from_secs(5),
            count_rx.wait_for(|count| *count >= 2),
        )
        .await
        .expect("reattach timeout")
        .expect("stub count channel");
        assert!(connection_count.load(Ordering::SeqCst) >= 2);
        handle.stop().await;
        let expected_safety_identifier = hash_session_id(&allocation.qsf_session_id);
        {
            let captured_safety_identifiers =
                safety_identifiers.lock().expect("safety identifiers");
            assert!(!captured_safety_identifiers.is_empty());
            assert!(
                captured_safety_identifiers
                    .iter()
                    .all(|identifier| identifier == &expected_safety_identifier)
            );
        }
        assert!(!runtime.lock().await.sideband_attached);
        server.await.expect("stub task");
    }
}
