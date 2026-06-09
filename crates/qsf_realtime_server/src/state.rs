use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use qsf_session::{MemorySourceConfig, SessionConfig as QsfSessionConfig, SessionState};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::cli::Args;
use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust, DiagnosticWriter};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_INSTRUCTIONS: &str = "Speak briefly. Keep the browser UI informed, keep secrets server-side, and preserve the QSF trust boundary.";

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    openai_api_key: String,
    openai_base_url: String,
    http_client: reqwest::Client,
    state_dir: PathBuf,
    diagnostics_dir: PathBuf,
    sessions: Mutex<HashMap<String, Arc<Mutex<SessionRuntime>>>>,
}

impl AppState {
    pub fn load(args: &Args) -> anyhow::Result<Self> {
        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY is required for qsf_realtime_server")?;
        Self::new(
            openai_api_key,
            DEFAULT_OPENAI_BASE_URL,
            args.state_dir.clone(),
        )
    }

    pub fn new(
        openai_api_key: impl Into<String>,
        openai_base_url: impl Into<String>,
        state_dir: impl Into<PathBuf>,
    ) -> anyhow::Result<Self> {
        let state_dir = state_dir.into();
        std::fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create state dir `{}`", state_dir.display()))?;
        let diagnostics_dir = state_dir.join("diagnostics");
        std::fs::create_dir_all(&diagnostics_dir).with_context(|| {
            format!(
                "failed to create diagnostics dir `{}`",
                diagnostics_dir.display()
            )
        })?;

        Ok(Self {
            inner: Arc::new(Inner {
                openai_api_key: openai_api_key.into(),
                openai_base_url: openai_base_url.into(),
                http_client: reqwest::Client::new(),
                state_dir,
                diagnostics_dir,
                sessions: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn state_dir(&self) -> &Path {
        &self.inner.state_dir
    }

    pub fn diagnostics_dir(&self) -> &Path {
        &self.inner.diagnostics_dir
    }

    pub fn openai_base_url(&self) -> &str {
        &self.inner.openai_base_url
    }

    pub fn openai_api_key(&self) -> &str {
        &self.inner.openai_api_key
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.inner.http_client
    }

    pub async fn create_session(&self) -> anyhow::Result<SessionAllocationResponse> {
        let qsf_session_id = Uuid::new_v4().to_string();
        let config = BrowserSessionConfig::default();
        let diagnostics = DiagnosticWriter::create(
            self.diagnostics_dir()
                .join(format!("{qsf_session_id}.jsonl")),
        )?;
        diagnostics.write(&DiagnosticRecord::SessionAllocated {
            qsf_session_id: qsf_session_id.clone(),
            at: OffsetDateTime::now_utc(),
        })?;
        diagnostics.write(&DiagnosticRecord::NoSecretEvidence {
            qsf_session_id: qsf_session_id.clone(),
            at: OffsetDateTime::now_utc(),
            note: "OPENAI_API_KEY stays server-side; no credential is returned to the browser"
                .to_string(),
        })?;

        let runtime = SessionRuntime::new(qsf_session_id.clone(), config.clone(), diagnostics);
        self.inner
            .sessions
            .lock()
            .await
            .insert(qsf_session_id.clone(), Arc::new(Mutex::new(runtime)));

        Ok(SessionAllocationResponse {
            qsf_session_id,
            session: config,
        })
    }

    pub async fn session_runtime(
        &self,
        qsf_session_id: &str,
    ) -> Option<Arc<Mutex<SessionRuntime>>> {
        self.inner
            .sessions
            .lock()
            .await
            .get(qsf_session_id)
            .cloned()
    }

    pub async fn remove_session(&self, qsf_session_id: &str) -> Option<Arc<Mutex<SessionRuntime>>> {
        self.inner.sessions.lock().await.remove(qsf_session_id)
    }

    pub fn openai_calls_url(&self) -> String {
        format!(
            "{}/v1/realtime/calls",
            self.openai_base_url().trim_end_matches('/')
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionAllocationResponse {
    pub qsf_session_id: String,
    pub session: BrowserSessionConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSessionConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub model: String,
    pub voice: String,
    pub reasoning_effort: String,
    pub output_modalities: Vec<String>,
    pub instructions: String,
    pub audio: BrowserSessionAudio,
}

impl BrowserSessionConfig {
    pub fn openai_session_request(&self) -> OpenAiRealtimeSessionRequest {
        OpenAiRealtimeSessionRequest {
            kind: "realtime".to_string(),
            model: self.model.clone(),
            // `reasoning_effort` is QSF session metadata, not a field the OpenAI
            // `/v1/realtime/calls` session object accepts — it rejects the
            // request with `unknown_parameter`. Keep it on BrowserSessionConfig
            // (and in the browser-facing allocation response) but do not forward
            // it to the provider.
            instructions: self.instructions.clone(),
            output_modalities: self.output_modalities.clone(),
            audio: OpenAiRealtimeSessionAudio {
                output: OpenAiRealtimeSessionAudioOutput {
                    voice: self.voice.clone(),
                },
                input: OpenAiRealtimeSessionAudioInput {
                    turn_detection: OpenAiRealtimeTurnDetection {
                        kind: "server_vad".to_string(),
                        create_response: true,
                        interrupt_response: true,
                    },
                },
            },
        }
    }
}

impl Default for BrowserSessionConfig {
    fn default() -> Self {
        Self {
            kind: "realtime".to_string(),
            model: "gpt-realtime-2".to_string(),
            voice: "marin".to_string(),
            reasoning_effort: "medium".to_string(),
            output_modalities: vec!["audio".to_string()],
            instructions: DEFAULT_INSTRUCTIONS.to_string(),
            audio: BrowserSessionAudio {
                output: BrowserSessionAudioOutput {
                    voice: "marin".to_string(),
                },
                input: BrowserSessionAudioInput {
                    turn_detection: BrowserSessionTurnDetection {
                        kind: "server_vad".to_string(),
                        create_response: true,
                        interrupt_response: true,
                    },
                },
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSessionAudio {
    pub output: BrowserSessionAudioOutput,
    pub input: BrowserSessionAudioInput,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSessionAudioOutput {
    pub voice: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSessionAudioInput {
    pub turn_detection: BrowserSessionTurnDetection,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSessionTurnDetection {
    #[serde(rename = "type")]
    pub kind: String,
    pub create_response: bool,
    pub interrupt_response: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiRealtimeSessionRequest {
    #[serde(rename = "type")]
    pub kind: String,
    pub model: String,
    pub instructions: String,
    pub output_modalities: Vec<String>,
    pub audio: OpenAiRealtimeSessionAudio,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiRealtimeSessionAudio {
    pub output: OpenAiRealtimeSessionAudioOutput,
    pub input: OpenAiRealtimeSessionAudioInput,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiRealtimeSessionAudioOutput {
    pub voice: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiRealtimeSessionAudioInput {
    pub turn_detection: OpenAiRealtimeTurnDetection,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenAiRealtimeTurnDetection {
    #[serde(rename = "type")]
    pub kind: String,
    pub create_response: bool,
    pub interrupt_response: bool,
}

pub struct SessionRuntime {
    pub qsf_session_id: String,
    pub config: BrowserSessionConfig,
    pub session_state: SessionState,
    pub next_exchange_index: usize,
    pub seen_event_ids: HashSet<String>,
    pub call_binding: Option<CallBinding>,
    pub diagnostics: DiagnosticWriter,
    pub trust: DiagnosticTrust,
    pub persisted_exchange_count: usize,
}

impl SessionRuntime {
    pub fn new(
        qsf_session_id: String,
        config: BrowserSessionConfig,
        diagnostics: DiagnosticWriter,
    ) -> Self {
        let session_state = SessionState::new_with_id(
            qsf_session_id.clone(),
            QsfRealtimeSessionConfig::from_browser_config(&config),
        );
        Self {
            qsf_session_id,
            config,
            session_state,
            next_exchange_index: 0,
            seen_event_ids: HashSet::new(),
            call_binding: None,
            diagnostics,
            trust: DiagnosticTrust::Untrusted,
            persisted_exchange_count: 0,
        }
    }

    pub fn new_exchange_index(&mut self) -> usize {
        let index = self.next_exchange_index;
        self.next_exchange_index += 1;
        index
    }
}

struct QsfRealtimeSessionConfig;

impl QsfRealtimeSessionConfig {
    fn from_browser_config(config: &BrowserSessionConfig) -> QsfSessionConfig {
        QsfSessionConfig {
            model_id: config.model.clone(),
            max_turns: usize::MAX,
            warm_threshold: 0,
            allow_over_limit: true,
            memory_source: MemorySourceConfig {
                source: "realtime".to_string(),
                file: None,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct CallBinding {
    pub call_id: String,
    pub bound_at: OffsetDateTime,
    pub invalidated_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
}
