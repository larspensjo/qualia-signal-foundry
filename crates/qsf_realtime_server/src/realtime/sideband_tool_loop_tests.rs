use std::sync::{Arc as StdArc, Condvar, Mutex as StdMutex};
use std::time::Duration;

use tempfile::TempDir;
use tokio::sync::mpsc;

use qsf_session::{ToolExecutionStatus, ToolPermissionDecision};

use super::*;
use crate::diagnostics::DiagnosticRecord;
use crate::realtime::sideband_provider_event::handle_provider_event;

fn assert_promoted_turn_response_is_owned(
    state: &qsf_session::SessionState,
    user_input: &str,
    response_id: &str,
) {
    let turn = state
        .turns
        .iter()
        .find(|turn| turn.user_input == user_input)
        .expect("promoted turn");
    let exchange = state
        .exchanges
        .iter()
        .find(|exchange| exchange.index == turn.index)
        .expect("promoted exchange for turn");
    assert_eq!(exchange.final_user_input(), turn.user_input);
    assert_eq!(
        exchange
            .output
            .as_ref()
            .and_then(|output| output.response_id.as_deref()),
        Some(response_id)
    );
    assert!(
        exchange
            .provider_events
            .iter()
            .all(|provider_event| provider_event.exchange_index == exchange.index)
    );
    assert!(
        exchange
            .provider_events
            .iter()
            .any(|provider_event| provider_event.response_id.as_deref() == Some(response_id))
    );
}

#[derive(Clone)]
struct BlockingTool {
    started: StdArc<(StdMutex<bool>, Condvar)>,
    release: StdArc<(StdMutex<bool>, Condvar)>,
}

impl qsf_tools::Tool for BlockingTool {
    fn metadata(&self) -> qsf_tools::ToolMetadata {
        qsf_tools::ToolMetadata {
            name: "blocking_tool".to_string(),
            description: "Blocks until the test releases it.".to_string(),
            category: qsf_session::ToolCategory::ReadOnly,
            side_effect_level: qsf_session::ToolSideEffectLevel::ReadOnly,
        }
    }

    fn execute(
        &self,
        request: &qsf_tools::ToolRequest,
        _ctx: &dyn qsf_tools::ToolContext,
    ) -> anyhow::Result<qsf_tools::ToolResult> {
        set_flag(&self.started);
        let (lock, cvar) = &*self.release;
        let mut released = lock.lock().expect("release lock");
        while !*released {
            released = cvar.wait(released).expect("release wait");
        }
        Ok(qsf_tools::ToolResult {
            tool_name: request.tool_name.clone(),
            category: qsf_session::ToolCategory::ReadOnly,
            side_effect_level: qsf_session::ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: "{}".to_string(),
            numeric_value: None,
            observation_summary: "blocking tool completed".to_string(),
        })
    }
}

fn wait_for_flag(flag: &StdArc<(StdMutex<bool>, Condvar)>) {
    let (lock, cvar) = &**flag;
    let mut value = lock.lock().expect("flag lock");
    while !*value {
        value = cvar.wait(value).expect("flag wait");
    }
}

fn set_flag(flag: &StdArc<(StdMutex<bool>, Condvar)>) {
    let (lock, cvar) = &**flag;
    let mut value = lock.lock().expect("flag lock");
    *value = true;
    cvar.notify_one();
}

#[tokio::test]
async fn continuation_noise_transcript_is_ignored_until_the_response_completes() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &function_call_response_done(
            "evt-tool-call",
            "response-tool-call",
            "completed",
            "tool-call-1",
            crate::realtime::tools::SEARCH_MEMORY_TOOL_NAME,
            r#"{"query":"Pineapple Radar"}"#,
        ),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    outbound_rx.recv().await.expect("function_call_output");
    outbound_rx
        .recv()
        .await
        .expect("continuation response.create");
    assert_eq!(runtime_state.turn_phase, TurnPhase::ToolLoop);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-noise",
            "item_id": "item-noise",
            "transcript": "Thank you."
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("ignored continuation transcript");

    assert!(outbound_rx.try_recv().is_err());

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let ignored = records
        .iter()
        .find(|record| {
            matches!(
                record,
                DiagnosticRecord::IgnoredContinuationTranscript { .. }
            )
        })
        .expect("ignored transcript diagnostic");
    match ignored {
        DiagnosticRecord::IgnoredContinuationTranscript {
            transcript,
            turn_phase,
            response_id,
            ..
        } => {
            assert_eq!(transcript, "Thank you.");
            assert_eq!(*turn_phase, TurnPhase::ToolLoop);
            assert!(response_id.is_none());
        }
        _ => unreachable!(),
    }

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-tool-final",
            "response": {
                "id": "response-tool-final",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "memory answer"
                    }]
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("continuation completion");

    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    let persisted = qsf_session::load_session_state(continuity_dir.join("session-state.json"))
        .expect("persisted state");
    assert_eq!(persisted.turns.len(), 1);
    assert_eq!(persisted.turns[0].user_input, "hello tool loop");
    assert_promoted_turn_response_is_owned(&persisted, "hello tool loop", "response-tool-final");
    assert!(
        persisted
            .turns
            .iter()
            .all(|turn| turn.user_input != "Thank you.")
    );
}

#[tokio::test]
async fn cancelled_continuation_finalizes_the_active_exchange_before_the_next_turn() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &function_call_response_done(
            "evt-tool-call",
            "response-tool-call",
            "completed",
            "tool-call-1",
            crate::realtime::tools::SEARCH_MEMORY_TOOL_NAME,
            r#"{"query":"Pineapple Radar"}"#,
        ),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    outbound_rx.recv().await.expect("function_call_output");
    outbound_rx
        .recv()
        .await
        .expect("continuation response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-noise",
            "item_id": "item-noise",
            "transcript": "Thank you."
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("ignored continuation transcript");

    let canceled_exchange_index = runtime_state
        .active_exchange_index
        .expect("active exchange before cancel");
    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &function_call_response_done(
            "evt-tool-cancelled",
            "response-tool-cancelled",
            "cancelled",
            "tool-call-2",
            crate::realtime::tools::SEARCH_MEMORY_TOOL_NAME,
            r#"{"query":"Pineapple Radar"}"#,
        ),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("cancelled continuation");

    assert!(runtime_state.active_exchange_index.is_none());
    assert!(runtime_state.pending_response_exchange.is_none());
    assert_eq!(runtime_state.turn_phase, TurnPhase::Idle);
    assert!(runtime_state.current_request_hash.is_none());
    assert_eq!(runtime_state.current_message_count, 0);
    assert_eq!(runtime_state.tool_calls_in_turn, 0);
    assert_eq!(runtime_state.accumulated_latency_ms, 0);
    assert_eq!(runtime_state.accumulated_input_tokens, 0);
    assert_eq!(runtime_state.accumulated_cached_input_tokens, 0);
    assert_eq!(runtime_state.accumulated_output_tokens, 0);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-empty-after-cancel",
            "item_id": "item-empty-after-cancel",
            "transcript": ""
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("empty post-cancel transcript");

    assert!(outbound_rx.try_recv().is_err());
    assert!(runtime_state.active_exchange_index.is_none());
    assert!(runtime_state.pending_response_exchange.is_none());
    assert_eq!(runtime_state.turn_phase, TurnPhase::Idle);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-fresh",
            "item_id": "item-fresh",
            "transcript": "thanks"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("fresh idle transcript");

    let fresh_response_create = outbound_rx.recv().await.expect("fresh response.create");
    assert!(
        fresh_response_create
            .to_text()
            .expect("text")
            .contains("\"response.create\"")
    );
    let fresh_exchange_index = runtime_state
        .active_exchange_index
        .expect("fresh active exchange");
    assert_ne!(fresh_exchange_index, canceled_exchange_index);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-fresh-done",
            "response": {
                "id": "response-fresh",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "fresh answer"
                    }]
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("fresh completion");

    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    let persisted = qsf_session::load_session_state(continuity_dir.join("session-state.json"))
        .expect("persisted state");
    assert_eq!(persisted.turns.len(), 1);
    assert_eq!(persisted.turns[0].user_input, "thanks");
    assert_promoted_turn_response_is_owned(&persisted, "thanks", "response-fresh");
    assert!(
        persisted
            .turns
            .iter()
            .all(|turn| turn.user_input != "Thank you.")
    );
}

#[tokio::test]
async fn stale_response_events_are_audited_without_mutating_the_fresh_exchange() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.created",
        &serde_json::json!({
            "type": "response.created",
            "event_id": "evt-created-old",
            "response": {
                "id": "response-old",
                "status": "in_progress"
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("old response.created");
    assert_eq!(runtime_state.response_id.as_deref(), Some("response-old"));
    let stale_request_hash = ContentHash([42; 32]);
    runtime_state.current_request_hash = Some(stale_request_hash);
    runtime_state.tool_calls_in_turn = 7;

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": "evt-interrupt",
            "item_id": "item-interrupt",
            "transcript": "stop"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("interrupting transcript");

    let cancel = outbound_rx.recv().await.expect("response.cancel");
    assert!(
        cancel
            .to_text()
            .expect("text")
            .contains("\"response.cancel\"")
    );
    let fresh_response_create = outbound_rx.recv().await.expect("fresh response.create");
    assert!(
        fresh_response_create
            .to_text()
            .expect("text")
            .contains("\"response.create\"")
    );

    let stale_response_id = "response-old";
    assert!(runtime_state.stale_response_ids.contains(stale_response_id));
    let fresh_exchange_index = runtime_state
        .active_exchange_index
        .expect("fresh active exchange");
    let fresh_request_hash = runtime_state
        .current_request_hash
        .expect("fresh request hash");
    assert_ne!(fresh_request_hash, stale_request_hash);
    assert_eq!(runtime_state.tool_calls_in_turn, 0);
    assert!(runtime_state.current_message_count > 0);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.created",
        &serde_json::json!({
            "type": "response.created",
            "event_id": "evt-created-stale",
            "response": {
                "id": stale_response_id,
                "status": "in_progress"
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("stale response.created");
    assert!(runtime_state.response_id.is_none());

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-stale-done",
            "response": {
                "id": stale_response_id,
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "stale answer"
                    }]
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("stale response.done");

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let stale_record = records
        .iter()
        .find(|record| matches!(record, DiagnosticRecord::StaleProviderEvent { .. }))
        .expect("stale provider diagnostic");
    match stale_record {
        DiagnosticRecord::StaleProviderEvent {
            response_id,
            status,
            exchange_index,
            ..
        } => {
            assert_eq!(response_id.as_deref(), Some(stale_response_id));
            assert_eq!(status.as_deref(), Some("completed"));
            assert_eq!(*exchange_index, Some(fresh_exchange_index));
        }
        _ => unreachable!(),
    }

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let exchange = guard
        .session_state
        .live
        .active_exchange
        .as_ref()
        .expect("fresh active exchange");
    assert!(exchange.provider_events.is_empty());
    assert!(guard.session_state.turns.is_empty());
    assert_eq!(runtime_state.turn_phase, TurnPhase::AwaitingResponse);
    assert_eq!(
        runtime_state.pending_response_exchange,
        Some(fresh_exchange_index)
    );
    drop(guard);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-fresh-done",
            "response": {
                "id": "response-fresh",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "fresh answer"
                    }]
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("fresh response.done");

    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    let persisted = qsf_session::load_session_state(continuity_dir.join("session-state.json"))
        .expect("persisted state");
    assert_eq!(persisted.turns.len(), 1);
    assert_eq!(persisted.turns[0].user_input, "stop");
    assert_eq!(persisted.turns[0].assistant_response, "fresh answer");
    assert_promoted_turn_response_is_owned(&persisted, "stop", "response-fresh");
    assert!(
        persisted
            .turns
            .iter()
            .all(|turn| turn.assistant_response != "stale answer")
    );
}

#[tokio::test]
async fn malformed_function_call_arguments_recover_with_denial_output() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-tool-malformed",
            "response": {
                "id": "response-tool",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "name": crate::realtime::tools::INSPECT_SESSION_STATE_TOOL_NAME,
                    "call_id": "tool-call-1",
                    "arguments": "{not-json"
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    let output = outbound_rx.recv().await.expect("function_call_output");
    assert!(output.to_text().expect("text").contains("denied"));
    let response_create = outbound_rx.recv().await.expect("response.create");
    assert!(
        response_create
            .to_text()
            .expect("text")
            .contains("\"response.create\"")
    );

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let exchange = guard
        .session_state
        .live
        .active_exchange
        .as_ref()
        .expect("active exchange");
    assert_eq!(exchange.tool_requests.len(), 1);
    assert_eq!(exchange.tool_executions.len(), 1);
    assert_eq!(
        exchange.tool_executions[0].status,
        ToolExecutionStatus::Failed
    );
    assert!(matches!(
        exchange.tool_executions[0].permission_decision,
        ToolPermissionDecision::Denied { .. }
    ));
}

#[tokio::test]
async fn loop_cap_forces_next_response_to_disable_tools() {
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
    outbound_rx.recv().await.expect("initial response.create");
    runtime_state.tool_calls_in_turn = 3;

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-tool-cap",
            "response": {
                "id": "response-tool",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "name": crate::realtime::tools::INSPECT_SESSION_STATE_TOOL_NAME,
                    "call_id": "tool-call-cap",
                    "arguments": "{}"
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    let output = outbound_rx.recv().await.expect("function_call_output");
    assert!(
        output
            .to_text()
            .expect("text")
            .contains("tool loop cap reached")
    );
    let response_create = outbound_rx.recv().await.expect("response.create");
    let payload: serde_json::Value =
        serde_json::from_str(response_create.to_text().expect("text")).expect("json");
    assert_eq!(payload["response"]["tool_choice"], "none");
}

#[tokio::test]
async fn non_allow_listed_tool_call_is_denied_and_recorded() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-tool-denied",
            "response": {
                "id": "response-tool",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "name": "not_allow_listed",
                    "call_id": "tool-call-denied",
                    "arguments": "{}"
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    let output = outbound_rx.recv().await.expect("function_call_output");
    let output_text = output.to_text().expect("text");
    assert!(output_text.contains("denied"));
    assert!(output_text.contains("not allow-listed"));

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    let exchange = guard
        .session_state
        .live
        .active_exchange
        .as_ref()
        .expect("active exchange");
    assert_eq!(exchange.tool_requests.len(), 1);
    assert_eq!(exchange.tool_executions.len(), 1);
    assert_eq!(exchange.tool_executions[0].tool_name, "not_allow_listed");
    assert!(matches!(
        exchange.tool_executions[0].permission_decision,
        ToolPermissionDecision::Denied { .. }
    ));
}

#[tokio::test]
async fn mixed_response_done_answers_function_call_without_finalizing_exchange() {
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
    outbound_rx.recv().await.expect("initial response.create");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-tools",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-tool-mixed",
            "response": {
                "id": "response-tool",
                "status": "completed",
                "output": [
                    {
                        "type": "function_call",
                        "name": crate::realtime::tools::INSPECT_SESSION_STATE_TOOL_NAME,
                        "call_id": "tool-call-mixed",
                        "arguments": "{}"
                    },
                    {
                        "type": "message",
                        "content": [{
                            "type": "output_text",
                            "text": "spoken too early"
                        }]
                    }
                ],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1
                }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool response done");

    let output = outbound_rx.recv().await.expect("function_call_output");
    assert!(
        output
            .to_text()
            .expect("text")
            .contains("function_call_output")
    );
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    assert!(guard.session_state.live.active_exchange.is_some());
    assert!(guard.session_state.turns.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_execution_does_not_hold_session_lock() {
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
    outbound_rx.recv().await.expect("initial response.create");

    let started = StdArc::new((StdMutex::new(false), Condvar::new()));
    let release = StdArc::new((StdMutex::new(false), Condvar::new()));
    {
        let runtime = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("runtime");
        let mut guard = runtime.lock().await;
        guard.config.tools = vec![qsf_realtime_protocol::RealtimeToolDefinition::function(
            "blocking_tool",
            "Blocks until the test releases it.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        )];
        let mut registry = qsf_tools::ToolRegistry::default();
        registry.register(BlockingTool {
            started: started.clone(),
            release: release.clone(),
        });
        guard.tool_registry = registry;
    }

    let task_state = state.clone();
    let task_session_id = allocation.qsf_session_id.clone();
    let task_outbound_tx = outbound_tx.clone();
    let task = tokio::spawn(async move {
        handle_provider_event(
            &task_state,
            &task_session_id,
            "call-tools",
            "response.done",
            &serde_json::json!({
                "type": "response.done",
                "event_id": "evt-tool-lock",
                "response": {
                    "id": "response-tool",
                    "status": "completed",
                    "output": [{
                        "type": "function_call",
                        "name": "blocking_tool",
                        "call_id": "tool-call-lock",
                        "arguments": "{}"
                    }],
                    "usage": {
                        "input_tokens": 1,
                        "output_tokens": 1
                    }
                }
            }),
            &mut runtime_state,
            &task_outbound_tx,
        )
        .await
    });

    wait_for_flag(&started);
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let lock_result = tokio::time::timeout(Duration::from_millis(250), runtime.lock()).await;
    assert!(
        lock_result.is_ok(),
        "session lock stayed held during tool execution"
    );
    drop(lock_result);
    set_flag(&release);

    task.await.expect("task join").expect("tool response done");
}
