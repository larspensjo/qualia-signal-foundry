use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::ContextAssembly;
use crate::memory::processed_ranges::ProcessedRange;

use super::{
    SessionEndReason, TurnSummary,
    exchange::{
        Exchange, ExchangeInput, ExchangeOutput, ExchangeRange, ExchangeStatus, InterruptionRecord,
        InterruptionStopOutcome, UtteranceRecord,
    },
};

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
    #[serde(default)]
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
        recalled_items: Vec<super::RecallRecord>,
        live_capture: Option<LiveCaptureContext>,
    },
    OutputProduced(ExchangeOutput),
    UserInterrupted(InterruptionRecord),
    ExchangeCompleted {
        completed_at: SystemTime,
    },
    ProcessedRangesRecorded {
        ranges: Vec<ProcessedRange>,
    },
    SessionEnded {
        reason: SessionEndReason,
    },
}

#[cfg_attr(not(test), allow(dead_code))]
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
        }
        LiveSessionEvent::SessionResumed => {
            state.prepare_for_awake_continuation();
        }
        LiveSessionEvent::ExchangeStarted(exchange) => {
            let mut exchange = *exchange;
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
                    exchange.status = ExchangeStatus::AwaitingModel;
                }
            }
            state.runtime_phase = RuntimePhase::Thinking;
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
        LiveSessionEvent::OutputProduced(output) => {
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
        }
        LiveSessionEvent::UserInterrupted(interruption) => {
            let stop_outcome = interruption.stop_outcome;
            let action = interruption.action;
            let mut handled = false;
            if let Some(exchange) = state.active_exchange.as_mut() {
                if exchange.index == interruption.exchange_index {
                    handled = true;
                    exchange.interruptions.push(interruption);
                    if !matches!(stop_outcome, InterruptionStopOutcome::Ignored) {
                        exchange.status = ExchangeStatus::Interrupted;
                        state.active_response = None;
                        state.runtime_phase = RuntimePhase::Idle;
                    }
                }
            }
            if handled
                && matches!(action, super::exchange::InterruptionAction::Ignore)
                && matches!(stop_outcome, InterruptionStopOutcome::Ignored)
                && state.active_response.is_some()
            {
                state.runtime_phase = RuntimePhase::Speaking;
            }
        }
        LiveSessionEvent::ExchangeCompleted { completed_at } => {
            if let Some(mut exchange) = state.active_exchange.take() {
                exchange.completed_at = Some(completed_at);
                exchange.status = ExchangeStatus::Completed;
                state.completed_exchanges.push(exchange);
            }
            state.active_response = None;
            state.partial_transcript = None;
            state.live_capture = None;
            state.runtime_phase = RuntimePhase::Idle;
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
    use crate::memory::processed_ranges::{ProcessedRange, ProcessedRangeKind};
    use crate::session::TurnSummary;
    use crate::session::exchange::{
        Exchange, ExchangeInput, ExchangeOutput, ExchangeRange, ExchangeStatus, InterruptionAction,
        InterruptionRecord, InterruptionStopOutcome, UtteranceRecord,
    };

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
