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
                .all(|message| message.role != qsf_models::ModelMessageRole::Tool)
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
        tool_requests: vec![],
        tool_executions: vec![],
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
