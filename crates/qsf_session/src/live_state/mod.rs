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
mod tests;
