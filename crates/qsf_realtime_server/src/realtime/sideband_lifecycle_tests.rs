use tempfile::TempDir;
use tokio::sync::mpsc;

use super::*;
use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};

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
