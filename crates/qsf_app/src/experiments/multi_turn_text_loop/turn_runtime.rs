use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use anyhow::Context;
use serde_json::json;

use crate::console::styling::ColorMode;
use crate::context::{ContextAssembly, ContextFragment, ContextSourceKind, assemble_context};
use crate::conversation::PromptAssembly;
use crate::conversation::prompt;
use crate::memory::RetrievalResult;
use crate::models::invoke_model_role;
use crate::observability::event_log::EventType;
use crate::observability::trace::elapsed_ms;
use crate::runtime::run_context::RunContext;
use crate::session::ageing::{age_out_warm_turns, maybe_run_token_budget_drop};
use crate::session::{
    Exchange, ExchangeModelUse, ExchangeOutput, LiveSessionEvent, SessionEvent, SessionState, Turn,
};
use crate::tools::{ResponderToolContext, ToolRegistry};
use qsf_models::{ModelClient, ModelMessage, ModelResponse, ModelRole, ModelRoleId};

use super::{
    MAX_RESPONDER_TOOL_ROUNDS_PER_TURN, NON_REPLAYED_TOOL_PROMPT_PREFIX_INVALIDATION,
    SessionMemorySourceSnapshot, apply_live_session_event, apply_session_event,
    assemble_session_prompt, completed_turn_count, execute_model_tool_calls, print_memory_blocks,
    project_doc_service_for_multi_turn_text_loop, record_context_assembly,
    reload_session_memory_source_snapshot, responder_request_for_messages,
    retrieve_session_memories, verify_prompt_prefix,
};

pub(crate) struct TurnRequest<'a> {
    pub(crate) user_input: &'a str,
    pub(crate) boot_brief_fragment: Option<String>,
    pub(crate) max_output_tokens: u32,
}

pub(crate) struct TurnConsole<'a, W: Write> {
    pub(crate) output: &'a mut W,
    pub(crate) color_mode: ColorMode,
}

struct TurnContextAssembly {
    turn_index: usize,
    retrieval: RetrievalResult,
    assembly: ContextAssembly,
    retrieved_memory_block: String,
    base_prompt: PromptAssembly,
}

struct ResponderExecution {
    response: ModelResponse,
    final_prompt_assembly: PromptAssembly,
    model_latency_ms: u64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    recalled_turns: Vec<crate::session::RecallRecord>,
    has_non_replayed_tool_messages: bool,
}

pub(crate) fn run_one_turn<W: Write>(
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
    let turn_index = completed_turn_count(state);
    apply_live_session_event(
        state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
            turn_index,
            user_input.to_string(),
            turn_started_at,
        ))),
    );

    let turn_context = assemble_turn_context(
        context,
        state,
        memory_snapshot,
        user_input,
        request.boot_brief_fragment,
    )?;
    print_memory_blocks(output, &turn_context.assembly, color_mode)?;

    let responder = execute_responder_turn(
        context,
        state,
        model_client,
        &turn_context.base_prompt,
        turn_context.turn_index,
        request.max_output_tokens,
    )?;

    let output_text = apply_post_response_updates(
        context,
        state,
        state_dir,
        memory_snapshot,
        user_input,
        turn_context,
        responder,
    )?;

    run_turn_ageing(
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

fn assemble_turn_context(
    context: &mut RunContext,
    state: &mut SessionState,
    memory_snapshot: &SessionMemorySourceSnapshot,
    user_input: &str,
    boot_brief_fragment: Option<String>,
) -> anyhow::Result<TurnContextAssembly> {
    let turn_index = completed_turn_count(state);
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
    let retrieved_memory_block = match boot_brief_fragment {
        Some(brief) if retrieved_memory_block.is_empty() => brief,
        Some(brief) => format!("{brief}\n\n{retrieved_memory_block}"),
        None => retrieved_memory_block,
    };
    let base_prompt = assemble_session_prompt(state, user_input, &retrieved_memory_block, true);
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

    Ok(TurnContextAssembly {
        turn_index,
        retrieval,
        assembly,
        retrieved_memory_block,
        base_prompt,
    })
}

fn execute_responder_turn(
    context: &mut RunContext,
    state: &mut SessionState,
    model_client: &dyn ModelClient,
    base_prompt: &PromptAssembly,
    turn_index: usize,
    max_output_tokens: u32,
) -> anyhow::Result<ResponderExecution> {
    let registry = ToolRegistry::default();
    let project_docs = Arc::new(project_doc_service_for_multi_turn_text_loop(context)?);
    let responder_role = super::conversational_responder_role_with_session_and_project_doc_tools();
    let mut final_messages = base_prompt.messages.clone();
    let mut project_doc_budget = crate::models::ProjectDocToolBudget::new(turn_index);
    let mut model_latency_ms: u64 = 0;
    let mut input_tokens: u32 = 0;
    let mut cached_input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut recalled_turns = vec![];
    let mut tool_rounds = 0usize;
    let mut has_non_replayed_tool_messages = false;
    let tool_state = Arc::new(state.clone());
    let tool_ctx = ResponderToolContext {
        state: tool_state,
        project_docs,
    };
    let mut response;
    let mut current_request = responder_request_for_messages(
        &responder_role,
        base_prompt.messages.clone(),
        context,
        state,
        &registry,
        max_output_tokens,
        true,
    );

    loop {
        let started_at = Instant::now();
        response = invoke_model_role(context, model_client, &current_request)?;
        model_latency_ms = model_latency_ms.saturating_add(elapsed_ms(started_at));
        if let Some(usage) = response.usage.as_ref() {
            input_tokens = input_tokens.saturating_add(usage.input_tokens);
            cached_input_tokens = cached_input_tokens.saturating_add(usage.cached_input_tokens);
            output_tokens = output_tokens.saturating_add(usage.output_tokens);
        }

        if response.tool_calls.is_empty() {
            break;
        }

        if tool_rounds >= MAX_RESPONDER_TOOL_ROUNDS_PER_TURN {
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "session_id": context.run_id(),
                    "stage": "bounded-tool-loop",
                    "error": "responder returned tool calls after the allowed tool rounds were exhausted",
                    "tool_call_count": response.tool_calls.len(),
                    "tool_calls": &response.tool_calls,
                }),
                None,
            )?;
            anyhow::bail!(
                "responder returned tool calls after the allowed tool rounds were exhausted"
            );
        }

        let tool_calls = response.tool_calls.clone();
        let tool_executions = execute_model_tool_calls(
            context,
            state,
            &tool_ctx,
            &current_request,
            &registry,
            &mut project_doc_budget,
            &tool_calls,
        )?;
        has_non_replayed_tool_messages |= tool_executions
            .iter()
            .any(|execution| execution.recall.is_none());
        recalled_turns.extend(
            tool_executions
                .iter()
                .filter_map(|execution| execution.recall.clone()),
        );
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

        tool_rounds += 1;
        let advertise_tools = tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN;
        current_request = responder_request_for_messages(
            &responder_role,
            final_messages.clone(),
            context,
            state,
            &registry,
            max_output_tokens,
            advertise_tools,
        );
    }

    Ok(ResponderExecution {
        response,
        final_prompt_assembly: prompt::prompt_assembly_from_messages(final_messages),
        model_latency_ms,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        recalled_turns,
        has_non_replayed_tool_messages,
    })
}

fn apply_post_response_updates(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    memory_snapshot: &mut SessionMemorySourceSnapshot,
    user_input: &str,
    turn_context: TurnContextAssembly,
    responder: ResponderExecution,
) -> anyhow::Result<String> {
    let TurnContextAssembly {
        turn_index,
        retrieval,
        assembly,
        retrieved_memory_block,
        ..
    } = turn_context;
    let ResponderExecution {
        response,
        final_prompt_assembly,
        model_latency_ms,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        recalled_turns,
        has_non_replayed_tool_messages,
    } = responder;

    apply_live_session_event(
        state,
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index: turn_index,
            context_assembly: assembly.clone(),
            retrieved_memory_block: retrieved_memory_block.clone(),
            recalled_items: recalled_turns.clone(),
            live_capture: None,
        },
    );
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
    let model_use = ExchangeModelUse {
        provider_name: Some(response.provider_name.clone()),
        model_id: response.model_name.clone(),
        latency_ms: model_latency_ms,
        input_tokens,
        cached_input_tokens,
        output_tokens,
        full_request_hash: final_prompt_assembly.full_request_hash,
        message_count: final_prompt_assembly.message_count,
    };
    apply_live_session_event(state, LiveSessionEvent::ModelRoleCompleted(model_use));
    let output_text = response.output_text.clone();
    apply_live_session_event(
        state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: None,
            text: output_text.clone(),
            produced_at: SystemTime::now(),
            provider_name: Some(response.provider_name.clone()),
            target: Some("text".to_string()),
            audio_marker: None,
        }),
    );
    crate::session::apply_live_memory_reinforcement(context, state, state_dir, &retrieval)?;
    crate::session::apply_live_memory_capture(context, state, state_dir, user_input, &output_text)?;
    let store_path = state_dir.join("memory-store.json");
    // Fixture-backed memory has no persisted store to reload. File-backed live
    // memory refreshes only after persistence creates or updates this store.
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }

    let completed_at = SystemTime::now();
    apply_live_session_event(
        state,
        LiveSessionEvent::ExchangeCompleted {
            exchange_index: turn_index,
            completed_at,
        },
    );
    let completed_exchange = state
        .live
        .completed_exchanges
        .last()
        .cloned()
        .context("completed exchange missing after ExchangeCompleted")?;
    let turn = Turn::try_from(&completed_exchange).with_context(|| {
        format!(
            "failed to convert completed exchange {} into a text turn",
            completed_exchange.index
        )
    })?;
    let completed_turn_index = turn.index;
    apply_session_event(context, state, SessionEvent::TurnCompleted(turn))?;
    if has_non_replayed_tool_messages {
        apply_session_event(
            context,
            state,
            SessionEvent::PromptPrefixInvalidated {
                after_turn_index: completed_turn_index,
                reason: NON_REPLAYED_TOOL_PROMPT_PREFIX_INVALIDATION.to_string(),
            },
        )?;
    }

    Ok(output_text)
}

fn run_turn_ageing<W: Write>(
    context: &mut RunContext,
    state: &mut SessionState,
    state_dir: &Path,
    memory_snapshot: &mut SessionMemorySourceSnapshot,
    model_client: &dyn ModelClient,
    output: &mut W,
    color_mode: ColorMode,
) -> anyhow::Result<()> {
    age_out_warm_turns(context, state, state_dir, model_client)?;
    let store_path = state_dir.join("memory-store.json");
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }
    maybe_run_token_budget_drop(context, state, state_dir, model_client, output, color_mode)?;
    if store_path.exists() {
        *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
    }

    Ok(())
}
