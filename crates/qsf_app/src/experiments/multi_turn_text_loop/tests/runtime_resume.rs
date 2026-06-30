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
        current_volition_snapshot_path: None,
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
        current_volition_snapshot_path: None,
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
