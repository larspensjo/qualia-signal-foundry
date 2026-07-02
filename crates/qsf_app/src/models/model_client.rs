use std::time::Instant;

use anyhow::Context;
use serde_json::json;

use qsf_models::{ModelClient, ModelInvoker, ModelRequest, ModelResponse, summarize_text};

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;

/// Invokes a model role and records the call as offline observability: a `ModelRoleRequested`
/// event before the call, then either a `"model-role"` `TraceRecord` + `ModelRoleCompleted`
/// event, or the same trace with an error + `ModelRoleFailed` event.
pub fn invoke_model_role(
    context: &mut RunContext,
    client: &dyn ModelClient,
    request: &ModelRequest,
) -> anyhow::Result<ModelResponse> {
    context.record_event(
        EventType::ModelRoleRequested,
        json!({
            "session_id": &request.session_id,
            "role_id": request.role.role_id,
            "provider_client": client.client_name(),
            "model_name": request.model_name,
            "message_count": request.messages.len(),
            "response_format": request.response_format,
            "tools": &request.tools,
        }),
        None,
    )?;

    let started_at = Instant::now();

    match client.complete(request) {
        Ok(response) => {
            let elapsed_ns = elapsed_ns(started_at);
            let trace = TraceRecord::new(
                context.experiment_id(),
                "model-role",
                model_input_summary(request),
                response.output_summary(),
            )
            .with_details(json!({
                "client": client.client_name(),
                "request": request,
                "response": &response,
            }))
            .with_latency_ns(elapsed_ns);
            let trace_id = trace.trace_id;

            context.record_trace(trace)?;
            context.record_event(
                EventType::ModelRoleCompleted,
                json!({
                    "session_id": &request.session_id,
                    "role_id": request.role.role_id,
                    "provider_name": &response.provider_name,
                    "model_name": &response.model_name,
                    "has_structured_output": response.structured_output.is_some(),
                    "tool_call_count": response.tool_calls.len(),
                    "tool_calls": &response.tool_calls,
                    "usage": &response.usage,
                    "finish_reason": &response.finish_reason,
                    "latency_ns": elapsed_ns,
                    "latency_ms": elapsed_ns / 1_000_000,
                }),
                Some(trace_id),
            )?;

            Ok(response)
        }
        Err(error) => {
            let elapsed_ns = elapsed_ns(started_at);
            let error_message = error_chain_summary(&error);
            let trace = TraceRecord::new(
                context.experiment_id(),
                "model-role",
                model_input_summary(request),
                "model role invocation failed",
            )
            .with_details(json!({
                "client": client.client_name(),
                "request": request,
            }))
            .with_latency_ns(elapsed_ns)
            .with_error(error_message.clone());
            let trace_id = trace.trace_id;

            context.record_trace(trace)?;
            context.record_event(
                EventType::ModelRoleFailed,
                json!({
                    "session_id": &request.session_id,
                    "role_id": request.role.role_id,
                    "provider_client": client.client_name(),
                    "model_name": request.model_name,
                    "error": error_message,
                    "latency_ns": elapsed_ns,
                    "latency_ms": elapsed_ns / 1_000_000,
                }),
                Some(trace_id),
            )?;

            Err(error).with_context(|| {
                format!(
                    "model role `{}` failed via client `{}`",
                    request.role.role_id,
                    client.client_name()
                )
            })
        }
    }
}

impl ModelInvoker for RunContext {
    fn invoke(
        &mut self,
        client: &dyn ModelClient,
        request: &ModelRequest,
    ) -> anyhow::Result<ModelResponse> {
        invoke_model_role(self, client, request)
    }
}

fn model_input_summary(request: &ModelRequest) -> String {
    let last_user = request.last_user_message().unwrap_or("<no user message>");
    format!(
        "role={} model={} messages={} last_user={}",
        request.role.role_id,
        request.model_name,
        request.messages.len(),
        summarize_text(last_user, 80)
    )
}

fn error_chain_summary(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use qsf_models::{
        ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId,
    };

    use super::invoke_model_role;
    use crate::runtime::run_context::RunContext;

    struct AlwaysFailClient;

    impl ModelClient for AlwaysFailClient {
        fn client_name(&self) -> &str {
            "always-fail"
        }

        fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            Err(anyhow!("boom"))
        }
    }

    #[test]
    fn invoke_model_role_records_failure_trace_and_event() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-model-fail-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "model-failure-test").unwrap();
        let role = ModelRole::predefined(ModelRoleId::Critic);
        let request = ModelRequest::new(role, vec![ModelMessage::user("review this plan")]);

        let error = invoke_model_role(&mut context, &AlwaysFailClient, &request).unwrap_err();
        let events = std::fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = std::fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();

        assert!(error.to_string().contains("always-fail"));
        assert!(events.contains("ModelRoleFailed"));
        assert!(traces.contains("boom"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
