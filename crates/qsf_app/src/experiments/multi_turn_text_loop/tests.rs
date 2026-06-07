use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::audio::{SimulatedSpeechOutputProvider, SimulatedTranscriptProvider};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    DEFAULT_SESSION_MODEL, SessionMemorySource, prompt_prefix_status_for_report, run_one_turn,
    run_with_io_and_components, run_with_io_and_components_at_state_dir,
    run_with_io_and_components_at_state_resolution,
};
use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSelection, ContextSourceKind,
};
use crate::conversation::ContentHash;
use crate::conversation::prompt::{
    PromptTurn, PromptTurnSummary, assemble_prompt_with_summaries_and_project_doc_channel,
    prior_request_prefix_hash,
};
use crate::experiments::text_owned_voice_loop::SharedVoiceMemorySource;
use crate::memory::{
    Association, LiveCaptureInput, MemoryFixture, MemoryRecord, MemoryRecordKind, MemoryStore,
    RetrievalStrategy, capture_live_memory_candidates, phase_four_fixture, retrieve_memories,
};
use crate::models::{
    MockModelClient, ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRoleId,
    ModelToolCall, ModelUsage,
};
use crate::observability::event_log::{EventRecord, EventType};
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::session::ageing;
use crate::session::{
    MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent, SessionState,
    StateDirectoryResolution, Turn, TurnRange, TurnSummary, reduce_session,
    resume_breaking_config_changed,
};
use crate::tools::{
    CALCULATOR_TOOL_NAME, READ_PROJECT_DOC_TOOL_NAME, RECALL_TURN_TOOL_NAME,
    SEARCH_PROJECT_DOCS_TOOL_NAME,
};

#[test]
fn live_retrieval_uses_keyword_tag_strategy() {
    assert_eq!(
        super::SESSION_RETRIEVAL_STRATEGY,
        crate::memory::RetrievalStrategy::KeywordTag,
        "Live loop must use KeywordTag so retrieval + hint expansion stay strict single-hop",
    );
}

#[test]
fn allow_over_limit_change_does_not_break_awake_resume() {
    let mut previous = test_config(5);
    let mut current = previous.clone();
    current.allow_over_limit = true;

    assert!(!resume_breaking_config_changed(&previous, &current));

    previous.model_id = "changed-model".to_string();
    assert!(resume_breaking_config_changed(&previous, &current));
}

#[test]
fn accepted_assistant_name_assignment_becomes_live_memory_candidate() {
    let candidates = capture_live_memory_candidates(&LiveCaptureInput {
        user_input: "I want you to use the name Ari.",
        assistant_response: "Absolutely - you can call me Ari.",
        previous_turn_index: None,
        previous_user_input: None,
        previous_assistant_response: None,
    });
    let candidate = candidates
        .first()
        .expect("expected assistant name candidate");

    assert_eq!(candidate.title, "Assistant name: Ari");
    assert!(candidate.summary.contains("use the name Ari"));
    assert_eq!(
        candidate.tags,
        vec![
            "assistant_identity".to_string(),
            "profile".to_string(),
            "name".to_string()
        ]
    );
}

#[test]
fn turn_max_output_tokens_defaults_above_short_response_cap() {
    const LEGACY_TRUNCATING_TURN_MAX_OUTPUT_TOKENS: u32 = 240;
    const INVALID_ZERO_TURN_MAX_OUTPUT_TOKENS: &str = "0";
    const INVALID_TEXT_TURN_MAX_OUTPUT_TOKENS: &str = "nope";

    let custom_turn_max_output_tokens = super::DEFAULT_TURN_MAX_OUTPUT_TOKENS * 2;
    let default_turn_max_output_tokens = super::parse_turn_max_output_tokens(None);

    assert!(default_turn_max_output_tokens > LEGACY_TRUNCATING_TURN_MAX_OUTPUT_TOKENS);
    assert_eq!(
        default_turn_max_output_tokens,
        super::DEFAULT_TURN_MAX_OUTPUT_TOKENS
    );
    assert_eq!(
        super::parse_turn_max_output_tokens(Some(INVALID_ZERO_TURN_MAX_OUTPUT_TOKENS.to_string())),
        super::DEFAULT_TURN_MAX_OUTPUT_TOKENS
    );
    assert_eq!(
        super::parse_turn_max_output_tokens(Some(INVALID_TEXT_TURN_MAX_OUTPUT_TOKENS.to_string())),
        super::DEFAULT_TURN_MAX_OUTPUT_TOKENS
    );
    assert_eq!(
        super::parse_turn_max_output_tokens(Some(custom_turn_max_output_tokens.to_string())),
        custom_turn_max_output_tokens
    );
}

#[test]
fn print_memory_blocks_no_color_mode_emits_plain_headers() {
    use crate::console::styling::ColorMode;

    let assembly = small_assembly_with_one_direct_one_hint();
    let mut buf: Vec<u8> = Vec::new();

    super::print_memory_blocks(&mut buf, &assembly, ColorMode::Disabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("=== Memories retrieved for this turn ==="));
    assert!(text.contains("=== Associated memories (hints - may or may not be relevant) ==="));
    assert!(!text.contains("\x1b["));
}

#[test]
fn print_memory_blocks_enabled_mode_wraps_headers_in_escapes() {
    use crate::console::styling::ColorMode;

    let assembly = small_assembly_with_one_direct_one_hint();
    let mut buf: Vec<u8> = Vec::new();

    super::print_memory_blocks(&mut buf, &assembly, ColorMode::Enabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("\x1b["), "expected ANSI escape codes");
    assert!(text.ends_with("\x1b[0m\n"));
}

#[test]
fn user_input_echo_enabled_mode_brackets_terminal_input_style() {
    use crate::console::styling::ColorMode;

    let mut buf: Vec<u8> = Vec::new();

    super::begin_user_input_echo(&mut buf, ColorMode::Enabled).unwrap();
    super::end_user_input_echo(&mut buf, ColorMode::Enabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.starts_with("\x1b[38;5;82m"));
    assert!(text.ends_with("\x1b[0m"));
}

#[test]
fn assistant_response_enabled_mode_wraps_response_in_color() {
    use crate::console::styling::ColorMode;

    let mut buf: Vec<u8> = Vec::new();

    super::print_assistant_response(&mut buf, "hello\nthere", ColorMode::Enabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.starts_with("\x1b[38;5;255m"));
    assert!(text.contains("hello\nthere"));
    assert!(text.ends_with("\x1b[0m\n"));
}

#[test]
fn conversation_role_color_helpers_are_plain_when_disabled() {
    use crate::console::styling::ColorMode;

    let mut buf: Vec<u8> = Vec::new();

    super::begin_user_input_echo(&mut buf, ColorMode::Disabled).unwrap();
    super::end_user_input_echo(&mut buf, ColorMode::Disabled).unwrap();
    super::print_assistant_response(&mut buf, "hello", ColorMode::Disabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert_eq!(text, "hello\n");
    assert!(!text.contains("\x1b["));
}

#[test]
fn print_drop_marker_renders_expected_format() {
    use crate::console::styling::ColorMode;

    let mut buf: Vec<u8> = Vec::new();

    ageing::print_drop_marker(&mut buf, 3, 2, 5, ColorMode::Disabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("aged 3 turns from prompt"));
    assert!(text.contains("+2 associations"));
    assert!(text.contains("*5 strengthened"));
}

#[test]
fn print_session_end_flush_marker_renders_expected_format() {
    use crate::console::styling::ColorMode;

    let mut buf: Vec<u8> = Vec::new();

    ageing::print_session_end_flush(&mut buf, 4, 1, ColorMode::Disabled).unwrap();

    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("session-end flush"));
    assert!(text.contains("+4 associations"));
    assert!(text.contains("*1 strengthened"));
}

#[test]
fn reload_snapshot_picks_up_freshly_persisted_associations() {
    let dir = tempfile::TempDir::new().unwrap();
    let store_path = dir.path().join("memory-store.json");

    let store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.persist().unwrap();

    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.contents_mut().records.push(memory_record(
        "a",
        "Alpha",
        "Alpha summary.",
        vec!["alpha"],
        10,
    ));
    store.contents_mut().records.push(memory_record(
        "b",
        "Beta",
        "Beta summary.",
        vec!["beta"],
        10,
    ));
    store.contents_mut().associations.push(Association::new(
        "a",
        "b",
        0.5,
        "r",
        time::OffsetDateTime::now_utc(),
    ));
    store.persist().unwrap();

    let refreshed = super::reload_session_memory_source_snapshot(&store_path).unwrap();
    assert_eq!(refreshed.associations.len(), 1);
}

#[test]
fn run_one_turn_emits_memory_hints_when_associations_exist() {
    let base_dir = std::env::temp_dir().join(format!("qsf-memory-hint-turn-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new(test_config_with_warm_threshold(10, 10));
    let foo = memory_record(
        "memory.foo",
        "Foo anchor",
        "Foo summary",
        vec!["foozle"],
        20,
    );
    let baz = MemoryRecord::new(
        "memory.baz",
        MemoryRecordKind::Observation,
        "Baz hint",
        "Baz summary",
        vec!["baz"],
        timestamp("2026-05-01T00:00:00Z"),
        0.0,
        0,
        "tests",
        20,
    );
    let mut records = vec![foo, baz];
    records.extend((0..7).map(|i| {
        MemoryRecord::new(
            format!("memory.filler.{i}"),
            MemoryRecordKind::Observation,
            format!("Filler {i}"),
            format!("Filler summary {i}"),
            vec!["filler"],
            timestamp("2026-05-23T00:00:00Z"),
            1.0,
            0,
            "tests",
            1_000,
        )
    }));
    let mut memory_snapshot = super::SessionMemorySourceSnapshot::from_fixture(
        "test",
        "test",
        MemoryFixture {
            records,
            associations: vec![Association::new(
                "memory.foo",
                "memory.baz",
                0.9,
                "foo suggests baz",
                timestamp("2026-05-24T00:00:00Z"),
            )],
        },
    );
    let mut output = Vec::new();

    run_one_turn(
        &mut context,
        &mut state,
        &state_dir,
        &mut memory_snapshot,
        &MockModelClient::default(),
        super::TurnRequest {
            user_input: "foozle",
            boot_brief_fragment: None,
            max_output_tokens: super::DEFAULT_TURN_MAX_OUTPUT_TOKENS,
        },
        super::TurnConsole {
            output: &mut output,
            color_mode: crate::console::styling::ColorMode::Disabled,
        },
    )
    .unwrap();

    let turn = state.turns.last().unwrap();
    let hint_ids = turn
        .context_assembly
        .selected
        .iter()
        .filter(|selection| selection.fragment.source_kind == ContextSourceKind::MemoryHint)
        .map(|selection| selection.fragment.fragment_id.clone())
        .collect::<Vec<_>>();

    assert!(
        hint_ids.contains(&"memory.baz".to_string()),
        "expected memory.baz as a hint, got: {hint_ids:?}"
    );
    assert!(
        turn.retrieved_memory_block
            .contains("=== Associated memories (hints - may or may not be relevant) ===")
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn run_one_turn_uses_default_turn_max_output_tokens() {
    let base_dir = std::env::temp_dir().join(format!("qsf-turn-output-cap-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new(test_config_with_warm_threshold(10, 10));
    let mut memory_snapshot = super::SessionMemorySourceSnapshot::from_fixture(
        "test",
        "test",
        MemoryFixture {
            records: vec![],
            associations: vec![],
        },
    );
    let mut output = Vec::new();
    let client = CapturingOpenAiRecallClient::default();

    run_one_turn(
        &mut context,
        &mut state,
        &state_dir,
        &mut memory_snapshot,
        &client,
        super::TurnRequest {
            user_input: "tell me about volition system design",
            boot_brief_fragment: None,
            max_output_tokens: super::DEFAULT_TURN_MAX_OUTPUT_TOKENS,
        },
        super::TurnConsole {
            output: &mut output,
            color_mode: crate::console::styling::ColorMode::Disabled,
        },
    )
    .unwrap();

    let calls = client.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].max_output_tokens,
        Some(super::DEFAULT_TURN_MAX_OUTPUT_TOKENS)
    );

    fs::remove_dir_all(base_dir).unwrap();
}

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

#[test]
fn mock_model_session_records_turns_events_and_report() {
    let base_dir = std::env::temp_dir().join(format!("qsf-multi-turn-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new(
        "What do you remember about context budgets?\nContinue that thought.\nWhat changed about model roles?\n:quit\n",
    );
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config(5),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("multi-turn-text-loop.md")).unwrap();
    let records = parse_event_records(&events);

    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        3
    );
    assert!(events.contains("ContextAssembled"));
    assert!(events.contains("PromptAssembled"));
    assert!(events.contains("no persistent memory store on cold start"));
    assert_event_order(
        &records,
        EventType::PromptAssembled,
        EventType::ModelRoleRequested,
    );
    assert!(report.contains("Hash prefix status"));
    assert!(report.contains("Cache misses at or above 1024 input tokens"));
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("QSF runtime voice loop")
    );

    let turns = records
        .iter()
        .filter(|record| record.event_type == EventType::TurnCompleted)
        .collect::<Vec<_>>();
    assert_eq!(turns[0].payload["turn"]["index"], 0);
    assert_eq!(turns[1].payload["turn"]["index"], 1);
    assert_eq!(turns[2].payload["turn"]["index"], 2);
    assert_turn_prefix_hashes_are_stable(&turns, true);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn multi_turn_loop_persists_and_resumes_awake_continuation() {
    let base_dir = std::env::temp_dir().join(format!("qsf-continuity-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_source = TestMemorySource;

    let mut first_context = test_context(base_dir.join("first"), "multi-turn-text-loop");
    let mut first_output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut first_context,
        Cursor::new("first turn\n:quit\n"),
        &mut first_output,
        &MockModelClient::default(),
        &memory_source,
        test_config(5),
        &state_dir,
    )
    .unwrap();

    let first_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(first_state.turns.len(), 1);
    let manifest = crate::session::manifest::ContinuityManifest::load_or_default(
        state_dir.join("continuity-manifest.json"),
    )
    .unwrap();
    assert!(manifest.sleep_pending);

    let mut second_context = test_context(base_dir.join("second"), "multi-turn-text-loop");
    let mut second_output = Vec::new();
    let mut second_config = test_config(5);
    second_config.allow_over_limit = true;
    run_with_io_and_components_at_state_dir(
        &mut second_context,
        Cursor::new("second turn\n:quit\n"),
        &mut second_output,
        &MockModelClient::default(),
        &memory_source,
        second_config,
        &state_dir,
    )
    .unwrap();

    let second_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(second_state.turns.len(), 2);
    assert_eq!(second_state.turns[0].index, 0);
    assert_eq!(second_state.turns[1].index, 1);
    assert_eq!(second_state.session_id, first_state.session_id);
    assert!(second_state.live.completed_exchanges.is_empty());
    let resumed_prompt = assemble_prompt_with_summaries_and_project_doc_channel(
        &[],
        &[PromptTurn {
            user_input: &second_state.turns[0].user_input,
            retrieved_memory_block: &second_state.turns[0].retrieved_memory_block,
            recalled_tool_messages: vec![],
            assistant_response: &second_state.turns[0].assistant_response,
        }],
        &second_state.turns[1].user_input,
        &second_state.turns[1].retrieved_memory_block,
        true,
    );
    assert_eq!(
        prior_request_prefix_hash(
            &resumed_prompt.messages,
            second_state.turns[0].message_count
        ),
        Some(second_state.turns[0].full_request_hash)
    );

    let events = fs::read_to_string(second_context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();
    assert_eq!(resumed.payload["mode"], "awake_continuation");
    assert_eq!(
        resumed.payload["previous_session_id"].as_str(),
        Some(first_state.session_id.as_str())
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn live_loop_reinforces_persistent_memory_store_and_emits_events() {
    let base_dir = std::env::temp_dir().join(format!("qsf-live-memory-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_store_path = state_dir.join("memory-store.json");
    let fixture = phase_four_fixture();
    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    store.append_records(fixture.records.clone());
    store.persist().unwrap();

    let memory_source = TestMemorySource;
    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let mut output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut context,
        Cursor::new("context budget memory retrieval\n:quit\n"),
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config(5),
        &state_dir,
    )
    .unwrap();

    let reloaded = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    assert!(
        reloaded
            .contents()
            .records
            .iter()
            .any(|record| record.last_reinforced_at.is_some())
    );
    assert!(reloaded.contents().associations.iter().any(|association| {
        association
            .reason
            .contains("co-retrieved in turn 0 of session")
    }));

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let retrieval_requested = records
        .iter()
        .find(|record| record.event_type == EventType::MemoryRetrievalRequested)
        .unwrap();
    let proposed = records
        .iter()
        .find(|record| record.event_type == EventType::CoRetrievalAssociationsProposed)
        .unwrap();
    let reinforced = records
        .iter()
        .find(|record| record.event_type == EventType::MemoryReinforced)
        .unwrap();
    assert!(
        records
            .iter()
            .any(|record| record.event_type == EventType::MemoryStorePersisted)
    );
    assert_eq!(retrieval_requested.payload["memory_source"], "memory_store");
    assert!(proposed.payload["created_count"].as_u64().unwrap() > 0);
    assert_eq!(reinforced.payload["timestamp_source"], "live_now");
    assert!(reinforced.payload["count"].as_u64().unwrap() > 0);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn live_loop_does_not_reinforce_relevance_skipped_memory() {
    let base_dir = std::env::temp_dir().join(format!("qsf-live-memory-skip-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_store_path = state_dir.join("memory-store.json");
    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    let records = vec![
        MemoryRecord::new(
            "memory.ari",
            MemoryRecordKind::Observation,
            "Assistant name: Ari",
            "The assistant accepted Ari as its name.",
            vec!["assistant_identity", "profile", "name"],
            time::OffsetDateTime::now_utc(),
            1.0,
            0,
            "tests",
            10,
        ),
        MemoryRecord::new(
            "memory.volition",
            MemoryRecordKind::Observation,
            "Volition systems",
            "Volition systems coordinate goals and arbitration.",
            vec!["volition", "goals"],
            time::OffsetDateTime::now_utc(),
            0.6,
            0,
            "tests",
            10,
        ),
    ];
    store.append_records(records.clone());
    store.persist().unwrap();
    let retrieval = retrieve_memories(
        &records,
        &[],
        "tell me about volition goals",
        RetrievalStrategy::KeywordTag,
        8,
    )
    .unwrap();
    assert_eq!(
        crate::memory::retrieved_memory_ids(&retrieval.selected),
        vec!["memory.volition".to_string()]
    );
    assert!(
        retrieval
            .omitted
            .iter()
            .any(|memory| memory.memory.id == "memory.ari"
                && memory.skip_reason.as_deref()
                    == Some(crate::memory::retrieval::RELEVANCE_GATE_SKIP_REASON))
    );

    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let state = SessionState::new(test_config(5));
    crate::session::apply_live_memory_reinforcement(&mut context, &state, &state_dir, &retrieval)
        .unwrap();

    let reloaded = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    let ari = reloaded
        .contents()
        .records
        .iter()
        .find(|record| record.id == "memory.ari")
        .unwrap();
    let volition = reloaded
        .contents()
        .records
        .iter()
        .find(|record| record.id == "memory.volition")
        .unwrap();
    assert_eq!(ari.reinforcement_count, 0);
    assert_eq!(volition.reinforcement_count, 1);

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let reinforced = records
        .iter()
        .find(|record| record.event_type == EventType::MemoryReinforced)
        .unwrap();
    assert_eq!(reinforced.payload["ids"], json!(["memory.volition"]));
    assert_eq!(reinforced.payload["skipped_relevance_count"], 1);
    assert_eq!(
        reinforced.payload["skipped_relevance_ids"],
        json!(["memory.ari"])
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn live_loop_captures_identity_names_to_memory_store() {
    let base_dir = std::env::temp_dir().join(format!("qsf-live-name-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_store_path = state_dir.join("memory-store.json");
    let memory_source = TestMemorySource;
    let model_client = MockModelClient::default().with_fixture(
        ModelRoleId::ConversationalResponder,
        "Absolutely - you can call me Ari.",
    );

    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let mut output = Vec::new();
    run_with_io_and_components_at_state_dir(
            &mut context,
            Cursor::new(
                "I want you to use the name Ari.\nMy name is Lars.\nWhat is your name?\nWhat is my name?\nTell me about volition goals.\n:quit\n",
            ),
            &mut output,
            &model_client,
            &memory_source,
            test_config(6),
            &state_dir,
        )
        .unwrap();

    let store = MemoryStore::load_or_empty(&memory_store_path).unwrap();
    assert_eq!(store.contents().records.len(), 2);
    let ari = store
        .contents()
        .records
        .iter()
        .find(|record| record.title == "Assistant name: Ari")
        .expect("expected Ari live memory");
    let lars = store
        .contents()
        .records
        .iter()
        .find(|record| record.title == "User name: Lars")
        .expect("expected Lars live memory");
    assert_eq!(ari.kind, MemoryRecordKind::Observation);
    assert_eq!(lars.kind, MemoryRecordKind::Observation);
    assert!(ari.tags.iter().any(|tag| tag == "assistant_identity"));
    assert!(lars.tags.iter().any(|tag| tag == "user_identity"));
    assert!(ari.source_reference.contains("live_memory_capture"));
    assert!(lars.source_reference.contains("live_memory_capture"));

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    assert!(events.contains("\"stage\":\"live_memory_capture\""));
    assert!(events.contains("\"candidate_count\":1"));
    let records = parse_event_records(&events);
    let turn_two_retrieval = records
        .iter()
        .find(|record| {
            record.event_type == EventType::MemoryRetrieved && record.payload["turn_index"] == 2
        })
        .expect("expected turn two retrieval event");
    let turn_three_retrieval = records
        .iter()
        .find(|record| {
            record.event_type == EventType::MemoryRetrieved && record.payload["turn_index"] == 3
        })
        .expect("expected turn three retrieval event");
    let unrelated_retrieval = records
        .iter()
        .find(|record| {
            record.event_type == EventType::MemoryRetrieved && record.payload["turn_index"] == 4
        })
        .expect("expected unrelated retrieval event");
    assert_eq!(
        turn_two_retrieval.payload["selected"],
        json!([ari.id.clone()])
    );
    assert_eq!(
        turn_three_retrieval.payload["selected"],
        json!([lars.id.clone()])
    );
    assert_eq!(unrelated_retrieval.payload["selected"], json!([]));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn live_loop_captures_remembered_topic_and_retrieves_it_end_to_end() {
    let base_dir = std::env::temp_dir().join(format!("qsf-live-remember-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_store_path = state_dir.join("memory-store.json");
    let memory_source = EmptyMemorySource;
    let model_client = MockModelClient::default().with_fixture(
            ModelRoleId::ConversationalResponder,
            "Absolutely - you can call me Ari. A good volition system should include needs/drives, goals, arbitration, and continuity.",
        );

    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let mut output = Vec::new();
    run_with_io_and_components_at_state_dir(
            &mut context,
            Cursor::new(
                "I want you to use the name Ari.\nMy name is Lars.\nTell me more what you think how a volition system should work.\nInteresting, please remember this for future discussions!\nWhat is your name?\nWhat is my name?\nWhat did I ask you to remember about volition?\nTell me about volition goals.\n:quit\n",
            ),
            &mut output,
            &model_client,
            &memory_source,
            test_config(10),
            &state_dir,
        )
        .unwrap();

    let store = MemoryStore::load_or_empty(&memory_store_path).unwrap();
    assert_eq!(store.contents().records.len(), 3);
    let ari = store
        .contents()
        .records
        .iter()
        .find(|record| record.title == "Assistant name: Ari")
        .expect("expected Ari live memory");
    let lars = store
        .contents()
        .records
        .iter()
        .find(|record| record.title == "User name: Lars")
        .expect("expected Lars live memory");
    let remembered = store
        .contents()
        .records
        .iter()
        .find(|record| record.title.starts_with("Remembered topic:"))
        .expect("expected remembered-topic live memory");
    assert!(remembered.summary.contains("Topic: volition system."));
    assert!(remembered.summary.contains("Source excerpt:"));
    assert!(remembered.tags.iter().any(|tag| tag == "remembered_topic"));
    assert!(remembered.tags.iter().any(|tag| tag == "volition"));
    assert!(remembered.tags.iter().any(|tag| tag == "system"));
    assert!(remembered.tags.iter().any(|tag| tag == "volition_system"));
    assert!(remembered.source_reference.contains("source-turn-002"));

    let assistant = retrieve_memories(
        &store.contents().records,
        &store.contents().associations,
        "What is your name?",
        RetrievalStrategy::KeywordTag,
        8,
    )
    .unwrap();
    assert_eq!(assistant.selected.len(), 1);
    assert_eq!(assistant.selected[0].memory.id, ari.id);

    let user = retrieve_memories(
        &store.contents().records,
        &store.contents().associations,
        "What is my name?",
        RetrievalStrategy::KeywordTag,
        8,
    )
    .unwrap();
    assert_eq!(user.selected.len(), 1);
    assert_eq!(user.selected[0].memory.id, lars.id);

    let volition = retrieve_memories(
        &store.contents().records,
        &store.contents().associations,
        "What did I ask you to remember about volition?",
        RetrievalStrategy::KeywordTag,
        8,
    )
    .unwrap();
    assert!(
        volition
            .selected
            .iter()
            .any(|memory| memory.memory.id == remembered.id)
    );
    assert!(
        !volition
            .selected
            .iter()
            .any(|memory| memory.memory.id == ari.id)
    );

    let state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(state.turns.len(), 8);
    assert!(
        state.summarized_turns.is_empty(),
        "the QA fixture should not persist warm summaries"
    );

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let unrelated_retrieval = records
        .iter()
        .find(|record| {
            record.event_type == EventType::MemoryRetrieved && record.payload["turn_index"] == 7
        })
        .expect("expected unrelated volition retrieval event");
    assert!(
        !unrelated_retrieval.payload["selected"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == &json!(ari.id)),
        "unrelated volition query must not select Ari"
    );

    let persisted = records
        .iter()
        .filter(|record| record.event_type == EventType::MemoryStorePersisted)
        .collect::<Vec<_>>();
    assert!(persisted.iter().any(|record| {
        record.payload["candidate_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "remembered-topic")
    }));
    let remember_trace = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
    assert!(remember_trace.contains("live-memory-capture"));
    assert!(remember_trace.contains("remember-this"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn live_loop_strengthens_existing_persistent_association() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-live-memory-strengthen-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_store_path = state_dir.join("memory-store.json");
    let fixture = phase_four_fixture();
    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    store.append_records(fixture.records.clone());
    store.append_associations([crate::memory::Association::new(
        "memory.context-budget",
        "memory.associative-memory",
        0.4,
        "existing edge before live turn",
        time::OffsetDateTime::from(std::time::SystemTime::UNIX_EPOCH),
    )]);
    store.persist().unwrap();

    let memory_source = TestMemorySource;
    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let mut output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut context,
        Cursor::new("context budget memory retrieval\n:quit\n"),
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config(5),
        &state_dir,
    )
    .unwrap();

    let reloaded = crate::memory::MemoryStore::load_or_empty(&memory_store_path).unwrap();
    let matching_associations = reloaded
        .contents()
        .associations
        .iter()
        .filter(|association| {
            (association.from_memory_id == "memory.context-budget"
                && association.to_memory_id == "memory.associative-memory")
                || (association.from_memory_id == "memory.associative-memory"
                    && association.to_memory_id == "memory.context-budget")
        })
        .collect::<Vec<_>>();
    assert_eq!(matching_associations.len(), 1);
    assert!((matching_associations[0].weight - 0.45).abs() < 1e-9);

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let proposed = records
        .iter()
        .find(|record| record.event_type == EventType::CoRetrievalAssociationsProposed)
        .unwrap();
    assert!(proposed.payload["strengthened_count"].as_u64().unwrap() > 0);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn awake_continuation_config_drift_downgrades_to_cold_start() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-continuity-config-drift-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let memory_source = TestMemorySource;

    let mut first_context = test_context(base_dir.join("first"), "multi-turn-text-loop");
    let mut first_output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut first_context,
        Cursor::new("first turn\n:quit\n"),
        &mut first_output,
        &MockModelClient::default(),
        &memory_source,
        test_config(5),
        &state_dir,
    )
    .unwrap();

    let first_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    let mut changed_config = test_config(5);
    changed_config.model_id = "changed-model".to_string();

    let mut second_context = test_context(base_dir.join("second"), "multi-turn-text-loop");
    let mut second_output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut second_context,
        Cursor::new("second turn\n:quit\n"),
        &mut second_output,
        &MockModelClient::default(),
        &memory_source,
        changed_config,
        &state_dir,
    )
    .unwrap();

    let second_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(second_state.turns.len(), 1);
    assert_eq!(second_state.turns[0].index, 0);
    assert_ne!(second_state.session_id, first_state.session_id);

    let events = fs::read_to_string(second_context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();
    assert_eq!(resumed.payload["mode"], "cold_start");
    assert_eq!(resumed.payload["classified_mode"], "awake_continuation");
    assert_eq!(resumed.payload["config_changed"], true);
    assert_eq!(resumed.payload["downgraded_for_config"], true);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn consolidated_brief_resume_starts_fresh_with_previous_session_id() {
    let base_dir = std::env::temp_dir().join(format!("qsf-continuity-brief-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/text-loop");
    let config = test_config(5);
    let mut previous = SessionState::new_with_id("brief-prev".to_string(), config.clone());
    previous.turns.push(test_turn(0));
    previous.turns.push(test_turn(1));
    crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
    fs::create_dir_all(&state_dir).unwrap();
    fs::write(
        state_dir.join("consolidated-brief.json"),
        serde_json::to_string_pretty(&crate::sleep::commit::ConsolidatedBrief {
            previous_session_summary: "The previous session established reducer purity."
                .to_string(),
            future_context_hints: vec!["Carry reducer purity forward.".to_string()],
            open_questions: vec!["How should sleep summarize associations?".to_string()],
            promoted_count: 1,
            new_associations_count: 0,
        })
        .unwrap(),
    )
    .unwrap();
    crate::session::manifest::ContinuityManifest {
        current_session_id: Some(previous.session_id.clone()),
        current_session_state_path: Some(PathBuf::from("session-state.json")),
        last_sleep_run_id: Some("sleep-1".to_string()),
        last_sleep_brief_path: Some(PathBuf::from("consolidated-brief.json")),
        last_sleep_consumed_session_id: Some(previous.session_id.clone()),
        sleep_pending: false,
        resume_mode: crate::session::manifest::ResumeMode::ConsolidatedBrief,
        ..crate::session::manifest::ContinuityManifest::default()
    }
    .persist(state_dir.join("continuity-manifest.json"))
    .unwrap();

    let memory_source = TestMemorySource;
    let mut context = test_context(base_dir.join("brief"), "multi-turn-text-loop");
    let mut output = Vec::new();
    run_with_io_and_components_at_state_dir(
        &mut context,
        Cursor::new("fresh after sleep\n:quit\n"),
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        config,
        &state_dir,
    )
    .unwrap();

    let resumed_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(resumed_state.turns.len(), 1);
    assert_eq!(resumed_state.turns[0].index, 0);
    assert!(
        resumed_state.turns[0]
            .retrieved_memory_block
            .contains("Previous session summary:")
    );
    assert!(
        resumed_state.turns[0]
            .retrieved_memory_block
            .contains("The previous session established reducer purity.")
    );
    assert_eq!(
        resumed_state.previous_session_id.as_deref(),
        Some(previous.session_id.as_str())
    );
    assert_ne!(resumed_state.session_id, previous.session_id);

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();
    assert_eq!(resumed.payload["mode"], "consolidated_brief");
    assert_eq!(
        resumed.payload["previous_session_id"].as_str(),
        Some(previous.session_id.as_str())
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn voice_and_text_runs_share_the_same_session_in_both_orders() {
    let voice_then_text_base =
        std::env::temp_dir().join(format!("qsf-voice-text-order-a-{}", Uuid::new_v4()));
    let text_then_voice_base =
        std::env::temp_dir().join(format!("qsf-voice-text-order-b-{}", Uuid::new_v4()));

    run_voice_then_text(&voice_then_text_base);
    run_text_then_voice(&text_then_voice_base);

    fs::remove_dir_all(voice_then_text_base).unwrap();
    fs::remove_dir_all(text_then_voice_base).unwrap();
}

#[test]
fn legacy_text_loop_boot_writes_upgraded_state_into_shared_session_dir() {
    let base_dir = std::env::temp_dir().join(format!("qsf-legacy-text-loop-{}", Uuid::new_v4()));
    let legacy_state_dir = base_dir.join("state/text-loop");
    let shared_state_dir = base_dir.join("state/session");
    let config = shared_text_config();
    let mut previous = SessionState::new_with_id("legacy-text-session".to_string(), config);
    previous.turns.push(test_turn(0));
    crate::session::persistence::persist_session_state(&previous, &legacy_state_dir).unwrap();
    let mut legacy_store =
        crate::memory::MemoryStore::load_or_empty(legacy_state_dir.join("memory-store.json"))
            .unwrap();
    legacy_store.contents_mut().records.push(memory_record(
        "memory.legacy.text",
        "Legacy text memory",
        "Legacy text-loop memory must survive the shared session migration.",
        vec!["legacy", "text", "session"],
        16,
    ));
    legacy_store.persist().unwrap();
    let legacy_session_before =
        fs::read_to_string(legacy_state_dir.join("session-state.json")).unwrap();
    let legacy_memory_before =
        fs::read_to_string(legacy_state_dir.join("memory-store.json")).unwrap();
    crate::session::manifest::ContinuityManifest {
        current_session_id: Some(previous.session_id.clone()),
        current_session_state_path: Some(PathBuf::from("session-state.json")),
        sleep_pending: true,
        resume_mode: crate::session::manifest::ResumeMode::AwakeContinuation,
        ..crate::session::manifest::ContinuityManifest::default()
    }
    .persist(legacy_state_dir.join("continuity-manifest.json"))
    .unwrap();

    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    run_with_io_and_components_at_state_resolution(
        &mut context,
        Cursor::new(":quit\n"),
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        shared_text_config(),
        StateDirectoryResolution {
            resume_state_dir: legacy_state_dir.clone(),
            persist_state_dir: shared_state_dir.clone(),
            legacy_fallback_used: true,
        },
    )
    .unwrap();

    let shared_state = crate::session::persistence::load_session_state(
        shared_state_dir.join("session-state.json"),
    )
    .unwrap();
    let legacy_session_after =
        fs::read_to_string(legacy_state_dir.join("session-state.json")).unwrap();
    let legacy_memory_after =
        fs::read_to_string(legacy_state_dir.join("memory-store.json")).unwrap();
    let shared_memory = fs::read_to_string(shared_state_dir.join("memory-store.json")).unwrap();
    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();
    let legacy_state_dir_string = legacy_state_dir.display().to_string();

    assert_eq!(shared_state.session_id, previous.session_id);
    assert_eq!(shared_state.turns.len(), 1);
    assert_eq!(
        shared_state.schema_version,
        crate::session::SESSION_STATE_SCHEMA_VERSION
    );
    assert_eq!(legacy_session_after, legacy_session_before);
    assert_eq!(legacy_memory_after, legacy_memory_before);
    assert!(shared_memory.contains("memory.legacy.text"));
    assert_eq!(shared_memory, legacy_memory_before);
    assert_eq!(resumed.payload["legacy_fallback_used"], true);
    assert_eq!(
        resumed.payload["resume_state_dir"].as_str(),
        Some(legacy_state_dir_string.as_str())
    );
    assert!(shared_state_dir.join("continuity-manifest.json").exists());

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn legacy_memory_snapshot_does_not_materialize_shared_dir_before_commit() {
    let base_dir = std::env::temp_dir().join(format!("qsf-legacy-memory-boot-{}", Uuid::new_v4()));
    let legacy_state_dir = base_dir.join("state/text-loop");
    let shared_state_dir = base_dir.join("state/session");
    crate::memory::MemoryStore::load_or_empty(legacy_state_dir.join("memory-store.json"))
        .unwrap()
        .persist()
        .unwrap();
    let memory_source = TestMemorySource;
    let mut context = test_context(base_dir.join("run"), "multi-turn-text-loop");

    let snapshot = super::load_session_memory_snapshot(
        &mut context,
        &memory_source,
        &legacy_state_dir,
        &shared_state_dir,
    )
    .unwrap();

    assert_eq!(
        snapshot.source_reference,
        legacy_state_dir
            .join("memory-store.json")
            .display()
            .to_string()
    );
    assert!(!shared_state_dir.exists());

    fs::remove_dir_all(base_dir).unwrap();
}

fn run_voice_then_text(base_dir: &Path) {
    let state_dir = base_dir.join("state/session");
    let voice_memory_source = SharedVoiceMemorySource::new(&state_dir);
    let mut voice_context = test_context(base_dir.join("voice"), "voice-loop");
    crate::experiments::text_owned_voice_loop::TextOwnedVoiceLoopExperiment
        .run_with_components_and_memory_source_at_state_dirs_with_config(
            &mut voice_context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &voice_memory_source,
            crate::experiments::text_owned_voice_loop::VoiceLoopSessionConfig {
                state_resolution: StateDirectoryResolution {
                    resume_state_dir: state_dir.clone(),
                    persist_state_dir: state_dir.clone(),
                    legacy_fallback_used: false,
                },
                config: shared_text_config(),
            },
        )
        .unwrap();
    let voice_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    let mut text_context = test_context(base_dir.join("text"), "multi-turn-text-loop");
    let mut output = Vec::new();
    let text_memory_source = TestMemorySource;
    run_with_io_and_components_at_state_resolution(
        &mut text_context,
        Cursor::new("follow-up from text\n:quit\n"),
        &mut output,
        &MockModelClient::default(),
        &text_memory_source,
        shared_text_config(),
        StateDirectoryResolution {
            resume_state_dir: state_dir.clone(),
            persist_state_dir: state_dir.clone(),
            legacy_fallback_used: false,
        },
    )
    .unwrap();
    let text_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    assert_eq!(voice_state.turns.len(), 1);
    assert_eq!(text_state.turns.len(), 2);
    assert_eq!(text_state.session_id, voice_state.session_id);
    assert!(state_dir.join("continuity-manifest.json").exists());
}

fn run_text_then_voice(base_dir: &Path) {
    let state_dir = base_dir.join("state/session");
    let mut text_context = test_context(base_dir.join("text"), "multi-turn-text-loop");
    let mut output = Vec::new();
    let text_memory_source = TestMemorySource;
    run_with_io_and_components_at_state_resolution(
        &mut text_context,
        Cursor::new("follow-up from text\n:quit\n"),
        &mut output,
        &MockModelClient::default(),
        &text_memory_source,
        shared_text_config(),
        StateDirectoryResolution {
            resume_state_dir: state_dir.clone(),
            persist_state_dir: state_dir.clone(),
            legacy_fallback_used: false,
        },
    )
    .unwrap();
    let text_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    let voice_memory_source = SharedVoiceMemorySource::new(&state_dir);
    let mut voice_context = test_context(base_dir.join("voice"), "voice-loop");
    crate::experiments::text_owned_voice_loop::TextOwnedVoiceLoopExperiment
        .run_with_components_and_memory_source_at_state_dirs_with_config(
            &mut voice_context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &voice_memory_source,
            crate::experiments::text_owned_voice_loop::VoiceLoopSessionConfig {
                state_resolution: StateDirectoryResolution {
                    resume_state_dir: state_dir.clone(),
                    persist_state_dir: state_dir.clone(),
                    legacy_fallback_used: false,
                },
                config: shared_text_config(),
            },
        )
        .unwrap();
    let voice_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    assert_eq!(text_state.turns.len(), 1);
    assert_eq!(voice_state.turns.len(), 2);
    assert_eq!(voice_state.session_id, text_state.session_id);
    assert!(state_dir.join("continuity-manifest.json").exists());
}

fn shared_text_config() -> SessionConfig {
    SessionConfig {
        model_id: DEFAULT_SESSION_MODEL.to_string(),
        max_turns: 10,
        warm_threshold: 6,
        allow_over_limit: false,
        memory_source: MemorySourceConfig {
            source: "phase_four_fixture".to_string(),
            file: None,
        },
    }
}

#[test]
fn warm_threshold_summarizes_oldest_turns_without_dropping_turn_records() {
    let base_dir = std::env::temp_dir().join(format!("qsf-warm-tier-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("multi-turn-text-loop.md")).unwrap();
    let records = parse_event_records(&events);

    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        3
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnSummarized)
            .count(),
        1
    );
    assert!(events.contains("session_turn_summarizer"));
    assert!(report.contains("Warm summaries produced: `1`"));
    assert!(report.contains("The user and assistant discussed QSF session continuity"));
    let summary_event = records
        .iter()
        .find(|record| record.event_type == EventType::TurnSummarized)
        .unwrap();
    assert_eq!(summary_event.payload["turn_index"], 0);
    assert_eq!(summary_event.payload["summary"]["turn_index"], 0);
    assert_eq!(
        summary_event.payload["summary"]["summarized_after_turn_index"],
        2
    );
    assert_eq!(summary_event.payload["summary"]["model_id"], "gpt-5.4-nano");
    assert!(summary_event.payload["summary"]["summary"].is_string());

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn warm_summary_retry_succeeds_after_truncation() {
    let base_dir = std::env::temp_dir().join(format!("qsf-warm-summary-retry-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedSummarizerClient::new(vec![
        SummaryReply::new("Partial warm summary.", "max_tokens"),
        SummaryReply::new(
            "The user and assistant discussed QSF session continuity in one aged-out turn.",
            "stop",
        ),
    ]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let state = crate::session::persistence::load_session_state(
        context.run_dir().join("state/text-loop/session-state.json"),
    )
    .unwrap();
    assert_eq!(state.summarized_turns.len(), 1);
    assert_eq!(
        state.summarized_turns[0].summary,
        "The user and assistant discussed QSF session continuity in one aged-out turn."
    );

    let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
    assert!(traces.contains("\"finish_reason\":\"max_tokens\""));
    assert!(traces.contains("\"finish_reason\":\"stop\""));

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    assert_eq!(
        records
            .iter()
            .filter(|record| {
                record.event_type == EventType::ModelRoleRequested
                    && record.payload["role_id"] == "session_turn_summarizer"
            })
            .count(),
        2
    );
    assert_eq!(
        client.summarizer_max_output_tokens(),
        vec![
            Some(ageing::WARM_SUMMARY_MAX_OUTPUT_TOKENS),
            Some(ageing::WARM_SUMMARY_RETRY_MAX_OUTPUT_TOKENS)
        ]
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::ErrorOccurred)
            .count(),
        0
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn warm_summary_double_truncation_leaves_turn_unsummarized() {
    let base_dir = std::env::temp_dir().join(format!("qsf-warm-summary-fail-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let store_path = context.run_dir().join("state/text-loop/memory-store.json");
    MemoryStore::load_or_empty(&store_path)
        .unwrap()
        .persist()
        .unwrap();
    let client = SequencedSummarizerClient::new(vec![
        SummaryReply::new("Partial warm summary.", "max_tokens"),
        SummaryReply::new("Still truncated.", "max_tokens"),
    ]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let state = crate::session::persistence::load_session_state(
        context.run_dir().join("state/text-loop/session-state.json"),
    )
    .unwrap();
    assert_eq!(state.summarized_turns.len(), 0);
    let store = MemoryStore::load_or_empty(&store_path).unwrap();
    assert!(
        store
            .contents()
            .processed_ranges
            .iter()
            .any(
                |range| range.kind == qsf_memory::ProcessedRangeKind::LiveBatch
                    && range.first_turn_index == 0
                    && range.last_turn_index == 0
            )
    );

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnSummarized)
            .count(),
        0
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| {
                record.event_type == EventType::ModelRoleRequested
                    && record.payload["role_id"] == "session_turn_summarizer"
            })
            .count(),
        2
    );

    let error_event = records
        .iter()
        .find(|record| {
            record.event_type == EventType::ErrorOccurred
                && record.payload["stage"] == "session-turn-summarization"
        })
        .unwrap();
    assert_eq!(error_event.payload["session_id"], state.session_id);
    assert_eq!(error_event.payload["turn_index"], 0);
    assert!(
        error_event.payload["error"]
            .as_str()
            .unwrap()
            .contains("truncated after retry")
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn summarize_aged_turns_returns_empty_for_inverted_range() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-inverted-summary-range-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new(test_config(10));
    state.turns = vec![test_turn(0), test_turn(1)];

    let summaries =
        ageing::summarize_aged_turns(&mut context, &state, 1, 0, &MockModelClient::default())
            .unwrap();

    assert!(summaries.is_empty());

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn recall_tool_expands_summarized_turn_and_freezes_tool_message() {
    let base_dir = std::env::temp_dir().join(format!("qsf-recall-tool-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\nplease recall turn 0\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("multi-turn-text-loop.md")).unwrap();
    let records = parse_event_records(&events);
    let turn_records = records
        .iter()
        .filter(|record| record.event_type == EventType::TurnCompleted)
        .collect::<Vec<_>>();
    let recalled_turn =
        serde_json::from_value::<Turn>(turn_records[3].payload["turn"].clone()).unwrap();
    let tool_requested = records
        .iter()
        .find(|record| record.event_type == EventType::ToolRequested)
        .unwrap();
    let tool_completed = records
        .iter()
        .find(|record| record.event_type == EventType::ToolCompleted)
        .unwrap();

    assert!(events.contains("ToolRequested"));
    assert!(events.contains("ToolCompleted"));
    assert_eq!(tool_requested.payload["category"], "compute_only");
    assert_eq!(tool_requested.payload["side_effect_level"], "none");
    assert_eq!(tool_completed.payload["category"], "compute_only");
    assert_eq!(tool_completed.payload["side_effect_level"], "none");
    assert_eq!(recalled_turn.recalled_turns.len(), 1);
    assert_eq!(recalled_turn.recalled_turns[0].turn_id, 0);
    assert!(
        recalled_turn.recalled_turns[0]
            .verbatim_text
            .contains("[Turn 0]")
    );
    assert!(report.contains("Recall tool executions: `1`"));
    assert!(report.contains("| 3 | 0 | `mock-recall-0`"));

    assert_turn_prefix_hashes_are_stable(&turn_records, true);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn calculator_tool_answers_arithmetic_turn_through_follow_up() {
    let base_dir = std::env::temp_dir().join(format!("qsf-calculator-tool-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("what is 1231231+12342134?\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let tool_requested = records
        .iter()
        .find(|record| {
            record.event_type == EventType::ToolRequested
                && record.payload["tool_name"] == CALCULATOR_TOOL_NAME
        })
        .unwrap();
    let tool_completed = records
        .iter()
        .find(|record| {
            record.event_type == EventType::ToolCompleted
                && record.payload["tool_name"] == CALCULATOR_TOOL_NAME
        })
        .unwrap();

    assert!(output.contains("The result is 13573365."));
    assert_eq!(tool_requested.payload["input"], "1231231+12342134");
    assert_eq!(tool_requested.payload["category"], "compute_only");
    assert_eq!(tool_completed.payload["category"], "compute_only");
    assert!(events.contains("mock-calculator-0"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn openai_recall_path_preserves_tool_call_id_across_batched_follow_up() {
    let base_dir = std::env::temp_dir().join(format!("qsf-openai-recall-path-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\nplease recall turn 0\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = CapturingOpenAiRecallClient::default();

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let calls = client.calls.lock().unwrap().clone();
    let tool_call_index = calls
        .iter()
        .position(|call| {
            call.role_id == ModelRoleId::ConversationalResponder
                && call
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == crate::models::ModelMessageRole::User)
                    .map(|message| message.content.to_ascii_lowercase().contains("recall turn"))
                    .unwrap_or(false)
        })
        .expect("expected one conversational request to ask for recall");
    assert!(tool_call_index + 1 < calls.len());

    let first_call = &calls[tool_call_index];
    assert_eq!(first_call.role_id, ModelRoleId::ConversationalResponder);
    assert_eq!(
        first_call.tools,
        vec![
            RECALL_TURN_TOOL_NAME.to_string(),
            CALCULATOR_TOOL_NAME.to_string(),
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ]
    );
    assert!(first_call.messages.iter().any(|message| message.role
        == crate::models::ModelMessageRole::User
        && message.content.to_ascii_lowercase().contains("recall turn")));

    let second_call = &calls[tool_call_index + 1];
    assert_eq!(second_call.role_id, ModelRoleId::ConversationalResponder);
    assert_eq!(
        second_call.tools,
        vec![
            RECALL_TURN_TOOL_NAME.to_string(),
            CALCULATOR_TOOL_NAME.to_string(),
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ]
    );
    let tool_message_index = second_call
        .messages
        .iter()
        .position(|message| message.role == crate::models::ModelMessageRole::Tool)
        .unwrap();
    assert!(tool_message_index > 0);
    let assistant_tool_call_message = &second_call.messages[tool_message_index - 1];
    assert_eq!(
        assistant_tool_call_message.role,
        crate::models::ModelMessageRole::Assistant
    );
    assert_eq!(assistant_tool_call_message.tool_calls.len(), 1);
    assert_eq!(
        assistant_tool_call_message.tool_calls[0].call_id,
        "openai-recall-0"
    );
    assert_eq!(
        second_call
            .messages
            .iter()
            .filter(|message| message.role == crate::models::ModelMessageRole::Tool)
            .count(),
        1
    );
    let tool_message = &second_call.messages[tool_message_index];
    assert_eq!(
        tool_message.tool_call_id.as_deref(),
        Some("openai-recall-0")
    );
    assert!(tool_message.content.contains("[recall_turn]"));

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    assert!(events.contains("ToolRequested"));
    assert!(events.contains("ToolCompleted"));
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::PromptAssembled)
            .count(),
        5
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn recall_tool_failure_does_not_append_turn() {
    let base_dir = std::env::temp_dir().join(format!("qsf-recall-tool-fail-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\nplease recall turn 0\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &MockModelClient::default(),
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);

    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        1
    );
    assert!(events.contains("ToolFailed"));
    assert!(events.contains("not summarized"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn follow_up_tool_calls_fail_without_appending_turn() {
    let base_dir = std::env::temp_dir().join(format!("qsf-recall-tool-loop-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("one\ntwo\nthree\nplease recall turn 0\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &RepeatingToolCallClient,
        &memory_source,
        test_config_with_warm_threshold(10, 2),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);

    assert_eq!(
        records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        3
    );
    assert!(events.contains("bounded-tool-loop"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn responder_can_search_then_read_across_two_tool_batches() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-project-doc-search-read-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("please summarize the project docs\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedResponderClient::new(vec![
        PlannedResponderResponse::tool_call(
            "search the docs",
            "project-doc-search-0",
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            json!({ "query": "vision" }),
        ),
        PlannedResponderResponse::tool_call(
            "read the doc",
            "project-doc-read-0",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({
                "path": "docs/ProjectFrame/ProjectVision.md",
                "focus": "vision",
                "max_tokens": 400
            }),
        ),
        PlannedResponderResponse::text(
            "The project's accepted framing says to keep the responder grounded in project docs.",
        ),
    ]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let calls = client.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].role_id, ModelRoleId::ConversationalResponder);
    assert_eq!(calls[0].tools, responder_tool_names());
    assert_eq!(calls[1].tools, responder_tool_names());
    assert!(calls[2].tools.is_empty());
    assert!(calls.iter().all(|call| {
        call.messages
            .first()
            .map(|message| {
                message.role == crate::models::ModelMessageRole::System
                    && message.content.contains("search_project_docs")
                    && message.content.contains("kind and maturity")
            })
            .unwrap_or(false)
    }));
    assert!(
        calls[2]
            .messages
            .first()
            .unwrap()
            .content
            .contains("search_project_docs")
    );

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);
    assert_eq!(
        event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        1
    );
    assert!(event_records.iter().any(|record| {
        record.event_type == EventType::ToolCompleted
            && record.payload["tool_name"] == SEARCH_PROJECT_DOCS_TOOL_NAME
    }));
    assert!(event_records.iter().any(|record| {
        record.event_type == EventType::ToolCompleted
            && record.payload["tool_name"] == READ_PROJECT_DOC_TOOL_NAME
    }));

    let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
    let trace_records = parse_trace_records(&traces);
    let search_trace = trace_records
        .iter()
        .find(|record| {
            record.operation == "project_doc_search"
                && !record.details["refused"].as_bool().unwrap_or(true)
        })
        .expect("search trace present");
    let read_trace = trace_records
        .iter()
        .find(|record| {
            record.operation == "project_doc_read"
                && !record.details["refused"].as_bool().unwrap_or(true)
        })
        .expect("read trace present");
    assert_eq!(
        search_trace.details["turn_index"],
        read_trace.details["turn_index"]
    );
    assert_eq!(search_trace.details["refused"], false);
    assert_eq!(read_trace.details["refused"], false);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn responder_reuses_project_doc_budget_across_tool_batches() {
    let base_dir = std::env::temp_dir().join(format!("qsf-project-doc-budget-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("please summarize the project docs\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedResponderClient::new(vec![
        PlannedResponderResponse::tool_call(
            "read the doc",
            "project-doc-read-0",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({
                "path": "docs/ProjectFrame/ProjectVision.md",
                "focus": "vision",
                "max_tokens": 400
            }),
        ),
        PlannedResponderResponse::tool_call(
            "read the doc again",
            "project-doc-read-1",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({
                "path": "docs/ProjectFrame/ProjectVision.md",
                "focus": "vision",
                "max_tokens": 400
            }),
        ),
        PlannedResponderResponse::text("The second read was refused by the per-turn cap."),
    ]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let calls = client.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].tools, responder_tool_names());
    assert_eq!(calls[1].tools, responder_tool_names());
    assert!(calls[2].tools.is_empty());
    assert!(calls[2].messages.iter().any(|message| {
        message.role == crate::models::ModelMessageRole::Tool
            && message.content.contains("per_turn_cap")
    }));

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);
    assert_eq!(
        event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        1
    );

    let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
    let trace_records = parse_trace_records(&traces);
    let refusal_trace = trace_records
        .iter()
        .find(|record| record.operation == "project_doc_read" && record.details["refused"] == true)
        .expect("refusal trace present");
    assert_eq!(refusal_trace.details["cap"], json!(1));
    assert_eq!(refusal_trace.details["attempted_count"], json!(2));
    assert_eq!(
        refusal_trace.details["tool_name"],
        READ_PROJECT_DOC_TOOL_NAME
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn third_tool_batch_is_rejected_without_appending_turn() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-project-doc-bounded-loop-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("please summarize the project docs\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedResponderClient::new(vec![
        PlannedResponderResponse::tool_call(
            "search the docs",
            "project-doc-search-0",
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            json!({ "query": "vision" }),
        ),
        PlannedResponderResponse::tool_call(
            "read the doc",
            "project-doc-read-0",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({
                "path": "docs/ProjectFrame/ProjectVision.md",
                "focus": "vision",
                "max_tokens": 400
            }),
        ),
        PlannedResponderResponse::tool_call(
            "search again",
            "project-doc-search-1",
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            json!({ "query": "vision" }),
        ),
    ]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);
    assert!(event_records.iter().any(|record| {
        record.event_type == EventType::ErrorOccurred
            && record.payload["stage"] == "bounded-tool-loop"
    }));
    assert_eq!(
        event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        0
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn ordinary_no_tool_response_still_completes_one_turn() {
    let base_dir = std::env::temp_dir().join(format!("qsf-project-doc-no-tool-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("what are you?\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedResponderClient::new(vec![PlannedResponderResponse::text(
        "I am a conversational responder.",
    )]);

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let calls = client.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tools, responder_tool_names());
    assert!(
        calls[0]
            .messages
            .first()
            .unwrap()
            .content
            .contains("search_project_docs")
    );

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);
    assert_eq!(
        event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        1
    );

    let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
    let trace_records = parse_trace_records(&traces);
    assert!(
        trace_records
            .iter()
            .all(|record| !record.operation.starts_with("project_doc_"))
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn self_question_battery_drives_real_bounded_loop() {
    let battery = load_self_question_battery();

    for question in battery.questions {
        let outcome = run_self_question_battery_question(&question);

        assert_eq!(
            outcome.calls.len(),
            question.tool_calls.len() + 1,
            "question {}: expected one provider call per tool round plus the final answer",
            question.id
        );

        for tool_call in &question.tool_calls {
            let call_index = tool_call.round - 1;
            assert_eq!(
                outcome.calls[call_index].tools,
                responder_tool_names(),
                "question {}: round {} should advertise the responder tools",
                question.id,
                tool_call.round
            );
        }

        if question.tool_calls.len() == super::MAX_RESPONDER_TOOL_ROUNDS_PER_TURN {
            assert!(
                outcome
                    .calls
                    .last()
                    .expect("final call present")
                    .tools
                    .is_empty(),
                "question {}: the final answer after the second tool round should stop advertising tools",
                question.id
            );
        } else {
            assert_eq!(
                outcome.calls.last().expect("final call present").tools,
                responder_tool_names(),
                "question {}: the final or only call should still advertise the responder tools when the second tool round has not been reached",
                question.id
            );
        }

        for call in &outcome.calls {
            let system_message = call
                .messages
                .first()
                .expect("provider request should include a system prompt");
            assert_eq!(
                system_message.role,
                crate::models::ModelMessageRole::System,
                "question {}: every provider call should begin with the system prompt",
                question.id
            );
            assert!(
                system_message.content.contains("search_project_docs"),
                "question {}: the voicing block should mention search_project_docs on every provider call",
                question.id
            );
            assert!(
                system_message.content.contains("kind and maturity"),
                "question {}: the voicing block should mention kind and maturity on every provider call",
                question.id
            );
        }

        let turn_completed_records = outcome
            .event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .collect::<Vec<_>>();
        assert_eq!(
            turn_completed_records.len(),
            1,
            "question {}: each battery question should complete exactly one turn",
            question.id
        );

        assert_eq!(
            outcome.reply, question.reply,
            "question {}: the scripted reply should round-trip through the turn payload",
            question.id
        );

        let expected_tool_names = question
            .tool_calls
            .iter()
            .map(|call| call.tool.as_str())
            .collect::<Vec<_>>();

        for tool_name in &expected_tool_names {
            assert!(
                outcome.event_records.iter().any(|record| {
                    record.event_type == EventType::ToolCompleted
                        && record.payload["tool_name"] == *tool_name
                }),
                "question {}: expected a ToolCompleted event for {tool_name}",
                question.id
            );
        }

        let project_doc_traces = outcome
            .trace_records
            .iter()
            .filter(|record| record.operation.starts_with("project_doc_"))
            .collect::<Vec<_>>();

        if question.tool_calls.is_empty() {
            assert!(
                project_doc_traces.is_empty(),
                "question {}: the off-topic control should not emit project-doc traces",
                question.id
            );
            assert!(
                outcome.event_records.iter().all(|record| {
                    !(record.event_type == EventType::ToolCompleted
                        && matches!(record.payload["tool_name"].as_str(), Some(name) if name == SEARCH_PROJECT_DOCS_TOOL_NAME || name == READ_PROJECT_DOC_TOOL_NAME))
                }),
                "question {}: the off-topic control should not complete any project-doc tools",
                question.id
            );
        } else {
            for tool_call in &question.tool_calls {
                let operation = match tool_call.tool.as_str() {
                    SEARCH_PROJECT_DOCS_TOOL_NAME => "project_doc_search",
                    READ_PROJECT_DOC_TOOL_NAME => "project_doc_read",
                    other => panic!("question {}: unexpected tool {other}", question.id),
                };
                let trace = outcome
                    .trace_records
                    .iter()
                    .find(|record| record.operation == operation)
                    .unwrap_or_else(|| {
                        panic!("question {}: missing trace for {}", question.id, operation)
                    });
                assert_eq!(
                    trace.details["refused"], false,
                    "question {}: {} should have executed successfully",
                    question.id, operation
                );
                assert_eq!(
                    trace.details["turn_index"],
                    project_doc_traces
                        .first()
                        .expect("project-doc trace present")
                        .details["turn_index"],
                    "question {}: all project-doc traces in the turn should share the same turn_index",
                    question.id
                );

                match tool_call.tool.as_str() {
                    SEARCH_PROJECT_DOCS_TOOL_NAME => {
                        assert_eq!(
                            trace.details["arguments"]["query"], tool_call.arguments["query"],
                            "question {}: search trace should preserve the scripted query",
                            question.id
                        );
                    }
                    READ_PROJECT_DOC_TOOL_NAME => {
                        assert_eq!(
                            trace.details["arguments"]["path"], tool_call.arguments["path"],
                            "question {}: read trace should preserve the scripted path",
                            question.id
                        );
                    }
                    _ => unreachable!(),
                }
            }

            if question.id == "framing_search_then_read" {
                let search_trace = outcome
                    .trace_records
                    .iter()
                    .find(|record| record.operation == "project_doc_search")
                    .expect("search trace present");
                let read_trace = outcome
                    .trace_records
                    .iter()
                    .find(|record| record.operation == "project_doc_read")
                    .expect("read trace present");
                assert_eq!(
                    search_trace.details["turn_index"], read_trace.details["turn_index"],
                    "question {}: search and read should share one turn_index",
                    question.id
                );
            }
        }

        assert_reply_expectations(
            &outcome.reply,
            &question.expected_reply_contains,
            &question.expected_reply_must_not_contain,
            &question.id,
        );

        fs::remove_dir_all(outcome.base_dir).unwrap();
    }
}

#[test]
fn report_marks_warm_invalidation_then_stable_prefix_resume() {
    let config = test_config_with_warm_threshold(5, 2);
    let summary = TurnSummary {
        turn_index: 0,
        summarized_after_turn_index: 0,
        summary: "The first turn was summarized.".to_string(),
        model_id: "gpt-5.4-nano".to_string(),
        model_latency_ms: 0,
        input_tokens: 0,
        output_tokens: 0,
    };
    let turn0_prompt =
        assemble_prompt_with_summaries_and_project_doc_channel(&[], &[], "input 0", "", true);
    let turn0 = test_turn_with_hash(
        0,
        turn0_prompt.full_request_hash,
        turn0_prompt.message_count,
    );
    let turn1_prompt = assemble_prompt_with_summaries_and_project_doc_channel(
        &[PromptTurnSummary {
            turn_index: 0,
            summary: &summary.summary,
        }],
        &[],
        "input 1",
        "",
        true,
    );
    let turn1 = test_turn_with_hash(
        1,
        turn1_prompt.full_request_hash,
        turn1_prompt.message_count,
    );
    let turn2_prompt = assemble_prompt_with_summaries_and_project_doc_channel(
        &[PromptTurnSummary {
            turn_index: 0,
            summary: &summary.summary,
        }],
        &[PromptTurn {
            user_input: &turn1.user_input,
            retrieved_memory_block: &turn1.retrieved_memory_block,
            recalled_tool_messages: vec![],
            assistant_response: &turn1.assistant_response,
        }],
        "input 2",
        "",
        true,
    );
    let turn2 = test_turn_with_hash(
        2,
        turn2_prompt.full_request_hash,
        turn2_prompt.message_count,
    );
    let state = SessionState {
        turns: vec![turn0, turn1, turn2],
        summarized_turns: vec![summary],
        live: crate::session::LiveSessionState::default(),
        ..SessionState::new(config)
    };

    assert_eq!(
        prompt_prefix_status_for_report(&state, 1),
        "invalidated_by_warm_summary"
    );
    assert_eq!(prompt_prefix_status_for_report(&state, 2), "true");
}

#[test]
fn warm_age_out_can_summarize_multiple_oldest_turns() {
    let base_dir = std::env::temp_dir().join(format!("qsf-warm-multi-summary-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let mut state = SessionState::new(test_config_with_warm_threshold(10, 2));
    let state_dir = base_dir.join("state/text-loop");
    state.turns = (0..5).map(test_turn).collect();

    ageing::age_out_warm_turns(
        &mut context,
        &mut state,
        &state_dir,
        &MockModelClient::default(),
    )
    .unwrap();

    assert_eq!(
        state
            .summarized_turns
            .iter()
            .map(|summary| summary.turn_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(state.turns.len(), 5);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn missing_file_memory_source_logs_fallback_event() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-missing-session-memory-{}", Uuid::new_v4()));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");

    let snapshot = super::MissingFileSessionMemorySource
        .load(&mut context)
        .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    assert_eq!(snapshot.source_name, "phase_four_fixture");
    assert!(events.contains("ErrorOccurred"));
    assert!(events.contains("QSF_SESSION_MEMORY_FILE"));
    assert!(events.contains("phase_four_fixture"));

    fs::remove_dir_all(base_dir).unwrap();
}

struct TestMemorySource;

impl super::SessionMemorySource for TestMemorySource {
    fn load(
        &self,
        _context: &mut RunContext,
    ) -> anyhow::Result<super::SessionMemorySourceSnapshot> {
        Ok(super::SessionMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "test",
            phase_four_fixture(),
        ))
    }
}

struct EmptyMemorySource;

impl super::SessionMemorySource for EmptyMemorySource {
    fn load(
        &self,
        _context: &mut RunContext,
    ) -> anyhow::Result<super::SessionMemorySourceSnapshot> {
        Ok(super::SessionMemorySourceSnapshot::from_fixture(
            "empty_fixture",
            "tests",
            MemoryFixture {
                records: vec![],
                associations: vec![],
            },
        ))
    }
}

#[derive(Clone)]
struct SummaryReply {
    output_text: String,
    finish_reason: String,
}

impl SummaryReply {
    fn new(output_text: impl Into<String>, finish_reason: impl Into<String>) -> Self {
        Self {
            output_text: output_text.into(),
            finish_reason: finish_reason.into(),
        }
    }
}

struct SequencedSummarizerClient {
    base: MockModelClient,
    replies: Vec<SummaryReply>,
    summarizer_calls: std::sync::Mutex<usize>,
    summarizer_max_output_tokens: std::sync::Mutex<Vec<Option<u32>>>,
}

impl SequencedSummarizerClient {
    fn new(replies: Vec<SummaryReply>) -> Self {
        Self {
            base: MockModelClient::default(),
            replies,
            summarizer_calls: std::sync::Mutex::new(0),
            summarizer_max_output_tokens: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn summarizer_max_output_tokens(&self) -> Vec<Option<u32>> {
        self.summarizer_max_output_tokens.lock().unwrap().clone()
    }
}

impl ModelClient for SequencedSummarizerClient {
    fn client_name(&self) -> &str {
        "sequenced-summarizer"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        if request.role.role_id == ModelRoleId::SessionTurnSummarizer {
            self.summarizer_max_output_tokens
                .lock()
                .unwrap()
                .push(request.max_output_tokens);
            let mut summarizer_calls = self.summarizer_calls.lock().unwrap();
            let reply_index = (*summarizer_calls).min(self.replies.len().saturating_sub(1));
            let reply = self.replies[reply_index].clone();
            *summarizer_calls += 1;
            let usage = ModelUsage::new(12, 4).with_estimated_cost_usd(0.0);

            return Ok(ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                reply.output_text,
            )
            .with_usage(usage)
            .with_finish_reason(reply.finish_reason));
        }

        self.base.complete(request)
    }
}

struct RepeatingToolCallClient;

struct FailingModelClient;

impl ModelClient for FailingModelClient {
    fn client_name(&self) -> &str {
        "failing-model"
    }

    fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        anyhow::bail!("intentional model failure")
    }
}

impl ModelClient for RepeatingToolCallClient {
    fn client_name(&self) -> &str {
        "repeating-tool-call"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let usage = ModelUsage::new(12, 4).with_estimated_cost_usd(0.0);
        let mut response = ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            "tool loop",
        )
        .with_usage(usage)
        .with_finish_reason("tool_calls");

        if request.role.role_id == ModelRoleId::SessionTurnSummarizer {
            response = ModelResponse::from_text(
                request,
                self.client_name(),
                request.model_name.clone(),
                "The user and assistant discussed QSF session continuity in one aged-out turn.",
            )
            .with_usage(ModelUsage::new(12, 4))
            .with_finish_reason("stop");
        } else if request
            .last_user_message()
            .map(|message| message.to_ascii_lowercase().contains("recall turn"))
            .unwrap_or(false)
            || request.messages.iter().any(|message| {
                message
                    .content
                    .to_ascii_lowercase()
                    .contains("[recall_turn]")
            })
        {
            response = response.with_tool_calls(vec![ModelToolCall::new(
                "loop-recall-0",
                "recall_turn",
                serde_json::json!({ "turn_id": 0 }),
            )]);
        } else {
            response = response.with_finish_reason("stop");
        }

        Ok(response)
    }
}

#[derive(Clone)]
struct PlannedResponderResponse {
    output_text: String,
    finish_reason: String,
    tool_calls: Vec<ModelToolCall>,
}

impl PlannedResponderResponse {
    fn text(output_text: impl Into<String>) -> Self {
        Self {
            output_text: output_text.into(),
            finish_reason: "stop".to_string(),
            tool_calls: vec![],
        }
    }

    fn tool_call(
        output_text: impl Into<String>,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            output_text: output_text.into(),
            finish_reason: "tool_calls".to_string(),
            tool_calls: vec![ModelToolCall::new(tool_call_id, tool_name, arguments)],
        }
    }
}

#[derive(Default)]
struct SequencedResponderClient {
    calls: std::sync::Mutex<Vec<CapturedRequest>>,
    responses: Vec<PlannedResponderResponse>,
    response_index: std::sync::Mutex<usize>,
}

impl SequencedResponderClient {
    fn new(responses: Vec<PlannedResponderResponse>) -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
            responses,
            response_index: std::sync::Mutex::new(0),
        }
    }

    fn calls(&self) -> Vec<CapturedRequest> {
        self.calls.lock().unwrap().clone()
    }
}

impl ModelClient for SequencedResponderClient {
    fn client_name(&self) -> &str {
        "sequenced-responder"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        self.calls.lock().unwrap().push(CapturedRequest {
            role_id: request.role.role_id,
            max_output_tokens: request.max_output_tokens,
            tools: request.tools.iter().map(|tool| tool.name.clone()).collect(),
            messages: request.messages.clone(),
        });

        let mut response_index = self.response_index.lock().unwrap();
        let selected_index = (*response_index).min(self.responses.len().saturating_sub(1));
        let planned = self.responses[selected_index].clone();
        *response_index += 1;

        let usage = ModelUsage::new(10, 5).with_estimated_cost_usd(0.0);
        let mut response = ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            planned.output_text,
        )
        .with_usage(usage)
        .with_finish_reason(planned.finish_reason);
        if !planned.tool_calls.is_empty() {
            response = response.with_tool_calls(planned.tool_calls);
        }

        Ok(response)
    }
}

#[derive(Default)]
struct CapturingOpenAiRecallClient {
    calls: std::sync::Mutex<Vec<CapturedRequest>>,
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    role_id: ModelRoleId,
    max_output_tokens: Option<u32>,
    tools: Vec<String>,
    messages: Vec<ModelMessage>,
}

impl ModelClient for CapturingOpenAiRecallClient {
    fn client_name(&self) -> &str {
        "openai"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        self.calls.lock().unwrap().push(CapturedRequest {
            role_id: request.role.role_id,
            max_output_tokens: request.max_output_tokens,
            tools: request.tools.iter().map(|tool| tool.name.clone()).collect(),
            messages: request.messages.clone(),
        });

        let mut response = ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            "openai tool response",
        )
        .with_usage(ModelUsage::new(10, 5))
        .with_finish_reason("stop");

        if request.role.role_id == ModelRoleId::ConversationalResponder
            && request
                .tools
                .iter()
                .any(|tool| tool.name == RECALL_TURN_TOOL_NAME)
            && request
                .messages
                .iter()
                .all(|message| message.role != crate::models::ModelMessageRole::Tool)
            && request
                .last_user_message()
                .map(|message| message.to_ascii_lowercase().contains("recall turn"))
                .unwrap_or(false)
        {
            response = response
                .with_tool_calls(vec![ModelToolCall::new(
                    "openai-recall-0",
                    "recall_turn",
                    serde_json::json!({ "turn_id": 0 }),
                )])
                .with_finish_reason("tool_calls");
        }

        Ok(response)
    }
}

fn test_config(max_turns: usize) -> SessionConfig {
    test_config_with_warm_threshold(max_turns, max_turns)
}

fn test_config_with_warm_threshold(max_turns: usize, warm_threshold: usize) -> SessionConfig {
    SessionConfig {
        model_id: DEFAULT_SESSION_MODEL.to_string(),
        max_turns,
        warm_threshold,
        allow_over_limit: false,
        memory_source: MemorySourceConfig {
            source: "phase_four_fixture".to_string(),
            file: None,
        },
    }
}

fn memory_record(
    id: &str,
    title: &str,
    summary: &str,
    tags: Vec<&str>,
    estimated_tokens: usize,
) -> MemoryRecord {
    MemoryRecord::new(
        id,
        MemoryRecordKind::Observation,
        title,
        summary,
        tags,
        timestamp("2026-05-24T00:00:00Z"),
        1.0,
        0,
        "tests",
        estimated_tokens,
    )
}

fn timestamp(value: &str) -> time::OffsetDateTime {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).unwrap()
}

fn small_assembly_with_one_direct_one_hint() -> ContextAssembly {
    let direct = ContextFragment {
        fragment_id: "memory.direct".to_string(),
        source_kind: ContextSourceKind::Memory,
        summary: "Direct memory summary.".to_string(),
        tags: vec!["direct".to_string()],
        score: 1.0,
        estimated_tokens: 20,
        source_reference: "tests".to_string(),
        selection_reason: "direct test".to_string(),
    };
    let hint = ContextFragment {
        fragment_id: "memory.hint".to_string(),
        source_kind: ContextSourceKind::MemoryHint,
        summary: "Hint memory summary.".to_string(),
        tags: vec!["hint".to_string()],
        score: 0.5,
        estimated_tokens: 20,
        source_reference: "tests".to_string(),
        selection_reason: "hint test".to_string(),
    };

    ContextAssembly {
        budget: ContextBudget::new(4, 100),
        selected: vec![
            ContextSelection {
                fragment: direct,
                cumulative_estimated_tokens: 20,
            },
            ContextSelection {
                fragment: hint,
                cumulative_estimated_tokens: 40,
            },
        ],
        omitted: vec![],
        used_estimated_tokens: 40,
    }
}

fn test_turn(index: usize) -> Turn {
    test_turn_with_hash(index, ContentHash([index as u8; 32]), 2)
}

fn synthetic_state_with_verbatim_sizes(sizes: &[usize]) -> SessionState {
    let mut state = SessionState::new(test_config(20));
    state.turns = sizes
        .iter()
        .enumerate()
        .map(|(index, tokens)| {
            let mut turn = test_turn(index);
            turn.user_input = "x".repeat(tokens * 4);
            turn.retrieved_memory_block.clear();
            turn.assistant_response.clear();
            turn
        })
        .collect();
    state
}

fn test_turn_with_memory_ids(index: usize, ids: &[&str]) -> Turn {
    let mut turn = test_turn(index);
    turn.context_assembly = ContextAssembly {
        budget: ContextBudget::new(8, 600),
        selected: ids
            .iter()
            .map(|id| ContextSelection {
                fragment: ContextFragment {
                    fragment_id: (*id).to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: format!("Summary {id}."),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 10,
                    source_reference: "tests".to_string(),
                    selection_reason: "tests".to_string(),
                },
                cumulative_estimated_tokens: 10,
            })
            .collect(),
        omitted: vec![],
        used_estimated_tokens: ids.len() * 10,
    };
    turn
}

fn test_turn_with_hash(index: usize, full_request_hash: ContentHash, message_count: usize) -> Turn {
    Turn {
        index,
        started_at: std::time::SystemTime::UNIX_EPOCH,
        completed_at: std::time::SystemTime::UNIX_EPOCH,
        user_input: format!("input {index}"),
        context_assembly: ContextAssembly {
            budget: ContextBudget::new(4, 600),
            selected: vec![],
            omitted: vec![],
            used_estimated_tokens: 0,
        },
        retrieved_memory_block: String::new(),
        assistant_response: format!("answer {index}"),
        recalled_turns: vec![],
        model_id: DEFAULT_SESSION_MODEL.to_string(),
        model_latency_ms: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        full_request_hash,
        message_count,
    }
}

fn parse_event_records(events: &str) -> Vec<EventRecord> {
    events
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|value| serde_json::from_value::<EventRecord>(value).unwrap())
        .collect()
}

fn parse_trace_records(traces: &str) -> Vec<TraceRecord> {
    traces
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|value| serde_json::from_value::<TraceRecord>(value).unwrap())
        .collect()
}

fn responder_tool_names() -> Vec<String> {
    vec![
        RECALL_TURN_TOOL_NAME.to_string(),
        CALCULATOR_TOOL_NAME.to_string(),
        SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
        READ_PROJECT_DOC_TOOL_NAME.to_string(),
    ]
}

#[derive(Debug, Deserialize)]
struct SelfQuestionBattery {
    questions: Vec<SelfQuestion>,
}

#[derive(Debug, Deserialize)]
struct SelfQuestion {
    id: String,
    prompt: String,
    reply: String,
    tool_calls: Vec<ToolCallFixture>,
    expected_reply_contains: Vec<String>,
    expected_reply_must_not_contain: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ToolCallFixture {
    round: usize,
    tool: String,
    arguments: Value,
}

struct SelfQuestionOutcome {
    calls: Vec<CapturedRequest>,
    event_records: Vec<EventRecord>,
    trace_records: Vec<TraceRecord>,
    reply: String,
    base_dir: PathBuf,
}

fn load_self_question_battery() -> SelfQuestionBattery {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/self_question_battery.json");
    let json = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read self-question battery fixture at {}: {error}",
            path.display()
        )
    });
    serde_json::from_str(&json).unwrap_or_else(|error| {
        panic!(
            "failed to parse self-question battery fixture at {}: {error}",
            path.display()
        )
    })
}

fn run_self_question_battery_question(question: &SelfQuestion) -> SelfQuestionOutcome {
    let tool_calls = validated_sorted_tool_calls(question);
    let mut responses = tool_calls
        .iter()
        .map(|call| {
            PlannedResponderResponse::tool_call(
                format!("{} round {}", question.id, call.round),
                format!("{}-{}", question.id, call.round),
                call.tool.clone(),
                call.arguments.clone(),
            )
        })
        .collect::<Vec<_>>();
    responses.push(PlannedResponderResponse::text(question.reply.clone()));

    let client = SequencedResponderClient::new(responses);
    let base_dir = std::env::temp_dir().join(format!(
        "qsf-self-question-battery-{}-{}",
        question.id,
        Uuid::new_v4()
    ));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new(format!("{}\n:quit\n", question.prompt));
    let mut output = Vec::new();
    let memory_source = TestMemorySource;

    run_with_io_and_components(
        &mut context,
        input,
        &mut output,
        &client,
        &memory_source,
        test_config_with_warm_threshold(10, 10),
    )
    .unwrap();

    let event_records =
        parse_event_records(&fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap());
    let trace_records =
        parse_trace_records(&fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap());
    let turn_completed = event_records
        .iter()
        .find(|record| record.event_type == EventType::TurnCompleted)
        .expect("turn completed event present");
    let turn: Turn = serde_json::from_value(turn_completed.payload["turn"].clone())
        .expect("turn payload should deserialize");

    SelfQuestionOutcome {
        calls: client.calls(),
        event_records,
        trace_records,
        reply: turn.assistant_response,
        base_dir,
    }
}

fn validated_sorted_tool_calls(question: &SelfQuestion) -> Vec<ToolCallFixture> {
    let mut tool_calls = question.tool_calls.clone();
    tool_calls.sort_by_key(|tool_call| tool_call.round);

    let rounds = tool_calls
        .iter()
        .map(|tool_call| tool_call.round)
        .collect::<Vec<_>>();
    let expected_rounds = (1..=tool_calls.len()).collect::<Vec<_>>();
    assert_eq!(
        rounds, expected_rounds,
        "question {}: tool call rounds must be unique, 1-based, and contiguous",
        question.id
    );
    assert!(
        tool_calls
            .iter()
            .all(|tool_call| tool_call.round <= super::MAX_RESPONDER_TOOL_ROUNDS_PER_TURN),
        "question {}: tool call rounds must not exceed {}",
        question.id,
        super::MAX_RESPONDER_TOOL_ROUNDS_PER_TURN
    );

    tool_calls
}

fn assert_reply_expectations(
    reply: &str,
    expected_contains: &[String],
    expected_must_not_contain: &[String],
    question_id: &str,
) {
    let lower_reply = reply.to_ascii_lowercase();

    for needle in expected_contains {
        assert!(
            lower_reply.contains(&needle.to_ascii_lowercase()),
            "question {}: reply should contain `{}`",
            question_id,
            needle
        );
    }

    for needle in expected_must_not_contain {
        assert!(
            !lower_reply.contains(&needle.to_ascii_lowercase()),
            "question {}: reply should not contain `{}`",
            question_id,
            needle
        );
    }
}

fn test_context(base_dir: impl AsRef<Path>, experiment_id: &str) -> RunContext {
    RunContext::create_in_with_workspace_root(
        base_dir,
        experiment_id,
        Some(workspace_root_for_tests()),
    )
    .unwrap()
}

fn workspace_root_for_tests() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assert_event_order(records: &[EventRecord], first: EventType, second: EventType) {
    let first_index = records
        .iter()
        .position(|record| record.event_type == first)
        .unwrap();
    let second_index = records
        .iter()
        .position(|record| record.event_type == second)
        .unwrap();

    assert!(first_index < second_index);
}

fn assert_turn_prefix_hashes_are_stable(
    turn_records: &[&EventRecord],
    project_doc_channel_enabled: bool,
) {
    let turns = turn_records
        .iter()
        .map(|record| serde_json::from_value::<Turn>(record.payload["turn"].clone()).unwrap())
        .collect::<Vec<_>>();

    for index in 1..turns.len() {
        let previous = &turns[index - 1];
        let current = &turns[index];
        let prior_turns = turns[..index]
            .iter()
            .map(|turn| PromptTurn {
                user_input: &turn.user_input,
                retrieved_memory_block: &turn.retrieved_memory_block,
                recalled_tool_messages: turn
                    .recalled_turns
                    .iter()
                    .map(super::prompt_tool_message_from_recall)
                    .collect(),
                assistant_response: &turn.assistant_response,
            })
            .collect::<Vec<_>>();
        let prompt = assemble_prompt_with_summaries_and_project_doc_channel(
            &[],
            &prior_turns,
            &current.user_input,
            &current.retrieved_memory_block,
            project_doc_channel_enabled,
        );

        assert_eq!(
            prior_request_prefix_hash(&prompt.messages, previous.message_count),
            Some(previous.full_request_hash)
        );
    }
}
