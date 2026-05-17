use std::time::Instant;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;

use super::model_role::{ModelOutputExpectation, ModelRole, ModelRoleId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

impl ModelMessage {
    pub fn new(role: ModelMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(ModelMessageRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(ModelMessageRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(ModelMessageRole::Assistant, content)
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self::new(ModelMessageRole::Tool, content)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ModelToolDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

impl ModelToolCall {
    pub fn new(call_id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            arguments,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelResponseFormat {
    Text,
    JsonObject,
}

impl From<ModelOutputExpectation> for ModelResponseFormat {
    fn from(value: ModelOutputExpectation) -> Self {
        match value {
            ModelOutputExpectation::Text => Self::Text,
            ModelOutputExpectation::JsonObject => Self::JsonObject,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelRequest {
    pub role: ModelRole,
    pub session_id: Option<String>,
    pub model_name: String,
    pub messages: Vec<ModelMessage>,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub response_format: ModelResponseFormat,
    pub tools: Vec<ModelToolDefinition>,
}

impl ModelRequest {
    pub fn new(role: ModelRole, messages: Vec<ModelMessage>) -> Self {
        let model_name = role.default_model.clone();
        let response_format = role.output_expectation.into();

        Self {
            role,
            session_id: None,
            model_name,
            messages,
            temperature: None,
            max_output_tokens: None,
            response_format,
            tools: vec![],
        }
    }

    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = model_name.into();
        self
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    pub fn with_tools(mut self, tools: Vec<ModelToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn last_user_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|message| message.role == ModelMessageRole::User)
            .map(|message| message.content.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_input_tokens: u32,
    pub estimated_cost_usd: Option<f64>,
}

impl ModelUsage {
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cached_input_tokens: 0,
            estimated_cost_usd: None,
        }
    }

    pub fn with_cached_input_tokens(mut self, cached_input_tokens: u32) -> Self {
        debug_assert!(
            cached_input_tokens <= self.input_tokens,
            "provider reported cached_input_tokens={} greater than input_tokens={}",
            cached_input_tokens,
            self.input_tokens
        );
        self.cached_input_tokens = cached_input_tokens.min(self.input_tokens);
        self
    }

    pub fn with_estimated_cost_usd(mut self, estimated_cost_usd: f64) -> Self {
        self.estimated_cost_usd = Some(estimated_cost_usd.max(0.0));
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelResponse {
    pub role_id: ModelRoleId,
    pub provider_name: String,
    pub model_name: String,
    pub output_text: String,
    pub structured_output: Option<Value>,
    pub usage: Option<ModelUsage>,
    pub finish_reason: Option<String>,
    pub tool_calls: Vec<ModelToolCall>,
}

impl ModelResponse {
    pub fn from_text(
        request: &ModelRequest,
        provider_name: impl Into<String>,
        model_name: impl Into<String>,
        output_text: impl Into<String>,
    ) -> Self {
        let output_text = output_text.into();
        let structured_output = match request.response_format {
            ModelResponseFormat::Text => None,
            ModelResponseFormat::JsonObject => serde_json::from_str(&output_text).ok(),
        };

        Self {
            role_id: request.role.role_id,
            provider_name: provider_name.into(),
            model_name: model_name.into(),
            output_text,
            structured_output,
            usage: None,
            finish_reason: None,
            tool_calls: vec![],
        }
    }

    pub fn with_usage(mut self, usage: ModelUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_finish_reason(mut self, finish_reason: impl Into<String>) -> Self {
        self.finish_reason = Some(finish_reason.into());
        self
    }

    pub fn with_tool_calls(mut self, tool_calls: Vec<ModelToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    pub fn output_summary(&self) -> String {
        summarize_text(&self.output_text, 120)
    }
}

pub trait ModelClient: Send + Sync {
    fn client_name(&self) -> &str;

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse>;
}

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

fn summarize_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let head: String = text.chars().take(max_chars).collect();
    format!("{head}...")
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
    use std::fs;

    use anyhow::anyhow;

    use super::{
        ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId, ModelUsage,
        invoke_model_role,
    };
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
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();

        assert!(error.to_string().contains("always-fail"));
        assert!(events.contains("ModelRoleFailed"));
        assert!(traces.contains("boom"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn cached_input_tokens_accept_exact_input_tokens() {
        let usage = ModelUsage::new(10, 4).with_cached_input_tokens(10);

        assert_eq!(usage.cached_input_tokens, 10);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "provider reported cached_input_tokens")]
    fn cached_input_tokens_debug_assert_when_provider_overreports() {
        let _ = ModelUsage::new(10, 4).with_cached_input_tokens(99);
    }
}
