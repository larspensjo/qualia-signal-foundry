use std::collections::BTreeSet;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ms};
use crate::runtime::run_context::RunContext;
use crate::tools::{
    CALCULATOR_TOOL_NAME, READ_PROJECT_DOC_TOOL_NAME, RECALL_TURN_TOOL_NAME,
    SEARCH_PROJECT_DOCS_TOOL_NAME, ToolCategory, ToolContext, ToolMetadata, ToolPermission,
    ToolRegistry, ToolRequest, ToolResult, ToolSideEffectLevel,
};

use super::{ModelRequest, ModelToolCall};

pub fn dispatch_model_tool_calls(
    context: &mut RunContext,
    request: &ModelRequest,
    registry: &ToolRegistry,
    state_ctx: &dyn ToolContext,
    project_doc_budget: &mut ProjectDocToolBudget,
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

        if let Some(cap_check) = project_doc_cap_check_for_call(tool_call, project_doc_budget) {
            if cap_check.over_cap {
                let result = refuse_project_doc_tool_call(
                    context,
                    request,
                    tool_call,
                    cap_check,
                    project_doc_budget.turn_index,
                )?;
                results.push(result);
                continue;
            }
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
                let tool_latency_ms = elapsed_ms(started_at);
                context.record_event(
                    EventType::ToolCompleted,
                    json!({
                        "session_id": &request.session_id,
                        "role_id": request.role.role_id,
                        "tool_name": &result.tool_name,
                        "call_id": &tool_call.call_id,
                        "category": result.category,
                        "side_effect_level": result.side_effect_level,
                        "latency_ms": tool_latency_ms,
                        "scope": "model_tool_dispatch",
                    }),
                    None,
                )?;
                if let Some((operation, trace_record)) = project_doc_success_trace(
                    context,
                    request,
                    tool_call,
                    &tool_request.tool_name,
                    &result,
                    project_doc_budget.turn_index,
                    tool_latency_ms,
                )? {
                    context.record_trace(trace_record)?;
                    debug_assert!(
                        operation == "project_doc_search" || operation == "project_doc_read"
                    );
                }
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

pub const PROJECT_DOC_SEARCH_CAP_PER_TURN: usize = 2;
pub const PROJECT_DOC_READ_CAP_PER_TURN: usize = 1;

#[derive(Clone, Debug)]
pub struct ProjectDocToolBudget {
    pub turn_index: usize,
    search_calls: usize,
    read_calls: usize,
}

impl ProjectDocToolBudget {
    pub fn new(turn_index: usize) -> Self {
        Self {
            turn_index,
            search_calls: 0,
            read_calls: 0,
        }
    }

    fn record_search_attempt(&mut self) -> ProjectDocCapCheck {
        self.search_calls = self.search_calls.saturating_add(1);
        ProjectDocCapCheck {
            cap: PROJECT_DOC_SEARCH_CAP_PER_TURN,
            attempted_count: self.search_calls,
            over_cap: self.search_calls > PROJECT_DOC_SEARCH_CAP_PER_TURN,
        }
    }

    fn record_read_attempt(&mut self) -> ProjectDocCapCheck {
        self.read_calls = self.read_calls.saturating_add(1);
        ProjectDocCapCheck {
            cap: PROJECT_DOC_READ_CAP_PER_TURN,
            attempted_count: self.read_calls,
            over_cap: self.read_calls > PROJECT_DOC_READ_CAP_PER_TURN,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProjectDocCapCheck {
    cap: usize,
    attempted_count: usize,
    over_cap: bool,
}

fn project_doc_cap_check_for_call(
    tool_call: &ModelToolCall,
    project_doc_budget: &mut ProjectDocToolBudget,
) -> Option<ProjectDocCapCheck> {
    match tool_call.name.as_str() {
        SEARCH_PROJECT_DOCS_TOOL_NAME => Some(project_doc_budget.record_search_attempt()),
        READ_PROJECT_DOC_TOOL_NAME => Some(project_doc_budget.record_read_attempt()),
        _ => None,
    }
}

fn refuse_project_doc_tool_call(
    context: &mut RunContext,
    request: &ModelRequest,
    tool_call: &ModelToolCall,
    cap_check: ProjectDocCapCheck,
    turn_index: usize,
) -> Result<ToolResult> {
    let operation = project_doc_operation_name(&tool_call.name);
    let arguments = sanitized_project_doc_arguments(&tool_call.name, &tool_call.arguments);
    let refusal_message = format!(
        "tool `{}` refused: per-turn budget exhausted",
        tool_call.name
    );

    context.record_event(
        EventType::ToolFailed,
        json!({
            "session_id": &request.session_id,
            "role_id": request.role.role_id,
            "tool_name": &tool_call.name,
            "call_id": &tool_call.call_id,
            "turn_index": turn_index,
            "error": &refusal_message,
            "refusal_reason": "per_turn_cap",
            "cap": cap_check.cap,
            "attempted_count": cap_check.attempted_count,
            "arguments": &arguments,
            "scope": "model_tool_dispatch",
        }),
        None,
    )?;
    context.record_trace(
        TraceRecord::new(
            context.experiment_id(),
            operation,
            "(refused)",
            "per_turn_cap",
        )
        .with_details(json!({
            "session_id": &request.session_id,
            "role_id": request.role.role_id,
            "call_id": &tool_call.call_id,
            "tool_name": &tool_call.name,
            "turn_index": turn_index,
            "refused": true,
            "refusal_reason": "per_turn_cap",
            "cap": cap_check.cap,
            "attempted_count": cap_check.attempted_count,
            "arguments": arguments,
        })),
    )?;

    Ok(ToolResult {
        tool_name: tool_call.name.clone(),
        category: ToolCategory::ReadOnly,
        side_effect_level: ToolSideEffectLevel::ReadOnly,
        input: arguments.to_string(),
        output_text: String::new(),
        numeric_value: None,
        observation_summary: format!(
            "{} refused: per_turn_cap (max {} call(s) per turn).",
            tool_call.name, cap_check.cap
        ),
    })
}

fn project_doc_operation_name(tool_name: &str) -> &'static str {
    match tool_name {
        SEARCH_PROJECT_DOCS_TOOL_NAME => "project_doc_search",
        READ_PROJECT_DOC_TOOL_NAME => "project_doc_read",
        _ => "project_doc_unknown",
    }
}

fn project_doc_success_trace(
    context: &RunContext,
    request: &ModelRequest,
    tool_call: &ModelToolCall,
    tool_name: &str,
    result: &ToolResult,
    turn_index: usize,
    tool_latency_ms: u64,
) -> Result<Option<(&'static str, TraceRecord)>> {
    let operation = project_doc_operation_name(tool_name);
    let arguments = sanitized_project_doc_arguments(tool_name, &tool_call.arguments);

    let trace = match tool_name {
        SEARCH_PROJECT_DOCS_TOOL_NAME => {
            let parsed_hits = serde_json::from_str::<serde_json::Value>(&result.output_text)
                .unwrap_or_else(|_| json!([]));
            let hit_count = parsed_hits.as_array().map(|hits| hits.len()).unwrap_or(0);

            TraceRecord::new(
                context.experiment_id(),
                operation,
                tool_call
                    .arguments
                    .get("query")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
                format!("{hit_count} hit(s)"),
            )
            .with_details(json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "call_id": &tool_call.call_id,
                "tool_name": tool_name,
                "turn_index": turn_index,
                "arguments": arguments,
                "hits": parsed_hits,
                "hit_count": hit_count,
                "refused": false,
            }))
            .with_latency_ms(tool_latency_ms)
        }
        READ_PROJECT_DOC_TOOL_NAME => {
            let parsed = serde_json::from_str::<serde_json::Value>(&result.output_text)
                .unwrap_or_else(|_| json!({}));
            let is_full = parsed
                .get("is_full")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let omitted_sections = parsed
                .get("omitted_sections")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            TraceRecord::new(
                context.experiment_id(),
                operation,
                tool_call
                    .arguments
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
                parsed
                    .get("is_full")
                    .map(|value| format!("is_full={value}"))
                    .unwrap_or_else(|| "?".to_string()),
            )
            .with_details(json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "call_id": &tool_call.call_id,
                "tool_name": tool_name,
                "turn_index": turn_index,
                "arguments": arguments,
                "read": parsed,
                "is_full": is_full,
                "omitted_sections": omitted_sections,
                "refused": false,
            }))
            .with_latency_ms(tool_latency_ms)
        }
        _ => return Ok(None),
    };

    Ok(Some((operation, trace)))
}

fn sanitized_project_doc_arguments(
    tool_name: &str,
    arguments: &serde_json::Value,
) -> serde_json::Value {
    let mut sanitized = serde_json::Map::new();
    let Some(object) = arguments.as_object() else {
        return serde_json::Value::Object(sanitized);
    };

    match tool_name {
        SEARCH_PROJECT_DOCS_TOOL_NAME => {
            if let Some(query) = object.get("query") {
                sanitized.insert("query".to_string(), query.clone());
            }
            if let Some(max_results) = object.get("max_results") {
                sanitized.insert("max_results".to_string(), max_results.clone());
            }
        }
        READ_PROJECT_DOC_TOOL_NAME => {
            if let Some(path) = object.get("path") {
                sanitized.insert("path".to_string(), path.clone());
            }
            if let Some(focus) = object.get("focus") {
                sanitized.insert("focus".to_string(), focus.clone());
            }
            if let Some(max_tokens) = object.get("max_tokens") {
                sanitized.insert("max_tokens".to_string(), max_tokens.clone());
            }
        }
        _ => {}
    }

    serde_json::Value::Object(sanitized)
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
    use std::path::PathBuf;

    use serde_json::json;

    use super::dispatch_model_tool_calls;
    use crate::models::{
        ModelRequest, ModelRole, ModelRoleId, ModelToolCall, ProjectDocToolBudget,
    };
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::observability::trace::TraceRecord;
    use crate::project_docs::{DocHit, DocRead, ProjectDocService};
    use crate::runtime::run_context::RunContext;
    use crate::session::{SessionConfig, SessionState, Turn, TurnSummary};
    use crate::tools::{
        READ_PROJECT_DOC_TOOL_NAME, RECALL_TURN_TOOL_NAME, ResponderToolContext,
        SEARCH_PROJECT_DOCS_TOOL_NAME, SessionToolContext, ToolCategory, ToolRegistry,
        ToolSideEffectLevel,
    };

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn project_doc_service() -> ProjectDocService {
        ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        )
    }

    fn responder_tool_context<'a>(
        state: &'a SessionState,
        service: &'a ProjectDocService,
    ) -> ResponderToolContext<'a> {
        ResponderToolContext {
            state,
            project_docs: service,
        }
    }

    #[test]
    fn dispatcher_rejects_tool_not_allowed_for_role() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let tool_ctx = SessionToolContext { state: &state };
        let mut project_doc_budget = ProjectDocToolBudget::new(0);
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

        let error = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut project_doc_budget,
            &tool_calls,
        )
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
        let mut project_doc_budget = ProjectDocToolBudget::new(0);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![RECALL_TURN_TOOL_NAME.to_string()];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let tool_definitions = registry.model_tool_definitions_for(&allowed);
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(tool_definitions);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            RECALL_TURN_TOOL_NAME,
            json!({ "turn_id": 0 }),
        )];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut project_doc_budget,
            &tool_calls,
        )
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
        let mut project_doc_budget = ProjectDocToolBudget::new(0);
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

        let error = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut project_doc_budget,
            &tool_calls,
        )
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
        let mut project_doc_budget = ProjectDocToolBudget::new(0);
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

        let _ = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut project_doc_budget,
            &tool_calls,
        );
    }

    #[test]
    fn third_search_call_in_one_turn_is_refused() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(3);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));
        let tool_calls = vec![
            ModelToolCall::new(
                "call-1",
                SEARCH_PROJECT_DOCS_TOOL_NAME,
                json!({"query": "Maturity"}),
            ),
            ModelToolCall::new(
                "call-2",
                SEARCH_PROJECT_DOCS_TOOL_NAME,
                json!({"query": "Maturity"}),
            ),
            ModelToolCall::new(
                "call-3",
                SEARCH_PROJECT_DOCS_TOOL_NAME,
                json!({"query": "Maturity"}),
            ),
        ];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tool_name, SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(results[0].category, ToolCategory::ReadOnly);
        assert_eq!(results[0].side_effect_level, ToolSideEffectLevel::ReadOnly);
        assert_eq!(results[1].tool_name, SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(results[1].category, ToolCategory::ReadOnly);
        assert!(results[2].observation_summary.contains("per_turn_cap"));

        let events = parse_event_records(
            &fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap(),
        );
        let failed_events = events
            .iter()
            .filter(|record| record.event_type == EventType::ToolFailed)
            .collect::<Vec<_>>();
        assert_eq!(failed_events.len(), 1);
        let payload = &failed_events[0].payload;
        assert_eq!(payload["refusal_reason"], "per_turn_cap");
        assert_eq!(payload["cap"], super::PROJECT_DOC_SEARCH_CAP_PER_TURN);
        assert_eq!(payload["attempted_count"], 3);
        assert_eq!(payload["arguments"]["query"], "Maturity");
        assert_eq!(payload["arguments"].get("max_results"), None);

        let traces = parse_trace_records(
            &fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap(),
        );
        let trace = traces
            .iter()
            .find(|record| {
                record.operation == "project_doc_search" && record.details["refused"] == true
            })
            .expect("refusal trace present");
        assert_eq!(trace.details["refused"], true);
        assert_eq!(trace.details["call_id"], "call-3");
        assert_eq!(trace.details["session_id"], context.run_id());
        assert_eq!(trace.details["tool_name"], SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(trace.details["turn_index"], 3);
        assert_eq!(trace.details["cap"], super::PROJECT_DOC_SEARCH_CAP_PER_TURN);
        assert_eq!(trace.details["attempted_count"], 3);
        assert_eq!(trace.details["arguments"]["query"], "Maturity");

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn second_read_call_in_one_turn_is_refused() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(4);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));
        let tool_calls = vec![
            ModelToolCall::new(
                "call-1",
                READ_PROJECT_DOC_TOOL_NAME,
                json!({"path": "sample_concept.md"}),
            ),
            ModelToolCall::new(
                "call-2",
                READ_PROJECT_DOC_TOOL_NAME,
                json!({"path": "sample_concept.md"}),
            ),
        ];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_name, READ_PROJECT_DOC_TOOL_NAME);
        assert_eq!(results[1].tool_name, READ_PROJECT_DOC_TOOL_NAME);
        assert!(results[1].observation_summary.contains("per_turn_cap"));

        let events = parse_event_records(
            &fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap(),
        );
        let failed_event = events
            .iter()
            .find(|record| record.event_type == EventType::ToolFailed)
            .expect("failed event present");
        assert_eq!(failed_event.payload["refusal_reason"], "per_turn_cap");
        assert_eq!(
            failed_event.payload["cap"],
            super::PROJECT_DOC_READ_CAP_PER_TURN
        );
        assert_eq!(failed_event.payload["attempted_count"], 2);
        assert_eq!(
            failed_event.payload["arguments"]["path"],
            "sample_concept.md"
        );

        let traces = parse_trace_records(
            &fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap(),
        );
        let trace = traces
            .iter()
            .find(|record| {
                record.operation == "project_doc_read" && record.details["refused"] == true
            })
            .expect("refusal trace present");
        assert_eq!(trace.details["call_id"], "call-2");
        assert_eq!(trace.details["turn_index"], 4);
        assert_eq!(trace.details["cap"], super::PROJECT_DOC_READ_CAP_PER_TURN);
        assert_eq!(trace.details["attempted_count"], 2);
        assert_eq!(trace.details["arguments"]["path"], "sample_concept.md");

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn budget_persists_across_dispatch_batches_in_same_turn() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(7);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));

        let first_batch = vec![ModelToolCall::new(
            "call-1",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({"path": "sample_concept.md"}),
        )];
        let first_results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &first_batch,
        )
        .unwrap();
        assert_eq!(first_results.len(), 1);
        assert!(first_results[0].observation_summary.contains("returned"));

        let second_batch = vec![ModelToolCall::new(
            "call-2",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({"path": "sample_concept.md"}),
        )];
        let second_results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &second_batch,
        )
        .unwrap();
        assert_eq!(second_results.len(), 1);
        assert!(
            second_results[0]
                .observation_summary
                .contains("per_turn_cap")
        );

        let events = parse_event_records(
            &fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap(),
        );
        let refusal = events
            .iter()
            .find(|record| {
                record.event_type == EventType::ToolFailed
                    && record.payload["refusal_reason"] == "per_turn_cap"
            })
            .expect("refusal event present");
        assert_eq!(refusal.payload["turn_index"], 7);
        assert_eq!(refusal.payload["attempted_count"], 2);
        assert_eq!(refusal.payload["arguments"]["path"], "sample_concept.md");

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn fresh_budget_allows_next_turn_to_use_caps_again() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));

        let mut budget_turn_7 = ProjectDocToolBudget::new(7);
        let exhausted = vec![
            ModelToolCall::new(
                "call-1",
                READ_PROJECT_DOC_TOOL_NAME,
                json!({"path": "sample_concept.md"}),
            ),
            ModelToolCall::new(
                "call-2",
                READ_PROJECT_DOC_TOOL_NAME,
                json!({"path": "sample_concept.md"}),
            ),
        ];
        let _ = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget_turn_7,
            &exhausted,
        )
        .unwrap();

        let mut budget_turn_8 = ProjectDocToolBudget::new(8);
        let fresh_results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget_turn_8,
            &[ModelToolCall::new(
                "call-3",
                READ_PROJECT_DOC_TOOL_NAME,
                json!({"path": "sample_concept.md"}),
            )],
        )
        .unwrap();

        assert_eq!(fresh_results.len(), 1);
        assert!(fresh_results[0].observation_summary.contains("returned"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn successful_search_emits_project_doc_search_trace() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(3);
        let role = responder_role();
        let request = allowed_responder_request(&context, &registry, role);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            json!({"query": "Maturity"}),
        )];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(results[0].category, ToolCategory::ReadOnly);
        assert_eq!(results[0].side_effect_level, ToolSideEffectLevel::ReadOnly);

        let hits: Vec<DocHit> = serde_json::from_str(&results[0].output_text).unwrap();
        assert!(!hits.is_empty());

        let events = read_events(&context);
        assert!(
            events
                .iter()
                .any(|record| record.event_type == EventType::ToolCompleted)
        );

        let traces = read_traces(&context);
        let trace = traces
            .iter()
            .find(|record| {
                record.operation == "project_doc_search"
                    && !record.details["refused"].as_bool().unwrap_or(true)
            })
            .expect("success trace present");
        assert_eq!(trace.input_summary, "Maturity");
        assert_eq!(trace.output_summary, format!("{} hit(s)", hits.len()));
        assert_eq!(trace.details["session_id"], context.run_id());
        assert_eq!(trace.details["role_id"], json!("conversational_responder"));
        assert_eq!(trace.details["call_id"], "call-1");
        assert_eq!(trace.details["tool_name"], SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(trace.details["turn_index"], json!(3));
        assert_eq!(trace.details["refused"], false);
        assert_eq!(trace.details["arguments"]["query"], "Maturity");
        assert_eq!(trace.details["arguments"].get("max_results"), None);
        assert_eq!(trace.details["hit_count"], json!(hits.len()));
        assert_eq!(trace.details["hits"], serde_json::to_value(&hits).unwrap());
        assert!(trace.latency_ms.is_some());
        assert!(trace.latency_ms.unwrap() < u64::MAX);

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn successful_read_emits_project_doc_read_trace() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(4);
        let role = responder_role();
        let request = allowed_responder_request(&context, &registry, role);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({
                "path": "sample_concept.md",
                "focus": "Maturity",
                "max_tokens": 1200
            }),
        )];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, READ_PROJECT_DOC_TOOL_NAME);
        assert_eq!(results[0].category, ToolCategory::ReadOnly);
        assert_eq!(results[0].side_effect_level, ToolSideEffectLevel::ReadOnly);

        let read: DocRead = serde_json::from_str(&results[0].output_text).unwrap();
        assert_eq!(read.path, "sample_concept.md");
        assert!(read.content.contains("Concept: Sample"));

        let events = read_events(&context);
        assert!(
            events
                .iter()
                .any(|record| record.event_type == EventType::ToolCompleted)
        );

        let traces = read_traces(&context);
        let trace = traces
            .iter()
            .find(|record| {
                record.operation == "project_doc_read"
                    && !record.details["refused"].as_bool().unwrap_or(true)
            })
            .expect("success trace present");
        assert_eq!(trace.input_summary, "sample_concept.md");
        assert_eq!(trace.output_summary, "is_full=false");
        assert_eq!(trace.details["session_id"], context.run_id());
        assert_eq!(trace.details["role_id"], json!("conversational_responder"));
        assert_eq!(trace.details["call_id"], "call-1");
        assert_eq!(trace.details["tool_name"], READ_PROJECT_DOC_TOOL_NAME);
        assert_eq!(trace.details["turn_index"], json!(4));
        assert_eq!(trace.details["refused"], false);
        assert_eq!(trace.details["arguments"]["path"], "sample_concept.md");
        assert_eq!(trace.details["arguments"]["focus"], "Maturity");
        assert_eq!(trace.details["arguments"]["max_tokens"], json!(1200));
        assert_eq!(trace.details["read"], serde_json::to_value(&read).unwrap());
        assert_eq!(trace.details["is_full"], json!(read.is_full));
        assert_eq!(
            trace.details["omitted_sections"],
            serde_json::to_value(&read.omitted_sections).unwrap()
        );
        assert!(trace.latency_ms.is_some());

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn failed_read_execution_emits_no_success_trace() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-tool-dispatch-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "tool-dispatch-test").unwrap();
        let registry = ToolRegistry::default();
        let state = SessionState::new(test_config());
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(5);
        let role = responder_role();
        let request = allowed_responder_request(&context, &registry, role);
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            READ_PROJECT_DOC_TOOL_NAME,
            json!({"path": "outside.txt"}),
        )];

        let error = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not in allowlist"));

        let events = read_events(&context);
        assert!(
            events
                .iter()
                .any(|record| record.event_type == EventType::ToolFailed)
        );

        let traces = read_traces(&context);
        assert!(!traces.iter().any(|record| {
            record.operation == "project_doc_read" && record.details["refused"] == false
        }));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn non_project_doc_tools_emit_no_project_doc_trace() {
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
        let mut budget = ProjectDocToolBudget::new(6);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![RECALL_TURN_TOOL_NAME.to_string()];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));
        let tool_calls = vec![ModelToolCall::new(
            "call-1",
            RECALL_TURN_TOOL_NAME,
            json!({ "turn_id": 0 }),
        )];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, RECALL_TURN_TOOL_NAME);

        let traces = read_traces(&context);
        assert!(!traces.iter().any(|record| {
            record.operation == "project_doc_search" || record.operation == "project_doc_read"
        }));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn mixed_batch_runs_recall_turn_and_project_doc_through_one_context() {
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
        let service = project_doc_service();
        let tool_ctx = responder_tool_context(&state, &service);
        let mut budget = ProjectDocToolBudget::new(9);
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            RECALL_TURN_TOOL_NAME.to_string(),
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
        ];
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        let request = ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed));
        let tool_calls = vec![
            ModelToolCall::new("call-1", RECALL_TURN_TOOL_NAME, json!({ "turn_id": 0 })),
            ModelToolCall::new(
                "call-2",
                SEARCH_PROJECT_DOCS_TOOL_NAME,
                json!({"query": "Maturity"}),
            ),
        ];

        let results = dispatch_model_tool_calls(
            &mut context,
            &request,
            &registry,
            &tool_ctx,
            &mut budget,
            &tool_calls,
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_name, RECALL_TURN_TOOL_NAME);
        assert!(results[0].output_text.contains("[Turn 0]"));
        assert_eq!(results[1].tool_name, SEARCH_PROJECT_DOCS_TOOL_NAME);
        assert_eq!(results[1].category, ToolCategory::ReadOnly);

        fs::remove_dir_all(base_dir).unwrap();
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

    fn parse_trace_records(traces: &str) -> Vec<TraceRecord> {
        traces
            .lines()
            .map(|line| serde_json::from_str::<TraceRecord>(line).unwrap())
            .collect()
    }

    fn read_traces(context: &RunContext) -> Vec<TraceRecord> {
        parse_trace_records(&fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap())
    }

    fn read_events(context: &RunContext) -> Vec<EventRecord> {
        parse_event_records(&fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap())
    }

    fn allowed_responder_request(
        context: &RunContext,
        registry: &ToolRegistry,
        role: ModelRole,
    ) -> ModelRequest {
        let allowed_tools = role.allowed_tools.clone();
        let allowed = allowed_tools.iter().map(String::as_str).collect::<Vec<_>>();
        ModelRequest::new(role, vec![])
            .with_session_id(context.run_id())
            .with_tools(registry.model_tool_definitions_for(&allowed))
    }

    fn responder_role() -> ModelRole {
        let mut role = ModelRole::predefined(ModelRoleId::ConversationalResponder);
        role.allowed_tools = vec![
            SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            READ_PROJECT_DOC_TOOL_NAME.to_string(),
            RECALL_TURN_TOOL_NAME.to_string(),
        ];
        role
    }
}
