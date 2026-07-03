use std::collections::HashSet;
use std::time::{Instant, SystemTime};

use qsf_realtime_protocol::build_openai_realtime_conversation_item_create;
use qsf_session::{ContentHash, Exchange, LiveSessionEvent, apply_live_session_event};
use sha2::Digest;
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::diagnostics::DiagnosticRecord;
use crate::realtime::sideband_connection::run_sideband;
use crate::realtime::sideband_turn_injection::inject_trusted_turn_context_and_response;
use crate::realtime::tools::VolitionStateSnapshot;
use crate::realtime::turn_integrity::TurnPhase;
use crate::state::{AppState, SessionRuntime};

pub(super) const DEFAULT_INJECTION_FRAGMENT_LIMIT: usize = 4;
pub(super) const DEFAULT_INJECTION_TOKEN_LIMIT: usize = 600;

#[derive(Debug)]
pub struct SidebandHandle {
    stop_tx: watch::Sender<bool>,
    command_tx: mpsc::UnboundedSender<SidebandCommand>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl SidebandHandle {
    pub fn spawn(state: AppState, qsf_session_id: String, call_id: String) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let join_handle = tokio::spawn(run_sideband(
            state,
            qsf_session_id,
            call_id,
            stop_rx,
            command_rx,
        ));
        Self {
            stop_tx,
            command_tx,
            join_handle,
        }
    }

    pub fn submit_text_turn(&self, text: String) -> anyhow::Result<()> {
        let text = text.trim().to_string();
        if text.is_empty() {
            anyhow::bail!("text turn cannot be empty");
        }
        self.command_tx
            .send(SidebandCommand::TextTurn { text })
            .map_err(|_| anyhow::anyhow!("sideband command channel is closed"))
    }

    pub async fn stop(self) {
        let _ = self.stop_tx.send(true);
        let _ = self.join_handle.await;
    }
}

#[derive(Debug)]
pub(super) enum SidebandCommand {
    TextTurn { text: String },
}

#[derive(Debug, Default)]
pub(super) struct SidebandRuntimeState {
    pub(super) active_exchange_index: Option<usize>,
    pub(super) pending_response_exchange: Option<usize>,
    pub(super) turn_phase: TurnPhase,
    pub(super) final_transcript_received_at: Option<OffsetDateTime>,
    pub(super) memory_injected_at: Option<OffsetDateTime>,
    pub(super) volition_context_injected_at: Option<OffsetDateTime>,
    pub(super) response_create_sent_at: Option<OffsetDateTime>,
    pub(super) response_created_at: Option<OffsetDateTime>,
    pub(super) first_audio_received_at: Option<OffsetDateTime>,
    pub(super) response_started_at: Option<Instant>,
    pub(super) response_id: Option<String>,
    pub(super) current_request_hash: Option<ContentHash>,
    pub(super) current_message_count: usize,
    pub(super) accumulated_latency_ms: u64,
    pub(super) accumulated_input_tokens: u32,
    pub(super) accumulated_cached_input_tokens: u32,
    pub(super) accumulated_output_tokens: u32,
    pub(super) tool_calls_in_turn: usize,
    pub(super) pending_context_retrieval_hints: Vec<String>,
    pub(super) previous_turn_surfaced_goal_id: Option<String>,
    pub(super) stale_response_ids: HashSet<String>,
    pub(super) latency_record_labels_emitted: HashSet<String>,
}

impl SidebandRuntimeState {
    /// Clears in-flight response accounting without touching exchange ownership
    /// or stale-response tracking.
    pub(super) fn clear_in_flight_response_state(&mut self) {
        self.final_transcript_received_at = None;
        self.memory_injected_at = None;
        self.volition_context_injected_at = None;
        self.response_create_sent_at = None;
        self.response_created_at = None;
        self.first_audio_received_at = None;
        self.response_id = None;
        self.response_started_at = None;
        self.current_request_hash = None;
        self.current_message_count = 0;
        self.accumulated_latency_ms = 0;
        self.accumulated_input_tokens = 0;
        self.accumulated_cached_input_tokens = 0;
        self.accumulated_output_tokens = 0;
        self.tool_calls_in_turn = 0;
        self.latency_record_labels_emitted.clear();
    }
}

pub(super) async fn handle_text_turn(
    state: &AppState,
    qsf_session_id: &str,
    call_id: &str,
    text: &str,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
) -> anyhow::Result<()> {
    let transcript = text.trim();
    if transcript.is_empty() {
        return Ok(());
    }

    let session = state
        .session_runtime(qsf_session_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown qsf_session_id `{qsf_session_id}`"))?;
    let mut guard = session.lock().await;
    let config = guard.config.clone();
    let exchange_index = ensure_authoritative_exchange(&mut guard);
    apply_live_session_event(
        &mut guard.session_state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index,
            utterance: qsf_session::UtteranceRecord {
                utterance_id: format!("typed-{exchange_index}"),
                revision_index: 0,
                transcript: transcript.to_string(),
                received_at: SystemTime::now(),
                provider_id: Some(format!("{call_id}:typed")),
                source_chunk_index: None,
            },
            final_transcript: transcript.to_string(),
        },
    );
    runtime_state.final_transcript_received_at = Some(OffsetDateTime::now_utc());
    runtime_state.active_exchange_index = Some(exchange_index);
    runtime_state.pending_response_exchange = None;
    runtime_state.turn_phase = TurnPhase::Idle;
    let volition_tick_before = guard.volition.state.tick;
    let events_applied = apply_trusted_transcript_to_volition(&mut guard, transcript);
    let volition_snapshot = VolitionStateSnapshot {
        state: guard.volition.state.clone(),
        fixture: guard.volition.fixture.clone(),
    };
    drop(guard);
    let input_transcript_ref = format!(
        "exchange:{exchange_index}/transcript:{}",
        hash_text(transcript)
    );
    inject_trusted_turn_context_and_response(
        state,
        session,
        qsf_session_id,
        transcript,
        &config,
        runtime_state,
        outbound_tx,
        Some(build_openai_realtime_conversation_item_create(
            "user", transcript,
        )),
        exchange_index,
        &volition_snapshot,
        events_applied,
        volition_tick_before,
        input_transcript_ref,
    )
    .await
}

pub(super) fn ensure_authoritative_exchange(runtime: &mut SessionRuntime) -> usize {
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

pub(super) fn send_json(
    sender: &mpsc::UnboundedSender<Message>,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    sender
        .send(Message::Text(serde_json::to_string(&value)?.into()))
        .map_err(|error| anyhow::anyhow!("failed to queue sideband message: {error}"))
}

pub(super) fn hash_request_sequence(values: &[serde_json::Value]) -> ContentHash {
    let mut hasher = sha2::Sha256::new();
    for value in values {
        hasher.update(serde_json::to_vec(value).unwrap_or_default());
    }
    ContentHash(hasher.finalize().into())
}

pub(super) fn hash_text(text: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record_latency_observation_once(
    runtime_state: &mut SidebandRuntimeState,
    diagnostics: &crate::diagnostics::DiagnosticWriter,
    qsf_session_id: &str,
    label: &'static str,
    started_at: OffsetDateTime,
    finished_at: OffsetDateTime,
) -> anyhow::Result<()> {
    if !runtime_state
        .latency_record_labels_emitted
        .insert(label.to_string())
    {
        return Ok(());
    }

    diagnostics.write(&DiagnosticRecord::LatencyObservation {
        qsf_session_id: qsf_session_id.to_string(),
        label: label.to_string(),
        started_at,
        finished_at,
        latency_ms: (finished_at - started_at).whole_milliseconds() as i64,
    })?;
    Ok(())
}

pub(super) fn record_latency_observation_if_ready(
    runtime_state: &mut SidebandRuntimeState,
    diagnostics: &crate::diagnostics::DiagnosticWriter,
    qsf_session_id: &str,
    label: &'static str,
    started_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
) -> anyhow::Result<()> {
    let (Some(started_at), Some(finished_at)) = (started_at, finished_at) else {
        return Ok(());
    };

    record_latency_observation_once(
        runtime_state,
        diagnostics,
        qsf_session_id,
        label,
        started_at,
        finished_at,
    )
}

/// Map a trusted user transcript to volition events and apply them to the session's
/// in-memory volition state. Called once per trusted turn boundary (StartTurn or Interrupt
/// disposition). Pure mapping — no external side effects, no diagnostics in Phase 2.
pub(super) fn apply_trusted_transcript_to_volition(
    guard: &mut SessionRuntime,
    transcript: &str,
) -> Vec<qsf_volition::VolitionEvent> {
    let new_tick = guard.volition.state.tick + 1;
    let events = crate::realtime::volition::events_for_trusted_transcript(
        transcript,
        &guard.volition.state,
        &guard.volition.fixture,
        new_tick,
    );
    guard.volition.apply_events(events.clone());
    events
}

#[cfg(test)]
#[path = "sideband_tests.rs"]
mod tests;
