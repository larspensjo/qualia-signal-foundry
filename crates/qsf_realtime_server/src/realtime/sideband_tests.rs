use std::fs;
use std::time::Duration;

use qsf_context::ContextBudget;
use tempfile::TempDir;
use tokio::sync::mpsc;

use super::*;
use crate::diagnostics::DiagnosticRecord;
use crate::realtime::sideband_provider_event::handle_provider_event;

fn state(tempdir: &TempDir) -> AppState {
    AppState::new_with_realtime_ws_base_url(
        "test-api-key",
        "http://127.0.0.1:9999",
        "wss://example.invalid/realtime",
        tempdir.path().to_path_buf(),
        crate::state::SessionIdMode::Default,
    )
    .expect("state")
}

async fn start_test_turn(
    state: &AppState,
    qsf_session_id: &str,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
) {
    handle_provider_event(
        state,
        qsf_session_id,
        "call-test",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-transcript",
            "item_id": "item-user",
            "transcript": "hello tool loop"
        }),
        runtime_state,
        outbound_tx,
    )
    .await
    .expect("transcript event");
}

async fn diagnostic_records(state: &AppState, qsf_session_id: &str) -> Vec<DiagnosticRecord> {
    let runtime = state
        .session_runtime(qsf_session_id)
        .await
        .expect("runtime");
    let diagnostics_path = runtime.lock().await.diagnostics.path().to_path_buf();
    let contents = fs::read_to_string(diagnostics_path).expect("diagnostics log");
    contents
        .lines()
        .map(|line| serde_json::from_str(line).expect("diagnostic record"))
        .collect()
}

fn function_call_response_done(
    event_id: &str,
    response_id: &str,
    status: &str,
    call_id: &str,
    tool_name: &str,
    arguments: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "response.done",
        "event_id": event_id,
        "response": {
            "id": response_id,
            "status": status,
            "output": [{
                "type": "function_call",
                "name": tool_name,
                "call_id": call_id,
                "arguments": arguments
            }],
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1
            }
        }
    })
}

fn drain_outbound_texts(outbound_rx: &mut mpsc::UnboundedReceiver<Message>) -> Vec<String> {
    let mut texts = Vec::new();
    while let Ok(message) = outbound_rx.try_recv() {
        if let Ok(text) = message.to_text() {
            texts.push(text.to_string());
        }
    }
    texts
}

#[test]
fn hash_request_sequence_is_deterministic() {
    let first = hash_request_sequence(&[serde_json::json!({"a": 1})]);
    let second = hash_request_sequence(&[serde_json::json!({"a": 1})]);
    assert_eq!(first, second);
}

#[cfg(test)]
#[path = "sideband_volition_tests.rs"]
mod sideband_volition_tests;

#[cfg(test)]
#[path = "sideband_tool_loop_tests.rs"]
mod sideband_tool_loop_tests;

#[cfg(test)]
#[path = "sideband_promotion_tests.rs"]
mod sideband_promotion_tests;

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
            "transcript": "please say hello without seeded memory"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("transcript event");

    let volition_packet = outbound_rx.recv().await.expect("volition packet");
    assert!(
        volition_packet
            .to_text()
            .expect("text")
            .contains("Simulated volition context for this turn")
    );
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
    assert_eq!(
        persisted.turns[0].user_input,
        "please say hello without seeded memory"
    );
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
async fn completed_trusted_turn_spawns_live_goal_formation() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-formation",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-transcript",
            "item_id": "item-user",
            "transcript": "let's talk about goals"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("transcript event");
    drain_outbound_texts(&mut outbound_rx);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-formation",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-done",
            "response": {
                "id": "response-formation",
                "status": "completed",
                "output": [{
                    "content": [{
                        "type": "output_text",
                        "text": "hi there"
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

    // Formation is spawned as a detached task off the hot path, so poll the diagnostics log
    // rather than asserting immediately after the response.done handler returns.
    let performed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
            if let Some(record) = records.into_iter().find(|record| {
                matches!(
                    record,
                    DiagnosticRecord::LiveGoalFormationPerformed { exchange_index, .. }
                        if *exchange_index == 0
                )
            }) {
                return record;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a live_goal_formation_performed diagnostic must be recorded for the completed turn");

    assert!(matches!(
        performed,
        DiagnosticRecord::LiveGoalFormationPerformed {
            exchange_index: 0,
            ..
        }
    ));

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let formation_row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "goal_formation")
        .expect("goal formation call must be recorded in the token ledger");
    assert_eq!(formation_row.calls, 1);
    assert!(formation_row.counts.text_input + formation_row.counts.cached_input > 0);
}

#[tokio::test]
async fn response_done_accumulates_realtime_token_usage() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    start_test_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
    )
    .await;
    drain_outbound_texts(&mut outbound_rx);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-test",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-done",
            "response": {
                "id": "response-usage",
                "status": "completed",
                "output": [{
                    "content": [{
                        "type": "output_text",
                        "text": "hi"
                    }]
                }],
                "usage": {
                    "input_tokens": 900,
                    "output_tokens": 100,
                    "input_token_details": {
                        "text_tokens": 300,
                        "audio_tokens": 600,
                        "cached_tokens": 500,
                        "cached_tokens_details": { "text_tokens": 200, "audio_tokens": 300 }
                    },
                    "output_token_details": { "text_tokens": 20, "audio_tokens": 80 }
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("response done");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "realtime_voice")
        .expect("realtime response must be recorded in the token ledger");
    assert_eq!(row.model_id, guard.config.model);
    assert_eq!(row.calls, 1);
    assert_eq!(row.counts.text_input, 100);
    assert_eq!(row.counts.audio_input, 300);
    assert_eq!(row.counts.cached_input, 500);
    assert_eq!(row.counts.text_output, 20);
    assert_eq!(row.counts.audio_output, 80);
}

#[tokio::test]
async fn stale_response_done_records_token_usage_without_promoting() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    start_test_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
    )
    .await;
    drain_outbound_texts(&mut outbound_rx);

    // Mark the response id stale before its response.done arrives, as a barge-in
    // cancellation does.
    runtime_state
        .stale_response_ids
        .insert("response-stale".to_string());

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-test",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-done-stale",
            "response": {
                "id": "response-stale",
                "status": "cancelled",
                "output": [],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 2,
                    "input_token_details": {
                        "text_tokens": 10,
                        "cached_tokens": 4,
                        "cached_tokens_details": { "text_tokens": 4, "audio_tokens": 0 }
                    }
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("stale response done");

    // The stale early-return still ran: the event was diagnosed as stale and no
    // trusted exchange was promoted to continuity storage.
    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .any(|record| matches!(record, DiagnosticRecord::StaleProviderEvent { .. })),
        "the stale path must be the one exercised"
    );
    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    assert!(
        !continuity_dir.join("session-state.json").exists(),
        "a stale response must not promote an exchange"
    );

    // ...but the provider billed the call, so the ledger recorded it anyway.
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let row = guard
        .token_usage
        .models
        .iter()
        .find(|row| row.role == "realtime_voice")
        .expect("stale response must still be recorded in the token ledger");
    assert_eq!(row.calls, 1);
    assert_eq!(row.counts.text_input, 6);
    assert_eq!(row.counts.cached_input, 4);
    assert_eq!(row.counts.text_output, 2);
}

#[tokio::test]
async fn typed_turn_emits_user_item_before_context_and_response_create() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    handle_text_turn(
        &state,
        &allocation.qsf_session_id,
        "call-typed",
        "how can you help me with this task",
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("typed turn");

    let user_item = outbound_rx.recv().await.expect("user item");
    let user_payload: serde_json::Value =
        serde_json::from_str(user_item.to_text().expect("text")).expect("json");
    assert_eq!(user_payload["type"], "conversation.item.create");
    assert_eq!(user_payload["item"]["role"], "user");
    assert_eq!(
        user_payload["item"]["content"][0]["text"],
        "how can you help me with this task"
    );

    let volition_packet = outbound_rx.recv().await.expect("volition packet");
    assert!(
        volition_packet
            .to_text()
            .expect("text")
            .contains("Simulated volition context for this turn")
    );
    let response_create = outbound_rx.recv().await.expect("response.create");
    assert!(
        response_create
            .to_text()
            .expect("text")
            .contains("\"response.create\"")
    );
}

#[cfg(test)]
#[path = "sideband_lifecycle_tests.rs"]
mod sideband_lifecycle_tests;

#[cfg(test)]
#[path = "sideband_status_tests.rs"]
mod sideband_status_tests;
