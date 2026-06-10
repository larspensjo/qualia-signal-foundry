use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OPENAI_REALTIME_WS_BASE_URL: &str = "wss://api.openai.com/v1/realtime";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl RealtimeToolDefinition {
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            kind: "function".to_string(),
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseDoneOutputKind {
    Empty,
    Spoken,
    FunctionCallOnly,
    Mixed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeFunctionCall {
    pub name: String,
    pub call_id: String,
    pub arguments: Value,
}

pub fn build_openai_realtime_voice_session_update(
    model: &str,
    voice: &str,
    instructions: &str,
    output_modalities: &[String],
    input_transcription_model: Option<&str>,
    pcm_rate_hz: u32,
) -> Value {
    let mut update = serde_json::json!({
        "type": "session.update",
        "session": {
            "type": "realtime",
            "model": model,
            "output_modalities": output_modalities,
            "audio": {
                "input": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": pcm_rate_hz,
                    },
                    "turn_detection": null,
                },
                "output": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": pcm_rate_hz,
                    },
                    "voice": voice,
                },
            },
            "instructions": instructions,
        },
    });

    if let Some(transcription_model) = input_transcription_model {
        update["session"]["audio"]["input"]["transcription"] = serde_json::json!({
            "model": transcription_model,
        });
    }

    update
}

#[allow(clippy::too_many_arguments)]
pub fn build_openai_realtime_conversation_session_update(
    model: &str,
    voice: &str,
    instructions: &str,
    output_modalities: &[String],
    pcm_rate_hz: u32,
    create_response: bool,
    tools: &[RealtimeToolDefinition],
    tool_choice: Option<&str>,
    input_transcription_model: Option<&str>,
) -> Value {
    let mut update = serde_json::json!({
        "type": "session.update",
        "session": {
            "type": "realtime",
            "model": model,
            "output_modalities": output_modalities,
            "audio": {
                "input": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": pcm_rate_hz,
                    },
                    "turn_detection": {
                        "type": "server_vad",
                        "create_response": create_response,
                        "interrupt_response": true,
                    },
                },
                "output": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": pcm_rate_hz,
                    },
                    "voice": voice,
                },
            },
            "instructions": instructions,
        },
    });

    if !tools.is_empty() {
        update["session"]["tools"] = serde_json::to_value(tools).unwrap_or_else(|error| {
            panic!("failed to serialize realtime tool definitions: {error}")
        });
    }

    if let Some(tool_choice) = tool_choice {
        update["session"]["tool_choice"] = serde_json::Value::String(tool_choice.to_string());
    }

    // Enabling input transcription makes the provider emit
    // `conversation.item.input_audio_transcription.completed`, which the
    // sideband relies on to retrieve memory and issue `response.create`.
    // Every session.update must re-assert it, otherwise a later update would
    // drop transcription for subsequent turns.
    if let Some(transcription_model) = input_transcription_model {
        update["session"]["audio"]["input"]["transcription"] = serde_json::json!({
            "model": transcription_model,
        });
    }

    update
}

pub fn build_openai_realtime_conversation_item_create(role: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "type": "message",
            "role": role,
            "content": [
                {
                    "type": "input_text",
                    "text": text,
                }
            ],
        },
    })
}

pub fn build_openai_realtime_response_create(
    voice: &str,
    instructions: &str,
    pcm_rate_hz: u32,
) -> Value {
    build_openai_realtime_response_create_with_tool_choice(voice, instructions, pcm_rate_hz, None)
}

pub fn build_openai_realtime_response_create_with_tool_choice(
    voice: &str,
    instructions: &str,
    pcm_rate_hz: u32,
    tool_choice: Option<&str>,
) -> Value {
    let mut value = serde_json::json!({
        "type": "response.create",
        "response": {
            "audio": {
                "output": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": pcm_rate_hz,
                    },
                    "voice": voice,
                },
            },
            "instructions": instructions,
        },
    });
    if let Some(tool_choice) = tool_choice {
        value["response"]["tool_choice"] = Value::String(tool_choice.to_string());
    }
    value
}

pub fn build_openai_realtime_function_call_output(call_id: &str, output: &str) -> Value {
    serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "type": "function_call_output",
            "call_id": call_id,
            "output": output,
        },
    })
}

pub fn parse_realtime_server_event(provider_name: &str, text: &str) -> Option<Value> {
    match serde_json::from_str(text) {
        Ok(event) => Some(event),
        Err(error) => {
            engine_logging::engine_warn!(
                "failed to parse realtime server event: provider={} error={}",
                provider_name,
                error
            );
            None
        }
    }
}

pub fn realtime_event_type(event: &Value) -> Option<&str> {
    event.get("type").and_then(Value::as_str)
}

pub fn realtime_event_delta_text(event: &Value) -> Option<&str> {
    event.get("delta").and_then(Value::as_str)
}

pub fn realtime_event_transcript(event: &Value) -> Option<&str> {
    event.get("transcript").and_then(Value::as_str)
}

pub fn realtime_event_text(event: &Value) -> Option<&str> {
    event.get("text").and_then(Value::as_str)
}

pub fn realtime_event_response_id(event: &Value) -> Option<&str> {
    event
        .get("response")
        .and_then(|response| response.get("id"))
        .and_then(Value::as_str)
}

pub fn realtime_event_response_status(event: &Value) -> Option<&str> {
    event
        .get("response")
        .and_then(|response| response.get("status"))
        .and_then(Value::as_str)
}

pub fn realtime_event_call_id(event: &Value) -> Option<&str> {
    event.get("call_id").and_then(Value::as_str)
}

pub fn realtime_event_function_call_arguments(event: &Value) -> Option<&str> {
    event.get("arguments").and_then(Value::as_str)
}

pub fn realtime_event_function_call_name(event: &Value) -> Option<&str> {
    event.get("name").and_then(Value::as_str)
}

pub fn realtime_response_done_output_kind(event: &Value) -> ResponseDoneOutputKind {
    let Some(output) = event
        .get("response")
        .and_then(|response| response.get("output"))
        .and_then(Value::as_array)
    else {
        return ResponseDoneOutputKind::Empty;
    };

    if output.is_empty() {
        return ResponseDoneOutputKind::Empty;
    }

    let mut has_function_call = false;
    let mut has_spoken_output = false;
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("function_call") | Some("tool_search_call") => has_function_call = true,
            Some(_) | None => has_spoken_output = true,
        }
    }

    match (has_function_call, has_spoken_output) {
        (true, false) => ResponseDoneOutputKind::FunctionCallOnly,
        (false, true) => ResponseDoneOutputKind::Spoken,
        (true, true) => ResponseDoneOutputKind::Mixed,
        (false, false) => ResponseDoneOutputKind::Empty,
    }
}

pub fn extract_response_function_calls(event: &Value) -> anyhow::Result<Vec<RealtimeFunctionCall>> {
    let Some(output) = event
        .get("response")
        .and_then(|response| response.get("output"))
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };

    let mut calls = Vec::new();
    for item in output {
        let Some(item_type) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        if item_type != "function_call" && item_type != "tool_search_call" {
            continue;
        }

        let call_id = item
            .get("call_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments_text = item
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arguments: Value = serde_json::from_str(arguments_text).map_err(|error| {
            anyhow::anyhow!(
                "failed to parse function call arguments for `{name}` call `{call_id}`: {error}"
            )
        })?;
        calls.push(RealtimeFunctionCall {
            name,
            call_id,
            arguments,
        });
    }

    Ok(calls)
}

pub fn extract_response_text(event: &Value) -> Option<String> {
    let output = event.get("response")?.get("output")?.as_array()?;
    let mut text = String::new();

    for item in output {
        if let Some(content) = item.get("content").and_then(Value::as_array) {
            for part in content {
                if let Some(value) = part
                    .get("transcript")
                    .and_then(Value::as_str)
                    .or_else(|| part.get("text").and_then(Value::as_str))
                {
                    text.push_str(value);
                }
            }
        }
    }

    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_session_update_includes_requested_transcription_model() {
        let update = build_openai_realtime_voice_session_update(
            "gpt-realtime-2",
            "marin",
            "Speak briefly.",
            &["audio".to_string()],
            Some("gpt-4o-mini-transcribe"),
            24_000,
        );

        assert_eq!(
            update["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-4o-mini-transcribe"
        );
        assert_eq!(
            update["session"]["audio"]["output"]["format"]["rate"],
            24_000
        );
    }

    #[test]
    fn conversation_session_update_disables_auto_response_when_requested() {
        let update = build_openai_realtime_conversation_session_update(
            "gpt-realtime-2",
            "marin",
            "Speak briefly.",
            &["audio".to_string()],
            24_000,
            false,
            &[],
            None,
            None,
        );

        assert_eq!(
            update["session"]["audio"]["input"]["turn_detection"]["create_response"],
            false
        );
        assert!(update["session"]["audio"]["input"]["transcription"].is_null());
    }

    #[test]
    fn conversation_session_update_enables_requested_transcription_model() {
        let update = build_openai_realtime_conversation_session_update(
            "gpt-realtime-2",
            "marin",
            "Speak briefly.",
            &["audio".to_string()],
            24_000,
            false,
            &[],
            None,
            Some("gpt-4o-mini-transcribe"),
        );

        assert_eq!(
            update["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-4o-mini-transcribe"
        );
    }

    #[test]
    fn response_text_extraction_handles_nested_output_items() {
        let event = serde_json::json!({
            "type": "response.done",
            "response": {
                "output": [
                    {
                        "content": [
                            { "transcript": "Hello" },
                            { "text": " world" }
                        ]
                    }
                ]
            }
        });

        assert_eq!(
            extract_response_text(&event).as_deref(),
            Some("Hello world")
        );
    }

    #[test]
    fn conversation_item_create_uses_requested_role() {
        let item = build_openai_realtime_conversation_item_create("user", "hello");

        assert_eq!(item["item"]["role"], "user");
        assert_eq!(item["item"]["content"][0]["text"], "hello");
    }

    #[test]
    fn conversation_session_update_includes_tool_definitions() {
        let update = build_openai_realtime_conversation_session_update(
            "gpt-realtime-2",
            "marin",
            "Speak briefly.",
            &["audio".to_string()],
            24_000,
            false,
            &[RealtimeToolDefinition::function(
                "lookup",
                "Read memory.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            )],
            Some("auto"),
            None,
        );

        assert_eq!(update["session"]["tools"][0]["name"], "lookup");
        assert_eq!(update["session"]["tool_choice"], "auto");
    }

    #[test]
    fn function_call_output_builder_uses_call_id_and_output_string() {
        let item = build_openai_realtime_function_call_output("call-1", "{\"ok\":true}");

        assert_eq!(item["item"]["type"], "function_call_output");
        assert_eq!(item["item"]["call_id"], "call-1");
        assert_eq!(item["item"]["output"], "{\"ok\":true}");
    }

    #[test]
    fn response_create_can_force_no_tools() {
        let item = build_openai_realtime_response_create_with_tool_choice(
            "marin",
            "Speak now.",
            24_000,
            Some("none"),
        );

        assert_eq!(item["response"]["tool_choice"], "none");
    }

    #[test]
    fn response_done_classifier_distinguishes_function_calls_and_mixed_output() {
        let function_call = serde_json::json!({
            "type": "response.done",
            "response": {
                "output": [
                    {
                        "type": "function_call",
                        "name": "lookup",
                        "call_id": "call-1",
                        "arguments": "{\"query\":\"hello\"}"
                    }
                ]
            }
        });
        let mixed = serde_json::json!({
            "type": "response.done",
            "response": {
                "output": [
                    {
                        "type": "function_call",
                        "name": "lookup",
                        "call_id": "call-1",
                        "arguments": "{\"query\":\"hello\"}"
                    },
                    {
                        "type": "message",
                        "content": [
                            { "type": "output_text", "text": "hi" }
                        ]
                    }
                ]
            }
        });

        assert_eq!(
            realtime_response_done_output_kind(&function_call),
            ResponseDoneOutputKind::FunctionCallOnly
        );
        assert_eq!(
            realtime_response_done_output_kind(&mixed),
            ResponseDoneOutputKind::Mixed
        );
    }

    #[test]
    fn response_done_function_call_arguments_parse_or_fail() {
        let event = serde_json::json!({
            "type": "response.done",
            "response": {
                "output": [
                    {
                        "type": "function_call",
                        "name": "lookup",
                        "call_id": "call-1",
                        "arguments": "{\"query\":\"hello\"}"
                    }
                ]
            }
        });

        let calls = extract_response_function_calls(&event).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id, "call-1");
        assert_eq!(calls[0].arguments["query"], "hello");
    }

    #[test]
    fn malformed_function_call_arguments_error() {
        let event = serde_json::json!({
            "type": "response.done",
            "response": {
                "output": [
                    {
                        "type": "function_call",
                        "name": "lookup",
                        "call_id": "call-1",
                        "arguments": "{not-json"
                    }
                ]
            }
        });

        let error = extract_response_function_calls(&event).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to parse function call arguments")
        );
    }
}
