#[test]
fn reducer_appends_turns_in_order() {
    let config = test_config(3);
    let state = SessionState::new(config);
    let first = test_turn(0);
    let second = test_turn(1);

    let state = reduce_session(state, SessionEvent::TurnCompleted(first.clone()));
    let state = reduce_session(state, SessionEvent::TurnCompleted(second.clone()));

    assert_eq!(state.turns, vec![first, second]);
}

#[test]
fn token_budget_drop_plan_ages_oldest_active_block_to_low_water() {
    let state = synthetic_state_with_verbatim_sizes(&[200, 200, 200, 200, 200, 200]);

    let plan = ageing::plan_token_budget_drop(&state, 1_000, 0.80, 0.50).unwrap();

    assert_eq!(plan.first_turn_index, 0);
    assert_eq!(plan.last_turn_index, 3);
    assert_eq!(plan.aged_count, 4);
    assert_eq!(plan.hot_tokens_before, 1_200);
    assert_eq!(plan.hot_tokens_after, 400);
}

#[test]
fn token_budget_drop_plan_noops_below_high_water() {
    let state = synthetic_state_with_verbatim_sizes(&[100, 100, 100]);

    let plan = ageing::plan_token_budget_drop(&state, 1_000, 0.80, 0.50);

    assert!(plan.is_none());
}

#[test]
fn turns_aged_and_co_retrieved_extends_summaries_without_dropping_turns() {
    let mut state = SessionState::new(test_config(10));
    state.turns = (0..4).map(test_turn).collect();
    let turns_before = state.turns.clone();
    let summary = TurnSummary {
        turn_index: 0,
        summarized_after_turn_index: 3,
        summary: "Turn zero summary.".to_string(),
        model_id: DEFAULT_SESSION_MODEL.to_string(),
        model_latency_ms: 0,
        input_tokens: 0,
        output_tokens: 0,
    };

    let state = reduce_session(
        state,
        SessionEvent::TurnsAgedAndCoRetrieved {
            range: TurnRange {
                first_index: 0,
                last_index: 0,
            },
            new_associations: 1,
            strengthened_associations: 0,
            persisted_at: std::time::SystemTime::UNIX_EPOCH,
            summaries: vec![summary],
        },
    );

    assert_eq!(state.turns, turns_before);
    assert_eq!(state.summarized_turns.len(), 1);
    assert!(state.prefix_invalidated_since_last_prompt);
}

#[test]
fn token_budget_drop_persists_associations_and_processed_range() {
    let base_dir = std::env::temp_dir().join(format!("qsf-token-drop-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records([
        memory_record("memory.a", "A", "A summary", vec!["a"], 10),
        memory_record("memory.b", "B", "B summary", vec!["b"], 10),
        memory_record("memory.c", "C", "C summary", vec!["c"], 10),
    ]);
    store.persist().unwrap();
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new_with_id("session-drop".to_string(), test_config(10));
    state.turns = vec![
        test_turn_with_memory_ids(0, &["memory.a"]),
        test_turn_with_memory_ids(1, &["memory.b"]),
        test_turn_with_memory_ids(2, &["memory.c"]),
    ];
    let plan = ageing::TokenBudgetDropPlan {
        first_turn_index: 0,
        last_turn_index: 1,
        aged_count: 2,
        hot_tokens_before: 1_000,
        hot_tokens_after: 400,
    };

    let event = ageing::run_token_budget_drop_side_effect(
        &mut context,
        &state,
        &state_dir,
        plan,
        timestamp("2026-05-24T00:00:00Z"),
        &MockModelClient::default(),
    )
    .unwrap();
    let reloaded = MemoryStore::load_or_empty(&store_path).unwrap();

    assert!(matches!(
        event,
        SessionEvent::TurnsAgedAndCoRetrieved {
            new_associations: 3,
            ..
        }
    ));
    assert_eq!(reloaded.contents().associations.len(), 3);
    let range = reloaded
        .contents()
        .processed_ranges
        .iter()
        .find(|range| range.kind == qsf_memory::ProcessedRangeKind::LiveBatch)
        .expect("expected live batch range");
    assert_eq!(range.first_turn_index, 0);
    assert_eq!(range.last_turn_index, 1);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn token_budget_drop_persists_processed_range_before_summary_failure() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-token-drop-summary-fail-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records([
        memory_record("memory.a", "A", "A summary", vec!["a"], 10),
        memory_record("memory.b", "B", "B summary", vec!["b"], 10),
    ]);
    store.persist().unwrap();
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new_with_id("session-drop-fail".to_string(), test_config(10));
    state.turns = vec![
        test_turn_with_memory_ids(0, &["memory.a"]),
        test_turn_with_memory_ids(1, &["memory.b"]),
    ];
    let plan = ageing::TokenBudgetDropPlan {
        first_turn_index: 0,
        last_turn_index: 1,
        aged_count: 2,
        hot_tokens_before: 1_000,
        hot_tokens_after: 400,
    };
    let client = SequencedSummarizerClient::new(vec![
        SummaryReply::new("Partial warm summary.", "max_tokens"),
        SummaryReply::new("Still truncated.", "max_tokens"),
    ]);

    let error = ageing::run_token_budget_drop_side_effect(
        &mut context,
        &state,
        &state_dir,
        plan,
        timestamp("2026-05-24T00:00:00Z"),
        &client,
    )
    .unwrap_err();
    let reloaded = MemoryStore::load_or_empty(&store_path).unwrap();

    assert!(error.to_string().contains("truncated after retry"));
    assert!(
        reloaded
            .contents()
            .processed_ranges
            .iter()
            .any(
                |range| range.kind == qsf_memory::ProcessedRangeKind::LiveBatch
                    && range.first_turn_index == 0
                    && range.last_turn_index == 1
            )
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn session_end_flush_covers_remaining_hot_turns_idempotently() {
    let base_dir = std::env::temp_dir().join(format!("qsf-session-flush-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records([
        memory_record("memory.a", "A", "A summary", vec!["a"], 10),
        memory_record("memory.b", "B", "B summary", vec!["b"], 10),
    ]);
    store.persist().unwrap();
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new_with_id("session-flush".to_string(), test_config(10));
    state.turns = vec![
        test_turn_with_memory_ids(0, &["memory.a"]),
        test_turn_with_memory_ids(1, &["memory.b"]),
    ];
    let mut output = Vec::new();
    let color_mode = crate::console::styling::ColorMode::Disabled;

    let first =
        ageing::run_session_end_flush(&mut context, &state, &state_dir, &mut output, color_mode)
            .unwrap()
            .expect("expected first flush");
    let second =
        ageing::run_session_end_flush(&mut context, &state, &state_dir, &mut output, color_mode)
            .unwrap();
    let reloaded = MemoryStore::load_or_empty(&store_path).unwrap();

    assert_eq!(first.new_associations, 1);
    assert!(second.is_none());
    assert!(
        reloaded
            .contents()
            .processed_ranges
            .iter()
            .any(
                |range| range.kind == qsf_memory::ProcessedRangeKind::SessionEnd
                    && range.first_turn_index == 0
                    && range.last_turn_index == 1
            )
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn session_end_flush_preserves_non_contiguous_processed_ranges() {
    let base_dir = std::env::temp_dir().join(format!("qsf-session-flush-gaps-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records([
        memory_record("memory.a", "A", "A summary", vec!["a"], 10),
        memory_record("memory.b", "B", "B summary", vec!["b"], 10),
    ]);
    store.contents_mut().processed_ranges.extend([
        qsf_memory::ProcessedRange {
            session_id: "session-flush-gaps".to_string(),
            first_turn_index: 1,
            last_turn_index: 1,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            at: timestamp("2026-05-24T00:00:00Z"),
        },
        qsf_memory::ProcessedRange {
            session_id: "session-flush-gaps".to_string(),
            first_turn_index: 3,
            last_turn_index: 3,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            at: timestamp("2026-05-24T00:00:00Z"),
        },
    ]);
    store.persist().unwrap();
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new_with_id("session-flush-gaps".to_string(), test_config(10));
    state.turns = vec![
        test_turn_with_memory_ids(0, &["memory.a"]),
        test_turn_with_memory_ids(1, &["memory.b"]),
        test_turn_with_memory_ids(2, &["memory.a"]),
        test_turn_with_memory_ids(3, &["memory.b"]),
        test_turn_with_memory_ids(4, &["memory.a"]),
    ];
    let mut output = Vec::new();
    let color_mode = crate::console::styling::ColorMode::Disabled;

    let outcome =
        ageing::run_session_end_flush(&mut context, &state, &state_dir, &mut output, color_mode)
            .unwrap()
            .expect("expected non-contiguous flush");
    let reloaded = MemoryStore::load_or_empty(&store_path).unwrap();
    let session_end_ranges = reloaded
        .contents()
        .processed_ranges
        .iter()
        .filter(|range| range.kind == qsf_memory::ProcessedRangeKind::SessionEnd)
        .map(|range| (range.first_turn_index, range.last_turn_index))
        .collect::<Vec<_>>();

    assert_eq!(outcome.aged_count, 3);
    assert_eq!(session_end_ranges, vec![(0, 0), (2, 2), (4, 4)]);
    assert!(!session_end_ranges.contains(&(0, 4)));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn cross_turn_persist_skips_already_processed_anchors_on_retry() {
    let base_dir = std::env::temp_dir().join(format!("qsf-cross-turn-retry-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records([
        memory_record("memory.a", "A", "A summary", vec!["a"], 10),
        memory_record("memory.b", "B", "B summary", vec!["b"], 10),
    ]);
    store.persist().unwrap();
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new_with_id("session-retry".to_string(), test_config(10));
    state.turns = vec![
        test_turn_with_memory_ids(0, &["memory.a"]),
        test_turn_with_memory_ids(1, &["memory.b"]),
    ];
    let request = ageing::CrossTurnPersistRequest {
        first_turn_index: 0,
        last_turn_index: 0,
        kind: qsf_memory::ProcessedRangeKind::LiveBatch,
        now: timestamp("2026-05-24T00:00:00Z"),
        event_kind: "test_retry",
    };

    let first =
        ageing::persist_cross_turn_range(&mut context, &state, &store_path, request).unwrap();
    let second = ageing::persist_cross_turn_range(
        &mut context,
        &state,
        &store_path,
        ageing::CrossTurnPersistRequest {
            first_turn_index: 0,
            last_turn_index: 0,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            now: timestamp("2026-05-24T00:00:00Z"),
            event_kind: "test_retry",
        },
    )
    .unwrap();
    let reloaded = MemoryStore::load_or_empty(&store_path).unwrap();

    assert_eq!(first.unwrap().new_associations, 1);
    assert_eq!(second.unwrap(), ageing::CrossTurnPersistOutcome::default());
    assert_eq!(reloaded.contents().associations.len(), 1);
    assert_eq!(reloaded.contents().processed_ranges.len(), 1);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn reducer_records_model_failure_without_appending_turn() {
    let state = SessionState::new(test_config(3));

    let state = reduce_session(
        state,
        SessionEvent::ModelRoleFailed {
            error_summary: "boom".to_string(),
        },
    );

    assert!(state.turns.is_empty());
    assert_eq!(state.last_model_error.as_deref(), Some("boom"));
}

#[test]
fn reducer_records_prompt_prefix_invalidation() {
    let state = SessionState::new(test_config(3));

    let state = reduce_session(
        state,
        SessionEvent::PromptPrefixInvalidated {
            after_turn_index: 2,
            reason: "non_replayed_tool_messages".to_string(),
        },
    );

    assert!(state.prefix_invalidated_since_last_prompt);
    assert_eq!(state.prompt_prefix_invalidations.len(), 1);
    assert_eq!(state.prompt_prefix_invalidations[0].after_turn_index, 2);
    assert_eq!(
        state.prompt_prefix_invalidations[0].reason,
        "non_replayed_tool_messages"
    );
}

#[test]
fn model_error_output_still_shows_assembled_memory_blocks() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-model-error-memory-blocks-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("associative memory\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &FailingModelClient,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("=== Memories retrieved for this turn ==="));
    assert!(output.contains("model unavailable, try again or :quit"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn prompt_continuity_errors_get_runtime_retry_message() {
    assert_eq!(
        super::retry_message_for_turn_error(
            "prompt continuity error before turn 2: new prompt did not contain the previous request prefix"
        ),
        "runtime prompt continuity error; see engine.log, then try again or :quit"
    );
    assert_eq!(
        super::retry_message_for_turn_error("provider unavailable"),
        "model unavailable, try again or :quit"
    );
}

#[test]
fn reducer_records_limit_without_appending_turn() {
    let state = SessionState::new(test_config(1));

    let state = reduce_session(
        state,
        SessionEvent::SessionLimitReached {
            current: 1,
            max: 1,
            override_active: false,
        },
    );

    assert!(state.turns.is_empty());
    assert_eq!(state.limit_reached.unwrap().current, 1);
}

#[test]
fn reducer_covers_session_lifecycle_events() {
    let config = test_config(2);
    let state = SessionState::new(config.clone());

    let state = reduce_session(state, SessionEvent::SessionStarted(config));
    let state = reduce_session(
        state,
        SessionEvent::InputReceived {
            input: "hello".to_string(),
        },
    );
    let state = reduce_session(
        state,
        SessionEvent::PromptAssembled {
            full_request_hash: ContentHash([1; 32]),
            message_count: 2,
            total_bytes: 10,
        },
    );
    let state = reduce_session(
        state,
        SessionEvent::ModelRoleCompleted {
            response: "hi".to_string(),
            latency_ms: 3,
            input_tokens: 4,
            cached_input_tokens: 1,
            output_tokens: 2,
        },
    );
    let state = reduce_session(
        state,
        SessionEvent::SessionEnded {
            reason: SessionEndReason::QuitCommand,
        },
    );

    assert_eq!(state.last_input.as_deref(), Some("hello"));
    assert_eq!(state.last_prompt_hash, Some(ContentHash([1; 32])));
    assert_eq!(state.ended_reason, Some(SessionEndReason::QuitCommand));
}

#[test]
fn reducer_memory_retrieved_is_non_mutating() {
    let state = SessionState::new(test_config(2));

    let next = reduce_session(state.clone(), SessionEvent::MemoryRetrieved);

    assert_eq!(next, state);
}

#[test]
fn reducer_context_assembled_is_non_mutating() {
    let state = SessionState::new(test_config(2));

    let next = reduce_session(
        state.clone(),
        SessionEvent::ContextAssembled(ContextAssembly {
            budget: ContextBudget::new(4, 600),
            selected: vec![],
            omitted: vec![],
            used_estimated_tokens: 0,
        }),
    );

    assert_eq!(next, state);
}

#[test]
fn reducer_tool_completed_is_non_mutating() {
    let state = SessionState::new(test_config(2));

    let next = reduce_session(
        state.clone(),
        SessionEvent::ToolCompleted(RecallRecord {
            call_id: "call-0".to_string(),
            turn_id: 0,
            tool_name: "recall_turn".to_string(),
            category: crate::tools::ToolCategory::ComputeOnly,
            side_effect_level: crate::tools::ToolSideEffectLevel::None,
            verbatim_text: "verbatim".to_string(),
            latency_ms: 0,
        }),
    );

    assert_eq!(next, state);
}

