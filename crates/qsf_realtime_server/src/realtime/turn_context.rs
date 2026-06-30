use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct TurnContextCapture {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    #[serde(with = "time::serde::rfc3339")]
    pub captured_at: OffsetDateTime,
    pub request_hash: String,
    pub messages: Vec<serde_json::Value>,
}

#[allow(dead_code)]
pub fn build_turn_context_capture(
    qsf_session_id: String,
    exchange_index: usize,
    request_hash: String,
    messages: Vec<serde_json::Value>,
) -> TurnContextCapture {
    TurnContextCapture {
        qsf_session_id,
        exchange_index,
        captured_at: OffsetDateTime::now_utc(),
        request_hash,
        messages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_preserves_messages_and_request_hash() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let capture = build_turn_context_capture(
            "session-123".to_string(),
            0,
            "abc123".to_string(),
            messages.clone(),
        );
        assert_eq!(capture.messages, messages);
        assert_eq!(capture.request_hash, "abc123");
        // captured_at is set (non-epoch)
        assert!(capture.captured_at > OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn no_secret_in_serialized_capture() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let capture = build_turn_context_capture(
            "session-123".to_string(),
            0,
            "abc123".to_string(),
            messages,
        );
        let serialized = serde_json::to_string(&capture).unwrap();
        assert!(!serialized.contains("OPENAI_API_KEY"));
        assert!(!serialized.contains("Bearer "));
    }

    #[test]
    fn captured_at_serializes_as_string() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let capture = build_turn_context_capture(
            "session-123".to_string(),
            0,
            "abc123".to_string(),
            messages,
        );
        let value = serde_json::to_value(&capture).unwrap();
        assert!(value["captured_at"].is_string());
    }
}
