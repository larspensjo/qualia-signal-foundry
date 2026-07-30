//! The two websocket shapes used by the realtime sideband.
//!
//! A browser-call attachment uses `?call_id=` to reattach to the browser-owned provider
//! conversation, whose state survives a websocket reconnect. A server-model attachment uses
//! `?model=` and the websocket itself is the stateful provider session. Reopening that URL would
//! create a new empty-conversation provider session while the local [`SessionRuntime`] still holds
//! the earlier turns, so model-scoped attachments fail closed after their first successful attach.
//! [`?call_id=`] attachments may continue to reattach to their owning call.
//!
//! [`SessionRuntime`]: crate::state::SessionRuntime
//! [`?call_id=`]: https://platform.openai.com/docs/api-reference/realtime-calls

use std::fmt;

use qsf_realtime_protocol::{
    build_openai_realtime_browser_call_ws_url, build_openai_realtime_model_ws_url,
};

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SidebandAttachment {
    BrowserCall { call_id: String },
    ServerModelSession { model: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconnectPolicy {
    ReattachToOwningCall,
    FailClosedAfterFirstAttach,
}

impl SidebandAttachment {
    pub fn websocket_url(&self, base_url: &str) -> String {
        match self {
            Self::BrowserCall { call_id } => {
                build_openai_realtime_browser_call_ws_url(base_url, call_id)
            }
            Self::ServerModelSession { model } => {
                build_openai_realtime_model_ws_url(base_url, model)
            }
        }
    }

    /// Return the concrete provider label for utterances originating on this attachment.
    pub fn provider_id(&self) -> &str {
        match self {
            Self::BrowserCall { call_id } => call_id,
            Self::ServerModelSession { model } => model,
        }
    }

    /// Return the provider label for typed turns injected through this attachment.
    pub fn typed_turn_provider_id(&self) -> String {
        format!("{}:typed", self.provider_id())
    }

    pub fn call_id(&self) -> Option<&str> {
        match self {
            Self::BrowserCall { call_id } => Some(call_id),
            Self::ServerModelSession { .. } => None,
        }
    }

    pub fn reconnect_policy(&self) -> ReconnectPolicy {
        match self {
            Self::BrowserCall { .. } => ReconnectPolicy::ReattachToOwningCall,
            Self::ServerModelSession { .. } => ReconnectPolicy::FailClosedAfterFirstAttach,
        }
    }
}

impl fmt::Display for SidebandAttachment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrowserCall { call_id } => write!(formatter, "browser-call(call_id={call_id})"),
            Self::ServerModelSession { model } => {
                write!(formatter, "server-model-session(model={model})")
            }
        }
    }
}

impl fmt::Display for ReconnectPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReattachToOwningCall => formatter.write_str("reattach-to-owning-call"),
            Self::FailClosedAfterFirstAttach => {
                formatter.write_str("fail-closed-after-first-attach")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_call_attachment_has_call_scoped_values() {
        let attachment = SidebandAttachment::BrowserCall {
            call_id: "call-123".to_string(),
        };

        assert_eq!(
            attachment.websocket_url("ws://localhost:9000/v1/realtime/"),
            "ws://localhost:9000/v1/realtime?call_id=call-123"
        );
        assert_eq!(attachment.provider_id(), "call-123");
        assert_eq!(attachment.typed_turn_provider_id(), "call-123:typed");
        assert_eq!(attachment.call_id(), Some("call-123"));
        assert_eq!(
            attachment.reconnect_policy(),
            ReconnectPolicy::ReattachToOwningCall
        );
    }

    #[test]
    fn server_model_attachment_has_model_scoped_values_without_a_fake_call_id() {
        let attachment = SidebandAttachment::ServerModelSession {
            model: "gpt-realtime".to_string(),
        };

        assert_eq!(
            attachment.websocket_url("ws://localhost:9000/v1/realtime"),
            "ws://localhost:9000/v1/realtime?model=gpt-realtime"
        );
        assert_eq!(attachment.provider_id(), "gpt-realtime");
        assert_eq!(attachment.typed_turn_provider_id(), "gpt-realtime:typed");
        assert_eq!(attachment.call_id(), None);
        assert_eq!(
            attachment.reconnect_policy(),
            ReconnectPolicy::FailClosedAfterFirstAttach
        );
    }
}
