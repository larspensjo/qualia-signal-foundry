use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{
    AudioLatencyMeasurement, AudioLatencyStage, TranscriptAudioChunk, TranscriptInputSource,
    total_latency_ms,
};

pub const OPENAI_REALTIME_VOICE_MODEL: &str = "gpt-realtime-2";
pub const REALTIME_SESSION_PROVIDER_ENV_VAR: &str = "QSF_REALTIME_SESSION_PROVIDER";
pub const REALTIME_SESSION_INPUT_SOURCE_ENV_VAR: &str = "QSF_REALTIME_SESSION_INPUT_SOURCE";
pub const REALTIME_SESSION_WAV_PATH_ENV_VAR: &str = "QSF_REALTIME_SESSION_WAV_PATH";
pub const REALTIME_SESSION_MIC_DEVICE_ENV_VAR: &str = "QSF_REALTIME_SESSION_MIC_DEVICE";
pub const REALTIME_SESSION_MIC_DURATION_MS_ENV_VAR: &str = "QSF_REALTIME_SESSION_MIC_DURATION_MS";
pub const OPENAI_REALTIME_VOICE_INPUT_TRANSCRIPTION_MODEL: &str = "gpt-4o-mini-transcribe";
const OPENAI_REALTIME_VOICE_WEBSOCKET_BASE_URL: &str = "wss://api.openai.com/v1/realtime";
const DEFAULT_LIVE_MICROPHONE_DURATION_MS: u64 = 4_000;
const SIMULATED_VOICE_CHUNK_COUNT: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeSessionConfig {
    pub voice: String,
    pub reasoning_effort: String,
    pub instructions: String,
    pub output_modalities: Vec<String>,
    pub input_transcription_model: Option<String>,
}

impl Default for RealtimeSessionConfig {
    fn default() -> Self {
        Self {
            voice: "marin".to_string(),
            reasoning_effort: "medium".to_string(),
            instructions: "Speak briefly. Keep tool calls transparent and do not take external actions unless QSF explicitly grants permission.".to_string(),
            output_modalities: vec!["audio".to_string()],
            input_transcription_model: Some(
                OPENAI_REALTIME_VOICE_INPUT_TRANSCRIPTION_MODEL.to_string(),
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeSessionRequest {
    pub session_id: String,
    pub input_source: TranscriptInputSource,
    pub prompt: String,
    pub config: RealtimeSessionConfig,
}

impl RealtimeSessionRequest {
    pub fn simulated(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            input_source: default_simulated_input_source(),
            prompt:
                "Say one short sentence about preserving QSF runtime boundaries in a voice session."
                    .to_string(),
            config: RealtimeSessionConfig::default(),
        }
    }

    pub fn from_env(session_id: impl Into<String>) -> Self {
        let mut request = Self::simulated(session_id);
        request.input_source = realtime_session_input_source_from_values(
            std::env::var(REALTIME_SESSION_INPUT_SOURCE_ENV_VAR)
                .ok()
                .as_deref(),
            std::env::var(REALTIME_SESSION_WAV_PATH_ENV_VAR)
                .ok()
                .as_deref(),
            std::env::var(REALTIME_SESSION_MIC_DEVICE_ENV_VAR)
                .ok()
                .as_deref(),
            std::env::var(REALTIME_SESSION_MIC_DURATION_MS_ENV_VAR)
                .ok()
                .as_deref(),
        );

        request
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeSessionTranscript {
    pub utterance_index: u32,
    pub received_at_ms: u64,
    pub transcript: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeResponse {
    pub response_id: String,
    pub started_at_ms: u64,
    pub first_audio_delta_at_ms: Option<u64>,
    pub completed_at_ms: u64,
    pub status: String,
    pub text: String,
    pub audio_output_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeInterruption {
    pub detected_at_ms: u64,
    pub source: String,
    pub action: RealtimeInterruptionAction,
    pub response_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeInterruptionAction {
    RecordedWithoutBypassingRuntime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealtimeToolCallRequest {
    pub call_id: String,
    pub name: String,
    pub arguments_summary: String,
    pub requested_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VoiceProviderSession {
    pub session_id: String,
    pub provider_name: String,
    pub model: Option<String>,
    pub input_source: TranscriptInputSource,
    pub config: RealtimeSessionConfig,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub chunks: Vec<TranscriptAudioChunk>,
    pub final_transcript: RealtimeSessionTranscript,
    pub preamble: Option<String>,
    pub response: RealtimeResponse,
    pub interruptions: Vec<RealtimeInterruption>,
    pub tool_calls: Vec<RealtimeToolCallRequest>,
}

impl VoiceProviderSession {
    pub fn response_latency_ms(&self) -> u64 {
        self.response
            .completed_at_ms
            .saturating_sub(self.response.started_at_ms)
    }

    pub fn response_start_offset_from_final_transcript_ms(&self) -> i64 {
        signed_timestamp_difference(
            self.response.started_at_ms,
            self.final_transcript.received_at_ms,
        )
    }

    pub fn first_audio_latency_ms(&self) -> Option<u64> {
        self.response
            .first_audio_delta_at_ms
            .map(|started| started.saturating_sub(self.response.started_at_ms))
    }

    pub fn total_latency_ms(&self) -> u64 {
        total_latency_ms(&self.latency_measurements())
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

        measurements.push(AudioLatencyMeasurement::new(
            AudioLatencyStage::FinalTranscription,
            self.started_at_ms,
            self.final_transcript.received_at_ms,
        ));

        measurements.push(AudioLatencyMeasurement::new(
            AudioLatencyStage::RealtimeResponseStart,
            self.final_transcript
                .received_at_ms
                .min(self.response.started_at_ms),
            self.final_transcript
                .received_at_ms
                .max(self.response.started_at_ms),
        ));

        if let Some(first_audio) = self.response.first_audio_delta_at_ms {
            measurements.push(AudioLatencyMeasurement::new(
                AudioLatencyStage::RealtimeAudioOutput,
                self.response.started_at_ms,
                first_audio,
            ));
        }

        if let Some(interruption) = self.interruptions.first() {
            measurements.push(AudioLatencyMeasurement::new(
                AudioLatencyStage::RealtimeInterruption,
                self.response.started_at_ms,
                interruption.detected_at_ms,
            ));
        }

        measurements.push(AudioLatencyMeasurement::new(
            AudioLatencyStage::RealtimeResponseCompletion,
            self.response.started_at_ms,
            self.response.completed_at_ms,
        ));

        measurements
    }
}

pub trait RealtimeSessionProvider {
    fn provider_name(&self) -> &str;

    fn run_session(
        &self,
        request: &RealtimeSessionRequest,
    ) -> Result<VoiceProviderSession, RealtimeSessionProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RealtimeSessionProviderError {
    Unavailable { provider: String, reason: String },
    SessionFailed { provider: String, message: String },
}

impl RealtimeSessionProviderError {
    pub fn provider(&self) -> &str {
        match self {
            Self::Unavailable { provider, .. } | Self::SessionFailed { provider, .. } => provider,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Unavailable { reason, .. } => reason,
            Self::SessionFailed { message, .. } => message,
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "provider_unavailable",
            Self::SessionFailed { .. } => "session_failed",
        }
    }

    pub fn sanitized_message(&self) -> String {
        super::transcript_provider::sanitize_provider_error_message(self.message())
    }
}

impl fmt::Display for RealtimeSessionProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { provider, reason } => {
                write!(
                    formatter,
                    "realtime session provider `{provider}` unavailable: {reason}"
                )
            }
            Self::SessionFailed { provider, message } => {
                write!(
                    formatter,
                    "realtime session provider `{provider}` failed: {message}"
                )
            }
        }
    }
}

impl std::error::Error for RealtimeSessionProviderError {}

impl From<super::transcript_provider::TranscriptProviderError> for RealtimeSessionProviderError {
    fn from(error: super::transcript_provider::TranscriptProviderError) -> Self {
        match error {
            super::transcript_provider::TranscriptProviderError::Unavailable {
                provider,
                reason,
            } => Self::Unavailable { provider, reason },
            super::transcript_provider::TranscriptProviderError::TranscriptionFailed {
                provider,
                message,
            } => Self::SessionFailed { provider, message },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimulatedRealtimeSessionProvider;

impl RealtimeSessionProvider for SimulatedRealtimeSessionProvider {
    fn provider_name(&self) -> &str {
        "simulated-realtime-session-provider"
    }

    fn run_session(
        &self,
        request: &RealtimeSessionRequest,
    ) -> Result<VoiceProviderSession, RealtimeSessionProviderError> {
        if !matches!(
            request.input_source,
            TranscriptInputSource::Simulated { .. }
        ) {
            engine_logging::engine_warn!(
                "simulated realtime session provider received non-simulated input source {}; using deterministic synthetic chunks",
                request.input_source.label()
            );
        }

        Ok(VoiceProviderSession {
            session_id: request.session_id.clone(),
            provider_name: self.provider_name().to_string(),
            model: None,
            input_source: request.input_source.clone(),
            config: request.config.clone(),
            started_at_ms: 0,
            completed_at_ms: 246,
            chunks: simulated_chunks(&request.input_source),
            final_transcript: RealtimeSessionTranscript {
                utterance_index: 0,
                received_at_ms: 84,
                transcript: "Can the voice session stay inside the QSF runtime boundaries?"
                    .to_string(),
            },
            preamble: Some("Briefly: yes.".to_string()),
            response: RealtimeResponse {
                response_id: "simulated-response-0".to_string(),
                started_at_ms: 112,
                first_audio_delta_at_ms: Some(132),
                completed_at_ms: 226,
                status: "completed".to_string(),
                text: "Briefly: yes. The provider can speak, but state and tools still flow through QSF events.".to_string(),
                audio_output_bytes: 4096,
            },
            interruptions: vec![RealtimeInterruption {
                detected_at_ms: 174,
                source: "simulated-user-speech-started".to_string(),
                action: RealtimeInterruptionAction::RecordedWithoutBypassingRuntime,
                response_id: Some("simulated-response-0".to_string()),
            }],
            tool_calls: vec![RealtimeToolCallRequest {
                call_id: "simulated-tool-call-0".to_string(),
                name: "qsf_memory_lookup".to_string(),
                arguments_summary: "{\"query\":\"voice session boundaries\"}".to_string(),
                requested_at_ms: 146,
            }],
        })
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiRealtimeSessionProvider {
    model: String,
}

impl Default for OpenAiRealtimeSessionProvider {
    fn default() -> Self {
        Self {
            model: OPENAI_REALTIME_VOICE_MODEL.to_string(),
        }
    }
}

impl OpenAiRealtimeSessionProvider {
    pub fn model(&self) -> &str {
        &self.model
    }

    fn run_openai_session(
        &self,
        request: &RealtimeSessionRequest,
    ) -> Result<VoiceProviderSession, RealtimeSessionProviderError> {
        validate_realtime_input_source_before_network(&request.input_source, self.provider_name())?;

        let api_key =
            std::env::var(super::transcript_provider::OPENAI_API_KEY_ENV_VAR).map_err(|_| {
                RealtimeSessionProviderError::Unavailable {
                    provider: self.provider_name().to_string(),
                    reason: format!(
                        "{} is required for the OpenAI realtime session provider",
                        super::transcript_provider::OPENAI_API_KEY_ENV_VAR
                    ),
                }
            })?;
        let timeout_ms = super::transcript_provider::read_env_u64(
            super::transcript_provider::OPENAI_REALTIME_TIMEOUT_MS_ENV_VAR,
            super::transcript_provider::DEFAULT_OPENAI_REALTIME_TIMEOUT_MS,
        );

        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(RealtimeSessionProviderError::Unavailable {
                provider: self.provider_name().to_string(),
                reason: "synchronous realtime session provider cannot be called from inside an existing Tokio runtime".to_string(),
            });
        }
        let runtime = super::transcript_provider::openai_realtime_runtime(self.provider_name())?;

        runtime.block_on(run_openai_realtime_voice_session(
            self.provider_name(),
            &self.model,
            &api_key,
            request,
            Duration::from_millis(timeout_ms),
        ))
    }
}

impl RealtimeSessionProvider for OpenAiRealtimeSessionProvider {
    fn provider_name(&self) -> &str {
        "openai-realtime-session-provider"
    }

    fn run_session(
        &self,
        request: &RealtimeSessionRequest,
    ) -> Result<VoiceProviderSession, RealtimeSessionProviderError> {
        self.run_openai_session(request)
    }
}

pub fn requested_realtime_session_provider(input: Option<&str>) -> &'static str {
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

pub fn requested_realtime_session_provider_from_env() -> &'static str {
    requested_realtime_session_provider(
        std::env::var(REALTIME_SESSION_PROVIDER_ENV_VAR)
            .ok()
            .as_deref(),
    )
}

pub fn build_realtime_session_provider(provider_name: &str) -> Box<dyn RealtimeSessionProvider> {
    match requested_realtime_session_provider(Some(provider_name)) {
        "openai-realtime" => Box::new(OpenAiRealtimeSessionProvider::default()),
        _ => Box::new(SimulatedRealtimeSessionProvider),
    }
}

fn simulated_chunks(input_source: &TranscriptInputSource) -> Vec<TranscriptAudioChunk> {
    let chunk_count = match input_source {
        TranscriptInputSource::Simulated { chunk_count, .. } => *chunk_count,
        _ => SIMULATED_VOICE_CHUNK_COUNT,
    };

    (0..chunk_count)
        .map(|chunk_index| TranscriptAudioChunk {
            chunk_index,
            captured_at_ms: u64::from(chunk_index) * 24,
            duration_ms: 24,
        })
        .collect()
}

fn parse_u64(value: Option<&str>, default: u64) -> u64 {
    value.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn realtime_session_input_source_from_values(
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
        label: "deterministic-simulated-voice-session".to_string(),
        chunk_count: SIMULATED_VOICE_CHUNK_COUNT,
    }
}
fn validate_realtime_input_source_before_network(
    input_source: &TranscriptInputSource,
    provider_name: &str,
) -> Result<(), RealtimeSessionProviderError> {
    if matches!(input_source, TranscriptInputSource::PrerecordedFile { .. }) {
        super::transcript_provider::realtime_audio_from_source(input_source, provider_name)?;
    }

    Ok(())
}
async fn run_openai_realtime_voice_session(
    provider_name: &str,
    model: &str,
    api_key: &str,
    request: &RealtimeSessionRequest,
    timeout: Duration,
) -> Result<VoiceProviderSession, RealtimeSessionProviderError> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let started_at = Instant::now();
    let websocket_url = format!("{OPENAI_REALTIME_VOICE_WEBSOCKET_BASE_URL}?model={model}");
    let mut ws_request = websocket_url.into_client_request().map_err(|e| {
        RealtimeSessionProviderError::Unavailable {
            provider: provider_name.to_string(),
            reason: format!("failed to build WebSocket request: {e}"),
        }
    })?;
    ws_request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|e| {
            RealtimeSessionProviderError::Unavailable {
                provider: provider_name.to_string(),
                reason: format!("failed to build authorization header: {e}"),
            }
        })?,
    );

    let connect_timeout = timeout.min(Duration::from_millis(
        super::transcript_provider::DEFAULT_OPENAI_REALTIME_CONNECT_TIMEOUT_MS,
    ));
    let (ws_stream, _) = tokio::time::timeout(
        connect_timeout,
        tokio_tungstenite::connect_async(ws_request),
    )
    .await
    .map_err(|_| RealtimeSessionProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: format!(
            "WebSocket connection timed out after {} ms",
            connect_timeout.as_millis()
        ),
    })?
    .map_err(|e| RealtimeSessionProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: format!("WebSocket connection to OpenAI realtime API failed: {e}"),
    })?;

    let (mut write, mut read) = ws_stream.split();
    let confirmed_model =
        wait_for_session_created(provider_name, model, &mut read, connect_timeout).await?;

    let session_update = build_openai_realtime_session_update(model, request);
    write
        .send(Message::Text(session_update.to_string().into()))
        .await
        .map_err(|e| RealtimeSessionProviderError::SessionFailed {
            provider: provider_name.to_string(),
            message: format!("failed to send session.update: {e}"),
        })?;

    let mut chunks = Vec::new();
    match &request.input_source {
        TranscriptInputSource::Simulated { .. } => {
            let input_item = serde_json::json!({
                "type": "conversation.item.create",
                "item": {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": request.prompt,
                        }
                    ],
                },
            });
            write
                .send(Message::Text(input_item.to_string().into()))
                .await
                .map_err(|e| RealtimeSessionProviderError::SessionFailed {
                    provider: provider_name.to_string(),
                    message: format!("failed to send text conversation item: {e}"),
                })?;
        }
        _ => {
            let audio = super::transcript_provider::realtime_audio_from_source(
                &request.input_source,
                provider_name,
            )?;
            chunks = send_audio_input(provider_name, &mut write, started_at, &audio).await?;
        }
    }

    write
        .send(Message::Text(
            build_openai_realtime_response_create(request)
                .to_string()
                .into(),
        ))
        .await
        .map_err(|e| RealtimeSessionProviderError::SessionFailed {
            provider: provider_name.to_string(),
            message: format!("failed to send response.create: {e}"),
        })?;

    collect_openai_voice_session(
        provider_name,
        confirmed_model,
        request,
        chunks,
        started_at,
        &mut read,
        timeout,
    )
    .await
}
fn build_openai_realtime_response_create(request: &RealtimeSessionRequest) -> serde_json::Value {
    serde_json::json!({
        "type": "response.create",
        "response": {
            "audio": {
                "output": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ,
                    },
                    "voice": request.config.voice,
                },
            },
            "instructions": request.config.instructions,
        },
    })
}
fn build_openai_realtime_session_update(
    model: &str,
    request: &RealtimeSessionRequest,
) -> serde_json::Value {
    let mut update = serde_json::json!({
        "type": "session.update",
        "session": {
            "type": "realtime",
            "model": model,
            "output_modalities": request.config.output_modalities,
            "audio": {
                "input": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ,
                    },
                    "turn_detection": null,
                },
                "output": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ,
                    },
                    "voice": request.config.voice,
                },
            },
            "instructions": request.config.instructions,
        },
    });

    if let Some(transcription_model) = &request.config.input_transcription_model {
        update["session"]["audio"]["input"]["transcription"] = serde_json::json!({
            "model": transcription_model,
        });
    }

    update
}
async fn wait_for_session_created<S>(
    provider_name: &str,
    model: &str,
    read: &mut S,
    timeout: Duration,
) -> Result<String, RealtimeSessionProviderError>
where
    S: futures_util::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    tokio::time::timeout(timeout, async {
        while let Some(msg) = read.next().await {
            let msg = msg.map_err(|e| RealtimeSessionProviderError::SessionFailed {
                provider: provider_name.to_string(),
                message: format!("read error before session.created: {e}"),
            })?;
            if let Message::Text(text) = msg {
                let Some(event) =
                    super::transcript_provider::parse_realtime_server_event(provider_name, &text)
                else {
                    continue;
                };
                match event["type"].as_str() {
                    Some("session.created") => {
                        return Ok::<String, RealtimeSessionProviderError>(
                            event["session"]["model"]
                                .as_str()
                                .unwrap_or(model)
                                .to_string(),
                        );
                    }
                    Some("error") => {
                        return Err(RealtimeSessionProviderError::SessionFailed {
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
        Err(RealtimeSessionProviderError::SessionFailed {
            provider: provider_name.to_string(),
            message: "connection closed before session.created".to_string(),
        })
    })
    .await
    .map_err(|_| RealtimeSessionProviderError::Unavailable {
        provider: provider_name.to_string(),
        reason: "timed out waiting for session.created".to_string(),
    })?
}
async fn send_audio_input<S>(
    provider_name: &str,
    write: &mut S,
    started_at: Instant,
    audio: &[u8],
) -> Result<Vec<TranscriptAudioChunk>, RealtimeSessionProviderError>
where
    S: futures_util::Sink<tokio_tungstenite::tungstenite::Message> + Unpin,
    S::Error: fmt::Display,
{
    use base64::Engine as _;
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let chunk_bytes = (super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ as usize
        * super::transcript_provider::OPENAI_REALTIME_PCM_CHANNELS as usize
        * 2
        * super::transcript_provider::OPENAI_REALTIME_CHUNK_MS as usize
        / 1000)
        .max(1);
    let mut chunks = Vec::new();

    for (chunk_index, chunk) in audio.chunks(chunk_bytes).enumerate() {
        let captured_at_ms = super::transcript_provider::elapsed_ms(started_at);
        let duration_ms = (chunk.len() as u64 * 1000)
            / (super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ as u64
                * 2
                * super::transcript_provider::OPENAI_REALTIME_PCM_CHANNELS as u64)
                .max(1);
        let append_msg = serde_json::json!({
            "type": "input_audio_buffer.append",
            "audio": base64::engine::general_purpose::STANDARD.encode(chunk),
        });
        write
            .send(Message::Text(append_msg.to_string().into()))
            .await
            .map_err(|e| RealtimeSessionProviderError::SessionFailed {
                provider: provider_name.to_string(),
                message: format!("failed to send audio chunk {chunk_index}: {e}"),
            })?;
        chunks.push(TranscriptAudioChunk {
            chunk_index: chunk_index as u32,
            captured_at_ms,
            duration_ms,
        });
    }

    write
        .send(Message::Text(
            serde_json::json!({"type": "input_audio_buffer.commit"})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|e| RealtimeSessionProviderError::SessionFailed {
            provider: provider_name.to_string(),
            message: format!("failed to commit audio buffer: {e}"),
        })?;

    Ok(chunks)
}
async fn collect_openai_voice_session<S>(
    provider_name: &str,
    confirmed_model: String,
    request: &RealtimeSessionRequest,
    chunks: Vec<TranscriptAudioChunk>,
    started_at: Instant,
    read: &mut S,
    timeout: Duration,
) -> Result<VoiceProviderSession, RealtimeSessionProviderError>
where
    S: futures_util::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    use base64::Engine as _;
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let mut final_transcript = if is_text_only_simulated_request(request) {
        Some(request.prompt.clone())
    } else {
        None
    };
    let mut final_transcript_at_ms = 0;
    let mut response_id = "openai-realtime-response".to_string();
    let mut response_started_at_ms = 0;
    let mut response_completed_at_ms = 0;
    let mut response_status = "incomplete".to_string();
    let mut response_text = String::new();
    let mut first_audio_delta_at_ms = None;
    let mut audio_output_bytes = 0usize;
    let mut preamble = None;
    let mut interruptions = Vec::new();
    let mut tool_calls = Vec::new();

    tokio::time::timeout(timeout, async {
        while let Some(msg) = read.next().await {
            match msg {
                Err(e) => {
                    return Err(RealtimeSessionProviderError::SessionFailed {
                        provider: provider_name.to_string(),
                        message: format!("WebSocket read error during realtime session: {e}"),
                    });
                }
                Ok(Message::Text(text)) => {
                    let Some(event) = super::transcript_provider::parse_realtime_server_event(
                        provider_name,
                        &text,
                    ) else {
                        continue;
                    };
                    let event_type = event["type"].as_str().unwrap_or_default();
                    match event_type {
                        "conversation.item.input_audio_transcription.completed" => {
                            final_transcript_at_ms =
                                super::transcript_provider::elapsed_ms(started_at);
                            final_transcript = event["transcript"].as_str().map(str::to_string);
                        }
                        "response.created" => {
                            response_started_at_ms =
                                super::transcript_provider::elapsed_ms(started_at);
                            response_id = event["response"]["id"]
                                .as_str()
                                .unwrap_or(&response_id)
                                .to_string();
                        }
                        "response.audio_transcript.delta"
                        | "response.output_audio_transcript.delta"
                        | "response.text.delta"
                        | "response.output_text.delta" => {
                            if let Some(delta) = event["delta"].as_str() {
                                response_text.push_str(delta);
                                if preamble.is_none() && !response_text.trim().is_empty() {
                                    preamble = Some(response_text.chars().take(120).collect());
                                }
                            }
                        }
                        "response.audio_transcript.done"
                        | "response.output_audio_transcript.done"
                        | "response.output_text.done"
                            if response_text.is_empty() =>
                        {
                            if let Some(text) = event["transcript"]
                                .as_str()
                                .or_else(|| event["text"].as_str())
                            {
                                response_text.push_str(text);
                            }
                        }
                        "response.audio.delta" | "response.output_audio.delta" => {
                            let received_at_ms = super::transcript_provider::elapsed_ms(started_at);
                            first_audio_delta_at_ms.get_or_insert(received_at_ms);
                            if let Some(delta) = event["delta"].as_str() {
                                if let Ok(bytes) =
                                    base64::engine::general_purpose::STANDARD.decode(delta)
                                {
                                    audio_output_bytes += bytes.len();
                                }
                            }
                        }
                        "input_audio_buffer.speech_started" => {
                            interruptions.push(RealtimeInterruption {
                                detected_at_ms: super::transcript_provider::elapsed_ms(started_at),
                                source: "input_audio_buffer.speech_started".to_string(),
                                action: RealtimeInterruptionAction::RecordedWithoutBypassingRuntime,
                                response_id: Some(response_id.clone()),
                            });
                        }
                        "response.function_call_arguments.done" => {
                            tool_calls.push(RealtimeToolCallRequest {
                                call_id: event["call_id"]
                                    .as_str()
                                    .unwrap_or("openai-tool-call")
                                    .to_string(),
                                name: event["name"]
                                    .as_str()
                                    .unwrap_or("unknown_provider_tool")
                                    .to_string(),
                                arguments_summary: event["arguments"]
                                    .as_str()
                                    .unwrap_or("{}")
                                    .chars()
                                    .take(240)
                                    .collect(),
                                requested_at_ms: super::transcript_provider::elapsed_ms(started_at),
                            });
                        }
                        "response.done" => {
                            response_completed_at_ms =
                                super::transcript_provider::elapsed_ms(started_at);
                            response_status = event["response"]["status"]
                                .as_str()
                                .unwrap_or("completed")
                                .to_string();
                            if response_text.is_empty() {
                                response_text = extract_response_text(&event)
                                    .unwrap_or_else(|| "<no-text-output>".to_string());
                            }
                            break;
                        }
                        "error" => {
                            return Err(RealtimeSessionProviderError::SessionFailed {
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
        Ok::<(), RealtimeSessionProviderError>(())
    })
    .await
    .map_err(|_| RealtimeSessionProviderError::SessionFailed {
        provider: provider_name.to_string(),
        message: format!(
            "realtime voice session timed out after {} ms",
            timeout.as_millis()
        ),
    })??;

    let completed_at_ms = super::transcript_provider::elapsed_ms(started_at);
    if response_completed_at_ms == 0 {
        response_completed_at_ms = completed_at_ms;
    }
    if response_started_at_ms == 0 {
        response_started_at_ms = final_transcript_at_ms;
    }
    if final_transcript_at_ms == 0 {
        final_transcript_at_ms = response_started_at_ms;
    }
    if response_text.is_empty() {
        response_text = "<no-text-output>".to_string();
    }

    Ok(VoiceProviderSession {
        session_id: request.session_id.clone(),
        provider_name: provider_name.to_string(),
        model: Some(confirmed_model),
        input_source: request.input_source.clone(),
        config: request.config.clone(),
        started_at_ms: 0,
        completed_at_ms,
        chunks,
        final_transcript: RealtimeSessionTranscript {
            utterance_index: 0,
            received_at_ms: final_transcript_at_ms,
            transcript: final_transcript.unwrap_or_else(|| fallback_input_transcript(request)),
        },
        preamble,
        response: RealtimeResponse {
            response_id,
            started_at_ms: response_started_at_ms,
            first_audio_delta_at_ms,
            completed_at_ms: response_completed_at_ms,
            status: response_status,
            text: response_text,
            audio_output_bytes,
        },
        interruptions,
        tool_calls,
    })
}

fn signed_timestamp_difference(lhs_ms: u64, rhs_ms: u64) -> i64 {
    if lhs_ms >= rhs_ms {
        let difference = lhs_ms - rhs_ms;
        i64::try_from(difference).unwrap_or(i64::MAX)
    } else {
        let difference = rhs_ms - lhs_ms;
        -i64::try_from(difference).unwrap_or(i64::MAX)
    }
}
fn is_text_only_simulated_request(request: &RealtimeSessionRequest) -> bool {
    matches!(
        request.input_source,
        TranscriptInputSource::Simulated { .. }
    )
}
fn fallback_input_transcript(request: &RealtimeSessionRequest) -> String {
    if is_text_only_simulated_request(request) {
        request.prompt.clone()
    } else {
        "<no-input-transcript>".to_string()
    }
}
fn extract_response_text(event: &serde_json::Value) -> Option<String> {
    let output = event["response"]["output"].as_array()?;
    let mut text = String::new();

    for item in output {
        if let Some(content) = item["content"].as_array() {
            for part in content {
                if let Some(value) = part["transcript"]
                    .as_str()
                    .or_else(|| part["text"].as_str())
                {
                    text.push_str(value);
                }
            }
        }
    }

    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::{
        OPENAI_REALTIME_VOICE_MODEL, OpenAiRealtimeSessionProvider, RealtimeSessionConfig,
        RealtimeSessionProvider, RealtimeSessionProviderError, RealtimeSessionRequest,
        SimulatedRealtimeSessionProvider, VoiceProviderSession,
        requested_realtime_session_provider,
    };
    use crate::audio::{AudioLatencyStage, TranscriptInputSource};

    #[test]
    fn simulated_provider_emits_voice_session_lifecycle_data() {
        let provider = SimulatedRealtimeSessionProvider;
        let request = RealtimeSessionRequest::simulated("voice-session-test");
        let session = provider.run_session(&request).unwrap();

        assert_eq!(session.provider_name, "simulated-realtime-session-provider");
        assert_eq!(session.final_transcript.received_at_ms, 84);
        assert_eq!(session.response.status, "completed");
        assert!(session.preamble.is_some());
        assert_eq!(session.tool_calls.len(), 1);
        assert_eq!(session.interruptions.len(), 1);
        assert_eq!(
            session.interruptions[0].action,
            super::RealtimeInterruptionAction::RecordedWithoutBypassingRuntime
        );
        assert_eq!(session.first_audio_latency_ms(), Some(20));
        assert_eq!(session.response_latency_ms(), 114);
        assert_eq!(session.total_latency_ms(), 226);
        assert!(
            !session
                .latency_measurements()
                .iter()
                .any(|measurement| measurement.stage == AudioLatencyStage::RuntimeInputDispatch)
        );
    }

    #[test]
    fn openai_realtime_session_provider_validates_local_inputs_before_network_call() {
        let provider = OpenAiRealtimeSessionProvider::default();
        let mut request = RealtimeSessionRequest::simulated("voice-session-test");
        request.input_source = TranscriptInputSource::PrerecordedFile {
            path: "does-not-exist.wav".to_string(),
        };

        let error = provider.run_session(&request).unwrap_err();

        assert_eq!(provider.model(), OPENAI_REALTIME_VOICE_MODEL);
        assert!(matches!(
            error,
            RealtimeSessionProviderError::Unavailable { .. }
        ));
        assert!(error.message().contains("does-not-exist.wav"));
    }

    #[test]
    fn openai_realtime_session_update_includes_output_audio_rate() {
        let request = RealtimeSessionRequest::simulated("voice-session-test");
        let update =
            super::build_openai_realtime_session_update(OPENAI_REALTIME_VOICE_MODEL, &request);

        assert_eq!(
            update["session"]["audio"]["input"]["format"]["rate"],
            super::super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ
        );
        assert_eq!(
            update["session"]["audio"]["output"]["format"]["rate"],
            super::super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ
        );
        assert_eq!(
            update["session"]["output_modalities"],
            serde_json::json!(["audio"])
        );
        assert_eq!(
            update["session"]["audio"]["input"]["transcription"]["model"],
            super::OPENAI_REALTIME_VOICE_INPUT_TRANSCRIPTION_MODEL
        );
    }

    #[test]
    fn openai_realtime_response_create_uses_session_modalities() {
        let request = RealtimeSessionRequest::simulated("voice-session-test");
        let response_create = super::build_openai_realtime_response_create(&request);

        assert_eq!(response_create["type"], "response.create");
        assert!(response_create["response"]["modalities"].is_null());
        assert_eq!(
            response_create["response"]["audio"]["output"]["format"]["rate"],
            super::super::transcript_provider::OPENAI_REALTIME_PCM_RATE_HZ
        );
    }

    #[test]
    fn realtime_session_provider_selector_defaults_to_simulated() {
        assert_eq!(requested_realtime_session_provider(None), "simulated");
        assert_eq!(
            requested_realtime_session_provider(Some("unknown")),
            "simulated"
        );
        assert_eq!(
            requested_realtime_session_provider(Some("openai")),
            "openai-realtime"
        );
        assert_eq!(
            requested_realtime_session_provider(Some("openai-realtime")),
            "openai-realtime"
        );
    }

    #[test]
    fn realtime_session_input_source_values_select_live_microphone() {
        let input_source = super::realtime_session_input_source_from_values(
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
    fn realtime_session_input_source_values_select_wav_path() {
        let input_source = super::realtime_session_input_source_from_values(
            Some("prerecorded"),
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
    fn realtime_session_input_source_values_default_to_simulated() {
        let input_source =
            super::realtime_session_input_source_from_values(Some("unknown"), None, None, None);

        assert_eq!(
            input_source,
            TranscriptInputSource::Simulated {
                label: "deterministic-simulated-voice-session".to_string(),
                chunk_count: 3
            }
        );
    }

    #[test]
    fn realtime_session_types_round_trip_through_json() {
        let provider = SimulatedRealtimeSessionProvider;
        let request = RealtimeSessionRequest::simulated("voice-session-test");
        let session = provider.run_session(&request).unwrap();

        let config_json = serde_json::to_string(&RealtimeSessionConfig::default()).unwrap();
        let session_json = serde_json::to_string(&session).unwrap();
        let config: RealtimeSessionConfig = serde_json::from_str(&config_json).unwrap();
        let session_round_trip: VoiceProviderSession = serde_json::from_str(&session_json).unwrap();

        assert_eq!(config, RealtimeSessionConfig::default());
        assert_eq!(session_round_trip, session);
    }

    #[test]
    fn response_start_measurement_handles_response_before_final_transcript() {
        let provider = SimulatedRealtimeSessionProvider;
        let request = RealtimeSessionRequest::simulated("voice-session-test");
        let mut session = provider.run_session(&request).unwrap();
        session.response.started_at_ms = 80;
        session.final_transcript.received_at_ms = 100;

        let response_start = session
            .latency_measurements()
            .into_iter()
            .find(|measurement| measurement.stage == AudioLatencyStage::RealtimeResponseStart)
            .unwrap();

        assert_eq!(
            session.response_start_offset_from_final_transcript_ms(),
            -20
        );
        assert_eq!(response_start.started_at_ms, 80);
        assert_eq!(response_start.completed_at_ms, 100);
        assert_eq!(response_start.duration_ms, 20);
    }

    #[test]
    fn non_simulated_voice_input_without_provider_transcript_uses_sentinel() {
        let mut request = RealtimeSessionRequest::simulated("voice-session-test");
        request.input_source = TranscriptInputSource::LiveMicrophone {
            device: "default".to_string(),
            duration_ms: 4000,
        };

        assert_eq!(
            super::fallback_input_transcript(&request),
            "<no-input-transcript>"
        );
    }
}
