use std::fmt;

use serde::{Deserialize, Serialize};

use super::{AudioLatencyMeasurement, AudioLatencyStage, total_latency_ms};

pub const OPENAI_REALTIME_TRANSCRIPTION_MODEL: &str = "gpt-realtime-whisper";
pub const TRANSCRIPT_PROVIDER_ENV_VAR: &str = "QSF_TRANSCRIPT_PROVIDER";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioSafetyMarkers {
    pub raw_audio_logged: bool,
    pub authorization_logged: bool,
    pub api_key_logged: bool,
}

impl AudioSafetyMarkers {
    pub const fn no_secret_or_raw_audio() -> Self {
        Self {
            raw_audio_logged: false,
            authorization_logged: false,
            api_key_logged: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptProviderRequest {
    pub session_id: String,
    pub input_source: TranscriptInputSource,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

impl TranscriptProviderRequest {
    pub fn simulated(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            input_source: TranscriptInputSource::Simulated {
                label: "deterministic-simulated-audio".to_string(),
                chunk_count: SIMULATED_TRANSCRIPT_CHUNK_COUNT,
            },
            language: Some("en".to_string()),
            prompt: Some("Expect project vocabulary such as Qualia Signal Foundry, transcript-first audio, and runtime events.".to_string()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptInputSource {
    Simulated { label: String, chunk_count: u32 },
    PrerecordedFile { path: String },
    LiveMicrophone { device: String },
}

impl TranscriptInputSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Simulated { label, .. } => label,
            Self::PrerecordedFile { path } => path,
            Self::LiveMicrophone { device } => device,
        }
    }

    pub fn stores_raw_audio(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptAudioChunk {
    pub chunk_index: u32,
    pub captured_at_ms: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PartialTranscript {
    pub utterance_index: u32,
    pub revision_index: u32,
    pub source_chunk_index: u32,
    pub received_at_ms: u64,
    pub transcript: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FinalTranscript {
    pub utterance_index: u32,
    pub received_at_ms: u64,
    pub transcript: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptProviderSession {
    pub session_id: String,
    pub provider_name: String,
    pub model: Option<String>,
    pub input_source: TranscriptInputSource,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub chunks: Vec<TranscriptAudioChunk>,
    pub partials: Vec<PartialTranscript>,
    pub final_transcript: FinalTranscript,
}

impl TranscriptProviderSession {
    pub fn partial_revision_count(&self) -> usize {
        self.partials.len()
    }

    pub fn final_transcript_text_length(&self) -> usize {
        self.final_transcript.transcript.chars().count()
    }

    pub fn first_partial_latency_ms(&self) -> Option<u64> {
        self.partials
            .first()
            .map(|partial| partial.received_at_ms.saturating_sub(self.started_at_ms))
    }

    pub fn final_transcript_latency_ms(&self) -> u64 {
        self.final_transcript
            .received_at_ms
            .saturating_sub(self.started_at_ms)
    }

    pub fn latency_measurements(&self) -> Vec<AudioLatencyMeasurement> {
        let mut measurements = Vec::new();

        if let Some(last_chunk) = self.chunks.last() {
            measurements.push(AudioLatencyMeasurement::new(
                AudioLatencyStage::Capture,
                self.started_at_ms,
                last_chunk.captured_at_ms + last_chunk.duration_ms,
            ));
        }

        if let Some(first_partial) = self.partials.first() {
            measurements.push(AudioLatencyMeasurement::new(
                AudioLatencyStage::PartialTranscription,
                self.started_at_ms,
                first_partial.received_at_ms,
            ));
        }

        measurements.push(AudioLatencyMeasurement::new(
            AudioLatencyStage::FinalTranscription,
            self.started_at_ms,
            self.final_transcript.received_at_ms,
        ));

        measurements.push(AudioLatencyMeasurement::new(
            AudioLatencyStage::RuntimeInputDispatch,
            self.final_transcript.received_at_ms,
            self.completed_at_ms,
        ));

        measurements
    }

    pub fn total_latency_ms(&self) -> u64 {
        total_latency_ms(&self.latency_measurements())
    }
}

pub trait TranscriptProvider {
    fn provider_name(&self) -> &str;

    fn transcribe(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptProviderError {
    Unavailable { provider: String, reason: String },
    TranscriptionFailed { provider: String, message: String },
}

impl TranscriptProviderError {
    pub fn provider(&self) -> &str {
        match self {
            Self::Unavailable { provider, .. } | Self::TranscriptionFailed { provider, .. } => {
                provider
            }
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Unavailable { reason, .. } => reason,
            Self::TranscriptionFailed { message, .. } => message,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "provider_unavailable",
            Self::TranscriptionFailed { .. } => "transcription_failed",
        }
    }

    pub fn sanitized_message(&self) -> String {
        sanitize_provider_error_message(self.message())
    }
}

impl fmt::Display for TranscriptProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { provider, reason } => {
                write!(
                    formatter,
                    "transcript provider `{provider}` unavailable: {reason}"
                )
            }
            Self::TranscriptionFailed { provider, message } => {
                write!(
                    formatter,
                    "transcript provider `{provider}` failed: {message}"
                )
            }
        }
    }
}

impl std::error::Error for TranscriptProviderError {}

#[derive(Clone, Debug, Default)]
pub struct SimulatedTranscriptProvider;

const SIMULATED_TRANSCRIPT_CHUNK_COUNT: u32 = 3;

impl TranscriptProvider for SimulatedTranscriptProvider {
    fn provider_name(&self) -> &str {
        "simulated-transcript-provider"
    }

    fn transcribe(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
        Ok(TranscriptProviderSession {
            session_id: request.session_id.clone(),
            provider_name: self.provider_name().to_string(),
            model: None,
            input_source: request.input_source.clone(),
            started_at_ms: 0,
            completed_at_ms: 92,
            chunks: simulated_chunks(&request.input_source),
            partials: vec![
                PartialTranscript {
                    utterance_index: 0,
                    revision_index: 0,
                    source_chunk_index: 0,
                    received_at_ms: 34,
                    transcript: "streaming transcription".to_string(),
                },
                PartialTranscript {
                    utterance_index: 0,
                    revision_index: 1,
                    source_chunk_index: 1,
                    received_at_ms: 58,
                    transcript: "streaming transcription should enter".to_string(),
                },
                PartialTranscript {
                    utterance_index: 0,
                    revision_index: 2,
                    source_chunk_index: 2,
                    received_at_ms: 74,
                    transcript: "streaming transcription should enter runtime as events"
                        .to_string(),
                },
            ],
            final_transcript: FinalTranscript {
                utterance_index: 0,
                received_at_ms: 86,
                transcript: "Streaming transcription should enter the runtime as events."
                    .to_string(),
            },
        })
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiRealtimeTranscriptProvider {
    model: String,
}

impl Default for OpenAiRealtimeTranscriptProvider {
    fn default() -> Self {
        Self {
            model: OPENAI_REALTIME_TRANSCRIPTION_MODEL.to_string(),
        }
    }
}

impl OpenAiRealtimeTranscriptProvider {
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl TranscriptProvider for OpenAiRealtimeTranscriptProvider {
    fn provider_name(&self) -> &str {
        "openai-realtime-transcript-provider"
    }

    fn transcribe(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
        let input_source = request.input_source.label();
        Err(TranscriptProviderError::Unavailable {
            provider: self.provider_name().to_string(),
            reason: format!(
                "adapter target `{}` is defined for input source `{input_source}`, but realtime WebSocket audio streaming is not implemented yet",
                self.model,
            ),
        })
    }
}

pub fn requested_transcript_provider(input: Option<&str>) -> &'static str {
    match input {
        Some(value)
            if value.eq_ignore_ascii_case("openai")
                || value.eq_ignore_ascii_case("openai-realtime") =>
        {
            "openai-realtime"
        }
        _ => "simulated",
    }
}

pub fn requested_transcript_provider_from_env() -> &'static str {
    requested_transcript_provider(std::env::var(TRANSCRIPT_PROVIDER_ENV_VAR).ok().as_deref())
}

pub fn build_transcript_provider(provider_name: &str) -> Box<dyn TranscriptProvider> {
    match requested_transcript_provider(Some(provider_name)) {
        "openai-realtime" => Box::new(OpenAiRealtimeTranscriptProvider::default()),
        _ => Box::new(SimulatedTranscriptProvider),
    }
}

fn simulated_chunks(input_source: &TranscriptInputSource) -> Vec<TranscriptAudioChunk> {
    let chunk_count = match input_source {
        TranscriptInputSource::Simulated { chunk_count, .. } => *chunk_count,
        _ => SIMULATED_TRANSCRIPT_CHUNK_COUNT,
    };

    (0..chunk_count)
        .map(|chunk_index| TranscriptAudioChunk {
            chunk_index,
            captured_at_ms: u64::from(chunk_index) * 20,
            duration_ms: 20,
        })
        .collect()
}

fn sanitize_provider_error_message(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("authorization")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("apikey")
    {
        return "provider error redacted because it may contain credential-like content"
            .to_string();
    }

    const MAX_SANITIZED_MESSAGE_CHARS: usize = 240;
    let mut chars = message.chars();
    let truncated: String = chars.by_ref().take(MAX_SANITIZED_MESSAGE_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OPENAI_REALTIME_TRANSCRIPTION_MODEL, OpenAiRealtimeTranscriptProvider,
        SimulatedTranscriptProvider, TranscriptInputSource, TranscriptProvider,
        TranscriptProviderError, TranscriptProviderRequest, requested_transcript_provider,
    };

    #[test]
    fn simulated_provider_emits_partial_and_final_transcripts() {
        let provider = SimulatedTranscriptProvider;
        let request = TranscriptProviderRequest::simulated("session-test");
        let session = provider.transcribe(&request).unwrap();

        assert_eq!(session.provider_name, "simulated-transcript-provider");
        assert_eq!(session.partials.len(), 3);
        assert_eq!(
            session.final_transcript.transcript,
            "Streaming transcription should enter the runtime as events."
        );
        assert_eq!(session.first_partial_latency_ms(), Some(34));
        assert_eq!(session.final_transcript_latency_ms(), 86);
        assert_eq!(session.total_latency_ms(), 92);
        assert!(!session.input_source.stores_raw_audio());
    }

    #[test]
    fn simulated_provider_uses_requested_chunk_count() {
        let provider = SimulatedTranscriptProvider;
        let mut request = TranscriptProviderRequest::simulated("session-test");
        request.input_source = TranscriptInputSource::Simulated {
            label: "short-simulated-audio".to_string(),
            chunk_count: 2,
        };
        let session = provider.transcribe(&request).unwrap();

        assert_eq!(session.chunks.len(), 2);
        assert_eq!(session.chunks[1].chunk_index, 1);
    }

    #[test]
    fn openai_realtime_provider_names_current_target_model() {
        let provider = OpenAiRealtimeTranscriptProvider::default();
        let request = TranscriptProviderRequest::simulated("session-test");
        let error = provider.transcribe(&request).unwrap_err();

        assert_eq!(provider.model(), OPENAI_REALTIME_TRANSCRIPTION_MODEL);
        assert!(matches!(error, TranscriptProviderError::Unavailable { .. }));
        assert!(
            error
                .message()
                .contains(OPENAI_REALTIME_TRANSCRIPTION_MODEL)
        );
    }

    #[test]
    fn transcript_provider_selector_defaults_to_simulated() {
        assert_eq!(requested_transcript_provider(None), "simulated");
        assert_eq!(requested_transcript_provider(Some("unknown")), "simulated");
        assert_eq!(
            requested_transcript_provider(Some("openai")),
            "openai-realtime"
        );
        assert_eq!(
            requested_transcript_provider(Some("openai-realtime")),
            "openai-realtime"
        );
    }

    #[test]
    fn provider_errors_redact_credential_like_messages() {
        let error = TranscriptProviderError::TranscriptionFailed {
            provider: "test-provider".to_string(),
            message: "upstream returned Authorization: Bearer sk-secret".to_string(),
        };

        assert_eq!(error.category(), "transcription_failed");
        assert!(!error.sanitized_message().contains("sk-secret"));
        assert!(!error.sanitized_message().contains("Bearer"));
    }
}
