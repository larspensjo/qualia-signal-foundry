use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::ContextAssembly;
use crate::conversation::ContentHash;
use crate::state::Turn;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Exchange {
    pub index: usize,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub input: ExchangeInput,
    #[serde(default)]
    pub output: Option<ExchangeOutput>,
    #[serde(default)]
    pub context_assembly: Option<ContextAssembly>,
    #[serde(default)]
    pub retrieved_memory_block: String,
    #[serde(default)]
    pub recalled_items: Vec<crate::state::RecallRecord>,
    #[serde(default)]
    pub model: Option<ExchangeModelUse>,
    #[serde(default)]
    pub interruptions: Vec<InterruptionRecord>,
    #[serde(default)]
    pub provider_events: Vec<ProviderEventRecord>,
    #[serde(default)]
    pub tool_requests: Vec<ToolRequestRecord>,
    #[serde(default)]
    pub status: ExchangeStatus,
}

impl Exchange {
    pub fn new_text(index: usize, user_input: impl Into<String>, started_at: SystemTime) -> Self {
        Self {
            index,
            started_at,
            completed_at: None,
            input: ExchangeInput::Text {
                text: user_input.into(),
            },
            output: None,
            context_assembly: None,
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            model: None,
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            status: ExchangeStatus::AwaitingModel,
        }
    }

    pub fn new_voice_pending(index: usize, started_at: SystemTime) -> Self {
        Self {
            index,
            started_at,
            completed_at: None,
            input: ExchangeInput::Voice {
                final_transcript: String::new(),
                utterances: vec![],
            },
            output: None,
            context_assembly: None,
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            model: None,
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            status: ExchangeStatus::Listening,
        }
    }

    pub fn completed(mut self, completed_at: SystemTime) -> Self {
        self.completed_at = Some(completed_at);
        self.status = ExchangeStatus::Completed;
        self
    }

    pub fn final_user_input(&self) -> &str {
        match &self.input {
            ExchangeInput::Text { text } => text,
            ExchangeInput::Voice {
                final_transcript, ..
            } => final_transcript,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeInput {
    Text {
        text: String,
    },
    Voice {
        final_transcript: String,
        utterances: Vec<UtteranceRecord>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExchangeOutput {
    #[serde(default)]
    pub response_id: Option<String>,
    pub text: String,
    pub produced_at: SystemTime,
    #[serde(default)]
    pub provider_name: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub audio_marker: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExchangeModelUse {
    #[serde(default)]
    pub provider_name: Option<String>,
    pub model_id: String,
    pub latency_ms: u64,
    pub input_tokens: u32,
    pub cached_input_tokens: u32,
    pub output_tokens: u32,
    pub full_request_hash: ContentHash,
    pub message_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExchangeRange {
    pub first_index: usize,
    pub last_index: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UtteranceRecord {
    pub utterance_id: String,
    pub revision_index: u32,
    pub transcript: String,
    pub received_at: SystemTime,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub source_chunk_index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InterruptionRecord {
    pub exchange_index: usize,
    #[serde(default)]
    pub response_id: Option<String>,
    pub detected_at: SystemTime,
    pub source: String,
    pub action: InterruptionAction,
    pub stop_outcome: InterruptionStopOutcome,
    #[serde(default)]
    pub partial_response_text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProviderEventRecord {
    pub exchange_index: usize,
    pub event_kind: ProviderEventKind,
    pub provider_id: String,
    pub received_at: SystemTime,
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub audio_marker: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEventKind {
    Preamble,
    ResponseStarted,
    ResponseCompleted,
    SpeechPlaybackStarted,
    SpeechPlaybackCompleted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolRequestRecord {
    pub exchange_index: usize,
    pub call_id: String,
    pub tool_name: String,
    pub arguments_summary: String,
    pub requested_at: SystemTime,
    pub source: String,
    #[serde(default)]
    pub routed_to: Option<String>,
    pub auto_executed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionAction {
    #[default]
    Stop,
    Ignore,
    MarkInterrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionStopOutcome {
    #[default]
    Stopped,
    Ignored,
    AlreadyComplete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeStatus {
    #[default]
    AwaitingModel,
    Listening,
    Speaking,
    Interrupted,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExchangeTurnConversionError {
    MissingCompletedAt { exchange_index: usize },
    MissingOutput { exchange_index: usize },
    MissingModelUse { exchange_index: usize },
    MissingContextAssembly { exchange_index: usize },
}

impl std::fmt::Display for ExchangeTurnConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCompletedAt { exchange_index } => {
                write!(
                    formatter,
                    "exchange {exchange_index} is missing completed_at"
                )
            }
            Self::MissingOutput { exchange_index } => {
                write!(formatter, "exchange {exchange_index} is missing output")
            }
            Self::MissingModelUse { exchange_index } => {
                write!(formatter, "exchange {exchange_index} is missing model use")
            }
            Self::MissingContextAssembly { exchange_index } => {
                write!(
                    formatter,
                    "exchange {exchange_index} is missing context assembly"
                )
            }
        }
    }
}

impl std::error::Error for ExchangeTurnConversionError {}

impl std::convert::TryFrom<&Exchange> for Turn {
    type Error = ExchangeTurnConversionError;

    fn try_from(exchange: &Exchange) -> Result<Self, Self::Error> {
        let output =
            exchange
                .output
                .as_ref()
                .ok_or(ExchangeTurnConversionError::MissingOutput {
                    exchange_index: exchange.index,
                })?;
        let model =
            exchange
                .model
                .as_ref()
                .ok_or(ExchangeTurnConversionError::MissingModelUse {
                    exchange_index: exchange.index,
                })?;
        let context_assembly = exchange.context_assembly.clone().ok_or(
            ExchangeTurnConversionError::MissingContextAssembly {
                exchange_index: exchange.index,
            },
        )?;
        let completed_at =
            exchange
                .completed_at
                .ok_or(ExchangeTurnConversionError::MissingCompletedAt {
                    exchange_index: exchange.index,
                })?;

        Ok(Self {
            index: exchange.index,
            started_at: exchange.started_at,
            completed_at,
            user_input: exchange.final_user_input().to_string(),
            context_assembly,
            retrieved_memory_block: exchange.retrieved_memory_block.clone(),
            assistant_response: output.text.clone(),
            recalled_turns: exchange.recalled_items.clone(),
            model_id: model.model_id.clone(),
            model_latency_ms: model.latency_ms,
            input_tokens: model.input_tokens,
            cached_input_tokens: model.cached_input_tokens,
            output_tokens: model.output_tokens,
            full_request_hash: model.full_request_hash,
            message_count: model.message_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget};
    use crate::conversation::ContentHash;

    #[test]
    fn completed_exchange_converts_to_turn() {
        let exchange = Exchange {
            index: 2,
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
                full_request_hash: ContentHash([2; 32]),
                message_count: 3,
            }),
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            status: ExchangeStatus::Completed,
        };

        let turn = Turn::try_from(&exchange).unwrap();
        assert_eq!(turn.index, 2);
        assert_eq!(turn.user_input, "hello");
        assert_eq!(turn.assistant_response, "hi");
        assert_eq!(turn.model_id, "mock");
        assert_eq!(turn.full_request_hash, ContentHash([2; 32]));
    }
}
