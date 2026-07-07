use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

use super::model_role::{ModelOutputExpectation, ModelRole, ModelRoleId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ModelToolCall>,
}

impl ModelMessage {
    fn new(role: ModelMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: vec![],
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

    pub fn assistant_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ModelToolCall>,
    ) -> Self {
        Self {
            role: ModelMessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ModelMessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: vec![],
        }
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
    /// Number of leading `messages` that form a stable, cacheable prefix (e.g. system
    /// instructions + a goal set), set via `with_stable_prefix_message_count`. `None` when the
    /// caller has not declared a boundary.
    ///
    /// There is no provider-side cache-breakpoint field to set here: neither
    /// `openai_provider_kit`'s `LlmRequest` nor the raw OpenAI Chat Completions API expose one -
    /// OpenAI's own prompt caching is automatic over a byte-stable prefix of the raw request.
    /// This boundary is an application-level seam so callers can assemble
    /// `{stable prefix} + {variable suffix}` consistently and hash the prefix
    /// (`stable_prefix_hash`) to tell cache-hit-eligible turns from prefix-invalidated ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_prefix_message_count: Option<usize>,
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
            stable_prefix_message_count: None,
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

    pub fn with_stable_prefix_message_count(mut self, count: usize) -> Self {
        self.stable_prefix_message_count = Some(count.min(self.messages.len()));
        self
    }

    /// Hashes the declared stable-prefix messages (see `with_stable_prefix_message_count`) for
    /// cache-eligibility tracking. `None` when no boundary was declared on this request. Callers
    /// compare this against the previous turn's hash to distinguish a cache-hit-eligible turn
    /// (hash unchanged) from one whose prefix was just invalidated (hash changed).
    pub fn stable_prefix_hash(&self) -> Option<String> {
        let count = self.stable_prefix_message_count?;
        let prefix = &self.messages[..count.min(self.messages.len())];
        let serialized =
            serde_json::to_vec(prefix).expect("ModelMessage slice always serializes to JSON");
        let hash = sha2::Sha256::digest(&serialized);
        Some(hash.iter().map(|byte| format!("{byte:02x}")).collect())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output_parse_error: Option<String>,
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
        let (structured_output, structured_output_parse_error) = match request.response_format {
            ModelResponseFormat::Text => (None, None),
            ModelResponseFormat::JsonObject => match serde_json::from_str(&output_text) {
                Ok(value) => (Some(value), None),
                Err(error) => (None, Some(error.to_string())),
            },
        };

        Self {
            role_id: request.role.role_id,
            provider_name: provider_name.into(),
            model_name: model_name.into(),
            output_text,
            structured_output,
            structured_output_parse_error,
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

/// Calls a `ModelClient` and wraps any error with the role/client identifying context that
/// callers (offline traces, live diagnostics) rely on. Carries no side effects of its own -
/// recording a request/response is the caller's responsibility, since offline (`RunContext`)
/// and live (`DiagnosticWriter`) callers use different observability mechanisms.
pub fn invoke_model(
    client: &dyn ModelClient,
    request: &ModelRequest,
) -> anyhow::Result<ModelResponse> {
    client.complete(request).with_context(|| {
        format!(
            "model role `{}` failed via client `{}`",
            request.role.role_id,
            client.client_name()
        )
    })
}

/// Decouples model callers (e.g. `CoherenceJudge`) from any one observability backend. Offline
/// callers implement this over `RunContext` (recording `TraceRecord`/`EventType`); the live
/// realtime loop uses `DirectModelInvoker` and records its own `DiagnosticRecord` around the
/// whole formation/detection call instead of per model invocation.
pub trait ModelInvoker {
    fn invoke(
        &mut self,
        client: &dyn ModelClient,
        request: &ModelRequest,
    ) -> anyhow::Result<ModelResponse>;
}

/// A `ModelInvoker` that only calls the client, with no recording side effect.
#[derive(Default)]
pub struct DirectModelInvoker;

impl ModelInvoker for DirectModelInvoker {
    fn invoke(
        &mut self,
        client: &dyn ModelClient,
        request: &ModelRequest,
    ) -> anyhow::Result<ModelResponse> {
        invoke_model(client, request)
    }
}

/// One model call observed by `UsageCapturingInvoker`: which model answered and what
/// it consumed, per the provider's usage report.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedModelUse {
    pub model_name: String,
    pub usage: ModelUsage,
}

/// A `ModelInvoker` that calls the client like `DirectModelInvoker` and additionally
/// captures the usage of every response the provider returned. Callers whose work can
/// still fail *after* the provider billed the call (structured-output parsing, semantic
/// validation) read `captured` afterwards, so provider spend is never lost with the
/// error.
#[derive(Default)]
pub struct UsageCapturingInvoker {
    pub captured: Vec<CapturedModelUse>,
}

impl ModelInvoker for UsageCapturingInvoker {
    fn invoke(
        &mut self,
        client: &dyn ModelClient,
        request: &ModelRequest,
    ) -> anyhow::Result<ModelResponse> {
        let response = invoke_model(client, request)?;
        if let Some(usage) = &response.usage {
            self.captured.push(CapturedModelUse {
                model_name: response.model_name.clone(),
                usage: usage.clone(),
            });
        }
        Ok(response)
    }
}

pub fn summarize_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let head: String = text.chars().take(max_chars).collect();
    format!("{head}...")
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::{
        DirectModelInvoker, ModelClient, ModelInvoker, ModelMessage, ModelMessageRole,
        ModelRequest, ModelResponse, ModelResponseFormat, ModelRole, ModelRoleId, ModelToolCall,
        ModelUsage,
    };

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
    fn direct_model_invoker_wraps_client_errors_with_role_and_client_context() {
        let role = ModelRole::predefined(ModelRoleId::Critic);
        let request = ModelRequest::new(role, vec![ModelMessage::user("review this plan")]);

        let error = DirectModelInvoker
            .invoke(&AlwaysFailClient, &request)
            .unwrap_err();

        assert!(error.to_string().contains("always-fail"));
        assert!(error.to_string().contains("critic"));
    }

    #[test]
    fn cached_input_tokens_accept_exact_input_tokens() {
        let usage = ModelUsage::new(10, 4).with_cached_input_tokens(10);

        assert_eq!(usage.cached_input_tokens, 10);
    }

    #[test]
    fn tool_result_message_preserves_tool_call_id() {
        let message = ModelMessage::tool_result("call-123", "tool output");

        assert_eq!(message.role, ModelMessageRole::Tool);
        assert_eq!(message.content, "tool output");
        assert_eq!(message.tool_call_id.as_deref(), Some("call-123"));
        assert!(message.tool_calls.is_empty());
    }

    #[test]
    fn assistant_tool_call_message_preserves_tool_calls() {
        let tool_call = ModelToolCall::new(
            "call-123",
            "recall_turn",
            serde_json::json!({ "turn_id": 0 }),
        );
        let message = ModelMessage::assistant_tool_calls("", vec![tool_call.clone()]);

        assert_eq!(message.role, ModelMessageRole::Assistant);
        assert_eq!(message.content, "");
        assert_eq!(message.tool_call_id, None);
        assert_eq!(message.tool_calls, vec![tool_call]);
    }

    #[test]
    fn tool_result_message_serialization_preserves_call_id() {
        let message = ModelMessage::tool_result("call-123", "tool output");
        let value = serde_json::to_value(&message).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "role": "tool",
                "tool_call_id": "call-123",
                "content": "tool output"
            })
        );
    }

    #[test]
    fn json_response_records_parse_error_when_output_is_malformed() {
        let mut role = ModelRole::predefined(ModelRoleId::Critic);
        role.output_expectation = crate::ModelOutputExpectation::JsonObject;
        let mut request = ModelRequest::new(role, vec![ModelMessage::user("json please")]);
        request.response_format = ModelResponseFormat::JsonObject;

        let response = ModelResponse::from_text(&request, "mock", "mock", "{not-json}");

        assert!(response.structured_output.is_none());
        assert!(response.structured_output_parse_error.is_some());
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "provider reported cached_input_tokens")]
    fn cached_input_tokens_debug_assert_when_provider_overreports() {
        let _ = ModelUsage::new(10, 4).with_cached_input_tokens(99);
    }

    #[test]
    fn stable_prefix_hash_is_none_when_no_boundary_declared() {
        let role = ModelRole::predefined(ModelRoleId::CoherenceJudge);
        let request = ModelRequest::new(role, vec![ModelMessage::system("goals")]);

        assert!(request.stable_prefix_hash().is_none());
    }

    #[test]
    fn stable_prefix_hash_is_stable_when_prefix_messages_are_unchanged() {
        let role = ModelRole::predefined(ModelRoleId::CoherenceJudge);
        let request_a = ModelRequest::new(
            role.clone(),
            vec![
                ModelMessage::system("goal set"),
                ModelMessage::user("turn one"),
            ],
        )
        .with_stable_prefix_message_count(1);
        let request_b = ModelRequest::new(
            role,
            vec![
                ModelMessage::system("goal set"),
                ModelMessage::user("turn two"),
            ],
        )
        .with_stable_prefix_message_count(1);

        assert_eq!(
            request_a.stable_prefix_hash(),
            request_b.stable_prefix_hash()
        );
    }

    #[test]
    fn stable_prefix_hash_changes_when_prefix_content_changes() {
        let role = ModelRole::predefined(ModelRoleId::CoherenceJudge);
        let request_a = ModelRequest::new(role.clone(), vec![ModelMessage::system("goal set v1")])
            .with_stable_prefix_message_count(1);
        let request_b = ModelRequest::new(role, vec![ModelMessage::system("goal set v2")])
            .with_stable_prefix_message_count(1);

        assert_ne!(
            request_a.stable_prefix_hash(),
            request_b.stable_prefix_hash()
        );
    }

    #[test]
    fn with_stable_prefix_message_count_clamps_to_message_len() {
        let role = ModelRole::predefined(ModelRoleId::CoherenceJudge);
        let request = ModelRequest::new(role, vec![ModelMessage::system("only message")])
            .with_stable_prefix_message_count(5);

        assert_eq!(request.stable_prefix_message_count, Some(1));
    }
}
