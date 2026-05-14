use std::fmt;

use serde::{Deserialize, Serialize};

use super::{AudioLatencyMeasurement, AudioLatencyStage, SimulatedAudioSession, total_latency_ms};

pub const SPEECH_OUTPUT_PROVIDER_ENV_VAR: &str = "QSF_SPEECH_OUTPUT_PROVIDER";
pub const SPEECH_OUTPUT_MODEL_ENV_VAR: &str = "QSF_SPEECH_OUTPUT_MODEL";
pub const SPEECH_OUTPUT_VOICE_ENV_VAR: &str = "QSF_SPEECH_OUTPUT_VOICE";
pub const SPEECH_OUTPUT_MODE_ENV_VAR: &str = "QSF_SPEECH_OUTPUT_MODE";
pub const DEFAULT_SPEECH_OUTPUT_MODEL: &str = "simulated-speech-output";
pub const DEFAULT_SPEECH_OUTPUT_VOICE: &str = "marin";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechOutputMode {
    MetadataOnly,
    File,
    Playback,
}

impl SpeechOutputMode {
    pub fn from_value(value: Option<&str>) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "file" => Self::File,
            "playback" => Self::Playback,
            _ => Self::MetadataOnly,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata-only",
            Self::File => "file",
            Self::Playback => "playback",
        }
    }
}

impl fmt::Display for SpeechOutputMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeechOutputRequest {
    pub session_id: String,
    pub text: String,
    pub voice: String,
    pub output_mode: SpeechOutputMode,
    pub model: Option<String>,
}

impl SpeechOutputRequest {
    pub fn from_env(session_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            text: text.into(),
            voice: std::env::var(SPEECH_OUTPUT_VOICE_ENV_VAR)
                .unwrap_or_else(|_| DEFAULT_SPEECH_OUTPUT_VOICE.to_string()),
            output_mode: SpeechOutputMode::from_value(
                std::env::var(SPEECH_OUTPUT_MODE_ENV_VAR).ok().as_deref(),
            ),
            model: Some(
                std::env::var(SPEECH_OUTPUT_MODEL_ENV_VAR)
                    .unwrap_or_else(|_| DEFAULT_SPEECH_OUTPUT_MODEL.to_string()),
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeechOutputSession {
    pub session_id: String,
    pub provider_name: String,
    pub model: Option<String>,
    pub voice: String,
    pub output_mode: SpeechOutputMode,
    pub text_length: usize,
    pub started_at_ms: u64,
    pub first_audio_at_ms: Option<u64>,
    pub completed_at_ms: u64,
    pub audio_output_bytes: usize,
    pub playback_adapter: String,
}

impl SpeechOutputSession {
    pub fn latency_measurements(&self) -> Vec<AudioLatencyMeasurement> {
        let first_audio_at_ms = self.first_audio_at_ms.unwrap_or(self.completed_at_ms);

        vec![
            AudioLatencyMeasurement::new(
                AudioLatencyStage::PlaybackQueue,
                self.started_at_ms,
                first_audio_at_ms,
            ),
            AudioLatencyMeasurement::new(
                AudioLatencyStage::PlaybackSynthesis,
                first_audio_at_ms,
                self.completed_at_ms,
            ),
        ]
    }

    pub fn total_latency_ms(&self) -> u64 {
        total_latency_ms(&self.latency_measurements())
    }
}

pub trait SpeechOutputProvider {
    fn provider_name(&self) -> &str;

    fn synthesize(
        &self,
        request: &SpeechOutputRequest,
    ) -> Result<SpeechOutputSession, SpeechOutputProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeechOutputProviderError {
    Unavailable { provider: String, reason: String },
    SynthesisFailed { provider: String, message: String },
}

impl SpeechOutputProviderError {
    pub fn provider(&self) -> &str {
        match self {
            Self::Unavailable { provider, .. } | Self::SynthesisFailed { provider, .. } => provider,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Unavailable { reason, .. } => reason,
            Self::SynthesisFailed { message, .. } => message,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "provider_unavailable",
            Self::SynthesisFailed { .. } => "synthesis_failed",
        }
    }

    pub fn sanitized_message(&self) -> String {
        super::transcript_provider::sanitize_provider_error_message(self.message())
    }
}

impl fmt::Display for SpeechOutputProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { provider, reason } => {
                write!(
                    formatter,
                    "speech output provider `{provider}` unavailable: {reason}"
                )
            }
            Self::SynthesisFailed { provider, message } => {
                write!(
                    formatter,
                    "speech output provider `{provider}` failed: {message}"
                )
            }
        }
    }
}

impl std::error::Error for SpeechOutputProviderError {}

#[derive(Clone, Debug, Default)]
pub struct SimulatedSpeechOutputProvider;

impl SpeechOutputProvider for SimulatedSpeechOutputProvider {
    fn provider_name(&self) -> &str {
        "simulated-speech-output-provider"
    }

    fn synthesize(
        &self,
        request: &SpeechOutputRequest,
    ) -> Result<SpeechOutputSession, SpeechOutputProviderError> {
        let fixture = SimulatedAudioSession::default();
        let measurements = fixture.playback_latency_measurements();
        let started_at_ms = measurements
            .first()
            .map(|measurement| measurement.started_at_ms)
            .unwrap_or(0);
        let first_audio_at_ms = measurements
            .first()
            .map(|measurement| measurement.completed_at_ms);
        let completed_at_ms = measurements
            .last()
            .map(|measurement| measurement.completed_at_ms)
            .unwrap_or(started_at_ms);

        Ok(SpeechOutputSession {
            session_id: request.session_id.clone(),
            provider_name: self.provider_name().to_string(),
            model: request
                .model
                .clone()
                .or_else(|| Some(DEFAULT_SPEECH_OUTPUT_MODEL.to_string())),
            voice: request.voice.clone(),
            output_mode: request.output_mode,
            text_length: request.text.chars().count(),
            started_at_ms,
            first_audio_at_ms,
            completed_at_ms,
            audio_output_bytes: simulated_audio_output_bytes(&request.text, request.output_mode),
            playback_adapter: fixture.playback_adapter,
        })
    }
}

pub fn requested_speech_output_provider(input: Option<&str>) -> &'static str {
    match input {
        Some(value) if value.eq_ignore_ascii_case("openai") => "openai",
        _ => "simulated",
    }
}

pub fn requested_speech_output_provider_from_env() -> &'static str {
    requested_speech_output_provider(
        std::env::var(SPEECH_OUTPUT_PROVIDER_ENV_VAR)
            .ok()
            .as_deref(),
    )
}

pub fn build_speech_output_provider(provider_name: &str) -> Box<dyn SpeechOutputProvider> {
    match requested_speech_output_provider(Some(provider_name)) {
        "openai" => Box::new(UnavailableSpeechOutputProvider {
            provider_name: "openai-speech-output-provider".to_string(),
            reason:
                "OpenAI speech output is intentionally deferred until the simulated boundary is proven"
                    .to_string(),
        }),
        _ => Box::new(SimulatedSpeechOutputProvider),
    }
}

fn simulated_audio_output_bytes(text: &str, output_mode: SpeechOutputMode) -> usize {
    match output_mode {
        SpeechOutputMode::MetadataOnly => 0,
        SpeechOutputMode::File | SpeechOutputMode::Playback => text.chars().count() * 32,
    }
}

#[derive(Clone, Debug)]
struct UnavailableSpeechOutputProvider {
    provider_name: String,
    reason: String,
}

impl SpeechOutputProvider for UnavailableSpeechOutputProvider {
    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn synthesize(
        &self,
        _request: &SpeechOutputRequest,
    ) -> Result<SpeechOutputSession, SpeechOutputProviderError> {
        Err(SpeechOutputProviderError::Unavailable {
            provider: self.provider_name.clone(),
            reason: self.reason.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SimulatedSpeechOutputProvider, SpeechOutputMode, SpeechOutputProvider, SpeechOutputRequest,
        requested_speech_output_provider,
    };

    #[test]
    fn simulated_speech_output_uses_existing_playback_timing_fixture() {
        let provider = SimulatedSpeechOutputProvider;
        let request = SpeechOutputRequest {
            session_id: "voice-loop-test".to_string(),
            text: "hello".to_string(),
            voice: "marin".to_string(),
            output_mode: SpeechOutputMode::MetadataOnly,
            model: None,
        };

        let session = provider.synthesize(&request).unwrap();

        assert_eq!(session.provider_name, "simulated-speech-output-provider");
        assert_eq!(session.text_length, 5);
        assert_eq!(session.started_at_ms, 120);
        assert_eq!(session.first_audio_at_ms, Some(134));
        assert_eq!(session.completed_at_ms, 260);
        assert_eq!(session.audio_output_bytes, 0);
        assert_eq!(session.total_latency_ms(), 140);
    }

    #[test]
    fn speech_output_provider_selector_defaults_to_simulated() {
        assert_eq!(requested_speech_output_provider(None), "simulated");
        assert_eq!(
            requested_speech_output_provider(Some("unknown")),
            "simulated"
        );
        assert_eq!(requested_speech_output_provider(Some("openai")), "openai");
    }
}
