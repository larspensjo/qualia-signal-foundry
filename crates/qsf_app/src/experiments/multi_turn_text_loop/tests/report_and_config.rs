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
