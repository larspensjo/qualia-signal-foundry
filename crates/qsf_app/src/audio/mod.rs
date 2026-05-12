use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::observability::event_log::EventType;

mod transcript_provider;

pub use transcript_provider::{
    AudioSafetyMarkers, FinalTranscript, OPENAI_REALTIME_TRANSCRIPTION_MODEL,
    OpenAiRealtimeTranscriptProvider, PartialTranscript, SimulatedTranscriptProvider,
    TRANSCRIPT_INPUT_SOURCE_ENV_VAR, TRANSCRIPT_MIC_DEVICE_ENV_VAR,
    TRANSCRIPT_MIC_DURATION_MS_ENV_VAR, TRANSCRIPT_PROVIDER_ENV_VAR, TRANSCRIPT_WAV_PATH_ENV_VAR,
    TranscriptAudioChunk, TranscriptInputSource, TranscriptProvider, TranscriptProviderError,
    TranscriptProviderRequest, TranscriptProviderSession, build_transcript_provider,
    requested_transcript_provider, requested_transcript_provider_from_env,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRuntimeEntryPoint {
    TranscriptProvider,
    RuntimeInput,
    RuntimeOutput,
    SpeechPlaybackAdapter,
}

impl AudioRuntimeEntryPoint {
    pub fn description(self) -> &'static str {
        match self {
            Self::TranscriptProvider => {
                "Realtime transcript providers emit observable audio events before the runtime treats finalized text as input."
            }
            Self::RuntimeInput => {
                "Finalized transcript text re-enters the runtime as a normal InputReceived event."
            }
            Self::RuntimeOutput => {
                "Runtime output stays text-first and only then becomes a speech playback request."
            }
            Self::SpeechPlaybackAdapter => {
                "Playback adapters acknowledge speech lifecycle events without taking ownership of runtime state."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioLatencyStage {
    Capture,
    PartialTranscription,
    FinalTranscription,
    RuntimeInputDispatch,
    PlaybackQueue,
    PlaybackSynthesis,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioLatencyMeasurement {
    pub stage: AudioLatencyStage,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub duration_ms: u64,
}

impl AudioLatencyMeasurement {
    pub fn new(stage: AudioLatencyStage, started_at_ms: u64, completed_at_ms: u64) -> Self {
        let duration_ms = completed_at_ms.saturating_sub(started_at_ms);

        Self {
            stage,
            started_at_ms,
            completed_at_ms,
            duration_ms,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioRuntimeBoundary {
    pub entry_point: AudioRuntimeEntryPoint,
    pub producer_event: EventType,
    pub runtime_event: EventType,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PreparedAudioEvent {
    pub event_type: EventType,
    pub payload: Value,
}

impl PreparedAudioEvent {
    pub fn new(event_type: EventType, payload: Value) -> Self {
        Self {
            event_type,
            payload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SimulatedAudioSession {
    pub session_id: String,
    pub input_device: String,
    pub transcript_provider: String,
    pub playback_adapter: String,
    pub captured_chunk_count: u32,
    pub partial_transcript: String,
    pub final_transcript: String,
    pub response_text: String,
}

impl Default for SimulatedAudioSession {
    fn default() -> Self {
        Self {
            session_id: "audio-session-phase-8".to_string(),
            input_device: "simulated-microphone".to_string(),
            transcript_provider: "simulated-transcript-provider".to_string(),
            playback_adapter: "simulated-speech-playback".to_string(),
            captured_chunk_count: 1,
            partial_transcript: "what context should we preserve for transcript-first audio".to_string(),
            final_transcript: "What context should we preserve for transcript-first audio input?".to_string(),
            response_text: "Carry forward the finalized transcript, the traceable latency record, and the explicit runtime boundary.".to_string(),
        }
    }
}

impl SimulatedAudioSession {
    pub fn transcription_boundary(&self) -> AudioRuntimeBoundary {
        AudioRuntimeBoundary {
            entry_point: AudioRuntimeEntryPoint::TranscriptProvider,
            producer_event: EventType::AudioFinalTranscript,
            runtime_event: EventType::InputReceived,
            description: "Final transcript text becomes normal runtime input only after the provider emits AudioFinalTranscript.".to_string(),
        }
    }

    pub fn playback_boundary(&self) -> AudioRuntimeBoundary {
        AudioRuntimeBoundary {
            entry_point: AudioRuntimeEntryPoint::RuntimeOutput,
            producer_event: EventType::OutputProduced,
            runtime_event: EventType::SpeechPlaybackRequested,
            description: "Runtime output stays inspectable as text and only then requests speech playback from an adapter.".to_string(),
        }
    }

    pub fn transcription_latency_measurements(&self) -> Vec<AudioLatencyMeasurement> {
        vec![
            AudioLatencyMeasurement::new(AudioLatencyStage::Capture, 0, 14),
            AudioLatencyMeasurement::new(AudioLatencyStage::PartialTranscription, 14, 46),
            AudioLatencyMeasurement::new(AudioLatencyStage::FinalTranscription, 46, 82),
            AudioLatencyMeasurement::new(AudioLatencyStage::RuntimeInputDispatch, 82, 88),
        ]
    }

    pub fn playback_latency_measurements(&self) -> Vec<AudioLatencyMeasurement> {
        vec![
            AudioLatencyMeasurement::new(AudioLatencyStage::PlaybackQueue, 120, 134),
            AudioLatencyMeasurement::new(AudioLatencyStage::PlaybackSynthesis, 134, 260),
        ]
    }

    pub fn input_observation_events(&self) -> Vec<PreparedAudioEvent> {
        vec![
            PreparedAudioEvent::new(
                EventType::AudioInputStarted,
                json!({
                    "session_id": self.session_id,
                    "input_device": self.input_device,
                    "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::AudioInputChunkCaptured,
                json!({
                    "session_id": self.session_id,
                    "chunk_index": 0,
                    "captured_chunk_count": self.captured_chunk_count,
                    "captured_at_ms": 0,
                    "duration_ms": 14,
                    "provider": self.transcript_provider,
                    "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::AudioPartialTranscript,
                json!({
                    "session_id": self.session_id,
                    "provider": self.transcript_provider,
                    "chunk_index": 0,
                    "transcript": self.partial_transcript,
                    "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
        ]
    }

    pub fn runtime_input_events(&self) -> Vec<PreparedAudioEvent> {
        let boundary = self.transcription_boundary();

        vec![
            PreparedAudioEvent::new(
                EventType::AudioFinalTranscript,
                json!({
                    "session_id": self.session_id,
                    "provider": self.transcript_provider,
                    "utterance_index": 0,
                    "transcript": self.final_transcript,
                    "boundary": boundary,
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::AudioInputEnded,
                json!({
                    "session_id": self.session_id,
                    "provider": self.transcript_provider,
                    "captured_chunk_count": self.captured_chunk_count,
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::LatencyMeasurementRecorded,
                // Keep stage timings in the event stream as chronological facts; the linked
                // trace carries the explanatory boundary context for the same measurements.
                json!({
                    "session_id": self.session_id,
                    "domain": "audio",
                    "stage": "transcription",
                    "measurements": self.transcription_latency_measurements(),
                    "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::InputReceived,
                json!({
                    "session_id": self.session_id,
                    "source_event": EventType::AudioFinalTranscript,
                    "input_text": self.final_transcript,
                    "entry_point": AudioRuntimeEntryPoint::RuntimeInput,
                    "entry_point_description": AudioRuntimeEntryPoint::RuntimeInput.description(),
                }),
            ),
        ]
    }

    pub fn playback_events(&self) -> Vec<PreparedAudioEvent> {
        let boundary = self.playback_boundary();

        vec![
            PreparedAudioEvent::new(
                EventType::OutputProduced,
                json!({
                    "session_id": self.session_id,
                    "message": self.response_text,
                    "target": "speech-playback",
                    "boundary": boundary,
                }),
            ),
            PreparedAudioEvent::new(
                EventType::SpeechPlaybackRequested,
                // Phase 8 uses this as a boundary marker only; later phases can route it to a
                // real playback adapter without changing the observable event shape.
                json!({
                    "session_id": self.session_id,
                    "adapter": self.playback_adapter,
                    "text": self.response_text,
                    "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
                    "entry_point_description": AudioRuntimeEntryPoint::RuntimeOutput.description(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::LatencyMeasurementRecorded,
                // Events record that latency measurements were produced; traces retain the
                // surrounding explanation of why this playback boundary exists.
                json!({
                    "session_id": self.session_id,
                    "domain": "audio",
                    "stage": "playback",
                    "measurements": self.playback_latency_measurements(),
                }),
            ),
            PreparedAudioEvent::new(
                EventType::SpeechPlaybackStarted,
                json!({
                    "session_id": self.session_id,
                    "adapter": self.playback_adapter,
                    "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
                }),
            ),
            PreparedAudioEvent::new(
                EventType::SpeechPlaybackCompleted,
                json!({
                    "session_id": self.session_id,
                    "adapter": self.playback_adapter,
                    "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
                }),
            ),
        ]
    }

    pub fn transcription_total_latency_ms(&self) -> u64 {
        total_latency_ms(&self.transcription_latency_measurements())
    }

    pub fn playback_total_latency_ms(&self) -> u64 {
        total_latency_ms(&self.playback_latency_measurements())
    }
}

pub fn total_latency_ms(measurements: &[AudioLatencyMeasurement]) -> u64 {
    match (measurements.first(), measurements.last()) {
        (Some(first), Some(last)) => last.completed_at_ms.saturating_sub(first.started_at_ms),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioRuntimeBoundary, AudioRuntimeEntryPoint, EventType, SimulatedAudioSession,
        total_latency_ms,
    };

    #[test]
    fn simulated_session_routes_final_transcript_back_to_runtime_input() {
        let session = SimulatedAudioSession::default();
        let runtime_events = session.runtime_input_events();

        assert!(
            runtime_events
                .iter()
                .any(|event| event.event_type == EventType::AudioFinalTranscript)
        );
        assert!(
            runtime_events
                .iter()
                .any(|event| event.event_type == EventType::InputReceived)
        );
        assert_eq!(
            runtime_events
                .iter()
                .find(|event| event.event_type == EventType::InputReceived)
                .unwrap()
                .payload["entry_point"],
            serde_json::json!(AudioRuntimeEntryPoint::RuntimeInput)
        );
    }

    #[test]
    fn simulated_session_keeps_output_text_visible_before_playback() {
        let session = SimulatedAudioSession::default();
        let playback_events = session.playback_events();

        let output_event = playback_events
            .iter()
            .find(|event| event.event_type == EventType::OutputProduced)
            .unwrap();
        let request_event = playback_events
            .iter()
            .find(|event| event.event_type == EventType::SpeechPlaybackRequested)
            .unwrap();

        assert_eq!(
            output_event.payload["message"],
            request_event.payload["text"]
        );
    }

    #[test]
    fn total_latency_uses_first_start_and_last_completion() {
        let session = SimulatedAudioSession::default();

        assert_eq!(
            total_latency_ms(&session.transcription_latency_measurements()),
            88
        );
        assert_eq!(
            total_latency_ms(&session.playback_latency_measurements()),
            140
        );
    }

    #[test]
    fn audio_runtime_boundary_round_trips_through_json() {
        let boundary = AudioRuntimeBoundary {
            entry_point: AudioRuntimeEntryPoint::TranscriptProvider,
            producer_event: EventType::AudioFinalTranscript,
            runtime_event: EventType::InputReceived,
            description: "Final transcript becomes normal runtime input.".to_string(),
        };

        let serialized = serde_json::to_string(&boundary).unwrap();
        let deserialized: AudioRuntimeBoundary = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized, boundary);
    }
}
