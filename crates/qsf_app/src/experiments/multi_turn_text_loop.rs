use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::context::{ContextAssembly, ContextFragment, assemble_context};
use crate::conversation::prompt::{self, PromptToolMessage, PromptTurn};
use crate::conversation::{PromptAssembly, PromptTurnSummary};
use crate::memory::{
    Association, MemoryFixture, MemoryRecord, RetrievalResult, RetrievalStrategy,
    phase_four_fixture, retrieve_memories, retrieved_memory_ids,
};
use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelRole, ModelRoleId, ModelToolCall, build_client,
    invoke_model_role, requested_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ms};
use crate::runtime::run_context::RunContext;
use crate::session::{
    MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent, SessionLimit,
    SessionState, Turn, TurnSummary, is_turn_summarized,
};
use crate::tools::{
    RECALL_TURN_TOOL_NAME, SessionToolContext, ToolRegistry, ToolRequest, ToolResult,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

const DEFAULT_SESSION_MODEL: &str = "gpt-5.4-mini";
const DEFAULT_MAX_TURNS: usize = 10;
const DEFAULT_WARM_THRESHOLD: usize = 6;
const SESSION_MEMORY_SOURCE_ENV_VAR: &str = "QSF_SESSION_MEMORY_SOURCE";
const SESSION_MEMORY_FILE_ENV_VAR: &str = "QSF_SESSION_MEMORY_FILE";
const SESSION_MODEL_ENV_VAR: &str = "QSF_CONVERSATION_MODEL";
const SESSION_MAX_TURNS_ENV_VAR: &str = "QSF_SESSION_MAX_TURNS";
const SESSION_ALLOW_OVER_LIMIT_ENV_VAR: &str = "QSF_SESSION_ALLOW_OVER_LIMIT";
const SESSION_WARM_THRESHOLD_ENV_VAR: &str = "QSF_SESSION_WARM_THRESHOLD";
const SESSION_RETRIEVAL_LIMIT: usize = 8;
const SESSION_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::AssociationWeighted;

pub struct MultiTurnTextLoopExperiment;

impl Experiment for MultiTurnTextLoopExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::MultiTurnTextLoop
    }

    fn description(&self) -> &'static str {
        "Run a human-driven text conversation with append-only session state and cache-stable prompt assembly"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let config = SessionConfig::from_env();
        let model_client = build_client(requested_provider_from_env())?;
        let memory_source = build_session_memory_source_from_env();
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        run_with_io_and_components(
            context,
            stdin.lock(),
            &mut stdout,
            model_client.as_ref(),
            memory_source.as_ref(),
            config,
        )
    }
}

impl SessionConfig {
    fn from_env() -> Self {
        let model_id = std::env::var(SESSION_MODEL_ENV_VAR)
            .unwrap_or_else(|_| DEFAULT_SESSION_MODEL.to_string());
        let max_turns = std::env::var(SESSION_MAX_TURNS_ENV_VAR)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MAX_TURNS);
        let warm_threshold = std::env::var(SESSION_WARM_THRESHOLD_ENV_VAR)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_WARM_THRESHOLD);
        let allow_over_limit = std::env::var(SESSION_ALLOW_OVER_LIMIT_ENV_VAR)
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let memory_source = MemorySourceConfig::from_env();

        Self {
            model_id,
            max_turns,
            warm_threshold,
            allow_over_limit,
            memory_source,
        }
    }
}

impl MemorySourceConfig {
    fn from_env() -> Self {
        let source = std::env::var(SESSION_MEMORY_SOURCE_ENV_VAR)
            .unwrap_or_else(|_| "phase_four_fixture".to_string());
        let file = std::env::var(SESSION_MEMORY_FILE_ENV_VAR)
            .ok()
            .map(PathBuf::from);

        Self { source, file }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn reduce_session(mut state: SessionState, event: SessionEvent) -> SessionState {
    reduce_session_in_place(&mut state, event);
    state
}

fn reduce_session_in_place(state: &mut SessionState, event: SessionEvent) {
    match event {
        SessionEvent::SessionStarted(config) => {
            state.config = config;
        }
        SessionEvent::InputReceived { input } => {
            state.last_input = Some(input);
            state.last_model_error = None;
        }
        SessionEvent::MemoryRetrieved | SessionEvent::ContextAssembled(_) => {}
        SessionEvent::PromptAssembled {
            full_request_hash, ..
        } => {
            state.last_prompt_hash = Some(full_request_hash);
            state.prefix_invalidated_since_last_prompt = false;
        }
        SessionEvent::ModelRoleCompleted { .. } => {
            state.last_model_error = None;
        }
        SessionEvent::ModelRoleFailed { error_summary } => {
            state.last_model_error = Some(error_summary);
        }
        SessionEvent::TurnCompleted(turn) => {
            state.turns.push(turn);
        }
        SessionEvent::TurnSummarized(summary) => {
            state.summarized_turns.push(summary);
            state.prefix_invalidated_since_last_prompt = true;
        }
        SessionEvent::ToolCompleted(_) => {}
        SessionEvent::SessionLimitReached {
            current,
            max,
            override_active,
        } => {
            state.limit_reached = Some(SessionLimit {
                current,
                max,
                override_active,
            });
        }
        SessionEvent::SessionEnded { reason } => {
            state.ended_reason = Some(reason);
        }
    }
}

fn run_with_io_and_components(
    context: &mut RunContext,
    mut input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
) -> anyhow::Result<ExperimentOutcome> {
    let mut state = SessionState::new(config.clone());
    apply_session_event(
        context,
        &mut state,
        SessionEvent::SessionStarted(config.clone()),
    )?;
    let memory_snapshot = memory_source.load(context)?;
    write_memory_source_snapshot(context, &memory_snapshot)?;
    writeln!(output, "multi-turn-text-loop ready; type :quit to exit")?;

    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = input.read_line(&mut line)?;
        if bytes_read == 0 {
            end_session(context, &mut state, SessionEndReason::Eof)?;
            break;
        }

        let user_input = line.trim_end_matches(['\r', '\n']).to_string();
        if user_input.trim() == ":quit" {
            end_session(context, &mut state, SessionEndReason::QuitCommand)?;
            break;
        }
        if user_input.trim().is_empty() {
            continue;
        }

        apply_session_event(
            context,
            &mut state,
            SessionEvent::InputReceived {
                input: user_input.clone(),
            },
        )?;

        if completed_turn_count(&state) >= state.config.max_turns {
            let current = completed_turn_count(&state);
            let max = state.config.max_turns;
            let override_active = state.config.allow_over_limit;
            apply_session_event(
                context,
                &mut state,
                SessionEvent::SessionLimitReached {
                    current,
                    max,
                    override_active,
                },
            )?;
            if !override_active {
                writeln!(
                    output,
                    "session limit reached; type :quit or restart with QSF_SESSION_ALLOW_OVER_LIMIT=true"
                )?;
                continue;
            }
        }

        match run_one_turn(
            context,
            &mut state,
            &memory_snapshot,
            model_client,
            &user_input,
        ) {
            Ok(response) => {
                writeln!(output, "{response}")?;
            }
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                apply_session_event(
                    context,
                    &mut state,
                    SessionEvent::ModelRoleFailed {
                        error_summary: error_summary.clone(),
                    },
                )?;
                writeln!(output, "model unavailable, try again or :quit")?;
                engine_logging::engine_error!(
                    "multi-turn model call failed: run_id={} error={}",
                    context.run_id(),
                    error_summary
                );
            }
        }
    }

    write_multi_turn_report(context, &state, &memory_snapshot)?;

    Ok(ExperimentOutcome {
        summary: format!(
            "The multi-turn text loop completed with {} appended turns, {} warm summaries, {} recall tool executions, and cache-stable prompt hashes recorded per turn.",
            completed_turn_count(&state),
            state.summarized_turns.len(),
            state.turns.iter().map(|turn| turn.recalled_turns.len()).sum::<usize>()
        ),
        observations: vec![
            "Session turns are append-only; older turns can age into stable warm summaries while completed turn records remain available for reporting.".to_string(),
            "Each turn retrieves memory from the latest user input only, then assembles selected fragments under the existing context budget.".to_string(),
            "The recall_turn tool can expand summarized turns into verbatim tool messages that are frozen into future prompt prefixes.".to_string(),
        ],
        failure_modes: vec![
            "Warm summarization invalidates the prompt cache prefix once per ageing event.".to_string(),
            "OpenAI prompt caching reports zero cached tokens below the 1024 input-token floor.".to_string(),
        ],
        follow_up_questions: vec![
            "Should the warm tier summarize by token pressure in addition to turn count?".to_string(),
            "Should later retrieval include recent session turns as query context?".to_string(),
        ],
        decision_candidates: vec![
            "Keep warm-tier summaries session-local and append-only unless explicitly promoted through the reviewed-memory pipeline.".to_string(),
        ],
        extra_artifacts: vec![
            "multi-turn-text-loop.md".to_string(),
            "session-memory-source.json".to_string(),
        ],
    })
}

fn run_one_turn(
    context: &mut RunContext,
    state: &mut SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
    model_client: &dyn ModelClient,
    user_input: &str,
) -> anyhow::Result<String> {
    let turn_started_at = SystemTime::now();
    let retrieval = retrieve_session_memories(context, state, memory_snapshot, user_input)?;
    let fragments = retrieval
        .selected
        .iter()
        .map(ContextFragment::from)
        .collect::<Vec<_>>();
    apply_session_event(context, state, SessionEvent::MemoryRetrieved)?;

    let assembly = assemble_context(
        fragments,
        ModelRole::predefined(ModelRoleId::ConversationalResponder).context_budget,
    );
    let context_trace_id = record_context_assembly(context, state, &assembly)?;
    context.record_event(
        EventType::ContextAssemblyRequested,
        json!({
            "session_id": context.run_id(),
            "turn_index": completed_turn_count(state),
            "source_event": EventType::InputReceived,
            "budget": &assembly.budget,
        }),
        Some(context_trace_id),
    )?;
    apply_session_event(
        context,
        state,
        SessionEvent::ContextAssembled(assembly.clone()),
    )?;

    let retrieved_memory_block = prompt::retrieved_memory_block(&assembly);
    let base_prompt = assemble_session_prompt(state, user_input, &retrieved_memory_block);
    verify_prompt_prefix(state, &base_prompt)?;
    apply_session_event(
        context,
        state,
        SessionEvent::PromptAssembled {
            full_request_hash: base_prompt.full_request_hash,
            message_count: base_prompt.message_count,
            total_bytes: base_prompt.total_bytes,
        },
    )?;

    let registry = ToolRegistry::default();
    let responder_role = conversational_responder_role_with_recall_tool();
    let allowed_tools = responder_role
        .allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let request = ModelRequest::new(responder_role.clone(), base_prompt.messages.clone())
        .with_session_id(context.run_id())
        .with_model_name(&state.config.model_id)
        .with_temperature(0.0)
        .with_max_output_tokens(240)
        .with_tools(registry.model_tool_definitions_for(&allowed_tools));
    let model_started_at = Instant::now();
    let mut response = invoke_model_role(context, model_client, &request)?;
    let mut model_latency_ms = elapsed_ms(model_started_at);
    let mut input_tokens = response
        .usage
        .as_ref()
        .map(|usage| usage.input_tokens)
        .unwrap_or(0);
    let mut cached_input_tokens = response
        .usage
        .as_ref()
        .map(|usage| usage.cached_input_tokens)
        .unwrap_or(0);
    let mut output_tokens = response
        .usage
        .as_ref()
        .map(|usage| usage.output_tokens)
        .unwrap_or(0);
    let mut recalled_turns = vec![];
    let mut final_messages = base_prompt.messages.clone();

    if !response.tool_calls.is_empty() {
        recalled_turns =
            execute_recall_tool_calls(context, state, &registry, &response.tool_calls)?;
        for recall in &recalled_turns {
            final_messages.push(ModelMessage::tool(format_recall_tool_message(recall)));
        }
        let augmented_prompt = prompt::prompt_assembly_from_messages(final_messages.clone());
        apply_session_event(
            context,
            state,
            SessionEvent::PromptAssembled {
                full_request_hash: augmented_prompt.full_request_hash,
                message_count: augmented_prompt.message_count,
                total_bytes: augmented_prompt.total_bytes,
            },
        )?;

        let follow_up_request = ModelRequest::new(
            conversational_responder_role_with_recall_tool(),
            final_messages.clone(),
        )
        .with_session_id(context.run_id())
        .with_model_name(&state.config.model_id)
        .with_temperature(0.0)
        .with_max_output_tokens(240);
        let follow_up_started_at = Instant::now();
        response = invoke_model_role(context, model_client, &follow_up_request)?;
        model_latency_ms = model_latency_ms.saturating_add(elapsed_ms(follow_up_started_at));
        if let Some(usage) = response.usage.as_ref() {
            input_tokens = input_tokens.saturating_add(usage.input_tokens);
            cached_input_tokens = cached_input_tokens.saturating_add(usage.cached_input_tokens);
            output_tokens = output_tokens.saturating_add(usage.output_tokens);
        }
        if !response.tool_calls.is_empty() {
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": context.run_id(),
                    "stage": "recall-tool-follow-up",
                    "error": "recall follow-up returned additional tool calls; multi-round tool calls are not supported",
                    "tool_call_count": response.tool_calls.len(),
                    "tool_calls": &response.tool_calls,
                }),
                None,
            )?;
            anyhow::bail!(
                "recall follow-up returned additional tool calls; multi-round tool calls are not supported"
            );
        }
    }

    let final_prompt_assembly = prompt::prompt_assembly_from_messages(final_messages);
    apply_session_event(
        context,
        state,
        SessionEvent::ModelRoleCompleted {
            response: response.output_text.clone(),
            latency_ms: model_latency_ms,
            input_tokens,
            cached_input_tokens,
            output_tokens,
        },
    )?;

    let turn = Turn {
        index: completed_turn_count(state),
        started_at: turn_started_at,
        completed_at: SystemTime::now(),
        user_input: user_input.to_string(),
        context_assembly: assembly,
        retrieved_memory_block,
        assistant_response: response.output_text.clone(),
        recalled_turns,
        model_id: response.model_name.clone(),
        model_latency_ms,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        full_request_hash: final_prompt_assembly.full_request_hash,
        message_count: final_prompt_assembly.message_count,
    };
    let output_text = response.output_text;
    apply_session_event(context, state, SessionEvent::TurnCompleted(turn))?;
    age_out_warm_turns(context, state, model_client)?;

    Ok(output_text)
}

fn completed_turn_count(state: &SessionState) -> usize {
    state.turns.len()
}

fn conversational_responder_role_with_recall_tool() -> ModelRole {
    let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
    role.allowed_tools = vec![RECALL_TURN_TOOL_NAME.to_string()];
    role
}

fn execute_recall_tool_calls(
    context: &mut RunContext,
    state: &SessionState,
    registry: &ToolRegistry,
    tool_calls: &[ModelToolCall],
) -> anyhow::Result<Vec<RecallRecord>> {
    let mut recalls = Vec::with_capacity(tool_calls.len());
    let tool_ctx = SessionToolContext { state };

    for tool_call in tool_calls {
        let request = recall_request_from_model_tool_call(tool_call, context.experiment_id())?;
        let metadata = registry
            .metadata_for(&request.tool_name)
            .with_context(|| format!("unknown tool `{}`", request.tool_name))?;
        context.record_event(
            EventType::ToolRequested,
            json!({
                "session_id": context.run_id(),
                "tool_name": &request.tool_name,
                "call_id": &tool_call.call_id,
                "arguments": &tool_call.arguments,
                "input": &request.input,
                "permission": &request.permission,
                "requested_by": &request.requested_by,
                "category": metadata.category,
                "side_effect_level": metadata.side_effect_level,
                "scope": "multi_turn_text_loop",
            }),
            None,
        )?;

        let started_at = Instant::now();
        match registry.validate_and_execute(&request, &tool_ctx) {
            Ok((_metadata, result)) => {
                let recall =
                    recall_record_from_tool_result(tool_call, result, elapsed_ms(started_at))?;
                apply_tool_completed_event(context, state, recall.clone())?;
                recalls.push(recall);
            }
            Err(error) => {
                context.record_event(
                    EventType::ToolFailed,
                    json!({
                        "session_id": context.run_id(),
                        "tool_name": &request.tool_name,
                        "call_id": &tool_call.call_id,
                        "error": sanitize_error(&error.to_string()),
                        "latency_ms": elapsed_ms(started_at),
                    }),
                    None,
                )?;
                return Err(error);
            }
        }
    }

    Ok(recalls)
}

fn recall_request_from_model_tool_call(
    tool_call: &ModelToolCall,
    requested_by: &str,
) -> anyhow::Result<ToolRequest> {
    anyhow::ensure!(
        tool_call.name == RECALL_TURN_TOOL_NAME,
        "unknown multi-turn tool `{}`",
        tool_call.name
    );
    let turn_id = tool_call
        .arguments
        .get("turn_id")
        .and_then(|value| value.as_u64())
        .context("recall_turn requires integer argument `turn_id`")? as usize;

    Ok(ToolRequest::recall_turn(
        tool_call.call_id.clone(),
        turn_id,
        requested_by,
    ))
}

fn recall_record_from_tool_result(
    tool_call: &ModelToolCall,
    result: ToolResult,
    latency_ms: u64,
) -> anyhow::Result<RecallRecord> {
    let turn_id = tool_call
        .arguments
        .get("turn_id")
        .and_then(|value| value.as_u64())
        .context("recall_turn requires integer argument `turn_id`")? as usize;
    Ok(RecallRecord {
        call_id: tool_call.call_id.clone(),
        turn_id,
        tool_name: result.tool_name,
        category: result.category,
        side_effect_level: result.side_effect_level,
        verbatim_text: result.output_text,
        latency_ms,
    })
}

fn apply_tool_completed_event(
    context: &mut RunContext,
    state: &SessionState,
    recall: RecallRecord,
) -> anyhow::Result<()> {
    record_session_event(context, &SessionEvent::ToolCompleted(recall.clone()))?;
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-recall-tool",
        format!("recall_turn turn_id={}", recall.turn_id),
        format!("recalled summarized turn {}", recall.turn_id),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "completed_turn_count": completed_turn_count(state),
        "recall": &recall,
    }))
    .with_latency_context("runtime", "recall-turn-tool")
    .with_latency_ms(recall.latency_ms);
    context.record_trace(trace)?;
    Ok(())
}

fn format_recall_tool_message(recall: &RecallRecord) -> String {
    format!(
        "[recall_turn]\nturn_id: {}\n{}",
        recall.turn_id,
        recall.verbatim_text.trim()
    )
}

fn prompt_tool_message_from_recall(recall: &RecallRecord) -> PromptToolMessage {
    PromptToolMessage {
        tool_name: recall.tool_name.clone(),
        call_id: recall.call_id.clone(),
        content: format_recall_tool_message(recall),
    }
}

fn age_out_warm_turns(
    context: &mut RunContext,
    state: &mut SessionState,
    model_client: &dyn ModelClient,
) -> anyhow::Result<()> {
    while active_turn_count(state) > state.config.warm_threshold {
        let Some(turn) = oldest_unsummarized_turn(state).cloned() else {
            break;
        };
        let summary = match summarize_turn(context, state, model_client, &turn) {
            Ok(summary) => summary,
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                context.record_event(
                    EventType::ErrorOccurred,
                    json!({
                        "stage": "session-turn-summarization",
                        "turn_index": turn.index,
                        "error": error_summary,
                    }),
                    None,
                )?;
                engine_logging::engine_error!(
                    "multi-turn summarization failed: run_id={} turn_index={} error={}",
                    context.run_id(),
                    turn.index,
                    error_summary
                );
                break;
            }
        };

        apply_session_event(context, state, SessionEvent::TurnSummarized(summary))?;
    }

    Ok(())
}

fn active_turn_count(state: &SessionState) -> usize {
    state
        .turns
        .len()
        .saturating_sub(state.summarized_turns.len())
}

fn oldest_unsummarized_turn(state: &SessionState) -> Option<&Turn> {
    state.turns.get(state.summarized_turns.len())
}

fn summarize_turn(
    context: &mut RunContext,
    state: &SessionState,
    model_client: &dyn ModelClient,
    turn: &Turn,
) -> anyhow::Result<TurnSummary> {
    let request = ModelRequest::new(
        ModelRole::predefined(ModelRoleId::SessionTurnSummarizer),
        vec![
            ModelMessage::system(
                "Summarize exactly one aged-out conversation turn in one sentence. Preserve concrete user intent, assistant commitments, and project-specific facts. Do not add new facts.",
            ),
            ModelMessage::user(format!(
                "[Turn {}]\n[User]\n{}\n\n[Assistant]\n{}",
                turn.index, turn.user_input, turn.assistant_response
            )),
        ],
    )
    .with_session_id(context.run_id())
    .with_temperature(0.0)
    .with_max_output_tokens(80);
    let started_at = Instant::now();
    let response = invoke_model_role(context, model_client, &request)?;
    let usage = response.usage.as_ref();

    Ok(TurnSummary {
        turn_index: turn.index,
        summarized_after_turn_index: completed_turn_count(state) - 1,
        summary: normalize_summary(&response.output_text),
        model_id: response.model_name,
        model_latency_ms: elapsed_ms(started_at),
        input_tokens: usage.map(|usage| usage.input_tokens).unwrap_or(0),
        output_tokens: usage.map(|usage| usage.output_tokens).unwrap_or(0),
    })
}

fn normalize_summary(summary: &str) -> String {
    summary
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn assemble_session_prompt(
    state: &SessionState,
    user_input: &str,
    retrieved_memory_block: &str,
) -> PromptAssembly {
    let summarized_turns = state
        .summarized_turns
        .iter()
        .map(|summary| PromptTurnSummary {
            turn_index: summary.turn_index,
            summary: &summary.summary,
        })
        .collect::<Vec<_>>();
    let prior_turns = state
        .turns
        .iter()
        .filter(|turn| !is_turn_summarized(state, turn.index))
        .map(|turn| PromptTurn {
            user_input: &turn.user_input,
            retrieved_memory_block: &turn.retrieved_memory_block,
            recalled_tool_messages: turn
                .recalled_turns
                .iter()
                .map(prompt_tool_message_from_recall)
                .collect(),
            assistant_response: &turn.assistant_response,
        })
        .collect::<Vec<_>>();

    prompt::assemble_prompt_with_summaries(
        &summarized_turns,
        &prior_turns,
        user_input,
        retrieved_memory_block,
    )
}

fn verify_prompt_prefix(
    state: &SessionState,
    prompt_assembly: &PromptAssembly,
) -> anyhow::Result<()> {
    if state.prefix_invalidated_since_last_prompt {
        return Ok(());
    }

    if let Some(previous_turn) = state.turns.last() {
        let prefix_hash = prompt::prior_request_prefix_hash(
            &prompt_assembly.messages,
            previous_turn.message_count,
        )
        .context("new prompt did not contain the previous request prefix")?;
        anyhow::ensure!(
            prefix_hash == previous_turn.full_request_hash,
            "prompt prefix hash mismatch before turn {}",
            completed_turn_count(state)
        );
    }

    Ok(())
}

fn apply_session_event(
    context: &mut RunContext,
    state: &mut SessionState,
    event: SessionEvent,
) -> anyhow::Result<()> {
    reduce_session_in_place(state, event.clone());
    record_session_event(context, &event)?;
    Ok(())
}

fn record_session_event(context: &mut RunContext, event: &SessionEvent) -> anyhow::Result<()> {
    match event {
        SessionEvent::SessionStarted(config) => {
            context.record_event(EventType::SessionStarted, json!({ "config": config }), None)?;
        }
        SessionEvent::InputReceived { input } => {
            context.record_event(
                EventType::InputReceived,
                json!({
                    "session_id": context.run_id(),
                    "input": input,
                    "input_chars": input.chars().count(),
                }),
                None,
            )?;
        }
        SessionEvent::MemoryRetrieved => {}
        SessionEvent::ContextAssembled(assembly) => {
            context.record_event(
                EventType::ContextAssembled,
                json!({
                    "session_id": context.run_id(),
                    "selected_count": assembly.selected.len(),
                    "omitted_count": assembly.omitted.len(),
                    "used_estimated_tokens": assembly.used_estimated_tokens,
                    "selected": &assembly.selected,
                    "omitted": &assembly.omitted,
                }),
                None,
            )?;
        }
        SessionEvent::PromptAssembled {
            full_request_hash,
            message_count,
            total_bytes,
        } => {
            context.record_event(
                EventType::PromptAssembled,
                json!({
                    "session_id": context.run_id(),
                    "full_request_hash": full_request_hash.hex(),
                    "message_count": message_count,
                    "total_bytes": total_bytes,
                }),
                None,
            )?;
        }
        SessionEvent::ModelRoleCompleted { .. } => {}
        SessionEvent::ModelRoleFailed { .. } => {}
        SessionEvent::TurnCompleted(turn) => {
            context.record_event(
                EventType::TurnCompleted,
                json!({
                    "session_id": context.run_id(),
                    "turn": turn,
                    "full_request_hash": turn.full_request_hash.hex(),
                }),
                None,
            )?;
        }
        SessionEvent::TurnSummarized(summary) => {
            context.record_event(
                EventType::TurnSummarized,
                json!({
                    "session_id": context.run_id(),
                    "turn_index": summary.turn_index,
                    "summary": summary,
                }),
                None,
            )?;
        }
        SessionEvent::ToolCompleted(recall) => {
            context.record_event(
                EventType::ToolCompleted,
                json!({
                    "session_id": context.run_id(),
                    "tool_name": &recall.tool_name,
                    "call_id": &recall.call_id,
                    "turn_id": recall.turn_id,
                    "category": recall.category,
                    "side_effect_level": recall.side_effect_level,
                    "latency_ms": recall.latency_ms,
                    "scope": "multi_turn_text_loop",
                }),
                None,
            )?;
        }
        SessionEvent::SessionLimitReached {
            current,
            max,
            override_active,
        } => {
            context.record_event(
                EventType::SessionLimitReached,
                json!({
                    "session_id": context.run_id(),
                    "current": current,
                    "max": max,
                    "override_active": override_active,
                }),
                None,
            )?;
        }
        SessionEvent::SessionEnded { reason } => {
            context.record_event(
                EventType::SessionEnded,
                json!({
                    "session_id": context.run_id(),
                    "reason": reason,
                }),
                None,
            )?;
        }
    }

    Ok(())
}

fn end_session(
    context: &mut RunContext,
    state: &mut SessionState,
    reason: SessionEndReason,
) -> anyhow::Result<()> {
    apply_session_event(context, state, SessionEvent::SessionEnded { reason })
}

trait SessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot>;
}

struct PhaseFourSessionMemorySource;

impl SessionMemorySource for PhaseFourSessionMemorySource {
    fn load(&self, _context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        Ok(SessionMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "crate::memory::phase_four_fixture",
            phase_four_fixture(),
        ))
    }
}

struct FileSessionMemorySource {
    path: PathBuf,
}

impl SessionMemorySource for FileSessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        match fs::read_to_string(&self.path)
            .with_context(|| {
                format!(
                    "failed to read session memory file `{}`",
                    self.path.display()
                )
            })
            .and_then(|contents| {
                serde_json::from_str::<MemoryFixture>(&contents).with_context(|| {
                    format!(
                        "failed to parse session memory file `{}`",
                        self.path.display()
                    )
                })
            }) {
            Ok(fixture) => Ok(SessionMemorySourceSnapshot::from_fixture(
                "file",
                self.path.display().to_string(),
                fixture,
            )),
            Err(error) => {
                let error_summary = sanitize_error(&error.to_string());
                context.record_event(
                    EventType::ErrorOccurred,
                    json!({
                        "stage": "session-memory-source",
                        "source": "file",
                        "path": self.path.display().to_string(),
                        "fallback": "phase_four_fixture",
                        "error": error_summary,
                    }),
                    None,
                )?;
                Ok(SessionMemorySourceSnapshot::from_fixture(
                    "phase_four_fixture",
                    "fallback_after_file_error",
                    phase_four_fixture(),
                ))
            }
        }
    }
}

struct MissingFileSessionMemorySource;

impl SessionMemorySource for MissingFileSessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot> {
        context.record_event(
            EventType::ErrorOccurred,
            json!({
                "stage": "session-memory-source",
                "source": "file",
                "missing_env_var": SESSION_MEMORY_FILE_ENV_VAR,
                "fallback": "phase_four_fixture",
                "error": format!("`{SESSION_MEMORY_FILE_ENV_VAR}` must be set when `{SESSION_MEMORY_SOURCE_ENV_VAR}=file`"),
            }),
            None,
        )?;
        Ok(SessionMemorySourceSnapshot::from_fixture(
            "phase_four_fixture",
            "fallback_after_missing_file_env",
            phase_four_fixture(),
        ))
    }
}

fn build_session_memory_source_from_env() -> Box<dyn SessionMemorySource> {
    let requested = std::env::var(SESSION_MEMORY_SOURCE_ENV_VAR)
        .unwrap_or_else(|_| "phase_four_fixture".to_string());
    match requested.trim().to_ascii_lowercase().as_str() {
        "file" => std::env::var(SESSION_MEMORY_FILE_ENV_VAR)
            .map(|path| {
                Box::new(FileSessionMemorySource { path: path.into() })
                    as Box<dyn SessionMemorySource>
            })
            .unwrap_or_else(|_| Box::new(MissingFileSessionMemorySource)),
        _ => Box::new(PhaseFourSessionMemorySource),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct SessionMemorySourceSnapshot {
    source_name: String,
    source_reference: String,
    records: Vec<MemoryRecord>,
    associations: Vec<Association>,
}

impl SessionMemorySourceSnapshot {
    fn from_fixture(
        source_name: impl Into<String>,
        source_reference: impl Into<String>,
        fixture: MemoryFixture,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            source_reference: source_reference.into(),
            records: fixture.records,
            associations: fixture.associations,
        }
    }
}

fn write_memory_source_snapshot(
    context: &RunContext,
    snapshot: &SessionMemorySourceSnapshot,
) -> anyhow::Result<()> {
    fs::write(
        context.run_dir().join("session-memory-source.json"),
        serde_json::to_string_pretty(snapshot)?,
    )
    .context("failed to write session memory source snapshot")
}

fn retrieve_session_memories(
    context: &mut RunContext,
    state: &SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
    query: &str,
) -> anyhow::Result<RetrievalResult> {
    context.record_event(
        EventType::MemoryRetrievalRequested,
        json!({
            "session_id": context.run_id(),
            "turn_index": completed_turn_count(state),
            "query": query,
            "strategy": SESSION_RETRIEVAL_STRATEGY,
            "retrieval_limit": SESSION_RETRIEVAL_LIMIT,
            "memory_source": &memory_snapshot.source_name,
            "memory_source_reference": &memory_snapshot.source_reference,
        }),
        None,
    )?;
    let retrieval = retrieve_memories(
        &memory_snapshot.records,
        &memory_snapshot.associations,
        query,
        SESSION_RETRIEVAL_STRATEGY,
        SESSION_RETRIEVAL_LIMIT,
    )?;
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-memory-retrieval",
        format!("turn={} query={}", completed_turn_count(state), query),
        format!(
            "selected {} and omitted {} memory candidates",
            retrieval.selected.len(),
            retrieval.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "turn_index": completed_turn_count(state),
        "retrieval": &retrieval,
        "memory_source": &memory_snapshot.source_name,
    }))
    .with_latency_context("runtime", "session-memory-retrieval")
    .with_latency_ns(retrieval.latency_ns);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    context.record_event(
        EventType::MemoryRetrieved,
        json!({
            "session_id": context.run_id(),
            "turn_index": completed_turn_count(state),
            "memory_source": &memory_snapshot.source_name,
            "strategy": retrieval.strategy,
            "selected": retrieved_memory_ids(&retrieval.selected),
            "omitted": retrieved_memory_ids(&retrieval.omitted),
            "latency_ms": retrieval.latency_ms,
            "latency_ns": retrieval.latency_ns,
        }),
        Some(trace_id),
    )?;

    Ok(retrieval)
}

fn record_context_assembly(
    context: &mut RunContext,
    state: &SessionState,
    assembly: &ContextAssembly,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-context-assembly",
        format!(
            "turn={} retrieved memory fragments",
            completed_turn_count(state)
        ),
        format!(
            "selected {} fragments and omitted {}",
            assembly.selected.len(),
            assembly.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "turn_index": completed_turn_count(state),
        "assembly": assembly,
    }))
    .with_latency_context("runtime", "session-context-assembly")
    .with_latency_ms(0);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn write_multi_turn_report(
    context: &RunContext,
    state: &SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Multi-Turn Text Loop\n\n");
    markdown.push_str("## Configuration\n\n");
    markdown.push_str(&format!("- Model: `{}`\n", state.config.model_id));
    markdown.push_str(&format!("- Max turns: `{}`\n", state.config.max_turns));
    markdown.push_str(&format!(
        "- Warm threshold: `{}` active verbatim turns\n",
        state.config.warm_threshold
    ));
    markdown.push_str(&format!(
        "- Allow over limit: `{}`\n",
        state.config.allow_over_limit
    ));
    markdown.push_str(&format!(
        "- Requested memory source: `{}`\n",
        state.config.memory_source.source
    ));
    markdown.push_str(&format!(
        "- Loaded memory source: `{}`\n",
        memory_snapshot.source_name
    ));
    markdown.push_str(&format!(
        "- Loaded memory source reference: `{}`\n\n",
        memory_snapshot.source_reference
    ));
    markdown.push_str("## Turns\n\n");
    markdown.push_str("| Turn | Input tokens | Cached input tokens | Cache ratio | Output tokens | Latency ms | Hash prefix status |\n");
    markdown.push_str("|---:|---:|---:|---:|---:|---:|---|\n");

    for (index, turn) in state.turns.iter().enumerate() {
        let ratio = if turn.input_tokens == 0 {
            0.0
        } else {
            f64::from(turn.cached_input_tokens) / f64::from(turn.input_tokens)
        };
        let prefix_status = prompt_prefix_status_for_report(state, index);
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.2} | {} | {} | {} |\n",
            turn.index,
            turn.input_tokens,
            turn.cached_input_tokens,
            ratio,
            turn.output_tokens,
            turn.model_latency_ms,
            prefix_status
        ));
    }

    markdown.push_str("\n## Cache Diagnostics\n\n");
    let cache_misses_above_floor = state
        .turns
        .iter()
        .filter(|turn| turn.input_tokens >= 1024 && turn.cached_input_tokens == 0)
        .count();
    markdown.push_str(&format!(
        "- Cache misses at or above 1024 input tokens: `{cache_misses_above_floor}`\n"
    ));
    markdown.push_str("- Prompt cache floor: `1024` input tokens\n");
    markdown.push_str(&format!(
        "- Warm summaries produced: `{}`\n",
        state.summarized_turns.len()
    ));
    let recall_count = state
        .turns
        .iter()
        .map(|turn| turn.recalled_turns.len())
        .sum::<usize>();
    markdown.push_str(&format!("- Recall tool executions: `{recall_count}`\n"));
    markdown.push_str("- Session state persistence: `in_memory_only`\n\n");
    markdown.push_str("## Warm Summaries\n\n");
    if state.summarized_turns.is_empty() {
        markdown.push_str("- None\n\n");
    } else {
        markdown.push_str(
            "| Turn | Summary model | Latency ms | Input tokens | Output tokens | Summary |\n",
        );
        markdown.push_str("|---:|---|---:|---:|---:|---|\n");
        for summary in &state.summarized_turns {
            markdown.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} |\n",
                summary.turn_index,
                summary.model_id,
                summary.model_latency_ms,
                summary.input_tokens,
                summary.output_tokens,
                summary.summary.replace('|', "\\|")
            ));
        }
        markdown.push('\n');
    }
    markdown.push_str("## Recall Tool\n\n");
    if recall_count == 0 {
        markdown.push_str("- None\n\n");
    } else {
        markdown.push_str("| Turn | Recalled turn | Call id | Latency ms |\n");
        markdown.push_str("|---:|---:|---|---:|\n");
        for turn in &state.turns {
            for recall in &turn.recalled_turns {
                markdown.push_str(&format!(
                    "| {} | {} | `{}` | {} |\n",
                    turn.index, recall.turn_id, recall.call_id, recall.latency_ms
                ));
            }
        }
        markdown.push('\n');
    }
    markdown.push_str("## Hashes\n\n");
    for turn in &state.turns {
        markdown.push_str(&format!(
            "- Turn {}: `{}` messages=`{}`\n",
            turn.index, turn.full_request_hash, turn.message_count
        ));
    }

    fs::write(context.run_dir().join("multi-turn-text-loop.md"), markdown)
        .context("failed to write multi-turn text loop report")
}

fn prompt_prefix_status_for_report(state: &SessionState, turn_position: usize) -> String {
    if turn_position == 0 {
        return "n/a".to_string();
    }

    let previous = &state.turns[turn_position - 1];
    if state
        .summarized_turns
        .iter()
        .any(|summary| summary.summarized_after_turn_index == previous.index)
    {
        return "invalidated_by_warm_summary".to_string();
    }

    let prompt_state = SessionState {
        turns: state.turns[..turn_position].to_vec(),
        summarized_turns: state
            .summarized_turns
            .iter()
            .filter(|summary| summary.summarized_after_turn_index < turn_position)
            .cloned()
            .collect(),
        ..state.clone()
    };
    let turn = &state.turns[turn_position];
    let prompt_assembly = assemble_session_prompt(
        &prompt_state,
        &turn.user_input,
        &turn.retrieved_memory_block,
    );

    (prompt::prior_request_prefix_hash(&prompt_assembly.messages, previous.message_count)
        == Some(previous.full_request_hash))
    .to_string()
}

fn sanitize_error(error: &str) -> String {
    if error.contains("sk-") || error.to_ascii_lowercase().contains("authorization") {
        "provider error redacted because it may contain credential-like content".to_string()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;

    use serde_json::Value;
    use uuid::Uuid;

    use super::{
        DEFAULT_SESSION_MODEL, SessionMemorySource, age_out_warm_turns,
        prompt_prefix_status_for_report, reduce_session, run_with_io_and_components,
    };
    use crate::context::{ContextAssembly, ContextBudget};
    use crate::conversation::ContentHash;
    use crate::conversation::prompt::{
        PromptTurn, PromptTurnSummary, assemble_prompt, assemble_prompt_with_summaries,
        prior_request_prefix_hash,
    };
    use crate::memory::phase_four_fixture;
    use crate::models::{
        MockModelClient, ModelClient, ModelRequest, ModelResponse, ModelRoleId, ModelToolCall,
        ModelUsage,
    };
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use crate::session::{
        MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent,
        SessionState, Turn, TurnSummary,
    };

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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
        assert_turn_prefix_hashes_are_stable(&turns);

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn warm_threshold_summarizes_oldest_turns_without_dropping_turn_records() {
        let base_dir = std::env::temp_dir().join(format!("qsf-warm-tier-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
    fn recall_tool_expands_summarized_turn_and_freezes_tool_message() {
        let base_dir = std::env::temp_dir().join(format!("qsf-recall-tool-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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

        assert_turn_prefix_hashes_are_stable(&turn_records);

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn recall_tool_failure_does_not_append_turn() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-recall-tool-fail-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-recall-tool-loop-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
        assert!(events.contains("multi-round tool calls are not supported"));

        fs::remove_dir_all(base_dir).unwrap();
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
        let turn0_prompt = assemble_prompt(&[], "input 0", "");
        let turn0 = test_turn_with_hash(
            0,
            turn0_prompt.full_request_hash,
            turn0_prompt.message_count,
        );
        let turn1_prompt = assemble_prompt_with_summaries(
            &[PromptTurnSummary {
                turn_index: 0,
                summary: &summary.summary,
            }],
            &[],
            "input 1",
            "",
        );
        let turn1 = test_turn_with_hash(
            1,
            turn1_prompt.full_request_hash,
            turn1_prompt.message_count,
        );
        let turn2_prompt = assemble_prompt_with_summaries(
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
        );
        let turn2 = test_turn_with_hash(
            2,
            turn2_prompt.full_request_hash,
            turn2_prompt.message_count,
        );
        let state = SessionState {
            turns: vec![turn0, turn1, turn2],
            summarized_turns: vec![summary],
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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-warm-multi-summary-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
        let mut state = SessionState::new(test_config_with_warm_threshold(10, 2));
        state.turns = (0..5).map(test_turn).collect();

        age_out_warm_turns(&mut context, &mut state, &MockModelClient::default()).unwrap();

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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();

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

    struct RepeatingToolCallClient;

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

    fn test_turn(index: usize) -> Turn {
        test_turn_with_hash(index, ContentHash([index as u8; 32]), 2)
    }

    fn test_turn_with_hash(
        index: usize,
        full_request_hash: ContentHash,
        message_count: usize,
    ) -> Turn {
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

    fn assert_turn_prefix_hashes_are_stable(turn_records: &[&EventRecord]) {
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
            let prompt = assemble_prompt(
                &prior_turns,
                &current.user_input,
                &current.retrieved_memory_block,
            );

            assert_eq!(
                prior_request_prefix_hash(&prompt.messages, previous.message_count),
                Some(previous.full_request_hash)
            );
        }
    }
}
