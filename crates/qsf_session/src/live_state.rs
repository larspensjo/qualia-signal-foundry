use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::ContextAssembly;
use crate::exchange::{
    Exchange, ExchangeInput, ExchangeModelUse, ExchangeOutput, ExchangeRange, ExchangeStatus,
    InterruptionAction, InterruptionRecord, InterruptionStopOutcome, ProviderEventKind,
    ProviderEventRecord, ToolExecutionRecord, ToolRequestRecord, UtteranceRecord,
};
use crate::state::{RecallRecord, SessionEndReason, TurnSummary};
use qsf_memory::processed_range::ProcessedRange;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    #[default]
    Starting,
    Streaming,
    Completed,
    Interrupted,
    Ignored,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PartialTranscript {
    pub exchange_index: usize,
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
pub struct ActiveResponseState {
    #[serde(default)]
    pub response_id: Option<String>,
    pub partial_text: String,
    pub status: ResponseStatus,
    pub observed_at: SystemTime,
    #[serde(default)]
    pub audio_marker: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LiveCaptureContext {
    #[serde(default)]
    pub source_exchange_index: Option<usize>,
    #[serde(default)]
    pub previous_user_input: Option<String>,
    #[serde(default)]
    pub previous_assistant_response: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgedCoRetrievalRecord {
    pub exchange_range: ExchangeRange,
    pub new_associations: usize,
    pub strengthened_associations: usize,
    pub persisted_at: SystemTime,
    pub summaries: Vec<TurnSummary>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct LiveSessionState {
    #[serde(default)]
    pub runtime_phase: RuntimePhase,
    #[serde(default)]
    pub active_exchange: Option<Exchange>,
    #[serde(skip)]
    pub completed_exchanges: Vec<Exchange>,
    #[serde(default)]
    pub partial_transcript: Option<PartialTranscript>,
    #[serde(default)]
    pub active_response: Option<ActiveResponseState>,
    #[serde(default)]
    pub live_capture: Option<LiveCaptureContext>,
    #[serde(default)]
    pub processed_ranges: Vec<ProcessedRange>,
    #[serde(default)]
    pub last_aged_co_retrieval: Option<AgedCoRetrievalRecord>,
    #[serde(skip)]
    suppressed_response_ids: Vec<String>,
}

impl LiveSessionState {
    pub fn prepare_for_awake_continuation(&mut self) {
        self.runtime_phase = RuntimePhase::Idle;
        self.partial_transcript = None;
        self.active_response = None;
        self.live_capture = None;

        if let Some(exchange) = self.active_exchange.as_mut() {
            if !matches!(exchange.status, ExchangeStatus::Completed) {
                exchange.status = ExchangeStatus::Interrupted;
            }
        }
    }

    fn suppress_response_id(&mut self, response_id: Option<&str>) {
        let Some(response_id) = response_id else {
            return;
        };
        if !self
            .suppressed_response_ids
            .iter()
            .any(|suppressed| suppressed == response_id)
        {
            self.suppressed_response_ids.push(response_id.to_string());
        }
    }

    fn response_is_suppressed(&self, response_id: Option<&str>) -> bool {
        let Some(response_id) = response_id else {
            return false;
        };
        self.suppressed_response_ids
            .iter()
            .any(|suppressed| suppressed == response_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiveSessionEvent {
    SessionStarted,
    SessionResumed,
    ExchangeStarted(Box<Exchange>),
    AudioPartialTranscriptRecorded(PartialTranscript),
    AudioFinalTranscriptCommitted {
        exchange_index: usize,
        utterance: UtteranceRecord,
        final_transcript: String,
    },
    ExchangesAgedAndCoRetrieved {
        exchange_range: ExchangeRange,
        new_associations: usize,
        strengthened_associations: usize,
        persisted_at: SystemTime,
        summaries: Vec<TurnSummary>,
    },
    MemoryContextRecorded {
        exchange_index: usize,
        context_assembly: ContextAssembly,
        retrieved_memory_block: String,
        recalled_items: Vec<RecallRecord>,
        live_capture: Option<LiveCaptureContext>,
    },
    ModelRoleCompleted(ExchangeModelUse),
    ModelRoleFailed {
        error_summary: String,
    },
    OutputProduced(ExchangeOutput),
    ProviderEventRecorded(ProviderEventRecord),
    ToolRequested(ToolRequestRecord),
    ToolResolved(ToolExecutionRecord),
    UserInterrupted(InterruptionRecord),
    ExchangeCompleted {
        exchange_index: usize,
        completed_at: SystemTime,
    },
    ProcessedRangesRecorded {
        ranges: Vec<ProcessedRange>,
    },
    SessionEnded {
        reason: SessionEndReason,
    },
}

pub fn reduce_live_session(
    mut state: LiveSessionState,
    event: LiveSessionEvent,
) -> LiveSessionState {
    reduce_live_session_in_place(&mut state, event);
    state
}

fn reduce_live_session_in_place(state: &mut LiveSessionState, event: LiveSessionEvent) {
    match event {
        LiveSessionEvent::SessionStarted => {
            state.runtime_phase = RuntimePhase::Idle;
            state.partial_transcript = None;
            state.active_response = None;
            state.live_capture = None;
            state.suppressed_response_ids.clear();
        }
        LiveSessionEvent::SessionResumed => {
            state.prepare_for_awake_continuation();
        }
        LiveSessionEvent::ExchangeStarted(exchange) => {
            let mut exchange = *exchange;
            let started_at = exchange.started_at;
            if let Some(mut previous_exchange) = state.active_exchange.take() {
                let previous_response_status = state
                    .active_response
                    .as_ref()
                    .map(|response| response.status);
                let previous_response_id = state
                    .active_response
                    .as_ref()
                    .and_then(|response| response.response_id.as_deref())
                    .map(str::to_string)
                    .or_else(|| {
                        previous_exchange
                            .output
                            .as_ref()
                            .and_then(|output| output.response_id.as_deref())
                            .map(str::to_string)
                    });
                state.suppress_response_id(previous_response_id.as_deref());
                state.active_response = None;
                state.partial_transcript = None;
                state.live_capture = None;

                previous_exchange.completed_at.get_or_insert(started_at);
                previous_exchange.status =
                    if matches!(previous_exchange.status, ExchangeStatus::Interrupted)
                        || matches!(
                            previous_response_status,
                            Some(ResponseStatus::Starting | ResponseStatus::Streaming)
                        )
                    {
                        ExchangeStatus::Interrupted
                    } else {
                        ExchangeStatus::Completed
                    };
                state.completed_exchanges.push(previous_exchange);
                state.runtime_phase = RuntimePhase::Idle;
            }
            state.runtime_phase = match &exchange.input {
                ExchangeInput::Text { .. } => RuntimePhase::Thinking,
                ExchangeInput::Voice {
                    final_transcript, ..
                } if final_transcript.trim().is_empty() => RuntimePhase::Listening,
                ExchangeInput::Voice { .. } => RuntimePhase::Thinking,
            };
            exchange.status = match state.runtime_phase {
                RuntimePhase::Listening => ExchangeStatus::Listening,
                RuntimePhase::Thinking | RuntimePhase::Idle | RuntimePhase::Speaking => {
                    ExchangeStatus::AwaitingModel
                }
            };
            state.active_exchange = Some(exchange);
            state.partial_transcript = None;
            state.active_response = None;
            state.live_capture = None;
            state.runtime_phase = match state.active_exchange.as_ref().map(|active| &active.input) {
                Some(ExchangeInput::Text { .. }) => RuntimePhase::Thinking,
                Some(ExchangeInput::Voice {
                    final_transcript, ..
                }) if final_transcript.trim().is_empty() => RuntimePhase::Listening,
                Some(ExchangeInput::Voice { .. }) => RuntimePhase::Thinking,
                None => RuntimePhase::Idle,
            };
        }
        LiveSessionEvent::AudioPartialTranscriptRecorded(partial) => {
            state.runtime_phase = RuntimePhase::Listening;
            state.partial_transcript = Some(partial);
        }
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index,
            utterance,
            final_transcript,
        } => {
            state.partial_transcript = None;
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == exchange_index {
                    let response_in_progress = state.active_response.is_some()
                        || matches!(exchange.status, ExchangeStatus::Speaking);
                    match &mut exchange.input {
                        ExchangeInput::Voice {
                            final_transcript: current_transcript,
                            utterances,
                        } => {
                            *current_transcript = final_transcript;
                            utterances.push(utterance);
                        }
                        ExchangeInput::Text { .. } => {
                            exchange.input = ExchangeInput::Voice {
                                final_transcript,
                                utterances: vec![utterance],
                            };
                        }
                    }
                    if !matches!(
                        exchange.status,
                        ExchangeStatus::Interrupted
                            | ExchangeStatus::Completed
                            | ExchangeStatus::Failed
                    ) {
                        exchange.status = if response_in_progress {
                            ExchangeStatus::Speaking
                        } else {
                            ExchangeStatus::AwaitingModel
                        };
                    }
                    state.runtime_phase = if response_in_progress {
                        RuntimePhase::Speaking
                    } else {
                        RuntimePhase::Thinking
                    };
                }
            }
        }
        LiveSessionEvent::ExchangesAgedAndCoRetrieved {
            exchange_range,
            new_associations,
            strengthened_associations,
            persisted_at,
            summaries,
        } => {
            state.last_aged_co_retrieval = Some(AgedCoRetrievalRecord {
                exchange_range,
                new_associations,
                strengthened_associations,
                persisted_at,
                summaries,
            });
        }
        LiveSessionEvent::MemoryContextRecorded {
            exchange_index,
            context_assembly,
            retrieved_memory_block,
            recalled_items,
            live_capture,
        } => {
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == exchange_index {
                    exchange.context_assembly = Some(context_assembly);
                    exchange.retrieved_memory_block = retrieved_memory_block;
                    exchange.recalled_items = recalled_items;
                    state.live_capture = live_capture;
                }
            }
        }
        LiveSessionEvent::ModelRoleCompleted(model) => {
            if let Some(exchange) = state.active_exchange.as_mut() {
                exchange.model = Some(model);
            }
        }
        LiveSessionEvent::ModelRoleFailed { .. } => {
            state.runtime_phase = RuntimePhase::Idle;
            if let Some(exchange) = state.active_exchange.as_mut() {
                if !matches!(exchange.status, ExchangeStatus::Completed) {
                    exchange.status = ExchangeStatus::Failed;
                }
            }
            state.active_exchange = None;
        }
        LiveSessionEvent::OutputProduced(output) => {
            let response_id = output.response_id.clone();
            let should_ignore = state.response_is_suppressed(response_id.as_deref())
                || state
                    .active_exchange
                    .as_ref()
                    .map(|exchange| {
                        matches!(
                            exchange.status,
                            ExchangeStatus::Interrupted
                                | ExchangeStatus::Completed
                                | ExchangeStatus::Failed
                        )
                    })
                    .unwrap_or(true);
            if !should_ignore {
                state.runtime_phase = RuntimePhase::Speaking;
                state.active_response = Some(ActiveResponseState {
                    response_id: output.response_id.clone(),
                    partial_text: output.text.clone(),
                    status: ResponseStatus::Completed,
                    observed_at: output.produced_at,
                    audio_marker: output.audio_marker.clone(),
                });
                if let Some(exchange) = state.active_exchange.as_mut() {
                    exchange.output = Some(output);
                    exchange.status = ExchangeStatus::Speaking;
                }
                state.suppress_response_id(response_id.as_deref());
            }
        }
        LiveSessionEvent::ProviderEventRecorded(provider_event) => {
            let response_id = provider_event.response_id.as_deref();
            let Some(active_exchange_status) =
                state.active_exchange.as_ref().and_then(|exchange| {
                    if exchange.index == provider_event.exchange_index {
                        Some(exchange.status)
                    } else {
                        None
                    }
                })
            else {
                return;
            };
            let should_ignore = state.response_is_suppressed(response_id)
                || matches!(
                    active_exchange_status,
                    ExchangeStatus::Interrupted
                        | ExchangeStatus::Completed
                        | ExchangeStatus::Failed
                );

            if !should_ignore {
                match provider_event.event_kind {
                    ProviderEventKind::ResponseStarted => {
                        let should_update = state.active_response.as_ref().map(|response| {
                            response.response_id.as_deref() != response_id
                                || !matches!(response.status, ResponseStatus::Completed)
                        });
                        if should_update.unwrap_or(true) {
                            state.runtime_phase = RuntimePhase::Thinking;
                            state.active_response = Some(ActiveResponseState {
                                response_id: provider_event.response_id.clone(),
                                partial_text: provider_event.text.clone().unwrap_or_default(),
                                status: ResponseStatus::Starting,
                                observed_at: provider_event.received_at,
                                audio_marker: provider_event.audio_marker.clone(),
                            });
                        }
                    }
                    ProviderEventKind::ResponseCompleted => {
                        state.runtime_phase = RuntimePhase::Speaking;
                        match state.active_response.as_mut() {
                            Some(active_response)
                                if active_response.response_id.as_deref() == response_id =>
                            {
                                active_response.status = ResponseStatus::Completed;
                                if let Some(text) = provider_event.text.as_ref() {
                                    active_response.partial_text = text.clone();
                                }
                                active_response.observed_at = provider_event.received_at;
                                active_response.audio_marker = provider_event.audio_marker.clone();
                            }
                            Some(active_response)
                                if matches!(active_response.status, ResponseStatus::Completed) => {}
                            Some(active_response) => {
                                active_response.response_id = provider_event.response_id.clone();
                                active_response.partial_text =
                                    provider_event.text.clone().unwrap_or_default();
                                active_response.status = ResponseStatus::Completed;
                                active_response.observed_at = provider_event.received_at;
                                active_response.audio_marker = provider_event.audio_marker.clone();
                            }
                            None => {
                                state.active_response = Some(ActiveResponseState {
                                    response_id: provider_event.response_id.clone(),
                                    partial_text: provider_event.text.clone().unwrap_or_default(),
                                    status: ResponseStatus::Completed,
                                    observed_at: provider_event.received_at,
                                    audio_marker: provider_event.audio_marker.clone(),
                                });
                            }
                        }
                    }
                    ProviderEventKind::SpeechPlaybackStarted => {
                        state.runtime_phase = RuntimePhase::Speaking;
                    }
                    ProviderEventKind::SpeechPlaybackCompleted => {
                        state.runtime_phase = RuntimePhase::Idle;
                    }
                    ProviderEventKind::FunctionCallCompleted => {}
                    ProviderEventKind::Preamble => {}
                }
            }

            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == provider_event.exchange_index {
                    exchange.provider_events.push(provider_event);
                }
            }
        }
        LiveSessionEvent::ToolRequested(tool_request) => {
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == tool_request.exchange_index {
                    exchange.tool_requests.push(tool_request);
                }
            }
        }
        LiveSessionEvent::ToolResolved(tool_execution) => {
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == tool_execution.exchange_index {
                    exchange.tool_executions.push(tool_execution);
                }
            }
        }
        LiveSessionEvent::UserInterrupted(interruption) => {
            let stop_outcome = interruption.stop_outcome;
            let action = interruption.action;
            let response_id = interruption.response_id.clone();
            let response_id_to_suppress = state
                .active_exchange
                .as_ref()
                .and_then(|exchange| {
                    if exchange.index == interruption.exchange_index {
                        exchange
                            .output
                            .as_ref()
                            .and_then(|output| output.response_id.as_deref())
                            .map(str::to_string)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    state
                        .active_response
                        .as_ref()
                        .and_then(|response| response.response_id.as_deref())
                        .map(str::to_string)
                })
                .or(response_id);
            let mut handled = false;
            let should_suppress = !matches!(stop_outcome, InterruptionStopOutcome::Ignored);
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == interruption.exchange_index {
                    handled = true;
                    exchange.interruptions.push(interruption);
                    if should_suppress {
                        exchange.status = ExchangeStatus::Interrupted;
                        state.active_response = None;
                        state.runtime_phase = RuntimePhase::Idle;
                    }
                }
            }
            if handled && should_suppress {
                state.suppress_response_id(response_id_to_suppress.as_deref());
            }
            if handled
                && matches!(action, InterruptionAction::Ignore)
                && matches!(stop_outcome, InterruptionStopOutcome::Ignored)
                && state.active_response.is_some()
            {
                state.runtime_phase = RuntimePhase::Speaking;
            }
        }
        LiveSessionEvent::ExchangeCompleted {
            exchange_index,
            completed_at,
        } => {
            if let Some(mut exchange) = state.active_exchange.take() {
                if exchange.index == exchange_index {
                    let response_id = state
                        .active_response
                        .as_ref()
                        .and_then(|response| response.response_id.as_deref())
                        .map(str::to_string)
                        .or_else(|| {
                            exchange
                                .output
                                .as_ref()
                                .and_then(|output| output.response_id.as_deref())
                                .map(str::to_string)
                        });
                    state.suppress_response_id(response_id.as_deref());
                    exchange.completed_at = Some(completed_at);
                    exchange.status = if matches!(exchange.status, ExchangeStatus::Interrupted) {
                        ExchangeStatus::Interrupted
                    } else {
                        ExchangeStatus::Completed
                    };
                    state.completed_exchanges.push(exchange);
                    // Completion cleanup is tied to closing the matching active exchange.
                    state.active_response = None;
                    state.partial_transcript = None;
                    state.live_capture = None;
                    state.runtime_phase = RuntimePhase::Idle;
                } else {
                    state.active_exchange = Some(exchange);
                }
            }
        }
        LiveSessionEvent::ProcessedRangesRecorded { ranges } => {
            state.processed_ranges.extend(ranges);
        }
        LiveSessionEvent::SessionEnded { .. } => {
            state.runtime_phase = RuntimePhase::Idle;
            state.active_response = None;
            state.partial_transcript = None;
            state.live_capture = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget};
    use crate::exchange::{
        Exchange, ExchangeInput, ExchangeOutput, ExchangeRange, ExchangeStatus, InterruptionAction,
        InterruptionRecord, InterruptionStopOutcome, ProviderEventKind, ProviderEventRecord,
        ToolRequestRecord, UtteranceRecord,
    };
    use crate::state::TurnSummary;
    use qsf_memory::processed_range::ProcessedRangeKind;

    #[allow(clippy::too_many_arguments)]
    fn provider_event_record(
        exchange_index: usize,
        event_kind: ProviderEventKind,
        provider_id: impl Into<String>,
        received_at: SystemTime,
        response_id: Option<&str>,
        text: Option<&str>,
        status: Option<&str>,
        audio_marker: Option<&str>,
    ) -> ProviderEventRecord {
        ProviderEventRecord {
            exchange_index,
            event_kind,
            provider_id: provider_id.into(),
            received_at,
            call_id: None,
            event_id: None,
            item_id: None,
            previous_item_id: None,
            response_id: response_id.map(str::to_string),
            text: text.map(str::to_string),
            status: status.map(str::to_string),
            audio_marker: audio_marker.map(str::to_string),
        }
    }

    #[test]
    fn text_exchange_completion_is_recorded() {
        let exchange = Exchange::new_text(0, "hello", SystemTime::UNIX_EPOCH);
        let mut state = LiveSessionState::default();

        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(exchange)),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::MemoryContextRecorded {
                exchange_index: 0,
                context_assembly: ContextAssembly {
                    budget: ContextBudget::new(4, 100),
                    selected: vec![],
                    omitted: vec![],
                    used_estimated_tokens: 0,
                },
                retrieved_memory_block: "memory".to_string(),
                recalled_items: vec![],
                live_capture: Some(LiveCaptureContext {
                    source_exchange_index: Some(0),
                    previous_user_input: Some("prev".to_string()),
                    previous_assistant_response: Some("answer".to_string()),
                }),
            },
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-1".to_string()),
                text: "hi".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("mock".to_string()),
                target: Some("text".to_string()),
                audio_marker: None,
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index: 0,
                completed_at: SystemTime::UNIX_EPOCH,
            },
        );

        assert_eq!(state.runtime_phase, RuntimePhase::Idle);
        assert!(state.active_exchange.is_none());
        assert_eq!(state.completed_exchanges.len(), 1);
        let completed = &state.completed_exchanges[0];
        assert_eq!(completed.status, ExchangeStatus::Completed);
        assert_eq!(completed.final_user_input(), "hello");
        assert_eq!(completed.retrieved_memory_block, "memory");
        assert!(completed.output.is_some());
    }

    #[test]
    fn completion_with_mismatched_exchange_index_is_ignored() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                3,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );

        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index: 99,
                completed_at: SystemTime::UNIX_EPOCH,
            },
        );

        assert!(state.active_exchange.is_some());
        assert!(state.completed_exchanges.is_empty());
        assert_eq!(state.runtime_phase, RuntimePhase::Thinking);
    }

    #[test]
    fn provider_events_and_tool_requests_attach_to_active_exchange_only() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                4,
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                4,
                ProviderEventKind::Preamble,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-4"),
                Some("hello"),
                None,
                None,
            )),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ToolRequested(ToolRequestRecord {
                exchange_index: 4,
                call_id: "call-4".to_string(),
                tool_name: "lookup".to_string(),
                arguments_summary: "{}".to_string(),
                requested_at: SystemTime::UNIX_EPOCH,
                source: "realtime_provider".to_string(),
                routed_to: Some("qsf_tool_permission_boundary".to_string()),
                auto_executed: false,
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ToolResolved(crate::exchange::ToolExecutionRecord {
                exchange_index: 4,
                call_id: "call-4".to_string(),
                tool_name: "lookup".to_string(),
                permission_decision: crate::exchange::ToolPermissionDecision::Allowed,
                status: crate::exchange::ToolExecutionStatus::Completed,
                result_summary: "ok".to_string(),
                error: None,
                requested_at: SystemTime::UNIX_EPOCH,
                completed_at: Some(SystemTime::UNIX_EPOCH),
                response_model_use: None,
                returning_event_id: Some("event-4".to_string()),
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ToolRequested(ToolRequestRecord {
                exchange_index: 99,
                call_id: "ignored".to_string(),
                tool_name: "lookup".to_string(),
                arguments_summary: "{}".to_string(),
                requested_at: SystemTime::UNIX_EPOCH,
                source: "realtime_provider".to_string(),
                routed_to: Some("qsf_tool_permission_boundary".to_string()),
                auto_executed: false,
            }),
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.provider_events.len(), 1);
        assert_eq!(exchange.tool_requests.len(), 1);
        assert_eq!(exchange.tool_executions.len(), 1);
        assert_eq!(exchange.tool_requests[0].call_id, "call-4");
        assert_eq!(exchange.tool_executions[0].call_id, "call-4");
    }

    #[test]
    fn tool_resolved_is_ignored_for_finalized_exchanges() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                6,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index: 6,
                completed_at: SystemTime::UNIX_EPOCH,
            },
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ToolResolved(crate::exchange::ToolExecutionRecord {
                exchange_index: 6,
                call_id: "call-6".to_string(),
                tool_name: "lookup".to_string(),
                permission_decision: crate::exchange::ToolPermissionDecision::Allowed,
                status: crate::exchange::ToolExecutionStatus::Completed,
                result_summary: "ok".to_string(),
                error: None,
                requested_at: SystemTime::UNIX_EPOCH,
                completed_at: Some(SystemTime::UNIX_EPOCH),
                response_model_use: None,
                returning_event_id: Some("event-6".to_string()),
            }),
        );

        assert_eq!(state.completed_exchanges.len(), 1);
        assert!(state.completed_exchanges[0].tool_executions.is_empty());
    }

    #[test]
    fn partial_transcript_updates_live_state() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioPartialTranscriptRecorded(PartialTranscript {
                exchange_index: 3,
                utterance_id: "utterance-3".to_string(),
                revision_index: 2,
                transcript: "hel".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: Some(9),
            }),
        );

        assert_eq!(state.runtime_phase, RuntimePhase::Listening);
        assert_eq!(
            state
                .partial_transcript
                .as_ref()
                .map(|partial| partial.transcript.as_str()),
            Some("hel")
        );
    }

    #[test]
    fn final_transcript_commits_voice_input() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                7,
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioPartialTranscriptRecorded(PartialTranscript {
                exchange_index: 7,
                utterance_id: "utterance-7".to_string(),
                revision_index: 1,
                transcript: "par".to_string(),
                received_at: SystemTime::UNIX_EPOCH,
                provider_id: Some("provider".to_string()),
                source_chunk_index: None,
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioFinalTranscriptCommitted {
                exchange_index: 7,
                utterance: UtteranceRecord {
                    utterance_id: "utterance-7".to_string(),
                    revision_index: 2,
                    transcript: "partial transcript".to_string(),
                    received_at: SystemTime::UNIX_EPOCH,
                    provider_id: Some("provider".to_string()),
                    source_chunk_index: Some(4),
                },
                final_transcript: "final transcript".to_string(),
            },
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        match &exchange.input {
            ExchangeInput::Voice {
                final_transcript,
                utterances,
            } => {
                assert_eq!(final_transcript, "final transcript");
                assert_eq!(utterances.len(), 1);
                assert_eq!(utterances[0].utterance_id, "utterance-7");
            }
            ExchangeInput::Text { .. } => panic!("expected voice input"),
        }
        assert!(state.partial_transcript.is_none());
    }

    #[test]
    fn final_transcript_appends_utterances() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                9,
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioFinalTranscriptCommitted {
                exchange_index: 9,
                utterance: UtteranceRecord {
                    utterance_id: "utterance-9a".to_string(),
                    revision_index: 1,
                    transcript: "first".to_string(),
                    received_at: SystemTime::UNIX_EPOCH,
                    provider_id: Some("provider".to_string()),
                    source_chunk_index: Some(1),
                },
                final_transcript: "first final".to_string(),
            },
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioFinalTranscriptCommitted {
                exchange_index: 9,
                utterance: UtteranceRecord {
                    utterance_id: "utterance-9b".to_string(),
                    revision_index: 2,
                    transcript: "second".to_string(),
                    received_at: SystemTime::UNIX_EPOCH,
                    provider_id: Some("provider".to_string()),
                    source_chunk_index: Some(2),
                },
                final_transcript: "second final".to_string(),
            },
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        match &exchange.input {
            ExchangeInput::Voice {
                final_transcript,
                utterances,
            } => {
                assert_eq!(final_transcript, "second final");
                assert_eq!(utterances.len(), 2);
                assert_eq!(utterances[0].utterance_id, "utterance-9a");
                assert_eq!(utterances[1].utterance_id, "utterance-9b");
            }
            ExchangeInput::Text { .. } => panic!("expected voice input"),
        }
    }

    #[test]
    fn aging_and_co_retrieval_state_records_counts_and_summaries() {
        let mut state = LiveSessionState::default();
        let summary = TurnSummary {
            turn_index: 3,
            summarized_after_turn_index: 7,
            summary: "Summarized exchange batch".to_string(),
            model_id: "mock".to_string(),
            model_latency_ms: 42,
            input_tokens: 10,
            output_tokens: 5,
        };

        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangesAgedAndCoRetrieved {
                exchange_range: ExchangeRange {
                    first_index: 2,
                    last_index: 7,
                },
                new_associations: 4,
                strengthened_associations: 2,
                persisted_at: SystemTime::UNIX_EPOCH,
                summaries: vec![summary.clone()],
            },
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProcessedRangesRecorded {
                ranges: vec![ProcessedRange {
                    session_id: "session-1".to_string(),
                    first_turn_index: 2,
                    last_turn_index: 7,
                    kind: ProcessedRangeKind::LiveBatch,
                    at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
                }],
            },
        );

        let aged = state
            .last_aged_co_retrieval
            .as_ref()
            .expect("aged co-retrieval record");
        assert_eq!(aged.exchange_range.first_index, 2);
        assert_eq!(aged.new_associations, 4);
        assert_eq!(aged.strengthened_associations, 2);
        assert_eq!(aged.summaries, vec![summary]);
        assert_eq!(state.processed_ranges.len(), 1);
    }

    #[test]
    fn memory_context_ignores_mismatched_exchange_updates() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                1,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::MemoryContextRecorded {
                exchange_index: 1,
                context_assembly: ContextAssembly {
                    budget: ContextBudget::new(4, 100),
                    selected: vec![],
                    omitted: vec![],
                    used_estimated_tokens: 0,
                },
                retrieved_memory_block: "memory-a".to_string(),
                recalled_items: vec![],
                live_capture: Some(LiveCaptureContext {
                    source_exchange_index: Some(1),
                    previous_user_input: Some("prev-a".to_string()),
                    previous_assistant_response: Some("resp-a".to_string()),
                }),
            },
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::MemoryContextRecorded {
                exchange_index: 99,
                context_assembly: ContextAssembly {
                    budget: ContextBudget::new(4, 100),
                    selected: vec![],
                    omitted: vec![],
                    used_estimated_tokens: 0,
                },
                retrieved_memory_block: "memory-b".to_string(),
                recalled_items: vec![],
                live_capture: Some(LiveCaptureContext {
                    source_exchange_index: Some(99),
                    previous_user_input: Some("prev-b".to_string()),
                    previous_assistant_response: Some("resp-b".to_string()),
                }),
            },
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.retrieved_memory_block, "memory-a");
        assert_eq!(
            state
                .live_capture
                .as_ref()
                .and_then(|capture| capture.previous_user_input.as_deref()),
            Some("prev-a")
        );
    }

    #[test]
    fn model_role_completion_records_exchange_use() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                2,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ModelRoleCompleted(ExchangeModelUse {
                provider_name: Some("mock".to_string()),
                model_id: "mock-model".to_string(),
                latency_ms: 11,
                input_tokens: 7,
                cached_input_tokens: 2,
                output_tokens: 3,
                full_request_hash: crate::conversation::ContentHash([2; 32]),
                message_count: 5,
            }),
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(
            exchange.model.as_ref().map(|model| model.model_id.as_str()),
            Some("mock-model")
        );
    }

    #[test]
    fn model_role_failure_clears_active_exchange() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                3,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );

        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ModelRoleFailed {
                error_summary: "network failed".to_string(),
            },
        );

        assert_eq!(state.runtime_phase, RuntimePhase::Idle);
        assert!(state.active_exchange.is_none());
        assert!(state.completed_exchanges.is_empty());
    }

    #[test]
    fn interrupted_response_cleanup_clears_volatile_state() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                4,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-4".to_string()),
                text: "working".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("mock".to_string()),
                target: Some("speech".to_string()),
                audio_marker: Some("marker".to_string()),
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::UserInterrupted(InterruptionRecord {
                exchange_index: 4,
                response_id: Some("response-4".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "user-speech".to_string(),
                action: InterruptionAction::Stop,
                stop_outcome: InterruptionStopOutcome::Stopped,
                partial_response_text: Some("working".to_string()),
            }),
        );

        assert_eq!(state.runtime_phase, RuntimePhase::Idle);
        assert!(state.active_response.is_none());
        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.status, ExchangeStatus::Interrupted);
        assert_eq!(exchange.interruptions.len(), 1);
    }

    #[test]
    fn interruption_ignore_only_updates_matching_active_response() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                4,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-4".to_string()),
                text: "working".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("mock".to_string()),
                target: Some("speech".to_string()),
                audio_marker: Some("marker".to_string()),
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::UserInterrupted(InterruptionRecord {
                exchange_index: 99,
                response_id: Some("response-4".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "user-speech".to_string(),
                action: InterruptionAction::Ignore,
                stop_outcome: InterruptionStopOutcome::Ignored,
                partial_response_text: Some("working".to_string()),
            }),
        );

        assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
        assert_eq!(
            state.active_response.as_ref().unwrap().partial_text,
            "working"
        );
        assert_eq!(
            state.active_exchange.as_ref().unwrap().interruptions.len(),
            0
        );
    }

    #[test]
    fn transcript_completion_after_response_start_keeps_user_input_and_response_state() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                10,
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                10,
                ProviderEventKind::ResponseStarted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-10"),
                Some("thinking"),
                None,
                None,
            )),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::AudioFinalTranscriptCommitted {
                exchange_index: 10,
                utterance: UtteranceRecord {
                    utterance_id: "utterance-10".to_string(),
                    revision_index: 1,
                    transcript: "hello".to_string(),
                    received_at: SystemTime::UNIX_EPOCH,
                    provider_id: Some("provider".to_string()),
                    source_chunk_index: Some(1),
                },
                final_transcript: "hello there".to_string(),
            },
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.final_user_input(), "hello there");
        assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
        assert_eq!(
            state
                .active_response
                .as_ref()
                .and_then(|response| response.response_id.as_deref()),
            Some("response-10")
        );
        assert_eq!(
            state
                .active_response
                .as_ref()
                .map(|response| response.status),
            Some(ResponseStatus::Starting)
        );
    }

    #[test]
    fn duplicate_provider_events_keep_the_active_response_stable() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                11,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );

        for _ in 0..2 {
            reduce_live_session_in_place(
                &mut state,
                LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                    11,
                    ProviderEventKind::ResponseStarted,
                    "provider",
                    SystemTime::UNIX_EPOCH,
                    Some("response-11"),
                    Some("working"),
                    None,
                    None,
                )),
            );
        }
        for _ in 0..2 {
            reduce_live_session_in_place(
                &mut state,
                LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                    11,
                    ProviderEventKind::ResponseCompleted,
                    "provider",
                    SystemTime::UNIX_EPOCH,
                    Some("response-11"),
                    Some("done"),
                    Some("completed"),
                    None,
                )),
            );
        }

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.provider_events.len(), 4);
        assert_eq!(
            state
                .active_response
                .as_ref()
                .and_then(|response| response.response_id.as_deref()),
            Some("response-11")
        );
        assert_eq!(
            state
                .active_response
                .as_ref()
                .map(|response| response.status),
            Some(ResponseStatus::Completed)
        );
        assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
    }

    #[test]
    fn interruption_before_response_created_suppresses_late_output() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                12,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::UserInterrupted(InterruptionRecord {
                exchange_index: 12,
                response_id: Some("response-12".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "user-speech".to_string(),
                action: InterruptionAction::Stop,
                stop_outcome: InterruptionStopOutcome::Stopped,
                partial_response_text: None,
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-12".to_string()),
                text: "late output".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("provider".to_string()),
                target: Some("speech".to_string()),
                audio_marker: Some("marker".to_string()),
            }),
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.status, ExchangeStatus::Interrupted);
        assert!(state.active_response.is_none());
        assert!(state.completed_exchanges.is_empty());
    }

    #[test]
    fn response_completion_after_interruption_is_ignored() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
                13,
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                13,
                ProviderEventKind::ResponseStarted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-13"),
                Some("working"),
                None,
                None,
            )),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::UserInterrupted(InterruptionRecord {
                exchange_index: 13,
                response_id: Some("response-13".to_string()),
                detected_at: SystemTime::UNIX_EPOCH,
                source: "user-speech".to_string(),
                action: InterruptionAction::Stop,
                stop_outcome: InterruptionStopOutcome::Stopped,
                partial_response_text: Some("working".to_string()),
            }),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                13,
                ProviderEventKind::ResponseCompleted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-13"),
                Some("done"),
                Some("completed"),
                None,
            )),
        );

        let exchange = state.active_exchange.as_ref().expect("active exchange");
        assert_eq!(exchange.status, ExchangeStatus::Interrupted);
        assert!(state.active_response.is_none());
        assert_eq!(exchange.provider_events.len(), 2);
        assert_eq!(
            exchange.provider_events[0].event_kind,
            ProviderEventKind::ResponseStarted
        );
        assert_eq!(
            exchange.provider_events[1].event_kind,
            ProviderEventKind::ResponseCompleted
        );
    }

    #[test]
    fn second_user_turn_finalizes_previous_streaming_exchange_before_opening_next_turn() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                14,
                "first",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                14,
                ProviderEventKind::ResponseStarted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-14"),
                Some("thinking"),
                None,
                None,
            )),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                15,
                "second",
                SystemTime::UNIX_EPOCH,
            ))),
        );

        assert_eq!(state.completed_exchanges.len(), 1);
        assert_eq!(state.completed_exchanges[0].index, 14);
        assert_eq!(
            state.completed_exchanges[0].status,
            ExchangeStatus::Interrupted
        );
        assert_eq!(
            state
                .active_exchange
                .as_ref()
                .map(|exchange| exchange.index),
            Some(15)
        );
        assert!(state.active_response.is_none());
        assert_eq!(state.runtime_phase, RuntimePhase::Thinking);

        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-14".to_string()),
                text: "stale output".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("provider".to_string()),
                target: Some("text".to_string()),
                audio_marker: None,
            }),
        );

        assert!(state.active_exchange.as_ref().unwrap().output.is_none());
    }

    #[test]
    fn out_of_order_response_completion_before_start_remains_completed() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_text(
                16,
                "hello",
                SystemTime::UNIX_EPOCH,
            ))),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                16,
                ProviderEventKind::ResponseCompleted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-16"),
                Some("done"),
                Some("completed"),
                None,
            )),
        );
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProviderEventRecorded(provider_event_record(
                16,
                ProviderEventKind::ResponseStarted,
                "provider",
                SystemTime::UNIX_EPOCH,
                Some("response-16"),
                Some("working"),
                None,
                None,
            )),
        );

        assert_eq!(
            state
                .active_response
                .as_ref()
                .map(|response| response.status),
            Some(ResponseStatus::Completed)
        );
        assert_eq!(state.runtime_phase, RuntimePhase::Speaking);
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some("response-16".to_string()),
                text: "done".to_string(),
                produced_at: SystemTime::UNIX_EPOCH,
                provider_name: Some("provider".to_string()),
                target: Some("text".to_string()),
                audio_marker: None,
            }),
        );
        assert_eq!(
            state.active_exchange.as_ref().unwrap().status,
            ExchangeStatus::Speaking
        );
        assert_eq!(
            state
                .active_exchange
                .as_ref()
                .and_then(|exchange| exchange.output.as_ref())
                .and_then(|output| output.response_id.as_deref()),
            Some("response-16")
        );
    }

    #[test]
    fn processed_ranges_are_appended_without_reduction() {
        let mut state = LiveSessionState::default();
        reduce_live_session_in_place(
            &mut state,
            LiveSessionEvent::ProcessedRangesRecorded {
                ranges: vec![
                    ProcessedRange {
                        session_id: "session-1".to_string(),
                        first_turn_index: 0,
                        last_turn_index: 2,
                        kind: ProcessedRangeKind::LiveBatch,
                        at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
                    },
                    ProcessedRange {
                        session_id: "session-1".to_string(),
                        first_turn_index: 4,
                        last_turn_index: 4,
                        kind: ProcessedRangeKind::SessionEnd,
                        at: time::OffsetDateTime::from_unix_timestamp(1).unwrap(),
                    },
                ],
            },
        );

        assert_eq!(state.processed_ranges.len(), 2);
    }

    #[test]
    fn serde_roundtrips() {
        let state = LiveSessionState::default();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: LiveSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }
}
