use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use crate::conversation::prompt::PromptToolMessage;
use crate::models::{
    ModelMessage, ModelRequest, ModelRole, ModelRoleId, ModelToolCall, dispatch_model_tool_calls,
};
use crate::observability::trace::{TraceRecord, elapsed_ms};
use crate::project_docs::ProjectDocService;
use crate::runtime::run_context::RunContext;
use crate::session::{RecallRecord, SessionState};
use crate::tools::{
    CALCULATOR_TOOL_NAME, READ_PROJECT_DOC_TOOL_NAME, RECALL_TURN_TOOL_NAME, ResponderToolContext,
    SEARCH_PROJECT_DOCS_TOOL_NAME, ToolRegistry, ToolResult,
};

pub(crate) struct ToolExecution {
    pub(crate) prompt_message: PromptToolMessage,
    pub(crate) recall: Option<RecallRecord>,
}

pub(crate) fn conversational_responder_role_with_session_and_project_doc_tools() -> ModelRole {
    let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
    role.allowed_tools = vec![
        RECALL_TURN_TOOL_NAME.to_string(),
        CALCULATOR_TOOL_NAME.to_string(),
        SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
        READ_PROJECT_DOC_TOOL_NAME.to_string(),
    ];
    role
}

pub(crate) fn project_doc_service_for_multi_turn_text_loop(
    context: &RunContext,
) -> anyhow::Result<ProjectDocService> {
    let repo_root = context
        .workspace_root()
        .context(
            "multi-turn-text-loop requires --workspace-root <path> to enable project-doc service",
        )?
        .to_path_buf();
    let allowlist_path = repo_root.join("config/project-doc-introspection.toml");
    Ok(ProjectDocService::new(repo_root, allowlist_path))
}

pub(crate) fn responder_request_for_messages(
    responder_role: &ModelRole,
    messages: Vec<ModelMessage>,
    context: &RunContext,
    state: &SessionState,
    registry: &ToolRegistry,
    max_output_tokens: u32,
    advertise_tools: bool,
) -> ModelRequest {
    let mut request = ModelRequest::new(responder_role.clone(), messages)
        .with_session_id(context.run_id())
        .with_model_name(&state.config.model_id)
        .with_temperature(0.0)
        .with_max_output_tokens(max_output_tokens);
    if advertise_tools {
        let allowed_tools = responder_role
            .allowed_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        request = request.with_tools(registry.model_tool_definitions_for(&allowed_tools));
    }
    request
}

pub(crate) fn execute_model_tool_calls(
    context: &mut RunContext,
    state: &SessionState,
    project_docs: &ProjectDocService,
    request: &ModelRequest,
    registry: &ToolRegistry,
    project_doc_budget: &mut crate::models::ProjectDocToolBudget,
    tool_calls: &[ModelToolCall],
) -> anyhow::Result<Vec<ToolExecution>> {
    let tool_ctx = ResponderToolContext {
        state,
        project_docs,
    };
    let dispatch_started_at = Instant::now();
    let tool_results = dispatch_model_tool_calls(
        context,
        request,
        registry,
        &tool_ctx,
        project_doc_budget,
        tool_calls,
    )?;
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

pub(crate) fn prompt_tool_message_from_recall(recall: &RecallRecord) -> PromptToolMessage {
    PromptToolMessage {
        tool_name: recall.tool_name.clone(),
        call_id: recall.call_id.clone(),
        arguments: serde_json::json!({ "turn_id": recall.turn_id }),
        content: format_recall_tool_message(recall),
    }
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
        "completed_turn_count": super::completed_turn_count(state),
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
        SEARCH_PROJECT_DOCS_TOOL_NAME | READ_PROJECT_DOC_TOOL_NAME => {
            format!(
                "[{}]\n{}\n{}",
                result.tool_name,
                result.observation_summary.trim(),
                result.output_text.trim()
            )
        }
        _ => result.observation_summary.clone(),
    }
}
