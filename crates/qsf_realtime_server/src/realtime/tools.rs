use std::collections::BTreeMap;

use anyhow::Context;
use qsf_memory::{Association, MemoryRecord, RetrievalStrategy};
use qsf_realtime_protocol::RealtimeToolDefinition;
use qsf_session::{
    ExchangeStatus, RuntimePhase, ToolCategory, ToolExecutionRecord, ToolExecutionStatus,
    ToolPermissionDecision, ToolRequestRecord,
};
use qsf_tools::{
    Tool, ToolContext, ToolDefinition, ToolMetadata, ToolRegistry, ToolRequest, ToolResult,
    ToolSideEffectLevel,
};
use qsf_volition::{VolitionFixture, VolitionState};
use serde::Serialize;

use crate::diagnostics::DiagnosticTrust;
use crate::realtime::memory_store::load_session_memory_store;
use crate::realtime::volition_tools::{
    INSPECT_VOLITION_STATE_TOOL_DESCRIPTION, INSPECT_VOLITION_STATE_TOOL_NAME,
    InspectVolitionStateTool, SELECT_VOLITION_GOALS_TOOL_DESCRIPTION,
    SELECT_VOLITION_GOALS_TOOL_NAME, SelectVolitionGoalsTool,
};
use crate::state::{AppState, SessionRuntime};

pub const SEARCH_MEMORY_TOOL_NAME: &str = "search_memory";
pub const GET_ASSOCIATIONS_TOOL_NAME: &str = "get_associations";
pub const INSPECT_SESSION_STATE_TOOL_NAME: &str = "inspect_session_state";

const SEARCH_MEMORY_LIMIT: usize = 4;
const SEARCH_MEMORY_TOKEN_LIMIT: usize = 600;
const ASSOCIATIONS_LIMIT: usize = 8;

#[derive(Clone, Debug, Serialize)]
pub struct ToolSessionSnapshot {
    pub runtime_phase: RuntimePhase,
    pub completed_exchange_count: usize,
    pub active_exchange_present: bool,
    pub active_exchange_index: Option<usize>,
    pub active_exchange_status: Option<ExchangeStatus>,
    pub trust: DiagnosticTrust,
    pub degraded: bool,
}

impl ToolSessionSnapshot {
    pub fn from_runtime(runtime: &SessionRuntime) -> Self {
        Self {
            runtime_phase: runtime.session_state.live.runtime_phase,
            completed_exchange_count: runtime.session_state.turns.len(),
            active_exchange_present: runtime.session_state.live.active_exchange.is_some(),
            active_exchange_index: runtime
                .session_state
                .live
                .active_exchange
                .as_ref()
                .map(|exchange| exchange.index),
            active_exchange_status: runtime
                .session_state
                .live
                .active_exchange
                .as_ref()
                .map(|exchange| exchange.status),
            trust: runtime.trust,
            degraded: runtime.degraded,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct VolitionStateSnapshot {
    pub state: VolitionState,
    pub fixture: VolitionFixture,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct RealtimeToolContext {
    pub state: AppState,
    pub qsf_session_id: String,
    pub snapshot: ToolSessionSnapshot,
    pub volition: Option<VolitionStateSnapshot>,
    pub exchange_index: usize,
    pub call_id: String,
}

impl ToolContext for RealtimeToolContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchMemoryHit {
    pub memory_id: String,
    pub title: String,
    pub summary: String,
    pub kind: String,
    pub score: f64,
    pub estimated_tokens: usize,
    pub source_reference: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssociationHit {
    pub direction: String,
    pub neighbor_memory_id: String,
    pub neighbor_title: Option<String>,
    pub neighbor_summary: Option<String>,
    pub weight: f64,
    pub reason: String,
    pub dangling: bool,
}

pub fn default_tool_definitions() -> Vec<RealtimeToolDefinition> {
    vec![
        RealtimeToolDefinition::function(
            SEARCH_MEMORY_TOOL_NAME,
            "Search the session memory store for relevant memories.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 4 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        RealtimeToolDefinition::function(
            GET_ASSOCIATIONS_TOOL_NAME,
            "Inspect the weighted association neighborhood for a memory id.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 8 }
                },
                "required": ["memory_id"],
                "additionalProperties": false
            }),
        ),
        RealtimeToolDefinition::function(
            INSPECT_SESSION_STATE_TOOL_NAME,
            "Summarize the live session state without exposing internals.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        RealtimeToolDefinition::function(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            INSPECT_VOLITION_STATE_TOOL_DESCRIPTION,
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        RealtimeToolDefinition::function(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            SELECT_VOLITION_GOALS_TOOL_DESCRIPTION,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub fn build_tool_registry(tool_definitions: &[RealtimeToolDefinition]) -> ToolRegistry {
    let mut registry = ToolRegistry::default();
    let allow_list = tool_definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    for tool in built_in_tools() {
        let metadata = tool.metadata();
        if allow_list.iter().any(|name| *name == metadata.name) {
            registry.register_boxed(tool);
        }
    }
    registry
}

pub fn tool_permission_decision(
    tool_name: &str,
    allow_list: &[String],
    metadata: Option<&ToolMetadata>,
) -> ToolPermissionDecision {
    if !allow_list.iter().any(|allowed| allowed == tool_name) {
        return ToolPermissionDecision::Denied {
            reason: format!("tool `{tool_name}` is not allow-listed for this session"),
        };
    }

    let Some(metadata) = metadata else {
        return ToolPermissionDecision::Denied {
            reason: format!("tool `{tool_name}` is not registered"),
        };
    };

    if metadata.category != ToolCategory::ReadOnly {
        return ToolPermissionDecision::Denied {
            reason: format!(
                "tool `{tool_name}` is not read-only (category={:?})",
                metadata.category
            ),
        };
    }

    if metadata.side_effect_level != ToolSideEffectLevel::ReadOnly {
        return ToolPermissionDecision::Denied {
            reason: format!(
                "tool `{tool_name}` exceeds read-only side-effect bounds ({:?})",
                metadata.side_effect_level
            ),
        };
    }

    ToolPermissionDecision::Allowed
}

pub fn tool_allow_list(tool_definitions: &[RealtimeToolDefinition]) -> Vec<String> {
    tool_definitions
        .iter()
        .map(|definition| definition.name.clone())
        .collect()
}

fn built_in_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(SearchMemoryTool),
        Box::new(GetAssociationsTool),
        Box::new(InspectSessionStateTool),
        Box::new(InspectVolitionStateTool),
        Box::new(SelectVolitionGoalsTool),
    ]
}

struct SearchMemoryTool;

impl Tool for SearchMemoryTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: SEARCH_MEMORY_TOOL_NAME.to_string(),
            description: "Search the session memory store for relevant memories.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            SEARCH_MEMORY_TOOL_NAME,
            "Search the session memory store for relevant memories.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 4 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;
        let query = request
            .structured
            .as_ref()
            .and_then(|value| value.get("query"))
            .and_then(|value| value.as_str())
            .context("search_memory requires `query`")?;
        let limit = request
            .structured
            .as_ref()
            .and_then(|value| value.get("limit"))
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .filter(|value| *value >= 1)
            .unwrap_or(SEARCH_MEMORY_LIMIT)
            .min(SEARCH_MEMORY_LIMIT);

        let retrieval = crate::realtime::memory_store::retrieve_session_memories(
            &ctx.state,
            &ctx.qsf_session_id,
            query,
            RetrievalStrategy::AssociationWeighted,
            limit,
        )?;
        let mut used_tokens = 0usize;
        let mut selected = Vec::new();
        for memory in retrieval.selected {
            if selected.len() >= limit {
                break;
            }
            let next_tokens = used_tokens + memory.memory.estimated_tokens;
            if next_tokens > SEARCH_MEMORY_TOKEN_LIMIT && !selected.is_empty() {
                break;
            }
            used_tokens = next_tokens.min(SEARCH_MEMORY_TOKEN_LIMIT);
            selected.push(SearchMemoryHit {
                memory_id: memory.memory.id.clone(),
                title: memory.memory.title.clone(),
                summary: memory.memory.summary.clone(),
                kind: format!("{:?}", memory.memory.kind),
                score: memory.score.total,
                estimated_tokens: memory.memory.estimated_tokens,
                source_reference: memory.memory.source_reference.clone(),
            });
        }

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::to_string(&selected)?,
            numeric_value: None,
            observation_summary: format!(
                "search_memory returned {} selected memories for `{query}`.",
                selected.len()
            ),
        })
    }
}

struct GetAssociationsTool;

impl Tool for GetAssociationsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: GET_ASSOCIATIONS_TOOL_NAME.to_string(),
            description: "Inspect the weighted association neighborhood for a memory id."
                .to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            GET_ASSOCIATIONS_TOOL_NAME,
            "Inspect the weighted association neighborhood for a memory id.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 8 }
                },
                "required": ["memory_id"],
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;
        let args = request
            .structured
            .as_ref()
            .context("get_associations requires structured arguments")?;
        let memory_id = args
            .get("memory_id")
            .and_then(|value| value.as_str())
            .context("get_associations requires `memory_id`")?;
        let limit = args
            .get("limit")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .filter(|value| *value >= 1)
            .unwrap_or(ASSOCIATIONS_LIMIT)
            .min(ASSOCIATIONS_LIMIT);

        let store = load_session_memory_store(&ctx.state, &ctx.qsf_session_id)?;
        let record_lookup: BTreeMap<&str, &MemoryRecord> = store
            .contents()
            .records
            .iter()
            .map(|record| (record.id.as_str(), record))
            .collect();
        if !record_lookup.contains_key(memory_id) {
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: serde_json::json!({
                    "status": "not_found",
                    "memory_id": memory_id,
                })
                .to_string(),
                numeric_value: None,
                observation_summary: format!("get_associations could not find `{memory_id}`."),
            });
        }

        let mut hits =
            collect_association_hits(memory_id, &store.contents().associations, &record_lookup);
        hits.sort_by(|left, right| {
            right
                .weight
                .total_cmp(&left.weight)
                .then_with(|| left.neighbor_memory_id.cmp(&right.neighbor_memory_id))
                .then_with(|| left.direction.cmp(&right.direction))
        });
        hits.truncate(limit);

        let status = if hits.is_empty() {
            "empty_neighborhood"
        } else {
            "ok"
        };
        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::json!({
                "status": status,
                "memory_id": memory_id,
                "associations": hits,
            })
            .to_string(),
            numeric_value: None,
            observation_summary: format!(
                "get_associations returned {} associations for `{memory_id}`.",
                hits.len()
            ),
        })
    }
}

struct InspectSessionStateTool;

impl Tool for InspectSessionStateTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: INSPECT_SESSION_STATE_TOOL_NAME.to_string(),
            description: "Summarize the live session state without exposing internals.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            INSPECT_SESSION_STATE_TOOL_NAME,
            "Summarize the live session state without exposing internals.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;
        let summary = serde_json::json!({
            "runtime_phase": ctx.snapshot.runtime_phase,
            "completed_exchange_count": ctx.snapshot.completed_exchange_count,
            "active_exchange_present": ctx.snapshot.active_exchange_present,
            "active_exchange_index": ctx.snapshot.active_exchange_index,
            "active_exchange_status": ctx.snapshot.active_exchange_status,
            "trust": ctx.snapshot.trust,
            "degraded": ctx.snapshot.degraded,
        });

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: summary.to_string(),
            numeric_value: None,
            observation_summary: format!(
                "inspect_session_state reported phase={:?} completed_exchange_count={} active_present={} active_status={:?} degraded={}",
                ctx.snapshot.runtime_phase,
                ctx.snapshot.completed_exchange_count,
                ctx.snapshot.active_exchange_present,
                ctx.snapshot.active_exchange_status,
                ctx.snapshot.degraded
            ),
        })
    }
}

fn collect_association_hits(
    memory_id: &str,
    associations: &[Association],
    record_lookup: &BTreeMap<&str, &MemoryRecord>,
) -> Vec<AssociationHit> {
    let mut hits = Vec::new();
    for association in associations {
        if association.from_memory_id == memory_id {
            hits.push(association_hit(
                "outbound",
                association.to_memory_id.as_str(),
                association,
                record_lookup,
            ));
        }
        if association.to_memory_id == memory_id {
            hits.push(association_hit(
                "inbound",
                association.from_memory_id.as_str(),
                association,
                record_lookup,
            ));
        }
    }
    hits
}

fn association_hit(
    direction: &str,
    neighbor_memory_id: &str,
    association: &Association,
    record_lookup: &BTreeMap<&str, &MemoryRecord>,
) -> AssociationHit {
    let neighbor = record_lookup.get(neighbor_memory_id);
    AssociationHit {
        direction: direction.to_string(),
        neighbor_memory_id: neighbor_memory_id.to_string(),
        neighbor_title: neighbor.map(|record| record.title.clone()),
        neighbor_summary: neighbor.map(|record| record.summary.clone()),
        weight: association.weight,
        reason: association.reason.clone(),
        dangling: neighbor.is_none(),
    }
}

fn realtime_context(ctx: &dyn ToolContext) -> anyhow::Result<&RealtimeToolContext> {
    ctx.as_any()
        .downcast_ref::<RealtimeToolContext>()
        .context("realtime tool context missing")
}

#[allow(clippy::too_many_arguments)]
pub fn tool_execution_record(
    exchange_index: usize,
    call_id: impl Into<String>,
    tool_name: impl Into<String>,
    decision: ToolPermissionDecision,
    status: ToolExecutionStatus,
    result_summary: impl Into<String>,
    output_text: impl Into<String>,
    error: Option<String>,
    requested_at: std::time::SystemTime,
    completed_at: Option<std::time::SystemTime>,
    response_model_use: Option<qsf_session::ExchangeModelUse>,
    returning_event_id: Option<String>,
) -> ToolExecutionRecord {
    ToolExecutionRecord {
        exchange_index,
        call_id: call_id.into(),
        tool_name: tool_name.into(),
        permission_decision: decision,
        status,
        result_summary: result_summary.into(),
        output_text: output_text.into(),
        error,
        requested_at,
        completed_at,
        response_model_use,
        returning_event_id,
    }
}

pub fn tool_request_record(
    exchange_index: usize,
    call_id: impl Into<String>,
    tool_name: impl Into<String>,
    arguments_summary: impl Into<String>,
    requested_at: std::time::SystemTime,
    source: impl Into<String>,
) -> ToolRequestRecord {
    ToolRequestRecord {
        exchange_index,
        call_id: call_id.into(),
        tool_name: tool_name.into(),
        arguments_summary: arguments_summary.into(),
        requested_at,
        source: source.into(),
        routed_to: Some("qsf_tool_permission_boundary".to_string()),
        auto_executed: false,
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use qsf_session::{
        ContentHash, ContextAssembly, ContextBudget, Exchange, ExchangeInput, ExchangeModelUse,
        ExchangeOutput, ExchangeStatus, Turn,
    };
    use qsf_tools::{ToolPermission, ToolRequest};
    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;
    use crate::state::BrowserSessionConfig;

    fn metadata(
        name: &str,
        category: ToolCategory,
        side_effect_level: ToolSideEffectLevel,
    ) -> ToolMetadata {
        ToolMetadata {
            name: name.to_string(),
            description: "test tool".to_string(),
            category,
            side_effect_level,
        }
    }

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state")
    }

    fn runtime(tempdir: &TempDir) -> SessionRuntime {
        let diagnostics =
            crate::diagnostics::DiagnosticWriter::create(tempdir.path().join("diagnostics.jsonl"))
                .expect("diagnostics");
        SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        )
    }

    fn completed_exchange(index: usize) -> Exchange {
        Exchange {
            index,
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            input: ExchangeInput::Text {
                text: "hello".to_string(),
            },
            output: Some(ExchangeOutput {
                response_id: Some("response-1".to_string()),
                text: "hi".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("mock-provider".to_string()),
                target: Some("text".to_string()),
                audio_marker: None,
            }),
            context_assembly: Some(ContextAssembly {
                budget: ContextBudget::new(4, 600),
                selected: vec![],
                omitted: vec![],
                used_estimated_tokens: 0,
            }),
            retrieved_memory_block: "memory".to_string(),
            recalled_items: vec![],
            model: Some(ExchangeModelUse {
                provider_name: Some("mock-provider".to_string()),
                model_id: "mock".to_string(),
                latency_ms: 12,
                input_tokens: 4,
                cached_input_tokens: 1,
                output_tokens: 2,
                full_request_hash: ContentHash([index as u8; 32]),
                message_count: 3,
            }),
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            tool_executions: vec![],
            status: ExchangeStatus::Completed,
        }
    }

    fn runtime_with_promoted_exchange(tempdir: &TempDir) -> SessionRuntime {
        let mut runtime = runtime(tempdir);
        let exchange = completed_exchange(1);
        let turn = Turn::try_from(&exchange).expect("turn");
        runtime
            .session_state
            .live
            .completed_exchanges
            .push(exchange);
        runtime.session_state.turns.push(turn);
        runtime
    }

    fn runtime_with_promoted_and_active_exchange(tempdir: &TempDir) -> SessionRuntime {
        let mut runtime = runtime_with_promoted_exchange(tempdir);
        runtime.session_state.live.active_exchange =
            Some(Exchange::new_voice_pending(1, SystemTime::UNIX_EPOCH));
        runtime
    }

    fn tool_context(tempdir: &TempDir, runtime: &SessionRuntime) -> RealtimeToolContext {
        RealtimeToolContext {
            state: state(tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(runtime),
            volition: None,
            exchange_index: 0,
            call_id: String::new(),
        }
    }

    #[test]
    fn permission_decision_allows_only_registered_read_only_allow_listed_tools() {
        let allow_list = vec![SEARCH_MEMORY_TOOL_NAME.to_string()];
        let read_only = metadata(
            SEARCH_MEMORY_TOOL_NAME,
            ToolCategory::ReadOnly,
            ToolSideEffectLevel::ReadOnly,
        );
        let write_capable = metadata(
            SEARCH_MEMORY_TOOL_NAME,
            ToolCategory::WriteCapable,
            ToolSideEffectLevel::ExternalWrite,
        );

        assert_eq!(
            tool_permission_decision(SEARCH_MEMORY_TOOL_NAME, &allow_list, Some(&read_only)),
            ToolPermissionDecision::Allowed
        );
        assert!(matches!(
            tool_permission_decision("unknown", &allow_list, None),
            ToolPermissionDecision::Denied { .. }
        ));
        assert!(matches!(
            tool_permission_decision(SEARCH_MEMORY_TOOL_NAME, &allow_list, Some(&write_capable)),
            ToolPermissionDecision::Denied { .. }
        ));
    }

    #[test]
    fn snapshot_counts_promoted_and_retained_exchange_once() {
        let tempdir = TempDir::new().expect("tempdir");
        let runtime = runtime_with_promoted_exchange(&tempdir);

        let snapshot = ToolSessionSnapshot::from_runtime(&runtime);

        assert_eq!(snapshot.completed_exchange_count, 1);
        assert!(!snapshot.active_exchange_present);
        assert_eq!(snapshot.active_exchange_index, None);
        assert_eq!(snapshot.active_exchange_status, None);
    }

    #[test]
    fn snapshot_reports_active_exchange_presence_without_affecting_completed_count() {
        let tempdir = TempDir::new().expect("tempdir");
        let runtime = runtime_with_promoted_and_active_exchange(&tempdir);

        let snapshot = ToolSessionSnapshot::from_runtime(&runtime);

        assert_eq!(snapshot.completed_exchange_count, 1);
        assert!(snapshot.active_exchange_present);
        assert_eq!(snapshot.active_exchange_index, Some(1));
        assert_eq!(
            snapshot.active_exchange_status,
            Some(ExchangeStatus::Listening)
        );
    }

    #[test]
    fn snapshot_counts_empty_session_as_zero() {
        let tempdir = TempDir::new().expect("tempdir");
        let runtime = runtime(&tempdir);

        let snapshot = ToolSessionSnapshot::from_runtime(&runtime);

        assert_eq!(snapshot.completed_exchange_count, 0);
        assert!(!snapshot.active_exchange_present);
        assert_eq!(snapshot.active_exchange_index, None);
        assert_eq!(snapshot.active_exchange_status, None);
    }

    #[test]
    fn snapshot_ignores_non_promotable_retained_exchange() {
        let tempdir = TempDir::new().expect("tempdir");
        let mut runtime = runtime(&tempdir);
        runtime
            .session_state
            .live
            .completed_exchanges
            .push(completed_exchange(2));

        let snapshot = ToolSessionSnapshot::from_runtime(&runtime);

        assert_eq!(snapshot.completed_exchange_count, 0);
        assert!(!snapshot.active_exchange_present);
        assert_eq!(snapshot.active_exchange_index, None);
        assert_eq!(snapshot.active_exchange_status, None);
    }

    #[test]
    fn inspect_session_state_execute_emits_new_contract_fields() {
        let tempdir = TempDir::new().expect("tempdir");
        let runtime = runtime_with_promoted_and_active_exchange(&tempdir);
        let snapshot = ToolSessionSnapshot::from_runtime(&runtime);
        let ctx = tool_context(&tempdir, &runtime);
        let tool = InspectSessionStateTool;
        let request = ToolRequest::new(
            INSPECT_SESSION_STATE_TOOL_NAME,
            "{}",
            None,
            ToolPermission::read_only(),
            "tester",
        );

        let result = tool.execute(&request, &ctx).expect("execute");
        let json: Value = serde_json::from_str(&result.output_text).expect("json");

        assert_eq!(json.get("exchange_count"), None);
        assert_eq!(json["completed_exchange_count"], serde_json::json!(1));
        assert_eq!(json["active_exchange_present"], serde_json::json!(true));
        assert_eq!(
            json["active_exchange_index"],
            serde_json::json!(snapshot.active_exchange_index)
        );
        assert_eq!(
            json["active_exchange_status"],
            serde_json::json!(snapshot.active_exchange_status)
        );
        assert!(json.get("runtime_phase").is_some());
        assert!(json.get("trust").is_some());
        assert!(json.get("degraded").is_some());
        assert!(
            result
                .observation_summary
                .contains("completed_exchange_count=1")
        );
        assert!(result.observation_summary.contains("active_present=true"));
    }

    #[test]
    fn default_tool_definitions_include_volition_tools() {
        let names: Vec<String> = default_tool_definitions()
            .into_iter()
            .map(|definition| definition.name)
            .collect();

        assert!(
            names
                .iter()
                .any(|name| name == INSPECT_VOLITION_STATE_TOOL_NAME)
        );
        assert!(
            names
                .iter()
                .any(|name| name == SELECT_VOLITION_GOALS_TOOL_NAME)
        );
    }

    #[test]
    fn default_volition_tool_descriptions_mention_subconscious_section() {
        // The realtime session tool list is the model-facing surface: it must tell the model that
        // subconscious goals are sectioned out, and that select reports the winner's visibility.
        let definitions = default_tool_definitions();
        let description_of = |name: &str| {
            definitions
                .iter()
                .find(|definition| definition.name == name)
                .unwrap_or_else(|| panic!("{name} must be in default_tool_definitions"))
                .description
                .clone()
        };

        let inspect = description_of(INSPECT_VOLITION_STATE_TOOL_NAME);
        assert!(
            inspect.contains("subconscious_goals"),
            "inspect description must mention the subconscious_goals section: {inspect}"
        );

        let select = description_of(SELECT_VOLITION_GOALS_TOOL_NAME);
        assert!(
            select.contains("subconscious_goals"),
            "select description must mention the subconscious_goals section: {select}"
        );
        assert!(
            select.contains("winner_visibility"),
            "select description must mention winner_visibility: {select}"
        );
    }

    #[test]
    fn permission_decision_allows_volition_tools_when_allow_listed() {
        let allow_list = vec![
            INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
            SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
        ];
        let inspect_meta = metadata(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            ToolCategory::ReadOnly,
            ToolSideEffectLevel::ReadOnly,
        );
        let select_meta = metadata(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            ToolCategory::ReadOnly,
            ToolSideEffectLevel::ReadOnly,
        );

        assert_eq!(
            tool_permission_decision(
                INSPECT_VOLITION_STATE_TOOL_NAME,
                &allow_list,
                Some(&inspect_meta)
            ),
            ToolPermissionDecision::Allowed
        );
        assert_eq!(
            tool_permission_decision(
                SELECT_VOLITION_GOALS_TOOL_NAME,
                &allow_list,
                Some(&select_meta)
            ),
            ToolPermissionDecision::Allowed
        );
    }

    #[test]
    fn association_hits_are_bidirectional_and_mark_dangling_neighbors() {
        let known = MemoryRecord::new(
            "known",
            qsf_memory::MemoryRecordKind::Observation,
            "Known",
            "Known summary",
            vec![],
            time::OffsetDateTime::UNIX_EPOCH,
            0.5,
            0,
            "test",
            8,
        );
        let records = BTreeMap::from([("known", &known)]);
        let associations = vec![
            Association::new(
                "anchor",
                "known",
                0.7,
                "outbound",
                time::OffsetDateTime::UNIX_EPOCH,
            ),
            Association::new(
                "missing",
                "anchor",
                0.4,
                "inbound",
                time::OffsetDateTime::UNIX_EPOCH,
            ),
        ];

        let hits = collect_association_hits("anchor", &associations, &records);

        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|hit| {
            hit.direction == "outbound"
                && hit.neighbor_memory_id == "known"
                && hit.neighbor_title.as_deref() == Some("Known")
                && !hit.dangling
        }));
        assert!(hits.iter().any(|hit| {
            hit.direction == "inbound" && hit.neighbor_memory_id == "missing" && hit.dangling
        }));
    }
}
