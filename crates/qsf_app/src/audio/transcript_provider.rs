use std::fmt;
#[cfg(feature = "openai")]
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{AudioLatencyMeasurement, AudioLatencyStage, total_latency_ms};

pub const OPENAI_REALTIME_TRANSCRIPTION_MODEL: &str = "gpt-realtime-whisper";
pub const TRANSCRIPT_PROVIDER_ENV_VAR: &str = "QSF_TRANSCRIPT_PROVIDER";
pub const TRANSCRIPT_INPUT_SOURCE_ENV_VAR: &str = "QSF_TRANSCRIPT_INPUT_SOURCE";
pub const TRANSCRIPT_WAV_PATH_ENV_VAR: &str = "QSF_TRANSCRIPT_WAV_PATH";
pub const TRANSCRIPT_MIC_DEVICE_ENV_VAR: &str = "QSF_TRANSCRIPT_MIC_DEVICE";
pub const TRANSCRIPT_MIC_DURATION_MS_ENV_VAR: &str = "QSF_TRANSCRIPT_MIC_DURATION_MS";
#[cfg(feature = "openai")]
pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
#[cfg(feature = "openai")]
pub const OPENAI_REALTIME_TIMEOUT_MS_ENV_VAR: &str = "QSF_OPENAI_REALTIME_TIMEOUT_MS";

#[cfg(feature = "openai")]
const OPENAI_REALTIME_WEBSOCKET_URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";
#[cfg(feature = "openai")]
const OPENAI_REALTIME_PCM_RATE_HZ: u32 = 24_000;
#[cfg(feature = "openai")]
const OPENAI_REALTIME_PCM_CHANNELS: u16 = 1;
#[cfg(feature = "openai")]
const OPENAI_REALTIME_CHUNK_MS: u64 = 100;
const DEFAULT_LIVE_MICROPHONE_DURATION_MS: u64 = 4_000;
#[cfg(feature = "openai")]
const DEFAULT_OPENAI_REALTIME_TIMEOUT_MS: u64 = 30_000;

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
            input_source: default_simulated_input_source(),
            language: Some("en".to_string()),
            prompt: Some("Expect project vocabulary such as Qualia Signal Foundry, transcript-first audio, and runtime events.".to_string()),
        }
    }

    pub fn from_env(session_id: impl Into<String>) -> Self {
        let mut request = Self::simulated(session_id);
        request.input_source = transcript_input_source_from_values(
            std::env::var(TRANSCRIPT_INPUT_SOURCE_ENV_VAR)
                .ok()
                .as_deref(),
            std::env::var(TRANSCRIPT_WAV_PATH_ENV_VAR).ok().as_deref(),
            std::env::var(TRANSCRIPT_MIC_DEVICE_ENV_VAR).ok().as_deref(),
            std::env::var(TRANSCRIPT_MIC_DURATION_MS_ENV_VAR)
                .ok()
                .as_deref(),
        );

        request
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptInputSource {
    Simulated { label: String, chunk_count: u32 },
    PrerecordedFile { path: String },
    LiveMicrophone { device: String, duration_ms: u64 },
}

impl TranscriptInputSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Simulated { label, .. } => label,
            Self::PrerecordedFile { path } => path,
            Self::LiveMicrophone { device, .. } => device,
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

    #[cfg(feature = "openai")]
    fn transcribe_realtime(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
        let api_key = std::env::var(OPENAI_API_KEY_ENV_VAR).map_err(|_| {
            TranscriptProviderError::Unavailable {
                provider: self.provider_name().to_string(),
                reason: format!(
                    "{OPENAI_API_KEY_ENV_VAR} is required for the OpenAI realtime transcript provider"
                ),
            }
        })?;
        let audio = realtime_audio_from_source(&request.input_source, self.provider_name())?;
        let timeout_ms = read_env_u64(
            OPENAI_REALTIME_TIMEOUT_MS_ENV_VAR,
            DEFAULT_OPENAI_REALTIME_TIMEOUT_MS,
        );

        let runtime = tokio::runtime::Runtime::new().map_err(|error| {
            TranscriptProviderError::Unavailable {
                provider: self.provider_name().to_string(),
                reason: format!(
                    "failed to build Tokio runtime for realtime transcription: {error}"
                ),
            }
        })?;

        runtime.block_on(transcribe_openai_realtime(
            self.provider_name(),
            &self.model,
            &api_key,
            request,
            audio,
            Duration::from_millis(timeout_ms),
        ))
    }

    #[cfg(not(feature = "openai"))]
    fn transcribe_realtime(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
        Err(TranscriptProviderError::Unavailable {
            provider: self.provider_name().to_string(),
            reason: format!(
                "adapter target `{}` is defined for input source `{}`, but qsf_app was built without the `openai` feature",
                self.model,
                request.input_source.label(),
            ),
        })
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
        self.transcribe_realtime(request)
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

#[cfg(feature = "openai")]
fn read_env_u64(env_var: &str, default: u64) -> u64 {
    std::env::var(env_var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn parse_u64(value: Option<&str>, default: u64) -> u64 {
    value.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn transcript_input_source_from_values(
    source_kind: Option<&str>,
    wav_path: Option<&str>,
    mic_device: Option<&str>,
    mic_duration_ms: Option<&str>,
) -> TranscriptInputSource {
    match source_kind
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" | "file" | "prerecorded" | "prerecorded-file" => {
            if let Some(path) = wav_path.filter(|path| !path.is_empty()) {
                TranscriptInputSource::PrerecordedFile {
                    path: path.to_string(),
                }
            } else {
                default_simulated_input_source()
            }
        }
        "mic" | "microphone" | "live" | "live-microphone" => {
            TranscriptInputSource::LiveMicrophone {
                device: mic_device.unwrap_or("default").to_string(),
                duration_ms: parse_u64(mic_duration_ms, DEFAULT_LIVE_MICROPHONE_DURATION_MS),
            }
        }
        _ => default_simulated_input_source(),
    }
}

fn default_simulated_input_source() -> TranscriptInputSource {
    TranscriptInputSource::Simulated {
        label: "deterministic-simulated-audio".to_string(),
        chunk_count: SIMULATED_TRANSCRIPT_CHUNK_COUNT,
    }
}

#[cfg(feature = "openai")]
fn pcm16_to_bytes(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

#[cfg(feature = "openai")]
fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis() as u64
}

#[cfg(feature = "openai")]
fn realtime_audio_from_source(
    input_source: &TranscriptInputSource,
    provider_name: &str,
) -> Result<Vec<u8>, TranscriptProviderError> {
    match input_source {
        TranscriptInputSource::Simulated { .. } => {
            Ok(vec![0u8; OPENAI_REALTIME_PCM_RATE_HZ as usize * 2])
        }
        TranscriptInputSource::PrerecordedFile { path } => read_wav_pcm16(path, provider_name),
        TranscriptInputSource::LiveMicrophone {
            device,
            duration_ms,
        } => capture_live_pcm16(device, *duration_ms, provider_name),
    }
}

#[cfg(feature = "openai")]
fn read_wav_pcm16(path: &str, provider_name: &str) -> Result<Vec<u8>, TranscriptProviderError> {
    let reader =
        hound::WavReader::open(path).map_err(|e| TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!("cannot open WAV file `{path}`: {e}"),
        })?;

    let spec = reader.spec();

    if spec.channels != OPENAI_REALTIME_PCM_CHANNELS {
        return Err(TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!(
                "WAV file `{path}` has {} channel(s); expected mono ({}); convert with: ffmpeg -i input.wav -ar {} -ac 1 output.wav",
                spec.channels, OPENAI_REALTIME_PCM_CHANNELS, OPENAI_REALTIME_PCM_RATE_HZ
            ),
        });
    }

    if spec.sample_rate != OPENAI_REALTIME_PCM_RATE_HZ {
        return Err(TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!(
                "WAV file `{path}` has sample rate {} Hz; expected {} Hz; convert with: ffmpeg -i input.wav -ar {} -ac 1 output.wav",
                spec.sample_rate, OPENAI_REALTIME_PCM_RATE_HZ, OPENAI_REALTIME_PCM_RATE_HZ
            ),
        });
    }

    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .map_err(|e| TranscriptProviderError::TranscriptionFailed {
                provider: provider_name.to_string(),
                message: format!("failed to read PCM samples from `{path}`: {e}"),
            })?,
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|r| r.map(|f| (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16))
            .collect::<Result<_, _>>()
            .map_err(|e| TranscriptProviderError::TranscriptionFailed {
                provider: provider_name.to_string(),
                message: format!("failed to read float samples from `{path}`: {e}"),
            })?,
    };

    Ok(pcm16_to_bytes(&samples))
}

#[cfg(feature = "openai")]
fn capture_live_pcm16(
    device_name: &str,
    duration_ms: u64,
    provider_name: &str,
) -> Result<Vec<u8>, TranscriptProviderError> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    let host = cpal::default_host();

    let device = if device_name == "default" {
        host.default_input_device()
            .ok_or_else(|| TranscriptProviderError::Unavailable {
                provider: provider_name.to_string(),
                reason: "no default audio input device found".to_string(),
            })?
    } else {
        host.input_devices()
            .map_err(|e| TranscriptProviderError::Unavailable {
                provider: provider_name.to_string(),
                reason: format!("cannot enumerate audio input devices: {e}"),
            })?
            .find(|d| {
                d.description()
                    .map(|description| description.name() == device_name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| TranscriptProviderError::Unavailable {
                provider: provider_name.to_string(),
                reason: format!("audio input device `{device_name}` not found"),
            })?
    };

    let stream_config = cpal::StreamConfig {
        channels: OPENAI_REALTIME_PCM_CHANNELS,
        sample_rate: OPENAI_REALTIME_PCM_RATE_HZ,
        buffer_size: cpal::BufferSize::Default,
    };

    let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let samples_writer = Arc::clone(&samples);
    let capture_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let error_writer = Arc::clone(&capture_error);

    let stream = device
        .build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                samples_writer.lock().unwrap().extend_from_slice(data);
            },
            move |err| {
                *error_writer.lock().unwrap() = Some(err.to_string());
            },
            None,
        )
        .map_err(|e| TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!("failed to build audio input stream for `{device_name}`: {e}"),
        })?;

    stream
        .play()
        .map_err(|e| TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!("failed to start audio capture on `{device_name}`: {e}"),
        })?;

    std::thread::sleep(Duration::from_millis(duration_ms));
    drop(stream);

    if let Some(err) = capture_error.lock().unwrap().take() {
        return Err(TranscriptProviderError::TranscriptionFailed {
            provider: provider_name.to_string(),
            message: format!("audio capture error on `{device_name}`: {err}"),
        });
    }

    Ok(pcm16_to_bytes(&samples.lock().unwrap()))
}

#[cfg(feature = "openai")]
async fn transcribe_openai_realtime(
    provider_name: &str,
    model: &str,
    api_key: &str,
    request: &TranscriptProviderRequest,
    audio: Vec<u8>,
    timeout: Duration,
) -> Result<TranscriptProviderSession, TranscriptProviderError> {
    use base64::Engine as _;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let started_at = Instant::now();
    let started_at_ms = 0;
    let mut ws_request = OPENAI_REALTIME_WEBSOCKET_URL
        .into_client_request()
        .map_err(|e| TranscriptProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!("failed to build WebSocket request: {e}"),
        })?;
    let headers = ws_request.headers_mut();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|e| {
            TranscriptProviderError::Unavailable {
                provider: provider_name.to_string(),
                reason: format!("failed to build authorization header: {e}"),
            }
        })?,
    );

    let (ws_stream, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async(ws_request),
    )
    .await
    .map_err(|_| TranscriptProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: "WebSocket connection timed out after 15 seconds".to_string(),
    })?
    .map_err(|e| TranscriptProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: format!("WebSocket connection to OpenAI realtime API failed: {e}"),
    })?;

    let (mut write, mut read) = ws_stream.split();

    // Await the transcription-session creation event before sending configuration.
    let confirmed_model = tokio::time::timeout(timeout, async {
        while let Some(msg) = read.next().await {
            let msg = msg.map_err(|e| TranscriptProviderError::TranscriptionFailed {
                provider: provider_name.to_string(),
                message: format!("read error before transcription_session.created: {e}"),
            })?;
            if let Message::Text(text) = msg {
                let event: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                match event["type"].as_str() {
                    Some("transcription_session.created") | Some("session.created") => {
                        return Ok::<String, TranscriptProviderError>(
                            event["session"]["input_audio_transcription"]["model"]
                                .as_str()
                                .or_else(|| event["session"]["model"].as_str())
                                .unwrap_or(model)
                                .to_string(),
                        );
                    }
                    Some("error") => {
                        return Err(TranscriptProviderError::TranscriptionFailed {
                            provider: provider_name.to_string(),
                            message: format!(
                                "OpenAI error during session setup: {}",
                                event["error"]["message"].as_str().unwrap_or("unknown")
                            ),
                        });
                    }
                    _ => {}
                }
            }
        }
        Err(TranscriptProviderError::TranscriptionFailed {
            provider: provider_name.to_string(),
            message: "connection closed before transcription_session.created".to_string(),
        })
    })
    .await
    .map_err(|_| TranscriptProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: "timed out waiting for transcription_session.created".to_string(),
    })??;

    // Configure session for streaming transcription. gpt-realtime-whisper rejects
    // prompt hints, so keep the provider payload to the supported core fields.
    let mut transcription_config = serde_json::json!({
        "model": model,
    });
    if let Some(language) = &request.language {
        transcription_config["language"] = serde_json::json!(language);
    }
    let session_update = serde_json::json!({
        "type": "session.update",
        "session": {
            "type": "transcription",
            "audio": {
                "input": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": OPENAI_REALTIME_PCM_RATE_HZ,
                    },
                    "transcription": transcription_config,
                    "turn_detection": null,
                },
            },
        }
    });
    write
        .send(Message::Text(session_update.to_string().into()))
        .await
        .map_err(|e| TranscriptProviderError::TranscriptionFailed {
            provider: provider_name.to_string(),
            message: format!("failed to send session.update: {e}"),
        })?;

    // Stream audio to the API in chunks
    let chunk_bytes = (OPENAI_REALTIME_PCM_RATE_HZ as usize
        * OPENAI_REALTIME_PCM_CHANNELS as usize
        * 2
        * OPENAI_REALTIME_CHUNK_MS as usize
        / 1000)
        .max(1);

    let mut chunk_records: Vec<TranscriptAudioChunk> = Vec::new();
    let mut chunk_index: u32 = 0;

    for chunk in audio.chunks(chunk_bytes) {
        let captured_at_ms = elapsed_ms(started_at);
        let duration_ms = (chunk.len() as u64 * 1000)
            / (OPENAI_REALTIME_PCM_RATE_HZ as u64 * 2 * OPENAI_REALTIME_PCM_CHANNELS as u64).max(1);

        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(chunk);
        let append_msg = serde_json::json!({
            "type": "input_audio_buffer.append",
            "audio": audio_b64,
        });
        write
            .send(Message::Text(append_msg.to_string().into()))
            .await
            .map_err(|e| TranscriptProviderError::TranscriptionFailed {
                provider: provider_name.to_string(),
                message: format!("failed to send audio chunk {chunk_index}: {e}"),
            })?;

        chunk_records.push(TranscriptAudioChunk {
            chunk_index,
            captured_at_ms,
            duration_ms,
        });
        chunk_index += 1;
    }

    // Commit the audio buffer to trigger transcription
    write
        .send(Message::Text(
            serde_json::json!({"type": "input_audio_buffer.commit"})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|e| TranscriptProviderError::TranscriptionFailed {
            provider: provider_name.to_string(),
            message: format!("failed to commit audio buffer: {e}"),
        })?;

    // Collect partial and final transcript events
    let mut partials: Vec<PartialTranscript> = Vec::new();
    let mut accumulated_text = String::new();
    let mut revision_index: u32 = 0;
    let mut final_text: Option<String> = None;
    let mut final_at_ms: u64 = 0;

    tokio::time::timeout(timeout, async {
        while let Some(msg) = read.next().await {
            match msg {
                Err(e) => {
                    return Err(TranscriptProviderError::TranscriptionFailed {
                        provider: provider_name.to_string(),
                        message: format!("WebSocket read error during transcription: {e}"),
                    });
                }
                Ok(Message::Text(text)) => {
                    let event: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
                    match event["type"].as_str() {
                        Some("conversation.item.input_audio_transcription.delta") => {
                            let received_at_ms = elapsed_ms(started_at);
                            if let Some(delta) = event["delta"].as_str() {
                                accumulated_text.push_str(delta);
                                partials.push(PartialTranscript {
                                    utterance_index: 0,
                                    revision_index,
                                    source_chunk_index: chunk_index.saturating_sub(1),
                                    received_at_ms,
                                    transcript: accumulated_text.clone(),
                                });
                                revision_index += 1;
                            }
                        }
                        Some("conversation.item.input_audio_transcription.completed") => {
                            final_at_ms = elapsed_ms(started_at);
                            final_text = event["transcript"]
                                .as_str()
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    if accumulated_text.is_empty() {
                                        None
                                    } else {
                                        Some(accumulated_text.clone())
                                    }
                                });
                            break;
                        }
                        Some("error") => {
                            return Err(TranscriptProviderError::TranscriptionFailed {
                                provider: provider_name.to_string(),
                                message: format!(
                                    "OpenAI realtime error: {}",
                                    event["error"]["message"].as_str().unwrap_or("unknown")
                                ),
                            });
                        }
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
            }
        }
        Ok::<(), TranscriptProviderError>(())
    })
    .await
    .map_err(|_| TranscriptProviderError::TranscriptionFailed {
        provider: provider_name.to_string(),
        message: format!("transcription timed out after {} ms", timeout.as_millis()),
    })??;

    let completed_at_ms = elapsed_ms(started_at);

    let Some(final_text) = final_text else {
        return Err(TranscriptProviderError::TranscriptionFailed {
            provider: provider_name.to_string(),
            message: "transcription completed without a final transcript event".to_string(),
        });
    };

    Ok(TranscriptProviderSession {
        session_id: request.session_id.clone(),
        provider_name: provider_name.to_string(),
        model: Some(confirmed_model),
        input_source: request.input_source.clone(),
        started_at_ms,
        completed_at_ms,
        chunks: chunk_records,
        partials,
        final_transcript: FinalTranscript {
            utterance_index: 0,
            received_at_ms: final_at_ms,
            transcript: final_text,
        },
    })
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

    #[cfg(not(feature = "openai"))]
    #[test]
    fn openai_realtime_provider_names_current_target_model_when_feature_disabled() {
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

    #[cfg(feature = "openai")]
    #[test]
    fn openai_realtime_provider_validates_local_inputs_before_network_call() {
        let provider = OpenAiRealtimeTranscriptProvider::default();
        let mut request = TranscriptProviderRequest::simulated("session-test");
        request.input_source = TranscriptInputSource::PrerecordedFile {
            path: "does-not-exist.wav".to_string(),
        };

        let error = provider.transcribe(&request).unwrap_err();

        assert_eq!(provider.model(), OPENAI_REALTIME_TRANSCRIPTION_MODEL);
        assert!(matches!(error, TranscriptProviderError::Unavailable { .. }));
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
    fn transcript_input_source_values_select_wav_path() {
        let input_source = super::transcript_input_source_from_values(
            Some("wav"),
            Some("fixtures/sample.wav"),
            None,
            None,
        );

        assert_eq!(
            input_source,
            TranscriptInputSource::PrerecordedFile {
                path: "fixtures/sample.wav".to_string()
            }
        );
    }

    #[test]
    fn transcript_input_source_values_select_live_microphone() {
        let input_source = super::transcript_input_source_from_values(
            Some("microphone"),
            None,
            Some("Studio Mic"),
            Some("2500"),
        );

        assert_eq!(
            input_source,
            TranscriptInputSource::LiveMicrophone {
                device: "Studio Mic".to_string(),
                duration_ms: 2500
            }
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
