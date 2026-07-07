use std::time::SystemTime;

use super::*;
use crate::context::{ContextAssembly, ContextBudget};
use crate::exchange::{
    Exchange, ExchangeInput, ExchangeOutput, ExchangeRange, ExchangeStatus, InterruptionAction,
    InterruptionRecord, InterruptionStopOutcome, ProviderEventKind, ProviderEventRecord,
    ToolRequestRecord, UtteranceRecord,
};
use crate::state::TurnSummary;
use qsf_memory::processed_range::ProcessedRangeKind;

#[allow(clippy::too_many_arguments)]
fn provider_event_record(
    exchange_index: usize,
    event_kind: ProviderEventKind,
    provider_id: impl Into<String>,
    received_at: SystemTime,
    response_id: Option<&str>,
    text: Option<&str>,
    status: Option<&str>,
    audio_marker: Option<&str>,
) -> ProviderEventRecord {
    ProviderEventRecord {
        exchange_index,
        event_kind,
        provider_id: provider_id.into(),
        received_at,
        call_id: None,
        event_id: None,
        item_id: None,
        previous_item_id: None,
        response_id: response_id.map(str::to_string),
        text: text.map(str::to_string),
        status: status.map(str::to_string),
        audio_marker: audio_marker.map(str::to_string),
    }
}

#[test]
fn text_exchange_completion_is_recorded() {
    let exchange = Exchange::new_text(0, "hello", SystemTime::UNIX_EPOCH);
    let mut state = LiveSessionState::default();

    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(exchange)),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index: 0,
            context_assembly: ContextAssembly {
                budget: ContextBudget::new(4, 100),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            },
            retrieved_memory_block: "memory".to_string(),
            recalled_items: vec![],
            live_capture: Some(LiveCaptureContext {
                source_exchange_index: Some(0),
                previous_user_input: Some("prev".to_string()),
                previous_assistant_response: Some("answer".to_string()),
            }),
        },
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-1".to_string()),
            text: "hi".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("mock".to_string()),
            target: Some("text".to_string()),
            audio_marker: None,
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeCompleted {
            exchange_index: 0,
            completed_at: SystemTime::UNIX_EPOCH,
        },
    );

    assert_eq!(state.runtime_phase, RuntimePhase::Idle);
    assert!(state.active_exchange.is_none());
    assert_eq!(state.completed_exchanges.len(), 1);
    let completed = &state.completed_exchanges[0];
    assert_eq!(completed.status, ExchangeStatus::Completed);
    assert_eq!(completed.final_user_input(), "hello");
    assert_eq!(completed.retrieved_memory_block, "memory");
    assert!(completed.output.is_some());
}

#[test]
fn completion_with_mismatched_exchange_index_is_ignored() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            3,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );

    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeCompleted {
            exchange_index: 99,
            completed_at: SystemTime::UNIX_EPOCH,
        },
    );

    assert!(state.active_exchange.is_some());
    assert!(state.completed_exchanges.is_empty());
    assert_eq!(state.runtime_phase, RuntimePhase::Thinking);
}

#[test]
fn provider_events_and_tool_requests_attach_to_active_exchange_only() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            4,
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            4,
            ProviderEventKind::Preamble,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-4"),
            Some("hello"),
            None,
            None,
        )),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ToolRequested(ToolRequestRecord {
            exchange_index: 4,
            call_id: "call-4".to_string(),
            tool_name: "lookup".to_string(),
            arguments_summary: "{}".to_string(),
            requested_at: SystemTime::UNIX_EPOCH,
            source: "realtime_provider".to_string(),
            routed_to: Some("qsf_tool_permission_boundary".to_string()),
            auto_executed: false,
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ToolResolved(crate::exchange::ToolExecutionRecord {
            exchange_index: 4,
            call_id: "call-4".to_string(),
            tool_name: "lookup".to_string(),
            permission_decision: crate::exchange::ToolPermissionDecision::Allowed,
            status: crate::exchange::ToolExecutionStatus::Completed,
            result_summary: "ok".to_string(),
            output_text: String::new(),
            error: None,
            requested_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            response_model_use: None,
            returning_event_id: Some("event-4".to_string()),
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ToolRequested(ToolRequestRecord {
            exchange_index: 99,
            call_id: "ignored".to_string(),
            tool_name: "lookup".to_string(),
            arguments_summary: "{}".to_string(),
            requested_at: SystemTime::UNIX_EPOCH,
            source: "realtime_provider".to_string(),
            routed_to: Some("qsf_tool_permission_boundary".to_string()),
            auto_executed: false,
        }),
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.provider_events.len(), 1);
    assert_eq!(exchange.tool_requests.len(), 1);
    assert_eq!(exchange.tool_executions.len(), 1);
    assert_eq!(exchange.tool_requests[0].call_id, "call-4");
    assert_eq!(exchange.tool_executions[0].call_id, "call-4");
}

#[test]
fn tool_resolved_is_ignored_for_finalized_exchanges() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            6,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeCompleted {
            exchange_index: 6,
            completed_at: SystemTime::UNIX_EPOCH,
        },
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ToolResolved(crate::exchange::ToolExecutionRecord {
            exchange_index: 6,
            call_id: "call-6".to_string(),
            tool_name: "lookup".to_string(),
            permission_decision: crate::exchange::ToolPermissionDecision::Allowed,
            status: crate::exchange::ToolExecutionStatus::Completed,
            result_summary: "ok".to_string(),
            output_text: String::new(),
            error: None,
            requested_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            response_model_use: None,
            returning_event_id: Some("event-6".to_string()),
        }),
    );

    assert_eq!(state.completed_exchanges.len(), 1);
    assert!(state.completed_exchanges[0].tool_executions.is_empty());
}

#[test]
fn partial_transcript_updates_live_state() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioPartialTranscriptRecorded(PartialTranscript {
            exchange_index: 3,
            utterance_id: "utterance-3".to_string(),
            revision_index: 2,
            transcript: "hel".to_string(),
            received_at: SystemTime::UNIX_EPOCH,
            provider_id: Some("provider".to_string()),
            source_chunk_index: Some(9),
        }),
    );

    assert_eq!(state.runtime_phase, RuntimePhase::Listening);
    assert_eq!(
        state
            .partial_transcript
            .as_ref()
            .map(|partial| partial.transcript.as_str()),
        Some("hel")
    );
}

#[test]
fn final_transcript_commits_voice_input() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            7,
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioPartialTranscriptRecorded(PartialTranscript {
            exchange_index: 7,
            utterance_id: "utterance-7".to_string(),
            revision_index: 1,
            transcript: "par".to_string(),
            received_at: SystemTime::UNIX_EPOCH,
            provider_id: Some("provider".to_string()),
            source_chunk_index: None,
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index: 7,
            utterance: UtteranceRecord {
                utterance_id: "utterance-7".to_string(),
                revision_index: 2,
                transcript: "partial transcript".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: Some(4),
            },
            final_transcript: "final transcript".to_string(),
        },
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    match &exchange.input {
        ExchangeInput::Voice {
            final_transcript,
            utterances,
        } => {
            assert_eq!(final_transcript, "final transcript");
            assert_eq!(utterances.len(), 1);
            assert_eq!(utterances[0].utterance_id, "utterance-7");
        }
        ExchangeInput::Text { .. } => panic!("expected voice input"),
    }
    assert!(state.partial_transcript.is_none());
}

#[test]
fn final_transcript_appends_utterances() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            9,
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index: 9,
            utterance: UtteranceRecord {
                utterance_id: "utterance-9a".to_string(),
                revision_index: 1,
                transcript: "first".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: Some(1),
            },
            final_transcript: "first final".to_string(),
        },
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index: 9,
            utterance: UtteranceRecord {
                utterance_id: "utterance-9b".to_string(),
                revision_index: 2,
                transcript: "second".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: Some(2),
            },
            final_transcript: "second final".to_string(),
        },
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    match &exchange.input {
        ExchangeInput::Voice {
            final_transcript,
            utterances,
        } => {
            assert_eq!(final_transcript, "second final");
            assert_eq!(utterances.len(), 2);
            assert_eq!(utterances[0].utterance_id, "utterance-9a");
            assert_eq!(utterances[1].utterance_id, "utterance-9b");
        }
        ExchangeInput::Text { .. } => panic!("expected voice input"),
    }
}

#[test]
fn aging_and_co_retrieval_state_records_counts_and_summaries() {
    let mut state = LiveSessionState::default();
    let summary = TurnSummary {
        turn_index: 3,
        summarized_after_turn_index: 7,
        summary: "Summarized exchange batch".to_string(),
        model_id: "mock".to_string(),
        model_latency_ms: 42,
        input_tokens: 10,
        output_tokens: 5,
    };

    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangesAgedAndCoRetrieved {
            exchange_range: ExchangeRange {
                first_index: 2,
                last_index: 7,
            },
            new_associations: 4,
            strengthened_associations: 2,
            persisted_at: SystemTime::UNIX_EPOCH,
            summaries: vec![summary.clone()],
        },
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProcessedRangesRecorded {
            ranges: vec![ProcessedRange {
                session_id: "session-1".to_string(),
                first_turn_index: 2,
                last_turn_index: 7,
                kind: ProcessedRangeKind::LiveBatch,
                at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
            }],
        },
    );

    let aged = state
        .last_aged_co_retrieval
        .as_ref()
        .expect("aged co-retrieval record");
    assert_eq!(aged.exchange_range.first_index, 2);
    assert_eq!(aged.new_associations, 4);
    assert_eq!(aged.strengthened_associations, 2);
    assert_eq!(aged.summaries, vec![summary]);
    assert_eq!(state.processed_ranges.len(), 1);
}

#[test]
fn memory_context_ignores_mismatched_exchange_updates() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            1,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index: 1,
            context_assembly: ContextAssembly {
                budget: ContextBudget::new(4, 100),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            },
            retrieved_memory_block: "memory-a".to_string(),
            recalled_items: vec![],
            live_capture: Some(LiveCaptureContext {
                source_exchange_index: Some(1),
                previous_user_input: Some("prev-a".to_string()),
                previous_assistant_response: Some("resp-a".to_string()),
            }),
        },
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index: 99,
            context_assembly: ContextAssembly {
                budget: ContextBudget::new(4, 100),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            },
            retrieved_memory_block: "memory-b".to_string(),
            recalled_items: vec![],
            live_capture: Some(LiveCaptureContext {
                source_exchange_index: Some(99),
                previous_user_input: Some("prev-b".to_string()),
                previous_assistant_response: Some("resp-b".to_string()),
            }),
        },
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.retrieved_memory_block, "memory-a");
    assert_eq!(
        state
            .live_capture
            .as_ref()
            .and_then(|capture| capture.previous_user_input.as_deref()),
        Some("prev-a")
    );
}

#[test]
fn model_role_completion_records_exchange_use() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            2,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ModelRoleCompleted(ExchangeModelUse {
            provider_name: Some("mock".to_string()),
            model_id: "mock-model".to_string(),
            latency_ms: 11,
            input_tokens: 7,
            cached_input_tokens: 2,
            output_tokens: 3,
            full_request_hash: crate::conversation::ContentHash([2; 32]),
            message_count: 5,
        }),
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(
        exchange.model.as_ref().map(|model| model.model_id.as_str()),
        Some("mock-model")
    );
}

#[test]
fn model_role_failure_clears_active_exchange() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            3,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );

    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ModelRoleFailed {
            error_summary: "network failed".to_string(),
        },
    );

    assert_eq!(state.runtime_phase, RuntimePhase::Idle);
    assert!(state.active_exchange.is_none());
    assert!(state.completed_exchanges.is_empty());
}

#[test]
fn interrupted_response_cleanup_clears_volatile_state() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            4,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-4".to_string()),
            text: "working".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("mock".to_string()),
            target: Some("speech".to_string()),
            audio_marker: Some("marker".to_string()),
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::UserInterrupted(InterruptionRecord {
            exchange_index: 4,
            response_id: Some("response-4".to_string()),
            detected_at: SystemTime::UNIX_EPOCH,
            source: "user-speech".to_string(),
            action: InterruptionAction::Stop,
            stop_outcome: InterruptionStopOutcome::Stopped,
            partial_response_text: Some("working".to_string()),
        }),
    );

    assert_eq!(state.runtime_phase, RuntimePhase::Idle);
    assert!(state.active_response.is_none());
    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.status, ExchangeStatus::Interrupted);
    assert_eq!(exchange.interruptions.len(), 1);
}

#[test]
fn interruption_ignore_only_updates_matching_active_response() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            4,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-4".to_string()),
            text: "working".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("mock".to_string()),
            target: Some("speech".to_string()),
            audio_marker: Some("marker".to_string()),
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::UserInterrupted(InterruptionRecord {
            exchange_index: 99,
            response_id: Some("response-4".to_string()),
            detected_at: SystemTime::UNIX_EPOCH,
            source: "user-speech".to_string(),
            action: InterruptionAction::Ignore,
            stop_outcome: InterruptionStopOutcome::Ignored,
            partial_response_text: Some("working".to_string()),
        }),
    );

    assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
    assert_eq!(
        state.active_response.as_ref().unwrap().partial_text,
        "working"
    );
    assert_eq!(
        state.active_exchange.as_ref().unwrap().interruptions.len(),
        0
    );
}

#[test]
fn transcript_completion_after_response_start_keeps_user_input_and_response_state() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            10,
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            10,
            ProviderEventKind::ResponseStarted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-10"),
            Some("thinking"),
            None,
            None,
        )),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index: 10,
            utterance: UtteranceRecord {
                utterance_id: "utterance-10".to_string(),
                revision_index: 1,
                transcript: "hello".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: Some(1),
            },
            final_transcript: "hello there".to_string(),
        },
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.final_user_input(), "hello there");
    assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
    assert_eq!(
        state
            .active_response
            .as_ref()
            .and_then(|response| response.response_id.as_deref()),
        Some("response-10")
    );
    assert_eq!(
        state
            .active_response
            .as_ref()
            .map(|response| response.status),
        Some(ResponseStatus::Starting)
    );
}

#[test]
fn duplicate_provider_events_keep_the_active_response_stable() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            11,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );

    for _ in 0..2 {
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                11,
                ProviderEventKind::ResponseStarted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-11"),
                Some("working"),
                None,
                None,
            )),
        );
    }
    for _ in 0..2 {
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                11,
                ProviderEventKind::ResponseCompleted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-11"),
                Some("done"),
                Some("completed"),
                None,
            )),
        );
    }

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.provider_events.len(), 4);
    assert_eq!(
        state
            .active_response
            .as_ref()
            .and_then(|response| response.response_id.as_deref()),
        Some("response-11")
    );
    assert_eq!(
        state
            .active_response
            .as_ref()
            .map(|response| response.status),
        Some(ResponseStatus::Completed)
    );
    assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
}

#[test]
fn interruption_before_response_created_suppresses_late_output() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            12,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::UserInterrupted(InterruptionRecord {
            exchange_index: 12,
            response_id: Some("response-12".to_string()),
            detected_at: SystemTime::UNIX_EPOCH,
            source: "user-speech".to_string(),
            action: InterruptionAction::Stop,
            stop_outcome: InterruptionStopOutcome::Stopped,
            partial_response_text: None,
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-12".to_string()),
            text: "late output".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("provider".to_string()),
            target: Some("speech".to_string()),
            audio_marker: Some("marker".to_string()),
        }),
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.status, ExchangeStatus::Interrupted);
    assert!(state.active_response.is_none());
    assert!(state.completed_exchanges.is_empty());
}

#[test]
fn response_completion_after_interruption_is_ignored() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            13,
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            13,
            ProviderEventKind::ResponseStarted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-13"),
            Some("working"),
            None,
            None,
        )),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::UserInterrupted(InterruptionRecord {
            exchange_index: 13,
            response_id: Some("response-13".to_string()),
            detected_at: SystemTime::UNIX_EPOCH,
            source: "user-speech".to_string(),
            action: InterruptionAction::Stop,
            stop_outcome: InterruptionStopOutcome::Stopped,
            partial_response_text: Some("working".to_string()),
        }),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            13,
            ProviderEventKind::ResponseCompleted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-13"),
            Some("done"),
            Some("completed"),
            None,
        )),
    );

    let exchange = state.active_exchange.as_ref().expect("active exchange");
    assert_eq!(exchange.status, ExchangeStatus::Interrupted);
    assert!(state.active_response.is_none());
    assert_eq!(exchange.provider_events.len(), 2);
    assert_eq!(
        exchange.provider_events[0].event_kind,
        ProviderEventKind::ResponseStarted
    );
    assert_eq!(
        exchange.provider_events[1].event_kind,
        ProviderEventKind::ResponseCompleted
    );
}

#[test]
fn second_user_turn_finalizes_previous_streaming_exchange_before_opening_next_turn() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            14,
            "first",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            14,
            ProviderEventKind::ResponseStarted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-14"),
            Some("thinking"),
            None,
            None,
        )),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            15,
            "second",
            SystemTime::UNIX_EPOCH,
        ))),
    );

    assert_eq!(state.completed_exchanges.len(), 1);
    assert_eq!(state.completed_exchanges[0].index, 14);
    assert_eq!(
        state.completed_exchanges[0].status,
        ExchangeStatus::Interrupted
    );
    assert_eq!(
        state
            .active_exchange
            .as_ref()
            .map(|exchange| exchange.index),
        Some(15)
    );
    assert!(state.active_response.is_none());
    assert_eq!(state.runtime_phase, RuntimePhase::Thinking);

    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-14".to_string()),
            text: "stale output".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("provider".to_string()),
            target: Some("text".to_string()),
            audio_marker: None,
        }),
    );

    assert!(state.active_exchange.as_ref().unwrap().output.is_none());
}

#[test]
fn out_of_order_response_completion_before_start_remains_completed() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            16,
            "hello",
            SystemTime::UNIX_EPOCH,
        ))),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            16,
            ProviderEventKind::ResponseCompleted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-16"),
            Some("done"),
            Some("completed"),
            None,
        )),
    );
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProviderEventRecorded(provider_event_record(
            16,
            ProviderEventKind::ResponseStarted,
            "provider",
            SystemTime::UNIX_EPOCH,
            Some("response-16"),
            Some("working"),
            None,
            None,
        )),
    );

    assert_eq!(
        state
            .active_response
            .as_ref()
            .map(|response| response.status),
        Some(ResponseStatus::Completed)
    );
    assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some("response-16".to_string()),
            text: "done".to_string(),
            produced_at: SystemTime::UNIX_EPOCH,
            provider_name: Some("provider".to_string()),
            target: Some("text".to_string()),
            audio_marker: None,
        }),
    );
    assert_eq!(
        state.active_exchange.as_ref().unwrap().status,
        ExchangeStatus::Speaking
    );
    assert_eq!(
        state
            .active_exchange
            .as_ref()
            .and_then(|exchange| exchange.output.as_ref())
            .and_then(|output| output.response_id.as_deref()),
        Some("response-16")
    );
}

#[test]
fn processed_ranges_are_appended_without_reduction() {
    let mut state = LiveSessionState::default();
    reduce_live_session_in_place(
        &mut state,
        LiveSessionEvent::ProcessedRangesRecorded {
            ranges: vec![
                ProcessedRange {
                    session_id: "session-1".to_string(),
                    first_turn_index: 0,
                    last_turn_index: 2,
                    kind: ProcessedRangeKind::LiveBatch,
                    at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
                },
                ProcessedRange {
                    session_id: "session-1".to_string(),
                    first_turn_index: 4,
                    last_turn_index: 4,
                    kind: ProcessedRangeKind::SessionEnd,
                    at: time::OffsetDateTime::from_unix_timestamp(1).unwrap(),
                },
            ],
        },
    );

    assert_eq!(state.processed_ranges.len(), 2);
}

#[test]
fn serde_roundtrips() {
    let state = LiveSessionState::default();
    let json = serde_json::to_string(&state).unwrap();
    let parsed: LiveSessionState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, state);
}
