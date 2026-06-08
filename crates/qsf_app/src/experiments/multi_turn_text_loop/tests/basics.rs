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
