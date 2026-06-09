use std::time::SystemTime;

use qsf_memory::processed_range::{ProcessedRange, ProcessedRangeKind};
use qsf_session::{
    ActiveResponseState, ContentHash, ContextAssembly, ContextBudget, ContextFragment,
    ContextOmission, ContextSelection, ContextSourceKind, Exchange, ExchangeModelUse,
    ExchangeOutput, ExchangeStatus, InterruptionAction, InterruptionRecord,
    InterruptionStopOutcome, LiveCaptureContext, LiveSessionState, MemorySourceConfig,
    PartialTranscript, PromptPrefixInvalidation, ProviderEventKind, ProviderEventRecord,
    RecallRecord, ResponseStatus, SESSION_STATE_SCHEMA_VERSION, SessionConfig, SessionEndReason,
    SessionLimit, SessionState, ToolCategory, ToolSideEffectLevel, TurnSummary,
};

#[test]
fn legacy_fixture_loads_with_live_defaults() {
    let raw = include_str!("fixtures/pre_migration_session_state.json");
    let parsed: SessionState = serde_json::from_str(raw).unwrap();

    assert_eq!(parsed.schema_version, 1);
    assert_eq!(parsed.session_id, "legacy-session");
    assert_eq!(parsed.turns.len(), 1);
    assert_eq!(parsed.summarized_turns.len(), 1);
    assert_eq!(
        parsed.turns[0].user_input,
        "I want you to use the name Ari."
    );
    assert_eq!(parsed.summarized_turns[0].turn_index, 0);
    assert_eq!(parsed.live, LiveSessionState::default());
}

#[test]
fn current_session_state_roundtrips_live_and_exchange_fields() {
    let mut state = SessionState::new_with_id("current-session".to_string(), config());
    state.previous_session_id = Some("previous-session".to_string());
    state.ended_reason = Some(SessionEndReason::QuitCommand);
    state.last_input = Some("hello".to_string());
    state.last_prompt_hash = Some(ContentHash([7; 32]));
    state.prefix_invalidated_since_last_prompt = true;
    state
        .prompt_prefix_invalidations
        .push(PromptPrefixInvalidation {
            after_turn_index: 1,
            reason: "prompt prefix changed".to_string(),
        });
    state.last_model_error = Some("transient".to_string());
    state.limit_reached = Some(SessionLimit {
        current: 2,
        max: 3,
        override_active: true,
    });
    state.turns.push(sample_turn(0));
    state.summarized_turns.push(TurnSummary {
        turn_index: 0,
        summarized_after_turn_index: 1,
        summary: "turn summary".to_string(),
        model_id: "mock".to_string(),
        model_latency_ms: 11,
        input_tokens: 4,
        output_tokens: 2,
    });
    state.exchanges.push(sample_exchange(3, true));
    state.live.runtime_phase = qsf_session::RuntimePhase::Speaking;
    state.live.active_exchange = Some(sample_exchange(4, false));
    state.live.active_response = Some(ActiveResponseState {
        response_id: Some("response-4".to_string()),
        partial_text: "working".to_string(),
        status: ResponseStatus::Streaming,
        observed_at: SystemTime::UNIX_EPOCH,
        audio_marker: Some("marker".to_string()),
    });
    state.live.partial_transcript = Some(PartialTranscript {
        exchange_index: 4,
        utterance_id: "utterance-4".to_string(),
        revision_index: 2,
        transcript: "hel".to_string(),
        received_at: SystemTime::UNIX_EPOCH,
        provider_id: Some("provider".to_string()),
        source_chunk_index: Some(1),
    });
    state.live.live_capture = Some(LiveCaptureContext {
        source_exchange_index: Some(4),
        previous_user_input: Some("prev".to_string()),
        previous_assistant_response: Some("resp".to_string()),
    });
    state.live.processed_ranges.push(ProcessedRange {
        session_id: "current-session".to_string(),
        first_turn_index: 0,
        last_turn_index: 3,
        kind: ProcessedRangeKind::LiveBatch,
        at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
    });
    state
        .live
        .completed_exchanges
        .push(sample_exchange(5, true));

    let json = serde_json::to_string(&state).unwrap();
    assert!(!json.contains("completed_exchanges"));

    let parsed: SessionState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.schema_version, SESSION_STATE_SCHEMA_VERSION);
    assert_eq!(parsed.session_id, "current-session");
    assert_eq!(
        parsed.previous_session_id.as_deref(),
        Some("previous-session")
    );
    assert_eq!(parsed.ended_reason, Some(SessionEndReason::QuitCommand));
    assert_eq!(parsed.last_prompt_hash, Some(ContentHash([7; 32])));
    assert!(parsed.limit_reached.unwrap().override_active);
    assert_eq!(parsed.turns.len(), 1);
    assert_eq!(parsed.exchanges.len(), 1);
    assert_eq!(
        parsed.live.runtime_phase,
        qsf_session::RuntimePhase::Speaking
    );
    assert_eq!(
        parsed
            .live
            .active_exchange
            .as_ref()
            .map(|exchange| exchange.index),
        Some(4)
    );
    assert_eq!(
        parsed
            .live
            .active_response
            .as_ref()
            .and_then(|response| response.response_id.as_deref()),
        Some("response-4")
    );
    assert_eq!(parsed.live.partial_transcript.unwrap().exchange_index, 4);
    assert_eq!(parsed.live.processed_ranges.len(), 1);
    assert!(parsed.live.completed_exchanges.is_empty());
}

fn config() -> SessionConfig {
    SessionConfig {
        model_id: "mock".to_string(),
        max_turns: 8,
        warm_threshold: 3,
        allow_over_limit: false,
        memory_source: MemorySourceConfig {
            source: "fixture".to_string(),
            file: None,
        },
    }
}

fn sample_turn(index: usize) -> qsf_session::Turn {
    qsf_session::Turn {
        index,
        started_at: SystemTime::UNIX_EPOCH,
        completed_at: SystemTime::UNIX_EPOCH,
        user_input: format!("turn-{index}"),
        context_assembly: ContextAssembly {
            budget: ContextBudget::new(4, 600),
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: format!("memory-{index}"),
                    source_kind: ContextSourceKind::Memory,
                    summary: "summary".to_string(),
                    tags: vec!["tag".to_string()],
                    score: 1.0,
                    estimated_tokens: 10,
                    source_reference: "fixture".to_string(),
                    selection_reason: "selected".to_string(),
                },
                cumulative_estimated_tokens: 10,
            }],
            omitted: vec![ContextOmission {
                fragment: ContextFragment {
                    fragment_id: format!("omitted-{index}"),
                    source_kind: ContextSourceKind::MemoryHint,
                    summary: "omitted".to_string(),
                    tags: vec![],
                    score: 0.2,
                    estimated_tokens: 8,
                    source_reference: "fixture".to_string(),
                    selection_reason: "omitted".to_string(),
                },
                reason: "budget".to_string(),
            }],
            used_estimated_tokens: 10,
        },
        retrieved_memory_block: "memory block".to_string(),
        assistant_response: format!("response-{index}"),
        recalled_turns: vec![RecallRecord {
            call_id: format!("call-{index}"),
            turn_id: index,
            tool_name: "recall_turn".to_string(),
            category: ToolCategory::ComputeOnly,
            side_effect_level: ToolSideEffectLevel::None,
            verbatim_text: "verbatim".to_string(),
            latency_ms: 5,
        }],
        model_id: "mock-model".to_string(),
        model_latency_ms: 12,
        input_tokens: 4,
        cached_input_tokens: 1,
        output_tokens: 2,
        full_request_hash: ContentHash([index as u8; 32]),
        message_count: 2,
    }
}

fn sample_exchange(index: usize, completed: bool) -> Exchange {
    let mut exchange = if completed {
        Exchange::new_text(index, format!("input-{index}"), SystemTime::UNIX_EPOCH)
            .completed(SystemTime::UNIX_EPOCH)
    } else {
        Exchange::new_voice_pending(index, SystemTime::UNIX_EPOCH)
    };

    exchange.output = Some(ExchangeOutput {
        response_id: Some(format!("response-{index}")),
        text: format!("answer-{index}"),
        produced_at: SystemTime::UNIX_EPOCH,
        provider_name: Some("mock".to_string()),
        target: Some("speech".to_string()),
        audio_marker: Some("marker".to_string()),
    });
    exchange.context_assembly = Some(ContextAssembly {
        budget: ContextBudget::new(2, 100),
        selected: vec![],
        omitted: vec![],
        used_estimated_tokens: 0,
    });
    exchange.retrieved_memory_block = format!("retrieved-{index}");
    exchange.recalled_items = vec![RecallRecord {
        call_id: format!("call-{index}"),
        turn_id: index,
        tool_name: "recall_turn".to_string(),
        category: ToolCategory::ReadOnly,
        side_effect_level: ToolSideEffectLevel::ReadOnly,
        verbatim_text: "verbatim".to_string(),
        latency_ms: 7,
    }];
    exchange.model = Some(ExchangeModelUse {
        provider_name: Some("mock".to_string()),
        model_id: "mock-model".to_string(),
        latency_ms: 14,
        input_tokens: 6,
        cached_input_tokens: 2,
        output_tokens: 3,
        full_request_hash: ContentHash([index as u8; 32]),
        message_count: 4,
    });
    exchange.interruptions = vec![InterruptionRecord {
        exchange_index: index,
        response_id: Some(format!("response-{index}")),
        detected_at: SystemTime::UNIX_EPOCH,
        source: "user-speech".to_string(),
        action: InterruptionAction::Stop,
        stop_outcome: InterruptionStopOutcome::Stopped,
        partial_response_text: Some("partial".to_string()),
    }];
    exchange.provider_events = vec![ProviderEventRecord {
        exchange_index: index,
        event_kind: ProviderEventKind::Preamble,
        provider_id: "provider".to_string(),
        received_at: SystemTime::UNIX_EPOCH,
        response_id: Some(format!("response-{index}")),
        text: Some("text".to_string()),
        status: Some("ok".to_string()),
        audio_marker: Some("marker".to_string()),
    }];
    exchange.tool_requests = vec![qsf_session::ToolRequestRecord {
        exchange_index: index,
        call_id: format!("call-{index}"),
        tool_name: "lookup".to_string(),
        arguments_summary: "{}".to_string(),
        requested_at: SystemTime::UNIX_EPOCH,
        source: "provider".to_string(),
        routed_to: Some("boundary".to_string()),
        auto_executed: false,
    }];
    exchange.status = if completed {
        ExchangeStatus::Completed
    } else {
        ExchangeStatus::Speaking
    };

    exchange
}
