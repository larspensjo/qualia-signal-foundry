use anyhow::{Context, bail};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ModelMessageRole, ModelRequest, ModelResponse, ModelResponseFormat, ModelToolCall, ModelUsage,
};
use crate::models::ModelMessage;

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";

pub struct OpenAiToolClient {
    http: Client,
    api_key: String,
}

impl OpenAiToolClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var(OPENAI_API_KEY_ENV_VAR).map_err(|_| {
            anyhow::anyhow!(
                "{OPENAI_API_KEY_ENV_VAR} is required for the OpenAI tool-capable model client"
            )
        })?;

        Ok(Self {
            http: Client::new(),
            api_key,
        })
    }

    pub fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let request_body = build_request_body(request)?;
        let response = self
            .http
            .post(OPENAI_CHAT_COMPLETIONS_URL)
            .bearer_auth(&self.api_key)
            .json(&request_body)
            .send()
            .context("OpenAI Chat Completions request failed")?;

        let status = response.status();
        let response_text = response
            .text()
            .context("failed to read OpenAI Chat Completions response body")?;

        if !status.is_success() {
            return Err(anyhow::anyhow!(sanitize_provider_error(
                status.as_u16(),
                &response_text
            )));
        }

        let completion: ChatCompletionResponse = serde_json::from_str(&response_text)
            .context("failed to parse OpenAI Chat Completions response")?;
        parse_completion_response(request, completion)
    }
}

pub(crate) fn build_request_body(request: &ModelRequest) -> anyhow::Result<Value> {
    let messages = request
        .messages
        .iter()
        .map(chat_message_to_value)
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut body = json!({
        "model": &request.model_name,
        "messages": messages,
    });

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_output_tokens) = request.max_output_tokens {
        body["max_completion_tokens"] = json!(max_output_tokens);
    }
    if matches!(request.response_format, ModelResponseFormat::JsonObject) {
        body["response_format"] = json!({
            "type": "json_object"
        });
    }
    if !request.tools.is_empty() {
        body["tools"] = json!(
            request
                .tools
                .iter()
                .map(model_tool_definition_to_value)
                .collect::<Vec<_>>()
        );
    }

    Ok(body)
}

fn chat_message_to_value(message: &ModelMessage) -> anyhow::Result<Value> {
    match message.role {
        ModelMessageRole::System => Ok(json!({
            "role": "system",
            "content": &message.content,
        })),
        ModelMessageRole::User => Ok(json!({
            "role": "user",
            "content": &message.content,
        })),
        ModelMessageRole::Assistant => {
            if message.tool_calls.is_empty() {
                return Ok(json!({
                    "role": "assistant",
                    "content": &message.content,
                }));
            }

            let content = if message.content.is_empty() {
                Value::Null
            } else {
                json!(&message.content)
            };

            Ok(json!({
                "role": "assistant",
                "content": content,
                "tool_calls": message
                    .tool_calls
                    .iter()
                    .map(model_tool_call_to_value)
                    .collect::<Vec<_>>(),
            }))
        }
        ModelMessageRole::Tool => {
            let Some(tool_call_id) = message.tool_call_id.as_ref() else {
                bail!("OpenAI tool messages require a tool_call_id");
            };

            Ok(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": &message.content,
            }))
        }
    }
}

fn model_tool_definition_to_value(tool: &crate::models::ModelToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": &tool.name,
            "description": &tool.description,
            "parameters": &tool.parameters,
        }
    })
}

fn model_tool_call_to_value(tool_call: &ModelToolCall) -> Value {
    json!({
        "id": &tool_call.call_id,
        "type": "function",
        "function": {
            "name": &tool_call.name,
            "arguments": tool_call.arguments.to_string(),
        }
    })
}

pub(crate) fn parse_completion_response(
    request: &ModelRequest,
    response: ChatCompletionResponse,
) -> anyhow::Result<ModelResponse> {
    let ChatCompletionResponse {
        model,
        choices,
        usage,
    } = response;

    let choice = choices
        .into_iter()
        .next()
        .context("OpenAI Chat Completions response did not contain any choices")?;

    let output_text = completion_message_text(&choice.message);
    let tool_calls = parse_tool_calls(choice.message.tool_calls)?;
    let usage = usage.map(parse_usage);
    let finish_reason = choice.finish_reason.map(|reason| reason.to_string());

    let mut model_response = ModelResponse::from_text(request, "openai", model, output_text);

    if let Some(usage) = usage {
        model_response = model_response.with_usage(usage);
    }
    if let Some(finish_reason) = finish_reason {
        model_response = model_response.with_finish_reason(finish_reason);
    }
    if !tool_calls.is_empty() {
        model_response = model_response.with_tool_calls(tool_calls);
    }

    Ok(model_response)
}

fn parse_usage(usage: ChatCompletionUsage) -> ModelUsage {
    let cached_input_tokens = usage
        .prompt_tokens_details
        .and_then(|details| details.cached_tokens)
        .unwrap_or(0);

    ModelUsage::new(usage.prompt_tokens, usage.completion_tokens)
        .with_cached_input_tokens(cached_input_tokens)
}

fn completion_message_text(message: &ChatCompletionMessage) -> String {
    match &message.content {
        None => String::new(),
        Some(MessageContent::Text(text)) => text.clone(),
        Some(MessageContent::Parts(parts)) => parts
            .iter()
            .filter(|part| part.kind == "text")
            .filter_map(|part| part.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn parse_tool_calls(
    tool_calls: Option<Vec<ChatCompletionToolCall>>,
) -> anyhow::Result<Vec<ModelToolCall>> {
    let Some(tool_calls) = tool_calls else {
        return Ok(vec![]);
    };

    let mut parsed_calls = Vec::with_capacity(tool_calls.len());
    for tool_call in tool_calls {
        // Chat Completions tool calls used here are function calls. Built-in
        // provider tool types need explicit parsing before they are accepted.
        if tool_call.kind != "function" {
            bail!(
                "OpenAI tool call type `{}` is not supported",
                tool_call.kind
            );
        }
        let call_id = tool_call
            .id
            .context("OpenAI tool call is missing the provider call id")?;
        let function = tool_call
            .function
            .context("OpenAI tool call is missing function details")?;
        let arguments = serde_json::from_str::<Value>(&function.arguments).with_context(|| {
            format!(
                "OpenAI tool call `{}` returned invalid JSON arguments",
                function.name
            )
        })?;

        parsed_calls.push(ModelToolCall::new(call_id, function.name, arguments));
    }

    Ok(parsed_calls)
}

fn sanitize_provider_error(status_code: u16, response_text: &str) -> String {
    let trimmed = response_text.trim();
    let mut collapsed = trimmed
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    collapsed = collapsed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 240 {
        collapsed = collapsed.chars().take(240).collect::<String>();
    }

    format!("OpenAI Chat Completions request failed with status {status_code}: {collapsed}")
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionResponse {
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Option<ChatCompletionUsage>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionChoice {
    pub message: ChatCompletionMessage,
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionMessage {
    pub content: Option<MessageContent>,
    pub tool_calls: Option<Vec<ChatCompletionToolCall>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum MessageContent {
    Text(String),
    Parts(Vec<MessageContentPart>),
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MessageContentPart {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionToolCall {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: Option<ChatCompletionFunction>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub prompt_tokens_details: Option<ChatCompletionPromptTokensDetails>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChatCompletionPromptTokensDetails {
    pub cached_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        ChatCompletionChoice, ChatCompletionFunction, ChatCompletionMessage,
        ChatCompletionPromptTokensDetails, ChatCompletionResponse, ChatCompletionToolCall,
        ChatCompletionUsage, MessageContent, build_request_body, parse_completion_response,
    };
    use crate::models::{
        ModelMessage, ModelRequest, ModelRole, ModelRoleId, ModelToolCall, ModelToolDefinition,
    };

    #[test]
    fn text_only_request_omits_tools() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::MockResponder),
            vec![ModelMessage::system("hello"), ModelMessage::user("world")],
        )
        .with_temperature(0.25)
        .with_max_output_tokens(42);

        let body = build_request_body(&request).unwrap();

        assert_eq!(body["model"], "gpt-5.4-nano");
        assert_eq!(body["temperature"], json!(0.25));
        assert_eq!(body["max_completion_tokens"], json!(42));
        assert!(body.get("tools").is_none());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn tool_request_serializes_function_tools_and_tool_ids() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::MockResponder),
            vec![
                ModelMessage::system("hello"),
                ModelMessage::user("please recall"),
                ModelMessage::assistant_tool_calls(
                    "",
                    vec![ModelToolCall::new(
                        "call-1",
                        "recall_turn",
                        json!({ "turn_id": 0 }),
                    )],
                ),
                ModelMessage::tool_result("call-1", "tool output"),
            ],
        )
        .with_tools(vec![ModelToolDefinition::new(
            "recall_turn",
            "Recall verbatim text for a summarized conversation turn by turn_id.",
            json!({
                "type": "object",
                "properties": {
                    "turn_id": {
                        "type": "integer"
                    }
                },
                "required": ["turn_id"],
                "additionalProperties": false
            }),
        )])
        .with_temperature(0.0)
        .with_max_output_tokens(80)
        .with_model_name("gpt-5.4-mini")
        .with_session_id("session-1");

        let body = build_request_body(&request).unwrap();

        assert_eq!(body["model"], "gpt-5.4-mini");
        assert_eq!(body["temperature"], json!(0.0));
        assert_eq!(body["max_completion_tokens"], json!(80));
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert_eq!(body["messages"][2]["content"], Value::Null);
        assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "call-1");
        assert_eq!(body["messages"][2]["tool_calls"][0]["type"], "function");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "recall_turn"
        );
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"turn_id\":0}"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call-1");
        assert_eq!(body["messages"][3]["content"], "tool output");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "recall_turn");
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn json_mode_request_serializes_response_format() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::SleepSummarizer),
            vec![ModelMessage::user("summarize")],
        );

        let body = build_request_body(&request).unwrap();

        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn response_parser_preserves_tool_calls_and_cached_tokens() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
            vec![ModelMessage::user("please recall turn 0")],
        )
        .with_tools(vec![ModelToolDefinition::new(
            "recall_turn",
            "Recall verbatim text for a summarized conversation turn by turn_id.",
            json!({
                "type": "object",
                "properties": {
                    "turn_id": { "type": "integer" }
                }
            }),
        )]);

        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: None,
                    tool_calls: Some(vec![ChatCompletionToolCall {
                        id: Some("call-1".to_string()),
                        kind: "function".to_string(),
                        function: Some(ChatCompletionFunction {
                            name: "recall_turn".to_string(),
                            arguments: "{\"turn_id\":0}".to_string(),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: Some(ChatCompletionUsage {
                prompt_tokens: 11,
                completion_tokens: 0,
                prompt_tokens_details: Some(ChatCompletionPromptTokensDetails {
                    cached_tokens: Some(7),
                }),
            }),
        };

        let parsed = parse_completion_response(&request, response).unwrap();

        assert_eq!(parsed.provider_name, "openai");
        assert_eq!(parsed.model_name, "gpt-5.4-mini");
        assert_eq!(parsed.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].call_id, "call-1");
        assert_eq!(parsed.tool_calls[0].name, "recall_turn");
        assert_eq!(parsed.tool_calls[0].arguments["turn_id"], 0);
        assert_eq!(parsed.usage.unwrap().cached_input_tokens, 7);
    }

    #[test]
    fn response_parser_preserves_multiple_tool_calls() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
            vec![ModelMessage::user("please recall turn 0 and 1")],
        );
        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: None,
                    tool_calls: Some(vec![
                        ChatCompletionToolCall {
                            id: Some("call-1".to_string()),
                            kind: "function".to_string(),
                            function: Some(ChatCompletionFunction {
                                name: "recall_turn".to_string(),
                                arguments: "{\"turn_id\":0}".to_string(),
                            }),
                        },
                        ChatCompletionToolCall {
                            id: Some("call-2".to_string()),
                            kind: "function".to_string(),
                            function: Some(ChatCompletionFunction {
                                name: "recall_turn".to_string(),
                                arguments: "{\"turn_id\":1}".to_string(),
                            }),
                        },
                    ]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };

        let parsed = parse_completion_response(&request, response).unwrap();

        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].call_id, "call-1");
        assert_eq!(parsed.tool_calls[1].call_id, "call-2");
    }

    #[test]
    fn response_parser_rejects_malformed_tool_arguments() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
            vec![ModelMessage::user("please recall turn 0")],
        );
        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: None,
                    tool_calls: Some(vec![ChatCompletionToolCall {
                        id: Some("call-1".to_string()),
                        kind: "function".to_string(),
                        function: Some(ChatCompletionFunction {
                            name: "recall_turn".to_string(),
                            arguments: "{not-json}".to_string(),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };

        let error = parse_completion_response(&request, response).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("returned invalid JSON arguments")
        );
    }

    #[test]
    fn response_parser_rejects_missing_tool_call_id() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::ConversationalResponder),
            vec![ModelMessage::user("please recall turn 0")],
        );
        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: None,
                    tool_calls: Some(vec![ChatCompletionToolCall {
                        id: None,
                        kind: "function".to_string(),
                        function: Some(ChatCompletionFunction {
                            name: "recall_turn".to_string(),
                            arguments: "{\"turn_id\":0}".to_string(),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };

        let error = parse_completion_response(&request, response).unwrap_err();

        assert!(error.to_string().contains("provider call id"));
    }

    #[test]
    fn response_parser_handles_text_content() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::MockResponder),
            vec![ModelMessage::user("say ok")],
        );
        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: Some(MessageContent::Text("ok".to_string())),
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(ChatCompletionUsage {
                prompt_tokens: 4,
                completion_tokens: 1,
                prompt_tokens_details: None,
            }),
        };

        let parsed = parse_completion_response(&request, response).unwrap();

        assert_eq!(parsed.output_text, "ok");
        assert_eq!(parsed.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn response_parser_handles_text_content_parts() {
        let request = ModelRequest::new(
            ModelRole::predefined(ModelRoleId::MockResponder),
            vec![ModelMessage::user("say ok")],
        );
        let response = ChatCompletionResponse {
            model: "gpt-5.4-mini".to_string(),
            choices: vec![ChatCompletionChoice {
                message: ChatCompletionMessage {
                    content: Some(MessageContent::Parts(vec![super::MessageContentPart {
                        kind: "text".to_string(),
                        text: Some("ok".to_string()),
                    }])),
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };

        let parsed = parse_completion_response(&request, response).unwrap();

        assert_eq!(parsed.output_text, "ok");
    }
}
