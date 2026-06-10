use std::convert::TryFrom;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use axum::http::{Request, header};
use futures_util::{SinkExt, StreamExt};
use qsf_context::ContextBudget;
use qsf_memory::RetrievalStrategy;
use qsf_realtime_protocol::{
    build_openai_realtime_conversation_session_update, build_openai_realtime_response_create,
    extract_response_text, parse_realtime_server_event, realtime_event_delta_text,
    realtime_event_response_id, realtime_event_response_status, realtime_event_text,
    realtime_event_transcript, realtime_event_type,
};
use qsf_session::{
    ContentHash, ContinuityManifest, Exchange, ExchangeModelUse, ExchangeOutput, LiveSessionEvent,
    ProviderEventKind, ProviderEventRecord, ResumeMode, SessionEndReason, SessionEvent, Turn,
    apply_live_session_event, persist_session_state, reduce_session_in_place,
};
use sha2::Digest;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::realtime::injection::{
    DEFAULT_PCM_RATE_HZ, MemoryInjectionRequest, assemble_memory_context,
    build_memory_injection_packet,
};
use crate::realtime::memory_store::retrieve_session_memories;
use crate::state::{AppState, SessionRuntime};

const DEFAULT_INJECTION_FRAGMENT_LIMIT: usize = 4;
const DEFAULT_INJECTION_TOKEN_LIMIT: usize = 600;
const INITIAL_BACKOFF_MS: u64 = 100;
const MAX_BACKOFF_MS: u64 = 2_000;

#[derive(Debug)]
pub struct SidebandHandle {
    stop_tx: watch::Sender<bool>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl SidebandHandle {
    pub fn spawn(state: AppState, qsf_session_id: String, call_id: String) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let join_handle = tokio::spawn(run_sideband(state, qsf_session_id, call_id, stop_rx));
        Self {
            stop_tx,
            join_handle,
        }
    }

    pub async fn stop(self) {
        let _ = self.stop_tx.send(true);
        let _ = self.join_handle.await;
    }
}

#[derive(Debug)]
enum SidebandExit {
    Stopped,
    Disconnected { reason: String },
}

#[derive(Debug, Default)]
struct SidebandRuntimeState {
    active_exchange_index: Option<usize>,
    response_started_at: Option<Instant>,
    response_id: Option<String>,
    current_request_hash: Option<ContentHash>,
    current_message_count: usize,
}

async fn run_sideband(
    state: AppState,
    qsf_session_id: String,
    call_id: String,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut backoff = Duration::from_millis(INITIAL_BACKOFF_MS);
    loop {
        if *stop_rx.borrow() {
            break;
        }

        match connect_and_run_once(&state, &qsf_session_id, &call_id, &mut stop_rx).await {
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
                mark_session_degraded(&state, &qsf_session_id, &call_id, &error.to_string()).await;
                log::warn!(
                    "sideband task failed for session `{qsf_session_id}` call `{call_id}`: {error}"
                );
                break;
            }
        }
    }
}

async fn connect_and_run_once(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    stop_rx: &mut watch::Receiver<bool>,
) -> anyhow::Result<SidebandExit> {
    let mut runtime_state = SidebandRuntimeState::default();
    let request = Request::builder()
        .uri(state.openai_realtime_ws_url(call_id))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.openai_api_key()),
        )
        .body(())
        .context("failed to build sideband websocket request")?;
    let (websocket, _response) = connect_async(request).await.with_context(|| {
        format!("failed to connect sideband websocket for session `{qsf_session_id}`")
    })?;

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

async fn handle_provider_event(
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
                guard.degraded = false;
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
            let exchange_index = ensure_authoritative_exchange(&mut guard);
            let transcript = realtime_event_transcript(event)
                .or_else(|| event.get("transcript").and_then(serde_json::Value::as_str))
                .unwrap_or_default()
                .to_string();
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
            runtime_state.active_exchange_index = Some(exchange_index);
            runtime_state.response_started_at = None;
            runtime_state.response_id = None;
            drop(guard);

            let mut turn_request_values = Vec::new();
            let retrieved_memories = match retrieve_session_memories(
                state,
                qsf_session_id,
                &transcript,
                RetrievalStrategy::AssociationWeighted,
                DEFAULT_INJECTION_FRAGMENT_LIMIT,
            ) {
                Ok(result) => result.selected,
                Err(error) => {
                    log::warn!("memory retrieval failed for session `{qsf_session_id}`: {error}");
                    Vec::new()
                }
            };

            let tone = format!(
                "voice={}, reasoning_effort={}",
                config.voice, config.reasoning_effort
            );
            let request = MemoryInjectionRequest {
                model: &config.model,
                voice: &config.voice,
                base_instructions: &config.instructions,
                output_modalities: &config.output_modalities,
                session_identity: qsf_session_id,
                tone: &tone,
                user_transcript: &transcript,
                retrieved_memories: &retrieved_memories,
                budget: ContextBudget::new(
                    DEFAULT_INJECTION_FRAGMENT_LIMIT,
                    DEFAULT_INJECTION_TOKEN_LIMIT,
                ),
                pcm_rate_hz: DEFAULT_PCM_RATE_HZ,
            };
            let packet = build_memory_injection_packet(&request);
            let context_assembly = packet
                .as_ref()
                .map(|packet| packet.context_assembly.clone())
                .unwrap_or_else(|| assemble_memory_context(&request));
            let retrieved_memory_block = packet
                .as_ref()
                .map(|packet| packet.memory_block.clone())
                .unwrap_or_default();

            let mut guard = session.lock().await;
            if let Some(exchange_index) = runtime_state.active_exchange_index {
                apply_live_session_event(
                    &mut guard.session_state,
                    LiveSessionEvent::MemoryContextRecorded {
                        exchange_index,
                        context_assembly,
                        retrieved_memory_block,
                        recalled_items: vec![],
                        live_capture: None,
                    },
                );
            }
            drop(guard);

            if let Some(packet) = packet {
                turn_request_values.push(packet.session_update.clone());
                turn_request_values.push(packet.conversation_item_create.clone());
                send_json(outbound_tx, packet.session_update.clone())?;
                send_json(outbound_tx, packet.conversation_item_create.clone())?;
            }

            let response_create = build_openai_realtime_response_create(
                &config.voice,
                &config.instructions,
                DEFAULT_PCM_RATE_HZ,
            );
            turn_request_values.push(response_create.clone());
            send_json(outbound_tx, response_create)?;
            runtime_state.current_request_hash = Some(hash_request_sequence(&turn_request_values));
            runtime_state.current_message_count = turn_request_values.len();
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
            runtime_state.response_id = response_id.clone();
            runtime_state.response_started_at = Some(Instant::now());

            let exchange_index = ensure_authoritative_exchange(&mut guard);
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
        }
        "response.output_audio.delta"
        | "response.audio.delta"
        | "response.output_audio_transcript.delta"
        | "response.output_audio_transcript.done" => {
            let exchange_index = ensure_authoritative_exchange(&mut guard);
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
        }
        "response.done" => {
            let exchange_index = ensure_authoritative_exchange(&mut guard);
            let response_id = runtime_state
                .response_id
                .clone()
                .or_else(|| realtime_event_response_id(event).map(str::to_string));
            let response_text = extract_response_text(event)
                .or_else(|| realtime_event_text(event).map(str::to_string))
                .unwrap_or_default();
            let response_status = realtime_event_response_status(event).unwrap_or("completed");
            let completed_at = SystemTime::now();
            let response_started_at = runtime_state
                .response_started_at
                .unwrap_or_else(Instant::now);
            let model_use = ExchangeModelUse {
                provider_name: Some("openai_realtime".to_string()),
                model_id: config.model.clone(),
                latency_ms: response_started_at.elapsed().as_millis() as u64,
                input_tokens: response_usage_input_tokens(event),
                cached_input_tokens: response_usage_cached_input_tokens(event),
                output_tokens: response_usage_output_tokens(event),
                full_request_hash: runtime_state
                    .current_request_hash
                    .unwrap_or_else(|| hash_request_sequence(&[])),
                message_count: runtime_state.current_message_count.max(1),
            };
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                    exchange_index,
                    event_kind: ProviderEventKind::ResponseCompleted,
                    provider_id: "openai_realtime".to_string(),
                    received_at: completed_at,
                    call_id: Some(call_id.to_string()),
                    event_id: Some(
                        event
                            .get("event_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("response.done")
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
                    response_id: response_id.clone(),
                    text: Some(response_text.clone()),
                    status: Some(response_status.to_string()),
                    audio_marker: event
                        .get("audio_marker")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
            );
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::OutputProduced(ExchangeOutput {
                    response_id: response_id.clone(),
                    text: response_text.clone(),
                    produced_at: completed_at,
                    provider_name: Some("openai_realtime".to_string()),
                    target: Some("speech".to_string()),
                    audio_marker: event
                        .get("audio_marker")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }),
            );
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ModelRoleCompleted(model_use),
            );
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::ExchangeCompleted {
                    exchange_index,
                    completed_at,
                },
            );
            if response_status != "completed" {
                guard.non_promotable_exchange_indices.insert(exchange_index);
                log::warn!(
                    "trusted exchange `{exchange_index}` for session `{qsf_session_id}` marked non-promotable because response status was `{response_status}`"
                );
            }
            promote_completed_trusted_exchanges(state, &mut guard).await?;
            runtime_state.active_exchange_index = None;
            runtime_state.response_id = None;
            runtime_state.response_started_at = None;
            runtime_state.current_request_hash = None;
            runtime_state.current_message_count = 0;
        }
        "session.closed" => {
            apply_live_session_event(
                &mut guard.session_state,
                LiveSessionEvent::SessionEnded {
                    reason: SessionEndReason::Eof,
                },
            );
            guard.degraded = true;
            runtime_state.active_exchange_index = None;
            runtime_state.response_id = None;
            runtime_state.response_started_at = None;
            runtime_state.current_request_hash = None;
            runtime_state.current_message_count = 0;
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

fn ensure_authoritative_exchange(runtime: &mut SessionRuntime) -> usize {
    if let Some(exchange) = runtime.session_state.live.active_exchange.as_ref() {
        return exchange.index;
    }
    let exchange =
        Exchange::new_voice_pending(runtime.new_trusted_exchange_index(), SystemTime::now());
    let exchange_index = exchange.index;
    apply_live_session_event(
        &mut runtime.session_state,
        LiveSessionEvent::ExchangeStarted(Box::new(exchange)),
    );
    exchange_index
}

async fn promote_completed_trusted_exchanges(
    state: &AppState,
    runtime: &mut SessionRuntime,
) -> anyhow::Result<()> {
    while runtime.trusted_promoted_exchange_count
        < runtime.session_state.live.completed_exchanges.len()
    {
        let exchange = runtime.session_state.live.completed_exchanges
            [runtime.trusted_promoted_exchange_count]
            .clone();
        runtime.trusted_promoted_exchange_count += 1;

        if runtime.degraded
            || runtime
                .non_promotable_exchange_indices
                .contains(&exchange.index)
        {
            log::warn!(
                "trusted exchange `{}` for session `{}` skipped for continuity promotion because it completed during an untrusted sideband window",
                exchange.index,
                runtime.qsf_session_id
            );
            continue;
        }

        let Ok(turn) = Turn::try_from(&exchange) else {
            log::warn!(
                "trusted exchange `{}` for session `{}` could not convert to a durable turn; skipping this exchange without degrading the session",
                exchange.index,
                runtime.qsf_session_id
            );
            runtime
                .non_promotable_exchange_indices
                .insert(exchange.index);
            continue;
        };

        reduce_session_in_place(
            &mut runtime.session_state,
            SessionEvent::ExchangeRecorded {
                session_id: runtime.qsf_session_id.clone(),
                exchange: Box::new(exchange.clone()),
            },
        );
        reduce_session_in_place(
            &mut runtime.session_state,
            SessionEvent::TurnCompleted(turn),
        );

        let continuity_dir = state.continuity_session_dir(&runtime.qsf_session_id);
        let state_path = persist_session_state(&runtime.session_state, &continuity_dir)?;
        let mut manifest = ContinuityManifest::load_or_default(
            state.continuity_manifest_path(&runtime.qsf_session_id),
        )?;
        manifest.current_session_id = Some(runtime.qsf_session_id.clone());
        manifest.current_session_state_path = Some(
            state_path
                .strip_prefix(&continuity_dir)
                .unwrap_or(&state_path)
                .to_path_buf(),
        );
        manifest.sleep_pending = true;
        manifest.resume_mode = ResumeMode::AwakeContinuation;
        manifest.persist(state.continuity_manifest_path(&runtime.qsf_session_id))?;
    }

    Ok(())
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
        guard.degraded = true;
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

fn send_json(
    sender: &mpsc::UnboundedSender<Message>,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    sender
        .send(Message::Text(serde_json::to_string(&value)?.into()))
        .map_err(|error| anyhow::anyhow!("failed to queue sideband message: {error}"))
}

fn hash_request_sequence(values: &[serde_json::Value]) -> ContentHash {
    let mut hasher = sha2::Sha256::new();
    for value in values {
        hasher.update(serde_json::to_vec(value).unwrap_or_default());
    }
    ContentHash(hasher.finalize().into())
}

fn response_usage_input_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["input_tokens"]).unwrap_or(0) as u32
}

fn response_usage_cached_input_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["input_token_details", "cached_tokens"])
        .or_else(|| response_usage_number(event, &["cached_input_tokens"]))
        .unwrap_or(0) as u32
}

fn response_usage_output_tokens(event: &serde_json::Value) -> u32 {
    response_usage_number(event, &["output_tokens"]).unwrap_or(0) as u32
}

fn response_usage_number(event: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = event.get("response")?.get("usage")?;
    for segment in path {
        current = current.get(segment)?;
    }
    current.as_u64()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use super::*;

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
        )
        .expect("state")
    }

    #[test]
    fn hash_request_sequence_is_deterministic() {
        let first = hash_request_sequence(&[serde_json::json!({"a": 1})]);
        let second = hash_request_sequence(&[serde_json::json!({"a": 1})]);
        assert_eq!(first, second);
    }

    #[test]
    fn response_usage_extractors_tolerate_missing_fields() {
        let event = serde_json::json!({
            "response": {
                "usage": {
                    "input_tokens": 3,
                    "input_token_details": {
                        "cached_tokens": 1
                    },
                    "output_tokens": 4
                }
            }
        });

        assert_eq!(response_usage_input_tokens(&event), 3);
        assert_eq!(response_usage_cached_input_tokens(&event), 1);
        assert_eq!(response_usage_output_tokens(&event), 4);
    }

    #[tokio::test]
    async fn empty_store_turn_records_empty_context_and_promotes() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

        handle_provider_event(
            &state,
            &allocation.qsf_session_id,
            "call-empty",
            "conversation.item.input_audio_transcription.completed",
            &serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "event_id": "evt-transcript",
                "item_id": "item-user",
                "transcript": "hello without seeded memory"
            }),
            &mut runtime_state,
            &outbound_tx,
        )
        .await
        .expect("transcript event");

        let response_create = outbound_rx.recv().await.expect("response.create");
        assert!(
            response_create
                .to_text()
                .expect("text")
                .contains("\"response.create\"")
        );
        assert!(outbound_rx.try_recv().is_err());

        handle_provider_event(
            &state,
            &allocation.qsf_session_id,
            "call-empty",
            "response.done",
            &serde_json::json!({
                "type": "response.done",
                "event_id": "evt-done",
                "response": {
                    "id": "response-empty",
                    "status": "completed",
                    "output": [{
                        "content": [{
                            "type": "output_text",
                            "text": "hi"
                        }]
                    }],
                    "usage": {
                        "input_tokens": 1,
                        "output_tokens": 2
                    }
                }
            }),
            &mut runtime_state,
            &outbound_tx,
        )
        .await
        .expect("response done");

        let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
        let persisted = qsf_session::load_session_state(continuity_dir.join("session-state.json"))
            .expect("persisted state");
        assert_eq!(persisted.turns.len(), 1);
        assert_eq!(persisted.turns[0].user_input, "hello without seeded memory");
        assert!(persisted.turns[0].context_assembly.selected.is_empty());
        assert_eq!(
            persisted.turns[0].context_assembly.budget,
            ContextBudget::new(
                DEFAULT_INJECTION_FRAGMENT_LIMIT,
                DEFAULT_INJECTION_TOKEN_LIMIT
            )
        );
    }

    #[tokio::test]
    async fn session_updated_ack_clears_degraded_after_reconnect() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, _outbound_rx) = mpsc::unbounded_channel();
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            runtime.lock().await.degraded = true;
        }

        handle_provider_event(
            &state,
            &allocation.qsf_session_id,
            "call-recovered",
            "session.updated",
            &serde_json::json!({
                "type": "session.updated",
                "event_id": "evt-session-updated"
            }),
            &mut runtime_state,
            &outbound_tx,
        )
        .await
        .expect("session updated");

        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        assert!(!runtime.lock().await.degraded);
    }

    #[tokio::test]
    async fn promote_trusted_exchange_writes_continuity_state() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            let mut guard = runtime.lock().await;
            let exchange = Exchange {
                index: 0,
                started_at: SystemTime::UNIX_EPOCH,
                completed_at: Some(SystemTime::UNIX_EPOCH),
                input: qsf_session::ExchangeInput::Voice {
                    final_transcript: "hello".to_string(),
                    utterances: vec![],
                },
                output: Some(ExchangeOutput {
                    response_id: Some("response-1".to_string()),
                    text: "hi".to_string(),
                    produced_at: SystemTime::UNIX_EPOCH,
                    provider_name: Some("openai_realtime".to_string()),
                    target: Some("speech".to_string()),
                    audio_marker: None,
                }),
                context_assembly: Some(qsf_context::ContextAssembly {
                    budget: ContextBudget::new(1, 16),
                    selected: vec![],
                    omitted: vec![],
                    used_estimated_tokens: 0,
                }),
                retrieved_memory_block: String::new(),
                recalled_items: vec![],
                model: Some(ExchangeModelUse {
                    provider_name: Some("openai_realtime".to_string()),
                    model_id: "gpt-realtime-2".to_string(),
                    latency_ms: 10,
                    input_tokens: 1,
                    cached_input_tokens: 0,
                    output_tokens: 2,
                    full_request_hash: ContentHash([1; 32]),
                    message_count: 3,
                }),
                interruptions: vec![],
                provider_events: vec![],
                tool_requests: vec![],
                status: qsf_session::ExchangeStatus::Completed,
            };
            guard.session_state.live.completed_exchanges.push(exchange);
            guard.session_state.live.active_exchange = None;
            guard.trusted_promoted_exchange_count = 0;
            guard.degraded = false;
            promote_completed_trusted_exchanges(&state, &mut guard)
                .await
                .expect("promotion");
        }

        let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
        let persisted = qsf_session::load_session_state(continuity_dir.join("session-state.json"))
            .expect("persisted state");
        assert_eq!(persisted.turns.len(), 1);
        let manifest = qsf_session::ContinuityManifest::load_or_default(
            continuity_dir.join("continuity-manifest.json"),
        )
        .expect("manifest");
        assert!(manifest.sleep_pending);
        assert_eq!(manifest.resume_mode, ResumeMode::AwakeContinuation);
    }

    #[tokio::test]
    async fn degraded_session_skips_promotion() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            let mut guard = runtime.lock().await;
            guard.degraded = true;
            guard.session_state.live.completed_exchanges.push(
                Exchange::new_text(0, "hello", SystemTime::UNIX_EPOCH)
                    .completed(SystemTime::UNIX_EPOCH),
            );
            promote_completed_trusted_exchanges(&state, &mut guard)
                .await
                .expect("promotion");
            assert!(guard.session_state.turns.is_empty());
        }
    }

    #[tokio::test]
    async fn gap_window_exchange_is_consumed_but_next_exchange_promotes_after_recovery() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        {
            let runtime = state
                .session_runtime(&allocation.qsf_session_id)
                .await
                .expect("runtime");
            let mut guard = runtime.lock().await;
            guard
                .session_state
                .live
                .completed_exchanges
                .push(completed_exchange(0, "gap turn", "gap answer"));
            guard
                .session_state
                .live
                .completed_exchanges
                .push(completed_exchange(1, "next turn", "next answer"));
            guard.non_promotable_exchange_indices.insert(0);
            guard.degraded = false;

            promote_completed_trusted_exchanges(&state, &mut guard)
                .await
                .expect("promotion");

            assert_eq!(guard.trusted_promoted_exchange_count, 2);
            assert_eq!(guard.session_state.turns.len(), 1);
            assert_eq!(guard.session_state.turns[0].user_input, "next turn");
        }
    }

    fn completed_exchange(index: usize, user_input: &str, assistant_response: &str) -> Exchange {
        Exchange {
            index,
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            input: qsf_session::ExchangeInput::Voice {
                final_transcript: user_input.to_string(),
                utterances: vec![],
            },
            output: Some(ExchangeOutput {
                response_id: Some(format!("response-{index}")),
                text: assistant_response.to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("openai_realtime".to_string()),
                target: Some("speech".to_string()),
                audio_marker: None,
            }),
            context_assembly: Some(qsf_context::ContextAssembly {
                budget: ContextBudget::new(1, 16),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            }),
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            model: Some(ExchangeModelUse {
                provider_name: Some("openai_realtime".to_string()),
                model_id: "gpt-realtime-2".to_string(),
                latency_ms: 10,
                input_tokens: 1,
                cached_input_tokens: 0,
                output_tokens: 2,
                full_request_hash: ContentHash([index as u8; 32]),
                message_count: 3,
            }),
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            status: qsf_session::ExchangeStatus::Completed,
        }
    }
}
