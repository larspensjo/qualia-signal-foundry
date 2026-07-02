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
                    .find(|message| message.role == qsf_models::ModelMessageRole::User)
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
        == qsf_models::ModelMessageRole::User
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
        .position(|message| message.role == qsf_models::ModelMessageRole::Tool)
        .unwrap();
    assert!(tool_message_index > 0);
    let assistant_tool_call_message = &second_call.messages[tool_message_index - 1];
    assert_eq!(
        assistant_tool_call_message.role,
        qsf_models::ModelMessageRole::Assistant
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
            .filter(|message| message.role == qsf_models::ModelMessageRole::Tool)
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
                message.role == qsf_models::ModelMessageRole::System
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
    let search_tool_message = calls[1]
        .messages
        .iter()
        .find(|message| message.role == qsf_models::ModelMessageRole::Tool)
        .expect("search follow-up should include tool result");
    assert!(
        search_tool_message
            .content
            .contains("[search_project_docs]")
    );
    assert!(
        search_tool_message
            .content
            .contains("docs/ProjectFrame/ProjectVision.md")
    );
    let read_tool_message = calls[2]
        .messages
        .iter()
        .rev()
        .find(|message| message.role == qsf_models::ModelMessageRole::Tool)
        .expect("final response request should include read tool result");
    assert!(read_tool_message.content.contains("[read_project_doc]"));
    assert!(read_tool_message.content.contains("Project Vision"));

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
fn non_replayed_project_doc_tool_turn_invalidates_next_prompt_prefix() {
    let base_dir = std::env::temp_dir().join(format!(
        "qsf-project-doc-prefix-invalidation-{}",
        Uuid::new_v4()
    ));
    let mut context = test_context(&base_dir, "multi-turn-text-loop");
    let input = Cursor::new("please search the project docs\nwhat next?\n:quit\n");
    let mut output = Vec::new();
    let memory_source = TestMemorySource;
    let client = SequencedResponderClient::new(vec![
        PlannedResponderResponse::tool_call(
            "search the docs",
            "project-doc-search-0",
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            json!({ "query": "vision" }),
        ),
        PlannedResponderResponse::text(
            "The project's accepted framing says the docs can ground project-self replies.",
        ),
        PlannedResponderResponse::text("We can continue with another project-doc question."),
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

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);
    assert_eq!(
        event_records
            .iter()
            .filter(|record| record.event_type == EventType::TurnCompleted)
            .count(),
        2
    );
    let invalidation = event_records
        .iter()
        .find(|record| record.event_type == EventType::PromptPrefixInvalidated)
        .expect("prompt-prefix invalidation event present");
    assert_eq!(invalidation.payload["after_turn_index"], json!(0));
    assert_eq!(
        invalidation.payload["reason"],
        json!("non_replayed_tool_messages")
    );

    let report = fs::read_to_string(context.run_dir().join("multi-turn-text-loop.md")).unwrap();
    assert!(report.contains("invalidated_by_non_replayed_tool_messages"));

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
        message.role == qsf_models::ModelMessageRole::Tool
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
                qsf_models::ModelMessageRole::System,
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
