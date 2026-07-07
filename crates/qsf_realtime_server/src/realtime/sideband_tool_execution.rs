use std::time::SystemTime;

use qsf_realtime_protocol::build_openai_realtime_function_call_output;
use qsf_session::{ExchangeModelUse, ToolExecutionStatus, ToolPermissionDecision};
use qsf_tools::ToolRequest;

use crate::realtime::tools::{self, RealtimeToolContext};

pub(crate) struct FunctionCallAttempt {
    pub(crate) name: String,
    pub(crate) call_id: String,
    pub(crate) arguments: Option<serde_json::Value>,
    pub(crate) arguments_summary: String,
    pub(crate) parse_error: Option<String>,
}

pub(crate) struct PendingToolExecution {
    pub(crate) name: String,
    pub(crate) call_id: String,
    pub(crate) arguments: serde_json::Value,
    pub(crate) arguments_summary: String,
    pub(crate) requested_at: SystemTime,
}

pub(crate) struct ToolResolutionOutput {
    pub(crate) record: qsf_session::ToolExecutionRecord,
    pub(crate) output_message: serde_json::Value,
}

pub(crate) fn summarize_function_call_arguments(arguments: &serde_json::Value) -> String {
    let text = serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
    text.chars().take(240).collect()
}

pub(crate) fn summarize_raw_function_call_arguments(arguments: &str) -> String {
    arguments.chars().take(240).collect()
}

pub(crate) fn extract_response_function_call_attempts(
    event: &serde_json::Value,
) -> Vec<FunctionCallAttempt> {
    let Some(output) = event
        .get("response")
        .and_then(|response| response.get("output"))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };

    let mut calls = Vec::new();
    for item in output {
        let Some(item_type) = item.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if item_type != "function_call" && item_type != "tool_search_call" {
            continue;
        }

        let call_id = item
            .get("call_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments_text = item
            .get("arguments")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match serde_json::from_str::<serde_json::Value>(arguments_text) {
            Ok(arguments) => calls.push(FunctionCallAttempt {
                name,
                call_id,
                arguments_summary: summarize_function_call_arguments(&arguments),
                arguments: Some(arguments),
                parse_error: None,
            }),
            Err(error) => calls.push(FunctionCallAttempt {
                name,
                call_id,
                arguments: None,
                arguments_summary: summarize_raw_function_call_arguments(arguments_text),
                parse_error: Some(error.to_string()),
            }),
        }
    }

    calls
}

pub(crate) fn execute_realtime_tool_call(
    registry: &qsf_tools::ToolRegistry,
    tool_context: &RealtimeToolContext,
    exchange_index: usize,
    pending: PendingToolExecution,
    response_model_use: &ExchangeModelUse,
    event_id: Option<String>,
    qsf_session_id: &str,
) -> ToolResolutionOutput {
    let tool_request = ToolRequest {
        tool_name: pending.name.clone(),
        input: pending.arguments_summary.clone(),
        structured: Some(pending.arguments),
        permission: qsf_tools::ToolPermission::read_only(),
        requested_by: qsf_session_id.to_string(),
    };

    let (status, result_summary, error, output_text, numeric_value, output_status) =
        match registry.validate_and_execute(&tool_request, tool_context) {
            Ok((_metadata, result)) => (
                ToolExecutionStatus::Completed,
                result.observation_summary.clone(),
                None,
                result.output_text.clone(),
                result.numeric_value,
                "completed",
            ),
            Err(exec_error) => (
                ToolExecutionStatus::Failed,
                "tool execution failed before producing a result".to_string(),
                Some(exec_error.to_string()),
                String::new(),
                None,
                "failed",
            ),
        };

    let record = tools::tool_execution_record(
        exchange_index,
        pending.call_id.clone(),
        pending.name.clone(),
        ToolPermissionDecision::Allowed,
        status,
        result_summary.clone(),
        output_text.clone(),
        error.clone(),
        pending.requested_at,
        Some(SystemTime::now()),
        Some(response_model_use.clone()),
        event_id,
    );
    let output_message = build_openai_realtime_function_call_output(
        &pending.call_id,
        &serde_json::json!({
            "status": output_status,
            "tool_name": pending.name,
            "result_summary": result_summary,
            "output_text": output_text,
            "numeric_value": numeric_value,
            "error": error,
        })
        .to_string(),
    );

    ToolResolutionOutput {
        record,
        output_message,
    }
}

pub(crate) fn aborted_tool_resolution(
    mut resolution: ToolResolutionOutput,
    response_model_use: &ExchangeModelUse,
    event_id: Option<String>,
) -> ToolResolutionOutput {
    resolution.record.status = ToolExecutionStatus::Aborted;
    resolution.record.result_summary =
        "tool execution aborted because the sideband became degraded before the result was returned"
            .to_string();
    resolution.record.error = Some("sideband degraded during tool execution".to_string());
    resolution.record.completed_at = Some(SystemTime::now());
    resolution.record.response_model_use = Some(response_model_use.clone());
    resolution.record.returning_event_id = event_id;
    resolution
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use qsf_session::ExchangeModelUse;
    use qsf_volition::{VolitionState, realtime_seed_fixture};
    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;
    use crate::diagnostics::DiagnosticWriter;
    use crate::realtime::tools::{
        RealtimeToolContext, ToolSessionSnapshot, VolitionStateSnapshot, build_tool_registry,
        default_tool_definitions,
    };
    use crate::state::{AppState, BrowserSessionConfig, SessionIdMode, SessionRuntime};

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            SessionIdMode::Default,
        )
        .expect("state")
    }

    fn runtime(tempdir: &TempDir) -> SessionRuntime {
        let diagnostics = DiagnosticWriter::create(tempdir.path().join("diagnostics.jsonl"))
            .expect("diagnostics");
        SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        )
    }

    #[test]
    fn execute_inspect_volition_persists_output_text() {
        let tempdir = TempDir::new().expect("tempdir");
        let state = state(&tempdir);
        let runtime = runtime(&tempdir);
        let registry = build_tool_registry(&default_tool_definitions());
        let fixture = realtime_seed_fixture();
        let volition = VolitionState::from_fixture(&fixture);
        let ctx = RealtimeToolContext {
            state,
            qsf_session_id: "session-1".to_string(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: volition,
                fixture,
            }),
            exchange_index: 0,
            call_id: "call-1".to_string(),
        };
        let pending = PendingToolExecution {
            name: "inspect_volition_state".to_string(),
            call_id: "call-1".to_string(),
            arguments: serde_json::json!({}),
            arguments_summary: "{}".to_string(),
            requested_at: SystemTime::UNIX_EPOCH,
        };
        let response_model_use = ExchangeModelUse {
            provider_name: Some("openai_realtime".to_string()),
            model_id: "test-model".to_string(),
            latency_ms: 1,
            input_tokens: 1,
            cached_input_tokens: 0,
            output_tokens: 1,
            full_request_hash: qsf_session::ContentHash([0; 32]),
            message_count: 1,
        };

        let resolution = execute_realtime_tool_call(
            &registry,
            &ctx,
            0,
            pending,
            &response_model_use,
            Some("event-1".to_string()),
            "session-1",
        );
        let output: Value = serde_json::from_str(&resolution.record.output_text).unwrap();
        assert!(output.get("note").is_some());
        assert!(
            output["note"]
                .as_str()
                .expect("note")
                .starts_with("This is your own internal state")
        );
    }
}
