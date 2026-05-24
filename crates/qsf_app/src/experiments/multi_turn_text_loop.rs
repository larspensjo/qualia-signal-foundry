use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use anyhow::Context;
use serde::Serialize;
use serde_json::json;

use crate::console::styling::ColorMode;
use crate::context::{ContextAssembly, ContextFragment, ContextSourceKind, assemble_context};
use crate::conversation::prompt::{self, PromptToolMessage, PromptTurn};
use crate::conversation::{PromptAssembly, PromptTurnSummary};
use crate::memory::{
    Association, MemoryFixture, MemoryRecord, RetrievalResult, RetrievalStrategy,
    phase_four_fixture, retrieve_memories, retrieved_memory_ids,
};
use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelRole, ModelRoleId, ModelToolCall, build_client,
    dispatch_model_tool_calls, invoke_model_role, requested_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ms};
use crate::runtime::run_context::RunContext;
use crate::session::{
    MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent, SessionLimit,
    SessionState, Turn, TurnRange, TurnSummary, is_turn_summarized,
};
use crate::tools::{
    CALCULATOR_TOOL_NAME, RECALL_TURN_TOOL_NAME, SessionToolContext, ToolRegistry, ToolResult,
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
const SESSION_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::KeywordTag;
const HOT_HIGH_WATER_FRACTION: f64 = 0.80;
const HOT_LOW_WATER_FRACTION: f64 = 0.50;

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
        let state_dir = crate::session::resume::state_dir_from_env();
        let model_client = build_client(requested_provider_from_env())?;
        let memory_source = build_session_memory_source_from_env();
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        run_with_io_and_components_at_state_dir(
            context,
            stdin.lock(),
            &mut stdout,
            model_client.as_ref(),
            memory_source.as_ref(),
            config,
            state_dir,
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
        SessionEvent::TurnsAgedAndCoRetrieved {
            range, summaries, ..
        } => {
            debug_assert!(range.last_index >= range.first_index);
            assert_eq!(
                summaries.len(),
                range.last_index + 1 - range.first_index,
                "TurnsAgedAndCoRetrieved summaries must match the aged range"
            );
            state.summarized_turns.extend(summaries);
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

#[cfg_attr(not(test), allow(dead_code))]
fn run_with_io_and_components(
    context: &mut RunContext,
    mut input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
) -> anyhow::Result<ExperimentOutcome> {
    let state_dir = context.run_dir().join("state/text-loop");
    run_with_io_and_components_at_state_dir(
        context,
        &mut input,
        output,
        model_client,
        memory_source,
        config,
        state_dir,
    )
}

fn run_with_io_and_components_at_state_dir(
    context: &mut RunContext,
    mut input: impl BufRead,
    output: &mut impl Write,
    model_client: &dyn ModelClient,
    memory_source: &dyn SessionMemorySource,
    config: SessionConfig,
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<ExperimentOutcome> {
    let state_dir = state_dir.as_ref().to_path_buf();
    let resume_inputs = crate::session::resume::load_resume_inputs(&state_dir)?;
    let classified_resume_mode = crate::session::resume::classify_resume_mode(&resume_inputs);
    let config_changed = resume_inputs
        .previous_session
        .as_ref()
        .map(|session| session.config != config)
        .unwrap_or(false);
    let downgraded_for_config = matches!(
        classified_resume_mode,
        crate::session::manifest::ResumeMode::AwakeContinuation
    ) && config_changed;
    let resume_mode = if downgraded_for_config {
        crate::session::manifest::ResumeMode::ColdStart
    } else {
        classified_resume_mode
    };
    let previous_session_id = resume_inputs
        .previous_session
        .as_ref()
        .map(|session| session.session_id.clone());
    let brief_path = resume_inputs.manifest.last_sleep_brief_path.clone();
    let mut pending_boot_brief: Option<crate::sleep::commit::ConsolidatedBrief> = None;
    let mut state = match resume_mode {
        crate::session::manifest::ResumeMode::ColdStart => SessionState::new(config.clone()),
        crate::session::manifest::ResumeMode::AwakeContinuation => {
            let previous = resume_inputs
                .previous_session
                .clone()
                .context("awake continuation requires a previous session")?;
            crate::session::continuation::prepare_awake_continuation(previous, &config)
        }
        crate::session::manifest::ResumeMode::ConsolidatedBrief => {
            if let Some(path) = &brief_path {
                let absolute_path = if path.is_absolute() {
                    path.clone()
                } else {
                    state_dir.join(path)
                };
                if absolute_path.exists() {
                    let raw = fs::read_to_string(&absolute_path).with_context(|| {
                        format!(
                            "failed to read consolidated brief `{}`",
                            absolute_path.display()
                        )
                    })?;
                    pending_boot_brief = Some(serde_json::from_str(&raw).with_context(|| {
                        format!(
                            "failed to parse consolidated brief `{}`",
                            absolute_path.display()
                        )
                    })?);
                }
            }

            let mut fresh = SessionState::new(config.clone());
            fresh.previous_session_id = previous_session_id.clone();
            fresh
        }
    };
    context.record_event(
        EventType::SessionResumed,
        json!({
            "mode": resume_mode,
            "classified_mode": classified_resume_mode,
            "config_changed": config_changed,
            "downgraded_for_config": downgraded_for_config,
            "session_id": state.session_id.clone(),
            "previous_session_id": previous_session_id,
            "brief_path": brief_path,
        }),
        None,
    )?;
    apply_session_event(
        context,
        &mut state,
        SessionEvent::SessionStarted(config.clone()),
    )?;
    let mut memory_snapshot = load_session_memory_snapshot(context, memory_source, &state_dir)?;
    write_memory_source_snapshot(context, &memory_snapshot)?;
    let color_mode = ColorMode::for_stdout();
    writeln!(output, "multi-turn-text-loop ready; type :quit to exit")?;

    let mut line = String::new();
    loop {
        line.clear();
        begin_user_input_echo(output, color_mode)?;
        let bytes_read = input.read_line(&mut line);
        end_user_input_echo(output, color_mode)?;
        let bytes_read = bytes_read?;
        if bytes_read == 0 {
            end_session(
                context,
                &mut state,
                &state_dir,
                output,
                color_mode,
                SessionEndReason::Eof,
            )?;
            break;
        }

        let user_input = line.trim_end_matches(['\r', '\n']).to_string();
        if user_input.trim() == ":quit" {
            end_session(
                context,
                &mut state,
                &state_dir,
                output,
                color_mode,
                SessionEndReason::QuitCommand,
            )?;
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

        let boot_brief_fragment = if completed_turn_count(&state) == 0 {
            pending_boot_brief
                .take()
                .map(|brief| format_boot_brief_for_context(&brief))
        } else {
            None
        };

        match run_one_turn(
            context,
            &mut state,
            &state_dir,
            &mut memory_snapshot,
            model_client,
            TurnRequest {
                user_input: &user_input,
                boot_brief_fragment,
            },
            TurnConsole { output, color_mode },
        ) {
            Ok(response) => {
                print_assistant_response(output, &response, color_mode)?;
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
    persist_continuity_state(&state, &state_dir, &resume_inputs.manifest)?;

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

struct TurnRequest<'a> {
    user_input: &'a str,
    boot_brief_fragment: Option<String>,
}

struct TurnConsole<'a, W: Write> {
    output: &'a mut W,
    color_mode: ColorMode,
}

fn run_one_turn<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    memory_snapshot: &mut SessionMemorySourceSnapshot,
    model_client: &dyn ModelClient,
    request: TurnRequest<'_>,
    console: TurnConsole<'_, W>,
) -> anyhow::Result<String> {
    let TurnConsole { output, color_mode } = console;
    let turn_started_at = SystemTime::now();
    let user_input = request.user_input;
    let retrieval = retrieve_session_memories(context, state, memory_snapshot, user_input)?;
    let fragments = retrieval
        .selected
        .iter()
        .map(ContextFragment::from)
        .collect::<Vec<_>>();
    let direct_ids = retrieval
        .selected
        .iter()
        .map(|memory| memory.memory.id.clone())
        .collect::<Vec<_>>();
    let hint_candidates = crate::memory::hint_expansion::expand_neighbors(
        &direct_ids,
        &memory_snapshot.records,
        &memory_snapshot.associations,
        crate::memory::hint_expansion::MAX_HINTS_PER_TURN,
    );
    let mut all_fragments = fragments;
    for hint in &hint_candidates {
        all_fragments.push(ContextFragment {
            fragment_id: hint.memory.id.clone(),
            source_kind: ContextSourceKind::MemoryHint,
            summary: hint.memory.summary.clone(),
            tags: hint.memory.tags.clone(),
            score: hint.weight,
            estimated_tokens: hint.memory.estimated_tokens,
            source_reference: hint.memory.source_reference.clone(),
            selection_reason: format!("via {} - {}", hint.via_direct_id, hint.association_reason),
        });
    }
    apply_session_event(context, state, SessionEvent::MemoryRetrieved)?;

    let assembly = assemble_context(
        all_fragments,
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
    let retrieved_memory_block = match request.boot_brief_fragment {
        Some(brief) if retrieved_memory_block.is_empty() => brief,
        Some(brief) => format!("{brief}\n\n{retrieved_memory_block}"),
        None => retrieved_memory_block,
    };
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
    print_memory_blocks(output, &assembly, color_mode)?;

    let registry = ToolRegistry::default();
    let responder_role = conversational_responder_role_with_session_tools();
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
        let tool_calls = response.tool_calls.clone();
        let tool_executions =
            execute_model_tool_calls(context, state, &request, &registry, &tool_calls)?;
        recalled_turns = tool_executions
            .iter()
            .filter_map(|execution| execution.recall.clone())
            .collect();
        final_messages.push(ModelMessage::assistant_tool_calls(
            response.output_text.clone(),
            tool_calls,
        ));
        for execution in &tool_executions {
            let tool_message = &execution.prompt_message;
            final_messages.push(ModelMessage::tool_result(
                &tool_message.call_id,
                prompt::format_tool_message(tool_message),
            ));
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
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
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
                    "stage": "tool-follow-up",
                    "error": "tool follow-up returned additional tool calls; multi-round tool calls are not supported",
                    "tool_call_count": response.tool_calls.len(),
                    "tool_calls": &response.tool_calls,
                }),
                None,
            )?;
            anyhow::bail!(
                "tool follow-up returned additional tool calls; multi-round tool calls are not supported"
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
    apply_live_memory_reinforcement(context, state, state_dir, &retrieval)?;
    let store_path = state_dir.join("memory-store.json");
    // Fixture-backed memory has no persisted store to reload. File-backed live
    // memory refreshes only after persistence creates or updates this store.
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }

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
    age_out_warm_turns(context, state, state_dir, model_client)?;
    let store_path = state_dir.join("memory-store.json");
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }
    maybe_run_token_budget_drop(
        context,
        state,
        state_dir,
        memory_snapshot,
        model_client,
        output,
        color_mode,
    )?;

    Ok(output_text)
}

fn begin_user_input_echo<W: Write>(output: &mut W, color_mode: ColorMode) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_USER_INPUT, style_prefix};

    write!(output, "{}", style_prefix(color_mode, STYLE_USER_INPUT))?;
    output.flush()
}

fn end_user_input_echo<W: Write>(output: &mut W, color_mode: ColorMode) -> std::io::Result<()> {
    use crate::console::styling::style_reset;

    write!(output, "{}", style_reset(color_mode))?;
    output.flush()
}

fn print_assistant_response<W: Write>(
    output: &mut W,
    response: &str,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_ASSISTANT_RESPONSE, paint};

    writeln!(
        output,
        "{}",
        paint(color_mode, STYLE_ASSISTANT_RESPONSE, response)
    )
}

fn completed_turn_count(state: &SessionState) -> usize {
    state.turns.len()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenBudgetDropPlan {
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub aged_count: usize,
    pub hot_tokens_before: usize,
    pub hot_tokens_after: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropOutcome {
    pub aged_count: usize,
    pub new_associations: usize,
    pub strengthened: usize,
}

fn turn_verbatim_estimated_tokens(turn: &Turn) -> usize {
    let total_chars = turn.user_input.chars().count()
        + turn.retrieved_memory_block.chars().count()
        + turn.assistant_response.chars().count();
    total_chars.div_ceil(4).max(1)
}

pub(crate) fn plan_token_budget_drop(
    state: &SessionState,
    model_window: usize,
    high_water_fraction: f64,
    low_water_fraction: f64,
) -> Option<TokenBudgetDropPlan> {
    let active_start = state.summarized_turns.len();
    let active_turns = state.turns.iter().skip(active_start).collect::<Vec<_>>();
    if active_turns.is_empty() {
        return None;
    }

    let per_turn = active_turns
        .iter()
        .map(|turn| turn_verbatim_estimated_tokens(turn))
        .collect::<Vec<_>>();
    let hot_tokens_before = per_turn.iter().sum::<usize>();
    let high_water = (model_window as f64 * high_water_fraction) as usize;
    if hot_tokens_before <= high_water {
        return None;
    }

    let low_water = (model_window as f64 * low_water_fraction) as usize;
    let mut tokens = hot_tokens_before;
    let mut aged_count = 0;
    for size in &per_turn {
        if tokens <= low_water {
            break;
        }
        tokens = tokens.saturating_sub(*size);
        aged_count += 1;
    }

    if aged_count == 0 {
        return None;
    }

    Some(TokenBudgetDropPlan {
        first_turn_index: active_start,
        last_turn_index: active_start + aged_count - 1,
        aged_count,
        hot_tokens_before,
        hot_tokens_after: tokens,
    })
}

fn apply_live_memory_reinforcement(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    retrieval: &RetrievalResult,
) -> anyhow::Result<()> {
    let turn_index = completed_turn_count(state);
    let memory_store_path = state_dir.join("memory-store.json");
    let retrieved_pairs = retrieval
        .selected
        .iter()
        .map(|memory| (memory.memory.id.clone(), memory.score.total))
        .collect::<Vec<_>>();
    let retrieved_ids = retrieved_pairs
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();

    if !memory_store_path.exists() {
        context.record_event(
            EventType::MemoryReinforced,
            json!({
                "turn_index": turn_index,
                "ids": Vec::<String>::new(),
                "requested_ids": retrieved_ids,
                "count": 0,
                "timestamp_source": "live_now",
                "skipped_reason": "no persistent memory store on cold start",
            }),
            None,
        )?;
        return Ok(());
    }

    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path)?;
    let now = time::OffsetDateTime::now_utc();
    let deltas = crate::memory::co_retrieval::generate_deltas(
        &retrieved_pairs,
        &store.contents().associations,
        turn_index,
        &state.session_id,
        now,
    );

    let mut created_count = 0;
    let mut strengthened_count = 0;
    for delta in &deltas {
        match delta {
            crate::memory::co_retrieval::CoRetrievalDelta::Create {
                from,
                to,
                weight,
                reason,
                at,
            } => {
                store
                    .contents_mut()
                    .associations
                    .push(crate::memory::Association::new(
                        from.clone(),
                        to.clone(),
                        *weight,
                        reason.clone(),
                        *at,
                    ));
                created_count += 1;
            }
            crate::memory::co_retrieval::CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight,
                at,
            } => {
                if let Some(existing) =
                    store
                        .contents_mut()
                        .associations
                        .iter_mut()
                        .find(|association| {
                            (association.from_memory_id == *from && association.to_memory_id == *to)
                                || (association.from_memory_id == *to
                                    && association.to_memory_id == *from)
                        })
                {
                    existing.weight = *new_weight;
                    existing.last_reinforced_at = *at;
                    strengthened_count += 1;
                }
            }
        }
    }

    let retrieved_id_set = retrieved_ids.iter().cloned().collect::<HashSet<_>>();
    let mut reinforced_ids = Vec::new();
    for record in &mut store.contents_mut().records {
        if retrieved_id_set.contains(&record.id) {
            record.reinforcement_count = record.reinforcement_count.saturating_add(1);
            record.last_reinforced_at = Some(now);
            reinforced_ids.push(record.id.clone());
        }
    }
    reinforced_ids.sort();
    let reinforced_count = reinforced_ids.len();

    let dropped_count = candidate_pair_count(&retrieved_pairs)
        .saturating_sub(created_count)
        .saturating_sub(strengthened_count);

    context.record_event(
        EventType::CoRetrievalAssociationsProposed,
        json!({
            "turn_index": turn_index,
            "proposed_count": deltas.len(),
            "created_count": created_count,
            "strengthened_count": strengthened_count,
            "dropped_count": dropped_count,
        }),
        None,
    )?;
    context.record_event(
        EventType::MemoryReinforced,
        json!({
            "turn_index": turn_index,
            "ids": reinforced_ids.clone(),
            "requested_ids": retrieved_ids,
            "count": reinforced_count,
            "timestamp_source": "live_now",
        }),
        None,
    )?;

    if !deltas.is_empty() || !reinforced_ids.is_empty() {
        store.persist()?;
        context.record_event(
            EventType::MemoryStorePersisted,
            json!({
                "turn_index": turn_index,
                "path": memory_store_path.display().to_string(),
                "records_count": store.contents().records.len(),
                "associations_count": store.contents().associations.len(),
            }),
            None,
        )?;
    }

    Ok(())
}

fn candidate_pair_count(retrieved: &[(String, f64)]) -> usize {
    let mut pairs = HashSet::new();
    for first_index in 0..retrieved.len() {
        for second_index in (first_index + 1)..retrieved.len() {
            let first_id = &retrieved[first_index].0;
            let second_id = &retrieved[second_index].0;
            if first_id == second_id {
                continue;
            }
            let pair = if first_id <= second_id {
                (first_id.clone(), second_id.clone())
            } else {
                (second_id.clone(), first_id.clone())
            };
            pairs.insert(pair);
        }
    }
    pairs.len()
}

fn conversational_responder_role_with_session_tools() -> ModelRole {
    let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
    role.allowed_tools = vec![
        RECALL_TURN_TOOL_NAME.to_string(),
        CALCULATOR_TOOL_NAME.to_string(),
    ];
    role
}

struct ToolExecution {
    prompt_message: PromptToolMessage,
    recall: Option<RecallRecord>,
}

fn execute_model_tool_calls(
    context: &mut RunContext,
    state: &SessionState,
    request: &ModelRequest,
    registry: &ToolRegistry,
    tool_calls: &[ModelToolCall],
) -> anyhow::Result<Vec<ToolExecution>> {
    let tool_ctx = SessionToolContext { state };
    let dispatch_started_at = Instant::now();
    let tool_results =
        dispatch_model_tool_calls(context, request, registry, &tool_ctx, tool_calls)?;
    let dispatch_latency_ms = elapsed_ms(dispatch_started_at);
    let tool_latency_ms = dispatch_latency_ms / tool_results.len().max(1) as u64;
    let mut executions = Vec::with_capacity(tool_results.len());

    for (tool_call, result) in tool_calls.iter().zip(tool_results) {
        let (prompt_message, recall) =
            prompt_tool_message_from_result(context, state, tool_call, result, tool_latency_ms)?;
        executions.push(ToolExecution {
            prompt_message,
            recall,
        });
    }

    Ok(executions)
}

fn prompt_tool_message_from_result(
    context: &mut RunContext,
    state: &SessionState,
    tool_call: &ModelToolCall,
    result: ToolResult,
    latency_ms: u64,
) -> anyhow::Result<(PromptToolMessage, Option<RecallRecord>)> {
    if result.tool_name == RECALL_TURN_TOOL_NAME {
        let recall = recall_record_from_tool_result(tool_call, result, latency_ms)?;
        record_recall_tool_trace(context, state, &recall)?;
        let prompt_message = prompt_tool_message_from_recall(&recall);
        return Ok((prompt_message, Some(recall)));
    }

    let prompt_message = PromptToolMessage {
        tool_name: result.tool_name.clone(),
        call_id: tool_call.call_id.clone(),
        arguments: tool_call.arguments.clone(),
        content: format_tool_result_message(&result),
    };
    Ok((prompt_message, None))
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

fn record_recall_tool_trace(
    context: &mut RunContext,
    state: &SessionState,
    recall: &RecallRecord,
) -> anyhow::Result<()> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "session-recall-tool",
        format!("recall_turn turn_id={}", recall.turn_id),
        format!("recalled summarized turn {}", recall.turn_id),
    )
    .with_details(json!({
        "session_id": context.run_id(),
        "completed_turn_count": completed_turn_count(state),
        "recall": recall,
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
        arguments: serde_json::json!({ "turn_id": recall.turn_id }),
        content: format_recall_tool_message(recall),
    }
}

fn format_tool_result_message(result: &ToolResult) -> String {
    match result.tool_name.as_str() {
        CALCULATOR_TOOL_NAME => {
            format!(
                "[calculator]\nexpression: {}\nresult: {}\n{}",
                result.input,
                result.output_text,
                result.observation_summary.trim()
            )
        }
        _ => result.observation_summary.clone(),
    }
}

fn age_out_warm_turns(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    model_client: &dyn ModelClient,
) -> anyhow::Result<()> {
    while active_turn_count(state) > state.config.warm_threshold {
        let Some(turn) = oldest_unsummarized_turn(state).cloned() else {
            break;
        };
        if let Err(error) = persist_cross_turn_range(
            context,
            state,
            &state_dir.join("memory-store.json"),
            CrossTurnPersistRequest {
                first_turn_index: turn.index,
                last_turn_index: turn.index,
                kind: qsf_memory::ProcessedRangeKind::LiveBatch,
                now: time::OffsetDateTime::now_utc(),
                event_kind: "live_count_threshold",
            },
        ) {
            let error_summary = sanitize_error(&error.to_string());
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": state.session_id,
                    "stage": "live-count-threshold-cross-turn",
                    "state_dir": state_dir.display().to_string(),
                    "turn_index": turn.index,
                    "error": error_summary,
                }),
                None,
            )?;
            engine_logging::engine_error!(
                "count-threshold cross-turn persistence failed: session_id={} state_dir={} turn_index={} error={}",
                state.session_id,
                state_dir.display(),
                turn.index,
                error_summary
            );
        }
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

fn maybe_run_token_budget_drop<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    memory_snapshot: &mut SessionMemorySourceSnapshot,
    model_client: &dyn ModelClient,
    output: &mut W,
    color_mode: ColorMode,
) -> anyhow::Result<Option<DropOutcome>> {
    let (window, known_model_window) =
        crate::runtime::model_context_window::model_max_tokens_or_default(&state.config.model_id);
    if !known_model_window {
        context.record_event(
            EventType::ErrorOccurred,
            json!({
                "session_id": state.session_id,
                "stage": "token-budget-aging-model-window",
                "model_id": state.config.model_id,
                "fallback_max_tokens": window,
                "message": "unknown model context window; using fallback for token-budget aging",
            }),
            None,
        )?;
    }

    let Some(plan) = plan_token_budget_drop(
        state,
        window,
        HOT_HIGH_WATER_FRACTION,
        HOT_LOW_WATER_FRACTION,
    ) else {
        return Ok(None);
    };

    let event = run_token_budget_drop_side_effect(
        context,
        state,
        state_dir,
        plan,
        time::OffsetDateTime::now_utc(),
        model_client,
    )?;
    let outcome = drop_outcome_from_event(&event);
    apply_session_event(context, state, event)?;

    let store_path = state_dir.join("memory-store.json");
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }

    if let Some(outcome) = &outcome {
        print_drop_marker(
            output,
            outcome.aged_count,
            outcome.new_associations,
            outcome.strengthened,
            color_mode,
        )?;
    }

    Ok(outcome)
}

fn run_token_budget_drop_side_effect(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    plan: TokenBudgetDropPlan,
    now: time::OffsetDateTime,
    model_client: &dyn ModelClient,
) -> anyhow::Result<SessionEvent> {
    let store_path = state_dir.join("memory-store.json");
    let persist = persist_cross_turn_range(
        context,
        state,
        &store_path,
        CrossTurnPersistRequest {
            first_turn_index: plan.first_turn_index,
            last_turn_index: plan.last_turn_index,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            now,
            event_kind: "live_batch",
        },
    )?
    .unwrap_or_default();

    let mut summaries = Vec::with_capacity(plan.aged_count);
    for index in plan.first_turn_index..=plan.last_turn_index {
        let turn = &state.turns[index];
        summaries.push(summarize_turn(context, state, model_client, turn)?);
    }

    Ok(SessionEvent::TurnsAgedAndCoRetrieved {
        range: TurnRange {
            first_index: plan.first_turn_index,
            last_index: plan.last_turn_index,
        },
        new_associations: persist.new_associations,
        strengthened_associations: persist.strengthened,
        persisted_at: SystemTime::now(),
        summaries,
    })
}

fn run_session_end_flush(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
) -> anyhow::Result<Option<DropOutcome>> {
    use crate::memory::processed_ranges::uncovered_turn_indices;

    let store_path = state_dir.join("memory-store.json");
    if !store_path.exists() {
        return Ok(None);
    }

    let store = crate::memory::MemoryStore::load_or_empty(&store_path)?;
    let active_start = state.summarized_turns.len();
    let total = state.turns.len();
    if total == 0 || active_start >= total {
        return Ok(None);
    }

    let uncovered = uncovered_turn_indices(
        &store.contents().processed_ranges,
        &state.session_id,
        active_start,
        total - 1,
    );
    if uncovered.is_empty() {
        return Ok(None);
    }

    let now = time::OffsetDateTime::now_utc();
    let persist = match persist_cross_turn_range(
        context,
        state,
        &store_path,
        CrossTurnPersistRequest {
            first_turn_index: active_start,
            last_turn_index: total - 1,
            kind: qsf_memory::ProcessedRangeKind::SessionEnd,
            now,
            event_kind: "session_end",
        },
    ) {
        Ok(Some(persist)) => persist,
        Ok(None) => return Ok(None),
        Err(error) => {
            let error_summary = sanitize_error(&error.to_string());
            engine_logging::engine_error!(
                "session-end flush persist failed; deferring to sleep safety net: session_id={} state_dir={} range={}..={} error={}",
                state.session_id,
                state_dir.display(),
                active_start,
                total - 1,
                error_summary
            );
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": state.session_id,
                    "stage": "session-end-cross-turn-flush",
                    "state_dir": state_dir.display().to_string(),
                    "first_turn_index": active_start,
                    "last_turn_index": total - 1,
                    "error": error_summary,
                }),
                None,
            )?;
            return Ok(None);
        }
    };

    Ok(Some(DropOutcome {
        aged_count: uncovered.len(),
        new_associations: persist.new_associations,
        strengthened: persist.strengthened,
    }))
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CrossTurnPersistOutcome {
    new_associations: usize,
    strengthened: usize,
}

struct CrossTurnPersistRequest<'a> {
    first_turn_index: usize,
    last_turn_index: usize,
    kind: qsf_memory::ProcessedRangeKind,
    now: time::OffsetDateTime,
    event_kind: &'a str,
}

fn persist_cross_turn_range(
    context: &mut RunContext,
    state: &SessionState,
    store_path: &Path,
    request: CrossTurnPersistRequest<'_>,
) -> anyhow::Result<Option<CrossTurnPersistOutcome>> {
    use crate::memory::co_retrieval::{
        CROSS_TURN_ASSOCIATION_WINDOW, CoRetrievalDelta, CrossTurnAnchorRange,
        generate_cross_turn_deltas_for_anchor_ranges,
    };
    use crate::memory::processed_ranges::{contiguous_ranges, uncovered_turn_indices};
    use qsf_memory::ProcessedRange;

    if !store_path.exists() || state.turns.is_empty() {
        return Ok(None);
    }

    let mut store = crate::memory::MemoryStore::load_or_empty(store_path)?;
    let last_requested = request
        .last_turn_index
        .min(state.turns.len().saturating_sub(1));
    if request.first_turn_index > last_requested {
        return Ok(Some(CrossTurnPersistOutcome::default()));
    }
    let uncovered = uncovered_turn_indices(
        &store.contents().processed_ranges,
        &state.session_id,
        request.first_turn_index,
        last_requested,
    );
    if uncovered.is_empty() {
        return Ok(Some(CrossTurnPersistOutcome::default()));
    }
    let ranges = contiguous_ranges(&uncovered);
    let anchor_ranges = ranges
        .iter()
        .map(|(first, last)| CrossTurnAnchorRange {
            first_turn: *first,
            last_turn: *last,
        })
        .collect::<Vec<_>>();
    let retrievals = state
        .turns
        .iter()
        .map(|turn| turn.context_assembly.retrieved_memory_ids())
        .collect::<Vec<_>>();
    let known_record_ids = store
        .contents()
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect::<HashSet<_>>();
    let deltas = generate_cross_turn_deltas_for_anchor_ranges(
        &retrievals,
        &store.contents().associations,
        &known_record_ids,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &state.session_id,
        request.now,
        &anchor_ranges,
    );

    let mut outcome = CrossTurnPersistOutcome::default();
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create {
                from,
                to,
                weight,
                reason,
                at,
            } => {
                store
                    .contents_mut()
                    .associations
                    .push(crate::memory::Association::new(
                        from, to, weight, reason, at,
                    ));
                outcome.new_associations += 1;
            }
            CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight,
                at,
            } => {
                if let Some(existing) =
                    store
                        .contents_mut()
                        .associations
                        .iter_mut()
                        .find(|association| {
                            (association.from_memory_id == from && association.to_memory_id == to)
                                || (association.from_memory_id == to
                                    && association.to_memory_id == from)
                        })
                {
                    existing.weight = new_weight;
                    existing.last_reinforced_at = at;
                    outcome.strengthened += 1;
                }
            }
        }
    }

    store
        .contents_mut()
        .processed_ranges
        .extend(ranges.iter().map(|(first, last)| ProcessedRange {
            session_id: state.session_id.clone(),
            first_turn_index: *first,
            last_turn_index: *last,
            kind: request.kind.clone(),
            at: request.now,
        }));
    store.persist()?;

    context.record_event(
        EventType::CoRetrievalAssociationsProposed,
        json!({
            "session_id": state.session_id,
            "kind": request.event_kind,
            "state_dir": store_path.parent().map(|path| path.display().to_string()),
            "first_turn_index": request.first_turn_index,
            "last_turn_index": request.last_turn_index,
            "processed_ranges": ranges,
            "processed_anchor_count": uncovered.len(),
            "new_count": outcome.new_associations,
            "strengthened_count": outcome.strengthened,
            "aged_turn_count": request.last_turn_index + 1 - request.first_turn_index,
        }),
        None,
    )?;
    context.record_event(
        EventType::MemoryStorePersisted,
        json!({
            "session_id": state.session_id,
            "stage": request.event_kind,
            "path": store_path.display().to_string(),
            "records_count": store.contents().records.len(),
            "associations_count": store.contents().associations.len(),
            "processed_ranges_count": store.contents().processed_ranges.len(),
        }),
        None,
    )?;

    Ok(Some(outcome))
}

fn drop_outcome_from_event(event: &SessionEvent) -> Option<DropOutcome> {
    match event {
        SessionEvent::TurnsAgedAndCoRetrieved {
            range,
            new_associations,
            strengthened_associations,
            ..
        } => Some(DropOutcome {
            aged_count: range.last_index + 1 - range.first_index,
            new_associations: *new_associations,
            strengthened: *strengthened_associations,
        }),
        _ => None,
    }
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

fn format_boot_brief_for_context(brief: &crate::sleep::commit::ConsolidatedBrief) -> String {
    let mut text = String::new();
    text.push_str("Previous session summary:\n");
    text.push_str(&brief.previous_session_summary);
    text.push('\n');

    if !brief.future_context_hints.is_empty() {
        text.push_str("\nFuture context hints:\n");
        for hint in &brief.future_context_hints {
            text.push_str("- ");
            text.push_str(hint);
            text.push('\n');
        }
    }

    if !brief.open_questions.is_empty() {
        text.push_str("\nOpen questions:\n");
        for question in &brief.open_questions {
            text.push_str("- ");
            text.push_str(question);
            text.push('\n');
        }
    }

    text
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
        SessionEvent::TurnsAgedAndCoRetrieved {
            range,
            new_associations,
            strengthened_associations,
            persisted_at,
            summaries,
        } => {
            context.record_event(
                EventType::TurnsAgedAndCoRetrieved,
                json!({
                    "session_id": context.run_id(),
                    "range": range,
                    "new_associations": new_associations,
                    "strengthened_associations": strengthened_associations,
                    "persisted_at": persisted_at,
                    "summary_count": summaries.len(),
                    "summaries": summaries,
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

fn end_session<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    output: &mut W,
    color_mode: ColorMode,
    reason: SessionEndReason,
) -> anyhow::Result<()> {
    if let Some(outcome) = run_session_end_flush(context, state, state_dir)? {
        print_session_end_flush(
            output,
            outcome.new_associations,
            outcome.strengthened,
            color_mode,
        )?;
    }
    apply_session_event(context, state, SessionEvent::SessionEnded { reason })
}

fn persist_continuity_state(
    state: &SessionState,
    state_dir: &Path,
    previous_manifest: &crate::session::manifest::ContinuityManifest,
) -> anyhow::Result<()> {
    let state_path = crate::session::persistence::persist_session_state(state, state_dir)?;
    let mut manifest = previous_manifest.clone();
    manifest.current_session_id = Some(state.session_id.clone());
    manifest.current_session_state_path = Some(
        state_path
            .strip_prefix(state_dir)
            .unwrap_or(&state_path)
            .to_path_buf(),
    );
    // Stage 4 will decide when stale sleep metadata is cleared or replaced after brief consumption.
    manifest.sleep_pending = true;
    manifest.resume_mode = crate::session::manifest::ResumeMode::AwakeContinuation;
    manifest.persist(state_dir.join("continuity-manifest.json"))?;
    Ok(())
}

trait SessionMemorySource {
    fn load(&self, context: &mut RunContext) -> anyhow::Result<SessionMemorySourceSnapshot>;
}

fn load_session_memory_snapshot(
    context: &mut RunContext,
    memory_source: &dyn SessionMemorySource,
    state_dir: &Path,
) -> anyhow::Result<SessionMemorySourceSnapshot> {
    let memory_store_path = state_dir.join("memory-store.json");
    if memory_store_path.exists() {
        let store = crate::memory::MemoryStore::load_or_empty(&memory_store_path)?;
        return Ok(SessionMemorySourceSnapshot::from_memory_store(
            &memory_store_path,
            store.contents().clone(),
        ));
    }

    memory_source.load(context)
}

/// Reload-on-change snapshot refresh. Called after persistence that may have
/// introduced or strengthened associations.
fn reload_session_memory_source_snapshot(
    memory_store_path: &Path,
) -> anyhow::Result<SessionMemorySourceSnapshot> {
    let store = crate::memory::MemoryStore::load_or_empty(memory_store_path)?;
    Ok(SessionMemorySourceSnapshot::from_memory_store(
        memory_store_path,
        store.contents().clone(),
    ))
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

    fn from_memory_store(path: &Path, contents: crate::memory::MemoryStoreContents) -> Self {
        Self {
            source_name: "memory_store".to_string(),
            source_reference: path.display().to_string(),
            records: contents.records,
            associations: contents.associations,
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

fn print_memory_blocks<W: Write>(
    output: &mut W,
    assembly: &ContextAssembly,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{
        STYLE_DIRECT_BODY, STYLE_DIRECT_HEADER, STYLE_HINT_BLOCK, paint,
    };

    let mut directs: Vec<String> = Vec::new();
    let mut hints: Vec<String> = Vec::new();

    for selection in &assembly.selected {
        let line = format!(
            "- {}: {}",
            selection.fragment.fragment_id, selection.fragment.summary
        );
        match selection.fragment.source_kind {
            ContextSourceKind::Memory => directs.push(line),
            ContextSourceKind::MemoryHint => hints.push(line),
            _ => {}
        }
    }

    if !directs.is_empty() {
        writeln!(
            output,
            "{}",
            paint(
                color_mode,
                STYLE_DIRECT_HEADER,
                "=== Memories retrieved for this turn ===",
            )
        )?;
        for line in &directs {
            writeln!(output, "{}", paint(color_mode, STYLE_DIRECT_BODY, line))?;
        }
    }

    if !hints.is_empty() {
        writeln!(
            output,
            "{}",
            paint(
                color_mode,
                STYLE_HINT_BLOCK,
                "=== Associated memories (hints - may or may not be relevant) ===",
            )
        )?;
        for line in &hints {
            writeln!(output, "{}", paint(color_mode, STYLE_HINT_BLOCK, line))?;
        }
    }

    Ok(())
}

fn print_drop_marker<W: Write>(
    output: &mut W,
    aged_turn_count: usize,
    new_associations: usize,
    strengthened: usize,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_DROP_MARKER, paint};

    let line = format!(
        "--- aged {} turns from prompt; +{} associations, *{} strengthened ---",
        aged_turn_count, new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
}

fn print_session_end_flush<W: Write>(
    output: &mut W,
    new_associations: usize,
    strengthened: usize,
    color_mode: ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{STYLE_DROP_MARKER, paint};

    let line = format!(
        "--- session-end flush; +{} associations, *{} strengthened ---",
        new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
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
    markdown.push_str("- Session state persistence: `continuity_manifest`\n\n");
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
    use std::path::PathBuf;

    use serde_json::Value;
    use uuid::Uuid;

    use super::{
        DEFAULT_SESSION_MODEL, SessionMemorySource, age_out_warm_turns,
        prompt_prefix_status_for_report, reduce_session, run_one_turn, run_with_io_and_components,
        run_with_io_and_components_at_state_dir,
    };
    use crate::context::{
        ContextAssembly, ContextBudget, ContextFragment, ContextSelection, ContextSourceKind,
    };
    use crate::conversation::ContentHash;
    use crate::conversation::prompt::{
        PromptTurn, PromptTurnSummary, assemble_prompt, assemble_prompt_with_summaries,
        prior_request_prefix_hash,
    };
    use crate::memory::{
        Association, MemoryFixture, MemoryRecord, MemoryRecordKind, MemoryStore, phase_four_fixture,
    };
    use crate::models::{
        MockModelClient, ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRoleId,
        ModelToolCall, ModelUsage,
    };
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use crate::session::{
        MemorySourceConfig, RecallRecord, SessionConfig, SessionEndReason, SessionEvent,
        SessionState, Turn, TurnRange, TurnSummary,
    };
    use crate::tools::{CALCULATOR_TOOL_NAME, RECALL_TURN_TOOL_NAME};

    #[test]
    fn live_retrieval_uses_keyword_tag_strategy() {
        assert_eq!(
            super::SESSION_RETRIEVAL_STRATEGY,
            crate::memory::RetrievalStrategy::KeywordTag,
            "Live loop must use KeywordTag so retrieval + hint expansion stay strict single-hop",
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

        super::print_drop_marker(&mut buf, 3, 2, 5, ColorMode::Disabled).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("aged 3 turns from prompt"));
        assert!(text.contains("+2 associations"));
        assert!(text.contains("*5 strengthened"));
    }

    #[test]
    fn print_session_end_flush_marker_renders_expected_format() {
        use crate::console::styling::ColorMode;

        let mut buf: Vec<u8> = Vec::new();

        super::print_session_end_flush(&mut buf, 4, 1, ColorMode::Disabled).unwrap();

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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-memory-hint-turn-{}", Uuid::new_v4()));
        let state_dir = base_dir.join("state/text-loop");
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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

        let plan = super::plan_token_budget_drop(&state, 1_000, 0.80, 0.50).unwrap();

        assert_eq!(plan.first_turn_index, 0);
        assert_eq!(plan.last_turn_index, 3);
        assert_eq!(plan.aged_count, 4);
        assert_eq!(plan.hot_tokens_before, 1_200);
        assert_eq!(plan.hot_tokens_after, 400);
    }

    #[test]
    fn token_budget_drop_plan_noops_below_high_water() {
        let state = synthetic_state_with_verbatim_sizes(&[100, 100, 100]);

        let plan = super::plan_token_budget_drop(&state, 1_000, 0.80, 0.50);

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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
        let mut state = SessionState::new_with_id("session-drop".to_string(), test_config(10));
        state.turns = vec![
            test_turn_with_memory_ids(0, &["memory.a"]),
            test_turn_with_memory_ids(1, &["memory.b"]),
            test_turn_with_memory_ids(2, &["memory.c"]),
        ];
        let plan = super::TokenBudgetDropPlan {
            first_turn_index: 0,
            last_turn_index: 1,
            aged_count: 2,
            hot_tokens_before: 1_000,
            hot_tokens_after: 400,
        };

        let event = super::run_token_budget_drop_side_effect(
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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
        let mut state = SessionState::new_with_id("session-flush".to_string(), test_config(10));
        state.turns = vec![
            test_turn_with_memory_ids(0, &["memory.a"]),
            test_turn_with_memory_ids(1, &["memory.b"]),
        ];

        let first = super::run_session_end_flush(&mut context, &state, &state_dir)
            .unwrap()
            .expect("expected first flush");
        let second = super::run_session_end_flush(&mut context, &state, &state_dir).unwrap();
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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-session-flush-gaps-{}", Uuid::new_v4()));
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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
        let mut state =
            SessionState::new_with_id("session-flush-gaps".to_string(), test_config(10));
        state.turns = vec![
            test_turn_with_memory_ids(0, &["memory.a"]),
            test_turn_with_memory_ids(1, &["memory.b"]),
            test_turn_with_memory_ids(2, &["memory.a"]),
            test_turn_with_memory_ids(3, &["memory.b"]),
            test_turn_with_memory_ids(4, &["memory.a"]),
        ];

        let outcome = super::run_session_end_flush(&mut context, &state, &state_dir)
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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-cross-turn-retry-{}", Uuid::new_v4()));
        let state_dir = base_dir.join("state/text-loop");
        let store_path = state_dir.join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
        store.append_records([
            memory_record("memory.a", "A", "A summary", vec!["a"], 10),
            memory_record("memory.b", "B", "B summary", vec!["b"], 10),
        ]);
        store.persist().unwrap();
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
        let mut state = SessionState::new_with_id("session-retry".to_string(), test_config(10));
        state.turns = vec![
            test_turn_with_memory_ids(0, &["memory.a"]),
            test_turn_with_memory_ids(1, &["memory.b"]),
        ];
        let request = super::CrossTurnPersistRequest {
            first_turn_index: 0,
            last_turn_index: 0,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            now: timestamp("2026-05-24T00:00:00Z"),
            event_kind: "test_retry",
        };

        let first =
            super::persist_cross_turn_range(&mut context, &state, &store_path, request).unwrap();
        let second = super::persist_cross_turn_range(
            &mut context,
            &state,
            &store_path,
            super::CrossTurnPersistRequest {
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
        assert_eq!(second.unwrap(), super::CrossTurnPersistOutcome::default());
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
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
        assert_turn_prefix_hashes_are_stable(&turns);

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn multi_turn_loop_persists_and_resumes_awake_continuation() {
        let base_dir = std::env::temp_dir().join(format!("qsf-continuity-{}", Uuid::new_v4()));
        let state_dir = base_dir.join("state/text-loop");
        let memory_source = TestMemorySource;

        let mut first_context =
            RunContext::create_in(base_dir.join("first"), "multi-turn-text-loop").unwrap();
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

        let mut second_context =
            RunContext::create_in(base_dir.join("second"), "multi-turn-text-loop").unwrap();
        let mut second_output = Vec::new();
        run_with_io_and_components_at_state_dir(
            &mut second_context,
            Cursor::new("second turn\n:quit\n"),
            &mut second_output,
            &MockModelClient::default(),
            &memory_source,
            test_config(5),
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
        let mut context =
            RunContext::create_in(base_dir.join("run"), "multi-turn-text-loop").unwrap();
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
        let mut context =
            RunContext::create_in(base_dir.join("run"), "multi-turn-text-loop").unwrap();
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

        let mut first_context =
            RunContext::create_in(base_dir.join("first"), "multi-turn-text-loop").unwrap();
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

        let mut second_context =
            RunContext::create_in(base_dir.join("second"), "multi-turn-text-loop").unwrap();
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
        let base_dir =
            std::env::temp_dir().join(format!("qsf-continuity-brief-{}", Uuid::new_v4()));
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
        let mut context =
            RunContext::create_in(base_dir.join("brief"), "multi-turn-text-loop").unwrap();
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
    fn calculator_tool_answers_arithmetic_turn_through_follow_up() {
        let base_dir = std::env::temp_dir().join(format!("qsf-calculator-tool-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
    fn openai_recall_path_preserves_tool_call_id_and_hides_tools_on_follow_up() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-openai-recall-path-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "multi-turn-text-loop").unwrap();
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
                CALCULATOR_TOOL_NAME.to_string()
            ]
        );
        assert!(first_call.messages.iter().any(|message| message.role
            == crate::models::ModelMessageRole::User
            && message.content.to_ascii_lowercase().contains("recall turn")));

        let second_call = &calls[tool_call_index + 1];
        assert_eq!(second_call.role_id, ModelRoleId::ConversationalResponder);
        assert!(second_call.tools.is_empty());
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
        let state_dir = base_dir.join("state/text-loop");
        state.turns = (0..5).map(test_turn).collect();

        age_out_warm_turns(
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

    #[derive(Default)]
    struct CapturingOpenAiRecallClient {
        calls: std::sync::Mutex<Vec<CapturedRequest>>,
    }

    #[derive(Clone, Debug)]
    struct CapturedRequest {
        role_id: ModelRoleId,
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
