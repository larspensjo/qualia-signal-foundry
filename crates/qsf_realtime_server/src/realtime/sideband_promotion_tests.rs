use tempfile::TempDir;

use base64::Engine;
use qsf_realtime_protocol::OPENAI_REALTIME_VOICE_MODEL;
use qsf_session::{ExchangeModelUse, ExchangeOutput, ResumeMode};

use super::*;
use crate::RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES;
use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust};
use crate::realtime::sideband_exchange_promotion::promote_completed_trusted_exchanges;

#[tokio::test]
async fn promoted_session_state_omits_raw_audio_payloads_but_preserves_transcript_deltas() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let mut runtime_state = SidebandRuntimeState::default();
    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel();
    let payload = base64::engine::general_purpose::STANDARD.encode(b"synthetic audio payload");

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
        &browser_call("call-audio-artifact"),
        "response.created",
        &serde_json::json!({
            "type": "response.created",
            "event_id": "evt-response-created",
            "response": { "id": "response-audio-artifact", "status": "in_progress" }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("response created");

    let attachments = [
        (
            "browser-call",
            SidebandAttachment::BrowserCall {
                call_id: "call-audio-artifact".to_string(),
            },
        ),
        (
            "server-model-session",
            SidebandAttachment::ServerModelSession {
                model: "test-model".to_string(),
            },
        ),
    ];
    let mut raw_audio_event_ids = Vec::new();
    for event_type in RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES {
        for (attachment_name, attachment) in &attachments {
            let event_id = format!("evt-{attachment_name}-{}", event_type.replace('.', "-"));
            handle_provider_event(
                &state,
                &allocation.qsf_session_id,
                attachment,
                event_type,
                &serde_json::json!({
                    "type": event_type,
                    "event_id": event_id,
                    "response": { "id": "response-audio-artifact", "status": "in_progress" },
                    "delta": payload.clone()
                }),
                &mut runtime_state,
                &outbound_tx,
            )
            .await
            .expect("raw audio delta");
            raw_audio_event_ids.push(event_id);
        }
    }

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        &browser_call("call-audio-artifact"),
        "response.output_audio_transcript.delta",
        &serde_json::json!({
            "type": "response.output_audio_transcript.delta",
            "event_id": "evt-audio-transcript",
            "response": { "id": "response-audio-artifact", "status": "in_progress" },
            "delta": "preserved transcript delta"
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("transcript delta");

    handle_provider_event(
        &state,
        &allocation.qsf_session_id,
        &browser_call("call-audio-artifact"),
        "response.done",
        &serde_json::json!({
            "type": "response.done",
            "event_id": "evt-response-done",
            "response": {
                "id": "response-audio-artifact",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{ "type": "output_text", "text": "completed answer" }]
                }],
                "usage": { "input_tokens": 1, "output_tokens": 1 }
            }
        }),
        &mut runtime_state,
        &outbound_tx,
    )
    .await
    .expect("response done");

    let continuity_dir = state.continuity_session_dir(&allocation.qsf_session_id);
    let session_state_path = continuity_dir.join("session-state.json");
    let persisted_json = std::fs::read_to_string(&session_state_path).expect("session state json");
    let persisted = qsf_session::load_session_state(session_state_path).expect("session state");
    let provider_events = &persisted.exchanges[0].provider_events;

    assert!(!persisted_json.contains(&payload));
    assert!(provider_events.iter().all(|provider_event| {
        !raw_audio_event_ids
            .iter()
            .any(|event_id| provider_event.event_id.as_ref() == Some(event_id))
            && provider_event.text.as_deref() != Some(payload.as_str())
    }));
    assert!(provider_events.iter().any(|provider_event| {
        provider_event.event_id.as_deref() == Some("evt-audio-transcript")
            && provider_event.text.as_deref() == Some("preserved transcript delta")
    }));
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
                model_id: OPENAI_REALTIME_VOICE_MODEL.to_string(),
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
        guard.set_sideband_status(false, None);
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
        guard.set_sideband_status(true, Some("test degradation".to_string()));
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
async fn promotion_publishes_one_completion_for_each_consumed_exchange() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let mut completion_rx = runtime.lock().await.subscribe_trusted_turn_completion();

    {
        let mut guard = runtime.lock().await;
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(0, "promote", "answer"));
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("promotion");
    }
    completion_rx.changed().await.expect("completion published");
    let completion = completion_rx.borrow().clone().expect("completion value");
    assert_eq!(completion.exchange_index, 0);
    assert!(completion.promoted);
    assert_eq!(completion.promoted_turn_count, 1);
    assert!(completion.skipped_reason.is_none());

    {
        let mut guard = runtime.lock().await;
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(1, "skip", "answer"));
        guard.non_promotable_exchange_indices.insert(1);
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("promotion");
    }
    completion_rx.changed().await.expect("skip published");
    let completion = completion_rx.borrow().clone().expect("completion value");
    assert_eq!(completion.exchange_index, 1);
    assert!(!completion.promoted);
    assert_eq!(
        completion.skipped_reason.as_deref(),
        Some("non_promotable_exchange")
    );
}

#[tokio::test]
async fn degraded_and_conversion_failure_exchanges_publish_completion_reasons() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let mut completion_rx = runtime.lock().await.subscribe_trusted_turn_completion();

    {
        let mut guard = runtime.lock().await;
        guard.set_sideband_status(true, Some("test degradation".to_string()));
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(0, "degraded", "answer"));
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("promotion");
    }
    completion_rx
        .changed()
        .await
        .expect("first degraded completion");
    assert_eq!(
        completion_rx
            .borrow()
            .as_ref()
            .and_then(|completion| completion.skipped_reason.as_deref()),
        Some("sideband_degraded")
    );

    {
        let mut guard = runtime.lock().await;
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(1, "also degraded", "answer"));
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("second promotion");
    }
    completion_rx
        .changed()
        .await
        .expect("second degraded completion");
    assert_eq!(
        completion_rx
            .borrow()
            .as_ref()
            .map(|value| value.exchange_index),
        Some(1)
    );

    {
        let mut guard = runtime.lock().await;
        guard.set_sideband_status(false, None);
        guard.session_state.live.completed_exchanges.push(
            Exchange::new_text(2, "missing durable fields", SystemTime::UNIX_EPOCH)
                .completed(SystemTime::UNIX_EPOCH),
        );
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("conversion failure is consumed");
    }
    completion_rx
        .changed()
        .await
        .expect("conversion failure published");
    let completion = completion_rx.borrow().clone().expect("completion value");
    assert_eq!(completion.exchange_index, 2);
    assert!(!completion.promoted);
    assert_eq!(
        completion.skipped_reason.as_deref(),
        Some("turn_conversion_failed")
    );
}

#[tokio::test]
async fn late_completion_subscriber_observes_latest_value() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    {
        let mut guard = runtime.lock().await;
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(0, "late subscriber", "answer"));
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect("promotion");
    }
    let completion = runtime
        .lock()
        .await
        .subscribe_trusted_turn_completion()
        .borrow()
        .clone()
        .expect("latest completion");
    assert_eq!(completion.exchange_index, 0);
    assert!(completion.promoted);
}

#[tokio::test]
async fn continuity_failure_reports_the_pre_reduction_promoted_turn_count() {
    let tempdir = TempDir::new().expect("tempdir");
    let state = state(&tempdir);
    let allocation = state.create_session().await.expect("session");
    let runtime = state
        .session_runtime(&allocation.qsf_session_id)
        .await
        .expect("runtime");
    let completion_rx = runtime.lock().await.subscribe_trusted_turn_completion();
    std::fs::create_dir_all(state.continuity_manifest_path(&allocation.qsf_session_id))
        .expect("manifest path directory");

    let error = {
        let mut guard = runtime.lock().await;
        guard
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(0, "persist", "failure"));
        promote_completed_trusted_exchanges(&state, &mut guard)
            .await
            .expect_err("manifest persistence should fail")
    };
    assert!(
        error.to_string().contains("continuity")
            || error.to_string().contains("directory")
            || error.to_string().contains("manifest")
    );

    let guard = runtime.lock().await;
    assert_eq!(guard.session_state.turns.len(), 1);
    let completion = completion_rx.borrow().clone().expect("completion value");
    assert!(!completion.promoted);
    assert_eq!(completion.promoted_turn_count, 0);
    assert_eq!(
        completion.skipped_reason.as_deref(),
        Some("continuity_persistence_failed")
    );
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
        guard.set_sideband_status(false, None);

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
            model_id: OPENAI_REALTIME_VOICE_MODEL.to_string(),
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
