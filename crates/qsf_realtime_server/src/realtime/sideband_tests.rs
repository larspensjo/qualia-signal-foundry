use std::fs;
use std::sync::{Arc as StdArc, Condvar, Mutex as StdMutex};
use std::time::Duration;

use qsf_context::ContextBudget;
use qsf_volition::VolitionEvent;
use tempfile::TempDir;
use tokio::sync::mpsc;

use qsf_session::{
    ExchangeModelUse, ExchangeOutput, ResumeMode, ToolExecutionStatus, ToolPermissionDecision,
};

use super::*;
use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};
use crate::realtime::sideband_exchange_promotion::promote_completed_trusted_exchanges;
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

async fn run_trusted_transcript_turn(
    state: &AppState,
    qsf_session_id: &str,
    runtime_state: &mut SidebandRuntimeState,
    outbound_tx: &mpsc::UnboundedSender<Message>,
    transcript: &str,
    call_id: &str,
) {
    handle_provider_event(
        state,
        qsf_session_id,
        call_id,
        "conversation.item.input_audio_transcription.completed",
        &serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "event_id": format!("evt-{call_id}"),
            "item_id": format!("item-{call_id}"),
            "transcript": transcript
        }),
        runtime_state,
        outbound_tx,
    )
    .await
    .expect("trusted transcript turn");
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

async fn set_volition_mode(state: &AppState, qsf_session_id: &str, mode: qsf_volition::Mode) {
    let runtime = state
        .session_runtime(qsf_session_id)
        .await
        .expect("runtime");
    let mut guard = runtime.lock().await;
    let tick = guard.volition.state.tick;
    guard
        .volition
        .apply_events(vec![VolitionEvent::ModeChanged { mode, tick }]);
}

#[test]
fn hash_request_sequence_is_deterministic() {
    let first = hash_request_sequence(&[serde_json::json!({"a": 1})]);
    let second = hash_request_sequence(&[serde_json::json!({"a": 1})]);
    assert_eq!(first, second);
}

#[tokio::test]
async fn trusted_selection_turn_publishes_volition_capture_and_cross_links_diagnostics() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let (turn_context_rx, volition_rx) = {
        let guard = runtime.lock().await;
        (
            guard.subscribe_turn_context(),
            guard.subscribe_volition_inspection(),
        )
    };

    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me",
        "selection-turn",
    )
    .await;

    let mut turn_context_rx = turn_context_rx;
    turn_context_rx
        .changed()
        .await
        .expect("turn context update");
    let mut volition_rx = volition_rx;
    volition_rx.changed().await.expect("volition update");

    let turn_context_capture = turn_context_rx
        .borrow()
        .clone()
        .expect("turn context capture");
    let volition_capture = volition_rx.borrow().clone().expect("volition capture");
    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("response.create")),
        "trusted selection turn must send response.create"
    );
    let decision = volition_capture
        .decision
        .as_ref()
        .expect("selection turn must include a decision");
    assert!(decision.protected_tier_active);
    assert_eq!(
        turn_context_capture.request_hash,
        volition_capture.response_create_event_ref
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let injected = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::VolitionContextInjected { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("volition context injected trace");
    assert_eq!(
        injected.response_create_event_ref,
        volition_capture.response_create_event_ref
    );
    let initiative = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("realtime bounded initiative trace");
    assert_eq!(
        initiative.response_create_event_ref,
        volition_capture.response_create_event_ref
    );
}

#[tokio::test]
async fn trusted_no_selection_turn_publishes_state_only_capture() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let (turn_context_rx, volition_rx) = {
        let guard = runtime.lock().await;
        (
            guard.subscribe_turn_context(),
            guard.subscribe_volition_inspection(),
        )
    };

    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "xyzzy frobnicator quux",
        "no-selection-turn",
    )
    .await;

    let mut turn_context_rx = turn_context_rx;
    turn_context_rx
        .changed()
        .await
        .expect("turn context update");
    let mut volition_rx = volition_rx;
    volition_rx.changed().await.expect("volition update");

    let turn_context_capture = turn_context_rx
        .borrow()
        .clone()
        .expect("turn context capture");
    let volition_capture = volition_rx.borrow().clone().expect("volition capture");
    assert!(
        volition_capture.decision.is_none(),
        "no-selection turn must publish a state-only capture"
    );
    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("response.create")),
        "trusted no-selection turn must still send response.create"
    );
    assert_eq!(
        turn_context_capture.request_hash,
        volition_capture.response_create_event_ref
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::VolitionContextInjected { .. })),
        "no-selection turn must not write a volition context-injected diagnostic"
    );
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. })),
        "no-selection turn must not write a bounded-initiative diagnostic"
    );
}

#[tokio::test]
async fn protected_winner_on_direct_request_records_but_does_not_surface_initiative() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me",
        "protected-direct",
    )
    .await;

    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative:")),
        "direct protected request should not surface initiative"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("initiative trace");
    assert!(
        !initiative_trace
            .bounded_or_external_output
            .external_effect_executed
    );
    assert!(initiative_trace.context_retrieval_hint_terms.is_none());
    assert!(!initiative_trace.response_create_event_ref.is_empty());
}

#[tokio::test]
async fn protected_direct_request_suppresses_surfaced_initiative_under_all_modes() {
    let modes = [
        qsf_volition::Mode::Neutral,
        qsf_volition::Mode::Focused,
        qsf_volition::Mode::Exploratory,
    ];

    for mode in modes {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        set_volition_mode(&state, &allocation.qsf_session_id, mode).await;
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            "how can you help me",
            &format!("protected-direct-{mode}"),
        )
        .await;

        let outbound_texts = drain_outbound_texts(&mut outbound_rx);
        assert!(
            outbound_texts
                .iter()
                .all(|text| !text.contains("Bounded initiative:")),
            "ordinary protected request should not surface initiative under {mode}"
        );

        let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
        let initiative_trace = records
            .iter()
            .find_map(|record| match record {
                DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
                _ => None,
            })
            .expect("initiative trace");
        assert_eq!(
            initiative_trace.winning_goal_id,
            "honor-explicit-user-request"
        );
    }
}

#[tokio::test]
async fn curiosity_terms_do_not_surface_curiosity_initiative_when_protected_goal_is_present() {
    let modes = [
        qsf_volition::Mode::Neutral,
        qsf_volition::Mode::Focused,
        qsf_volition::Mode::Exploratory,
    ];

    for mode in modes {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let allocation = state.create_session().await.expect("session");
        set_volition_mode(&state, &allocation.qsf_session_id, mode).await;
        let mut runtime_state = SidebandRuntimeState::default();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            "how can you help me compare the memory evidence",
            &format!("curiosity-suppression-{mode}"),
        )
        .await;

        let outbound_texts = drain_outbound_texts(&mut outbound_rx);
        assert!(
            outbound_texts
                .iter()
                .all(|text| !text.contains("Clarify weak evidence topic")),
            "curiosity initiative should not be the surfaced line under {mode}"
        );

        let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
        let initiative_trace = records
            .iter()
            .find_map(|record| match record {
                DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
                _ => None,
            })
            .expect("initiative trace");
        assert_eq!(
            initiative_trace.winning_goal_id,
            "honor-explicit-user-request"
        );
    }
}

#[tokio::test]
async fn protected_winner_with_genuine_opportunity_surfaces_initiative() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me? I'm uncertain about the request.",
        "protected-surface",
    )
    .await;

    let outbound_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        outbound_texts
            .iter()
            .any(|text| text.contains("Bounded initiative:")),
        "protected winner with an uncertainty signal should surface initiative"
    );
    assert!(
        outbound_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative: Bounded initiative:")),
        "initiative prefix should be rendered only once"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative { trace, .. } => Some(trace),
            _ => None,
        })
        .expect("initiative trace");
    assert!(
        !initiative_trace
            .bounded_or_external_output
            .external_effect_executed
    );
    assert!(initiative_trace.context_retrieval_hint_terms.is_none());
}

#[tokio::test]
async fn context_retrieval_hints_round_trip_into_the_next_turn() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "continuity thread",
        "hint-source",
    )
    .await;
    let first_turn_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        first_turn_texts
            .iter()
            .all(|text| !text.contains("Bounded initiative:")),
        "context retrieval should not surface a model-facing initiative line"
    );

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "how can you help me? I'm uncertain about the request.",
        "hint-consumer",
    )
    .await;
    let second_turn_texts = drain_outbound_texts(&mut outbound_rx);
    assert!(
        second_turn_texts
            .iter()
            .any(|text| text.contains("Bounded initiative:")),
        "the next turn should be able to surface a bounded initiative"
    );
    assert!(
        runtime_state.pending_context_retrieval_hints.is_empty(),
        "pending retrieval hints must be consumed on the next turn"
    );

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let first_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } if *exchange_index == 0 => Some(trace),
            _ => None,
        })
        .expect("first initiative trace");
    assert!(first_trace.context_retrieval_hint_terms.is_some());
    assert!(!first_trace.hint_consumed_by_next_memory_injection);

    let second_trace = records
        .iter()
        .find_map(|record| match record {
            DiagnosticRecord::RealtimeBoundedInitiative {
                exchange_index,
                trace,
                ..
            } if *exchange_index == 1 => Some(trace),
            _ => None,
        })
        .expect("second initiative trace");
    assert!(second_trace.hint_consumed_by_next_memory_injection);
}

#[tokio::test]
async fn repeated_surfaceable_winner_alternates_surface_and_suppression() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
    let transcript = "how can you help me? I'm uncertain about the request.";

    let mut surfaced = Vec::new();
    for turn in 0..3 {
        run_trusted_transcript_turn(
            &state,
            &allocation.qsf_session_id,
            &mut runtime_state,
            &outbound_tx,
            transcript,
            &format!("nag-{turn}"),
        )
        .await;
        let texts = drain_outbound_texts(&mut outbound_rx);
        surfaced.push(
            texts
                .iter()
                .any(|text| text.contains("Bounded initiative:")),
        );
    }

    assert_eq!(surfaced, vec![true, false, true]);

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let initiative_count = records
        .iter()
        .filter(|record| matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. }))
        .count();
    assert_eq!(initiative_count, 3);
}

#[tokio::test]
async fn tool_loop_continuation_does_not_emit_bounded_initiative_record() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();

    run_trusted_transcript_turn(
        &state,
        &allocation.qsf_session_id,
        &mut runtime_state,
        &outbound_tx,
        "xyzzy frobnicator quux",
        "tool-loop-source",
    )
    .await;
    let _ = drain_outbound_texts(&mut outbound_rx);

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "tool-loop-continuation",
        "response.done",
        &function_call_response_done(
            "evt-tool-loop",
            "response-tool-loop",
            "completed",
            "tool-call-loop",
            "inspect_session_state",
            "{}",
        ),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("tool loop continuation");

    let _ = drain_outbound_texts(&mut outbound_rx);
    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, DiagnosticRecord::RealtimeBoundedInitiative { .. }))
    );
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
async fn live_loop_latency_observations_record_each_stage_once() {
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
        "call-latency",
        "response.created",
        &serde_json::json!({
            "type": "response.created",
            "event_id": "evt-response-created",
            "response": {
                "id": "response-latency",
                "status": "in_progress"
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("response.created");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-latency",
        "response.audio.delta",
        &serde_json::json!({
            "type": "response.audio.delta",
            "event_id": "evt-first-audio",
            "response": {
                "id": "response-latency",
                "status": "in_progress"
            },
            "delta": "AA=="
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("first audio");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        "call-latency",
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-response-done",
            "response": {
                "id": "response-latency",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "latency answer"
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
    .expect("response done");

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let latency_labels = records
        .iter()
        .filter_map(|record| match record {
            DiagnosticRecord::LatencyObservation { label, .. } => Some(label.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        latency_labels,
        vec![
            "final_transcript_received_to_memory_injected",
            "memory_injected_to_response_create_sent",
            "response_create_sent_to_response_created",
            "response_created_to_first_audio",
            "final_transcript_received_to_first_audio",
        ]
    );
    assert!(records.iter().all(|record| {
        !serde_json::to_string(record)
            .expect("diagnostic json")
            .contains("test-api-key")
    }));
    for record in records {
        if let DiagnosticRecord::LatencyObservation { latency_ms, .. } = record {
            assert!(latency_ms >= 0);
        }
    }
}

#[tokio::test]
async fn interrupted_exchange_is_persisted_as_a_trusted_diagnostic() {
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
        "call-interrupt",
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

    outbound_rx.recv().await.expect("response.cancel");
    outbound_rx.recv().await.expect("fresh response.create");

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let interrupted = records
        .iter()
        .find(|record| {
            if let DiagnosticRecord::DiagnosticExchangeRecorded { source, trust, .. } = record {
                source == "sideband_interruption" && *trust == DiagnosticTrust::Trusted
            } else {
                false
            }
        })
        .expect("interrupted exchange diagnostic");

    match interrupted {
        DiagnosticRecord::DiagnosticExchangeRecorded {
            exchange,
            source,
            trust,
            ..
        } => {
            assert_eq!(source, "sideband_interruption");
            assert_eq!(*trust, DiagnosticTrust::Trusted);
            assert_eq!(exchange.status, qsf_session::ExchangeStatus::Interrupted);
            let interrupted_json = serde_json::to_string(interrupted).expect("diagnostic json");
            assert!(!interrupted_json.contains("test-api-key"));
        }
        _ => unreachable!(),
    }

    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let guard = runtime.lock().await;
    assert!(guard.session_state.turns.is_empty());
    assert_eq!(guard.session_state.live.completed_exchanges.len(), 1);
    assert_eq!(
        guard.session_state.live.completed_exchanges[0].status,
        qsf_session::ExchangeStatus::Interrupted
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

#[tokio::test]
async fn set_sideband_status_notifies_subscribers() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let mut status_rx = runtime.lock().await.subscribe_status();
    assert!(!status_rx.borrow().degraded);

    runtime
        .lock()
        .await
        .set_sideband_status(true, Some("boom".to_string()));

    status_rx.changed().await.expect("status changed");
    let status = status_rx.borrow().clone();
    assert!(status.degraded);
    assert_eq!(status.detail.as_deref(), Some("boom"));
    assert!(runtime.lock().await.degraded);
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
            tool_requests: vec![],
            tool_executions: vec![],
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

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    let trusted_completion = records
        .iter()
        .find(|record| {
            if let DiagnosticRecord::DiagnosticExchangeRecorded { source, trust, .. } = record {
                source == "sideband_trusted" && *trust == DiagnosticTrust::Trusted
            } else {
                false
            }
        })
        .expect("trusted completion diagnostic");
    match trusted_completion {
        DiagnosticRecord::DiagnosticExchangeRecorded {
            exchange,
            source,
            trust,
            ..
        } => {
            assert_eq!(source, "sideband_trusted");
            assert_eq!(*trust, DiagnosticTrust::Trusted);
            assert_eq!(exchange.index, 0);
            assert_eq!(exchange.status, qsf_session::ExchangeStatus::Completed);
            assert_eq!(
                exchange.output.as_ref().map(|output| output.text.as_str()),
                Some("hi")
            );
        }
        _ => unreachable!(),
    }
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

    let records = diagnostic_records(&state, &allocation.qsf_session_id).await;
    assert!(records.iter().all(|record| {
            !matches!(
                record,
                DiagnosticRecord::DiagnosticExchangeRecorded { source, .. } if source == "sideband_trusted"
            )
        }));
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
        tool_requests: vec![],
        tool_executions: vec![],
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
        status: qsf_session::ExchangeStatus::Completed,
    }
}
