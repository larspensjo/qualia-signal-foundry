use std::collections::BTreeSet;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::elapsed_ms;
use crate::runtime::run_context::RunContext;
use crate::tools::{
    CALCULATOR_TOOL_NAME, RECALL_TURN_TOOL_NAME, ToolContext, ToolMetadata, ToolPermission,
    ToolRegistry, ToolRequest, ToolResult,
};

use super::{ModelRequest, ModelToolCall};

pub fn dispatch_model_tool_calls(
    context: &mut RunContext,
    request: &ModelRequest,
    registry: &ToolRegistry,
    state_ctx: &dyn ToolContext,
    tool_calls: &[ModelToolCall],
) -> Result<Vec<ToolResult>> {
    debug_assert_request_tools_match_role(request);

    let mut results = Vec::with_capacity(tool_calls.len());
    let allowed_tools = request
        .role
        .allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for tool_call in tool_calls {
        if !allowed_tools.contains(tool_call.name.as_str()) {
            let message = format!(
                "tool `{}` is not permitted for role `{}`",
                tool_call.name, request.role.role_id
            );
            context.record_event(
                EventType::ToolFailed,
                json!({
                    "session_id": &request.session_id,
                    "role_id": request.role.role_id,
                    "tool_name": &tool_call.name,
                    "call_id": &tool_call.call_id,
                    "error": &message,
                }),
                None,
            )?;
            bail!(message);
        }

        let tool_request =
            match tool_request_from_model_tool_call(tool_call, context.experiment_id(), registry) {
                Ok(tool_request) => tool_request,
                Err(error) => {
                    context.record_event(
                        EventType::ToolFailed,
                        json!({
                            "session_id": &request.session_id,
                            "role_id": request.role.role_id,
                            "tool_name": &tool_call.name,
                            "call_id": &tool_call.call_id,
                            "error": error.to_string(),
                        }),
                        None,
                    )?;
                    return Err(error);
                }
            };
        let metadata = registry.metadata_for(&tool_request.tool_name);
        context.record_event(
            EventType::ToolRequested,
            json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "tool_name": &tool_request.tool_name,
                "call_id": &tool_call.call_id,
                "arguments": &tool_call.arguments,
                "input": &tool_request.input,
                "permission": &tool_request.permission,
                "requested_by": &tool_request.requested_by,
                "category": metadata.as_ref().map(|metadata| metadata.category),
                "side_effect_level": metadata.as_ref().map(|metadata| metadata.side_effect_level),
                "scope": "model_tool_dispatch",
            }),
            None,
        )?;

        let started_at = Instant::now();
        match registry.validate_and_execute(&tool_request, state_ctx) {
            Ok((_metadata, result)) => {
                context.record_event(
                    EventType::ToolCompleted,
                    json!({
                        "session_id": &request.session_id,
                        "role_id": request.role.role_id,
                        "tool_name": &result.tool_name,
                        "call_id": &tool_call.call_id,
                        "category": result.category,
                        "side_effect_level": result.side_effect_level,
                        "latency_ms": elapsed_ms(started_at),
                        "scope": "model_tool_dispatch",
                    }),
                    None,
                )?;
                results.push(result);
            }
            Err(error) => {
                context.record_event(
                    EventType::ToolFailed,
                    json!({
                        "session_id": &request.session_id,
                        "role_id": request.role.role_id,
                        "tool_name": &tool_request.tool_name,
                        "call_id": &tool_call.call_id,
                        "error": error.to_string(),
                        "latency_ms": elapsed_ms(started_at),
                    }),
                    None,
                )?;
                return Err(error);
            }
        }
    }

    Ok(results)
}

fn debug_assert_request_tools_match_role(request: &ModelRequest) {
    let allowed = request
        .role
        .allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let advertised = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<BTreeSet<_>>();

    debug_assert_eq!(
        allowed, advertised,
        "ModelRequest.tools must be derived from ModelRole.allowed_tools"
    );
}

fn tool_request_from_model_tool_call(
    tool_call: &ModelToolCall,
    requested_by: &str,
    registry: &ToolRegistry,
) -> Result<ToolRequest> {
    let metadata = registry
        .metadata_for(&tool_call.name)
        .ok_or_else(|| anyhow::anyhow!("unknown tool `{}`", tool_call.name))?;
    let permission = permission_from_metadata(&metadata);

    match tool_call.name.as_str() {
        RECALL_TURN_TOOL_NAME => {
            let turn_id = tool_call
                .arguments
                .get("turn_id")
                .and_then(|value| value.as_u64())
                .context("recall_turn requires integer argument `turn_id`")?
                as usize;
            let mut request =
                ToolRequest::recall_turn(tool_call.call_id.clone(), turn_id, requested_by);
            request.permission = permission;
            Ok(request)
        }
        CALCULATOR_TOOL_NAME => {
            let expression = tool_call
                .arguments
                .get("expression")
                .and_then(|value| value.as_str())
                .context("calculator requires string argument `expression`")?;
            let mut request = ToolRequest::calculator(expression, requested_by);
            request.permission = permission;
            Ok(request)
        }
        _ => Ok(ToolRequest {
            tool_name: tool_call.name.clone(),
            input: tool_call.arguments.to_string(),
            structured: Some(tool_call.arguments.clone()),
            permission,
            requested_by: requested_by.to_string(),
        }),
    }
}

fn permission_from_metadata(metadata: &ToolMetadata) -> ToolPermission {
    ToolPermission {
        allowed_categories: vec![metadata.category],
        max_side_effect_level: metadata.side_effect_level,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::dispatch_model_tool_calls;
    use crate::models::{ModelRequest, ModelRole, ModelRoleId, ModelToolCall};
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use crate::session::{SessionConfig, SessionState, Turn, TurnSummary};
    use crate::tools::{RECALL_TURN_TOOL_NAME, SessionToolContext, ToolRegistry};

    #[test]
    fn dispatcher_rejects_tool_not_allowed_for_role() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let tool_ctx = SessionToolContext { state: &state };
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
            vec![],
        )
        .with_session_id(context.run_id());
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            RECALL_TURN_TOOL_NAME,
            json!({ "turn_id": 0 }),
        )];

        let error =
            dispatch_model_tool_calls(&mut context, &request, &registry, &tool_ctx, &tool_calls)
                .unwrap_err();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        assert!(error.to_string().contains("not permitted for role"));
        assert!(
            parse_event_records(&events)
                .iter()
                .any(|record| record.event_type == EventType::ToolFailed)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn dispatcher_executes_allowed_recall_turn_tool() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let mut state = SessionState::new(test_config());
        state.turns.push(test_turn(0));
        state.summarized_turns.push(TurnSummary {
            turn_index: 0,
            summarized_after_turn_index: 0,
            summary: "user said one; assistant replied".to_string(),
            model_id: "mock".to_string(),
            model_latency_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
        });
        let tool_ctx = SessionToolContext { state: &state };
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![RECALL_TURN_TOOL_NAME.to_string()];
        let allowed = role
            .allowed_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let tool_definitions = registry.model_tool_definitions_for(&allowed);
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(tool_definitions);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            RECALL_TURN_TOOL_NAME,
            json!({ "turn_id": 0 }),
        )];

        let results =
            dispatch_model_tool_calls(&mut context, &request, &registry, &tool_ctx, &tool_calls)
                .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, RECALL_TURN_TOOL_NAME);
        assert!(results[0].output_text.contains("[Turn 0]"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn dispatcher_reports_registry_unknown_tool_for_allowed_missing_tool() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let tool_ctx = SessionToolContext { state: &state };
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec!["missing_tool".to_string()];
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(vec![crate::models::ModelToolDefinition::new(
                "missing_tool",
                "Missing test tool",
                json!({ "type": "object" }),
            )]);
        let tool_calls = vec![ModelToolCall::new("call-1", "missing_tool", json!({}))];

        let error =
            dispatch_model_tool_calls(&mut context, &request, &registry, &tool_ctx, &tool_calls)
                .unwrap_err();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        assert!(error.to_string().contains("unknown tool"));
        assert!(
            parse_event_records(&events)
                .iter()
                .any(|record| record.event_type == EventType::ToolFailed)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "ModelRequest.tools must be derived from ModelRole.allowed_tools")]
    fn dispatcher_debug_asserts_when_advertised_tools_drift_from_role() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let tool_ctx = SessionToolContext { state: &state };
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![RECALL_TURN_TOOL_NAME.to_string()];
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(vec![]);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            RECALL_TURN_TOOL_NAME,
            json!({ "turn_id": 0 }),
        )];

        let _ =
            dispatch_model_tool_calls(&mut context, &request, &registry, &tool_ctx, &tool_calls);
    }

    fn test_turn(index: usize) -> Turn {
        Turn {
            index,
            started_at: std::time::SystemTime::UNIX_EPOCH,
            completed_at: std::time::SystemTime::UNIX_EPOCH,
            user_input: "one".to_string(),
            context_assembly: crate::context::ContextAssembly {
                selected: vec![],
                omitted: vec![],
                budget: crate::context::ContextBudget::new(1, 100),
                used_estimated_tokens: 0,
            },
            retrieved_memory_block: String::new(),
            assistant_response: "assistant replied".to_string(),
            recalled_turns: vec![],
            model_id: "mock".to_string(),
            model_latency_ms: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            full_request_hash: crate::conversation::ContentHash([index as u8; 32]),
            message_count: 1,
        }
    }

    fn test_config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: crate::session::MemorySourceConfig {
                source: "test".to_string(),
                file: None,
            },
        }
    }

    fn parse_event_records(events: &str) -> Vec<EventRecord> {
        events
            .lines()
            .map(|line| serde_json::from_str::<EventRecord>(line).unwrap())
            .collect()
    }
}
