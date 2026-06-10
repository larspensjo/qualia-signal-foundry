use serde_json::Value;

pub const OPENAI_REALTIME_WS_BASE_URL: &str = "wss://api.openai.com/v1/realtime";

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

pub fn build_openai_realtime_conversation_session_update(
    model: &str,
    voice: &str,
    instructions: &str,
    output_modalities: &[String],
    pcm_rate_hz: u32,
    create_response: bool,
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
    serde_json::json!({
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
}
