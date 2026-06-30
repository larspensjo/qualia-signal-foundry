use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use qsf_realtime_protocol::{OPENAI_REALTIME_WS_BASE_URL, RealtimeToolDefinition};
use qsf_session::LiveSessionState;
use qsf_session::{MemorySourceConfig, SessionConfig as QsfSessionConfig, SessionState};
use qsf_volition::{Mode, VolitionContinuitySnapshot, realtime_seed_fixture};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::cli::Args;
use crate::diagnostics::{DiagnosticRecord, DiagnosticTrust, DiagnosticWriter};
use crate::realtime::turn_context::TurnContextCapture;
use crate::realtime::volition_injection::build_stable_baseline_instructions;
use crate::realtime::volition_inspection_capture::VolitionInspectionCapture;

pub const DEFAULT_QSF_SESSION_ID: &str = "default";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_INSTRUCTIONS: &str = "\
Speak briefly. Keep the browser UI informed, keep secrets server-side, and preserve the QSF trust boundary. \
You have read-only access to your simulated internal volition state through tools. \
When asked about your current focus, goals, motivations, or internal state, call inspect_volition_state first. \
When asked which goals relate to a specific topic or how you can help with something, call select_volition_goals with the relevant query. \
Frame any volition tool result as simulated internal state — not a claim of real desire, consciousness, or subjective experience.";
/// Input transcription model for realtime voice. Enabling it makes the provider
/// emit `conversation.item.input_audio_transcription.completed`, which the
/// sideband requires to retrieve memory and issue `response.create`.
const DEFAULT_INPUT_TRANSCRIPTION_MODEL: &str = "gpt-4o-mini-transcribe";

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    openai_api_key: String,
    openai_base_url: String,
    openai_realtime_ws_base_url: String,
    http_client: reqwest::Client,
    session_id_mode: SessionIdMode,
    state_dir: PathBuf,
    continuity_root: PathBuf,
    diagnostics_dir: PathBuf,
    sessions: Mutex<HashMap<String, Arc<Mutex<SessionRuntime>>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionIdMode {
    Default,
    RandomUuid,
}

impl AppState {
    pub fn load(args: &Args) -> anyhow::Result<Self> {
        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY is required for qsf_realtime_server")?;
        Self::new(
            openai_api_key,
            DEFAULT_OPENAI_BASE_URL,
            args.state_dir.clone(),
            if args.random_session_id {
                SessionIdMode::RandomUuid
            } else {
                SessionIdMode::Default
            },
        )
    }

    pub fn new(
        openai_api_key: impl Into<String>,
        openai_base_url: impl Into<String>,
        state_dir: impl Into<PathBuf>,
        session_id_mode: SessionIdMode,
    ) -> anyhow::Result<Self> {
        Self::new_with_realtime_ws_base_url(
            openai_api_key,
            openai_base_url,
            OPENAI_REALTIME_WS_BASE_URL,
            state_dir,
            session_id_mode,
        )
    }

    pub fn new_with_realtime_ws_base_url(
        openai_api_key: impl Into<String>,
        openai_base_url: impl Into<String>,
        openai_realtime_ws_base_url: impl Into<String>,
        state_dir: impl Into<PathBuf>,
        session_id_mode: SessionIdMode,
    ) -> anyhow::Result<Self> {
        let state_dir = state_dir.into();
        std::fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create state dir `{}`", state_dir.display()))?;
        let continuity_root = state_dir.join("continuity");
        std::fs::create_dir_all(&continuity_root).with_context(|| {
            format!(
                "failed to create continuity root `{}`",
                continuity_root.display()
            )
        })?;
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
                openai_realtime_ws_base_url: openai_realtime_ws_base_url.into(),
                http_client: reqwest::Client::new(),
                session_id_mode,
                state_dir,
                continuity_root,
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

    pub fn openai_realtime_ws_base_url(&self) -> &str {
        &self.inner.openai_realtime_ws_base_url
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.inner.http_client
    }

    pub fn continuity_root(&self) -> &Path {
        &self.inner.continuity_root
    }

    pub fn continuity_session_dir(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_root().join(qsf_session_id)
    }

    pub fn continuity_session_state_path(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_session_dir(qsf_session_id)
            .join("session-state.json")
    }

    pub fn continuity_manifest_path(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_session_dir(qsf_session_id)
            .join("continuity-manifest.json")
    }

    pub fn continuity_memory_store_path(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_session_dir(qsf_session_id)
            .join("memory-store.json")
    }

    pub fn continuity_volition_snapshot_path(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_session_dir(qsf_session_id)
            .join("volition-state.json")
    }

    pub fn continuity_reviewed_volition_seed_path(&self, qsf_session_id: &str) -> PathBuf {
        self.continuity_session_dir(qsf_session_id)
            .join("volition-seed.reviewed.json")
    }

    pub fn openai_realtime_ws_url(&self, call_id: &str) -> String {
        format!(
            "{}?call_id={}",
            self.openai_realtime_ws_base_url().trim_end_matches('/'),
            call_id
        )
    }

    pub async fn create_session(&self) -> anyhow::Result<SessionAllocationResponse> {
        let qsf_session_id = self.allocate_session_id();
        let config = BrowserSessionConfig::default();
        {
            let sessions = self.inner.sessions.lock().await;
            if sessions.contains_key(&qsf_session_id) {
                anyhow::bail!("session `{qsf_session_id}` is already active");
            }
        }

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

        let mut runtime = SessionRuntime::new(qsf_session_id.clone(), config.clone(), diagnostics);

        let snapshot_path = self.continuity_volition_snapshot_path(&qsf_session_id);
        if snapshot_path.exists() {
            match VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path) {
                Ok(snapshot) => {
                    let tick = snapshot.state.tick;
                    runtime.volition.state = snapshot.state;
                    runtime
                        .diagnostics
                        .write(&DiagnosticRecord::VolitionContinuityNote {
                            qsf_session_id: qsf_session_id.clone(),
                            recorded_at: OffsetDateTime::now_utc(),
                            note: format!(
                                "restored volition state from continuity snapshot (tick={tick})"
                            ),
                            artifact_reference: snapshot_path.display().to_string(),
                        })?;
                }
                Err(error) => {
                    runtime
                        .diagnostics
                        .write(&DiagnosticRecord::VolitionContinuityNote {
                            qsf_session_id: qsf_session_id.clone(),
                            recorded_at: OffsetDateTime::now_utc(),
                            note: format!(
                                "volition continuity snapshot could not be loaded: {error}"
                            ),
                            artifact_reference: snapshot_path.display().to_string(),
                        })?;
                }
            }
        }

        let reviewed_seed_path = self.continuity_reviewed_volition_seed_path(&qsf_session_id);
        if let Some(reviewed_seed) =
            crate::realtime::volition_continuity::load_reviewed_seed_or_note(
                &qsf_session_id,
                &reviewed_seed_path,
                &runtime.diagnostics,
            )
        {
            if let Err(error) = crate::realtime::volition_continuity::apply_reviewed_seed_to_runtime(
                &mut runtime.volition,
                &reviewed_seed,
            ) {
                runtime
                    .diagnostics
                    .write(&DiagnosticRecord::VolitionContinuityNote {
                        qsf_session_id: qsf_session_id.clone(),
                        recorded_at: OffsetDateTime::now_utc(),
                        note: format!(
                            "reviewed volition seed `{}` was rejected during seeding: {error}",
                            reviewed_seed_path.display()
                        ),
                        artifact_reference: reviewed_seed_path.display().to_string(),
                    })?;
            }
        }
        let mut sessions = self.inner.sessions.lock().await;
        if sessions.contains_key(&qsf_session_id) {
            anyhow::bail!("session `{qsf_session_id}` is already active");
        }
        sessions.insert(qsf_session_id.clone(), Arc::new(Mutex::new(runtime)));

        Ok(SessionAllocationResponse {
            qsf_session_id,
            session: config,
        })
    }

    fn allocate_session_id(&self) -> String {
        match self.inner.session_id_mode {
            SessionIdMode::Default => DEFAULT_QSF_SESSION_ID.to_string(),
            SessionIdMode::RandomUuid => Uuid::new_v4().to_string(),
        }
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
    #[serde(default)]
    pub input_transcription_model: Option<String>,
    #[serde(default)]
    pub tools: Vec<RealtimeToolDefinition>,
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
                        create_response: false,
                        interrupt_response: false,
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
            instructions: default_instructions_with_volition_baseline(),
            input_transcription_model: Some(DEFAULT_INPUT_TRANSCRIPTION_MODEL.to_string()),
            tools: crate::realtime::tools::default_tool_definitions(),
            audio: BrowserSessionAudio {
                output: BrowserSessionAudioOutput {
                    voice: "marin".to_string(),
                },
                input: BrowserSessionAudioInput {
                    turn_detection: BrowserSessionTurnDetection {
                        kind: "server_vad".to_string(),
                        create_response: false,
                        interrupt_response: false,
                    },
                },
            },
        }
    }
}

fn default_instructions_with_volition_baseline() -> String {
    format!(
        "{base}\n\n{baseline}",
        base = DEFAULT_INSTRUCTIONS.trim(),
        baseline = build_stable_baseline_instructions(&realtime_seed_fixture(), Mode::Neutral),
    )
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

/// Health of the server-side sideband, pushed to the browser over the events
/// socket so the UI can surface server-only failures (e.g. a sideband that
/// cannot attach) instead of sitting silently in "Listening".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidebandStatus {
    pub degraded: bool,
    pub detail: Option<String>,
}

pub struct SessionRuntime {
    pub qsf_session_id: String,
    pub config: BrowserSessionConfig,
    pub session_state: SessionState,
    pub relay_state: LiveSessionState,
    pub tool_registry: qsf_tools::ToolRegistry,
    pub next_exchange_index: usize,
    pub next_trusted_exchange_index: usize,
    pub seen_event_ids: HashSet<String>,
    pub call_binding: Option<CallBinding>,
    pub diagnostics: DiagnosticWriter,
    pub trust: DiagnosticTrust,
    pub persisted_exchange_count: usize,
    pub trusted_promoted_exchange_count: usize,
    pub non_promotable_exchange_indices: HashSet<usize>,
    pub degraded: bool,
    pub sideband: Option<crate::realtime::sideband::SidebandHandle>,
    pub volition: crate::realtime::volition::VolitionRuntimeState,
    status_tx: watch::Sender<SidebandStatus>,
    turn_context_tx: watch::Sender<Option<TurnContextCapture>>,
    volition_inspection_tx: watch::Sender<Option<VolitionInspectionCapture>>,
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
        let tool_registry = crate::realtime::tools::build_tool_registry(&config.tools);
        Self {
            qsf_session_id,
            config,
            session_state,
            relay_state: LiveSessionState::default(),
            tool_registry,
            next_exchange_index: 0,
            next_trusted_exchange_index: 0,
            seen_event_ids: HashSet::new(),
            call_binding: None,
            diagnostics,
            trust: DiagnosticTrust::Untrusted,
            persisted_exchange_count: 0,
            trusted_promoted_exchange_count: 0,
            non_promotable_exchange_indices: HashSet::new(),
            degraded: false,
            sideband: None,
            volition: crate::realtime::volition::VolitionRuntimeState::new(),
            status_tx: watch::channel(SidebandStatus::default()).0,
            turn_context_tx: watch::channel(None).0,
            volition_inspection_tx: watch::channel(None).0,
        }
    }

    /// Subscribe to sideband health updates. The receiver immediately observes
    /// the latest status, so a socket that attaches after a failure still sees
    /// the degraded state.
    pub fn subscribe_status(&self) -> watch::Receiver<SidebandStatus> {
        self.status_tx.subscribe()
    }

    /// Subscribe to turn-context captures. A late-joining subscriber immediately
    /// observes the most recent capture stored in the channel, even if it was
    /// published before this call (watch channel guarantee).
    pub fn subscribe_turn_context(&self) -> watch::Receiver<Option<TurnContextCapture>> {
        self.turn_context_tx.subscribe()
    }

    /// Return a clone of the turn-context sender so callers outside
    /// `SessionRuntime` can publish captures without holding a `MutexGuard`.
    pub fn turn_context_sender(&self) -> watch::Sender<Option<TurnContextCapture>> {
        self.turn_context_tx.clone()
    }

    /// Subscribe to volition inspection captures. A late-joining subscriber
    /// immediately observes the most recent capture stored in the channel, even
    /// if it was published before this call (watch channel guarantee).
    pub fn subscribe_volition_inspection(
        &self,
    ) -> watch::Receiver<Option<VolitionInspectionCapture>> {
        self.volition_inspection_tx.subscribe()
    }

    /// Return a clone of the volition-inspection sender so callers outside
    /// `SessionRuntime` can publish captures without holding a `MutexGuard`.
    pub fn volition_inspection_sender(&self) -> watch::Sender<Option<VolitionInspectionCapture>> {
        self.volition_inspection_tx.clone()
    }

    /// Record the current sideband health and notify any status subscribers.
    /// Keeps `degraded` and the broadcast status in lockstep.
    pub fn set_sideband_status(&mut self, degraded: bool, detail: Option<String>) {
        self.degraded = degraded;
        self.status_tx
            .send_replace(SidebandStatus { degraded, detail });
    }

    pub fn new_exchange_index(&mut self) -> usize {
        let index = self.next_exchange_index;
        self.next_exchange_index += 1;
        index
    }

    pub fn new_trusted_exchange_index(&mut self) -> usize {
        let index = self.next_trusted_exchange_index;
        self.next_trusted_exchange_index += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_session_id_is_stable_and_rejects_second_active_session() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            SessionIdMode::Default,
        )
        .expect("state");

        let allocation = state.create_session().await.expect("session");

        assert_eq!(allocation.qsf_session_id, DEFAULT_QSF_SESSION_ID);
        assert!(
            state
                .session_runtime(DEFAULT_QSF_SESSION_ID)
                .await
                .is_some()
        );

        let error = state.create_session().await.unwrap_err();
        assert!(error.to_string().contains("already active"));
    }

    #[tokio::test]
    async fn random_session_id_mode_allocates_distinct_uuids() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            SessionIdMode::RandomUuid,
        )
        .expect("state");

        let first = state.create_session().await.expect("first session");
        let second = state.create_session().await.expect("second session");

        assert_ne!(first.qsf_session_id, second.qsf_session_id);
        Uuid::parse_str(&first.qsf_session_id).expect("first uuid");
        Uuid::parse_str(&second.qsf_session_id).expect("second uuid");
    }

    #[tokio::test]
    async fn new_session_has_fixture_backed_volition_state() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            SessionIdMode::Default,
        )
        .expect("state");

        let allocation = state.create_session().await.expect("session");
        let session = state
            .session_runtime(&allocation.qsf_session_id)
            .await
            .expect("session runtime");
        let guard = session.lock().await;

        assert_eq!(guard.volition.state.tick, 0);
        assert!(
            guard
                .volition
                .state
                .goals
                .contains_key("honor-explicit-user-request"),
            "realtime seed must include tier-2 protected goal"
        );
        assert!(
            guard
                .volition
                .state
                .goals
                .contains_key("complete-current-task"),
            "realtime seed must include tier-3 protected goal"
        );
    }

    #[tokio::test]
    async fn random_sessions_get_isolated_volition_state() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            SessionIdMode::RandomUuid,
        )
        .expect("state");

        let first_alloc = state.create_session().await.expect("first session");
        let second_alloc = state.create_session().await.expect("second session");

        let first = state
            .session_runtime(&first_alloc.qsf_session_id)
            .await
            .expect("first runtime");
        {
            let mut guard = first.lock().await;
            let new_tick = guard.volition.state.tick + 1;
            let events = crate::realtime::volition::events_for_trusted_transcript(
                "how can you help",
                &guard.volition.state,
                &guard.volition.fixture,
                new_tick,
            );
            guard.volition.apply_events(events);
        }

        let second = state
            .session_runtime(&second_alloc.qsf_session_id)
            .await
            .expect("second runtime");
        let second_guard = second.lock().await;
        assert_eq!(
            second_guard.volition.state.tick, 0,
            "mutating first session volition must not affect second session"
        );
    }

    #[tokio::test]
    async fn removed_session_cleans_up_volition_state() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
            SessionIdMode::Default,
        )
        .expect("state");

        state.create_session().await.expect("session");
        let removed = state.remove_session(DEFAULT_QSF_SESSION_ID).await;
        assert!(removed.is_some(), "remove_session must return the runtime");
        assert!(
            state
                .session_runtime(DEFAULT_QSF_SESSION_ID)
                .await
                .is_none(),
            "session must be gone after removal"
        );
    }

    #[test]
    fn turn_context_watch_holds_value_for_late_subscriber() {
        use crate::realtime::turn_context::build_turn_context_capture;

        let tempdir = tempfile::tempdir().expect("tempdir");
        let diagnostics =
            DiagnosticWriter::create(tempdir.path().join("test.jsonl")).expect("diagnostics");
        let runtime = SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        );

        let capture = build_turn_context_capture(
            "test-session".to_string(),
            0,
            "hash-abc".to_string(),
            vec![serde_json::json!({"role": "user", "content": "hello"})],
        );

        // Publish with no active receivers — the initial receiver was dropped at
        // construction (only the Sender half is stored in SessionRuntime).
        runtime
            .turn_context_sender()
            .send_replace(Some(capture.clone()));

        // A late-joining subscriber must immediately see the stored capture.
        let rx = runtime.subscribe_turn_context();
        let received = rx.borrow().clone();
        assert!(
            received.is_some(),
            "late subscriber must see stored capture"
        );
        let received = received.unwrap();
        assert_eq!(received.request_hash, "hash-abc");
        assert_eq!(received.qsf_session_id, "test-session");
        assert_eq!(received.exchange_index, 0);
    }

    #[test]
    fn volition_inspection_watch_holds_value_for_late_subscriber() {
        use crate::realtime::volition_inspection_capture::build_volition_inspection_capture;
        use qsf_volition::{VolitionState, build_state_inspection, realtime_seed_fixture};

        let tempdir = tempfile::tempdir().expect("tempdir");
        let diagnostics =
            DiagnosticWriter::create(tempdir.path().join("test.jsonl")).expect("diagnostics");
        let runtime = SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        );

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let capture = build_volition_inspection_capture(
            "test-session".to_string(),
            0,
            time::OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            inspection,
            None,
        );

        runtime
            .volition_inspection_sender()
            .send_replace(Some(capture.clone()));

        let rx = runtime.subscribe_volition_inspection();
        let received = rx.borrow().clone();
        assert!(
            received.is_some(),
            "late subscriber must see stored capture"
        );
        let received = received.unwrap();
        assert_eq!(received.response_create_event_ref, "request-ref");
        assert_eq!(received.qsf_session_id, "test-session");
        assert_eq!(received.exchange_index, 0);
    }

    #[test]
    fn volition_inspection_state_isolated_between_sessions() {
        use crate::realtime::volition_inspection_capture::build_volition_inspection_capture;
        use qsf_volition::{VolitionState, build_state_inspection, realtime_seed_fixture};

        let tempdir = tempfile::tempdir().expect("tempdir");
        let diagnostics_a =
            DiagnosticWriter::create(tempdir.path().join("test-a.jsonl")).expect("diagnostics");
        let diagnostics_b =
            DiagnosticWriter::create(tempdir.path().join("test-b.jsonl")).expect("diagnostics");
        let runtime_a = SessionRuntime::new(
            "session-a".to_string(),
            BrowserSessionConfig::default(),
            diagnostics_a,
        );
        let runtime_b = SessionRuntime::new(
            "session-b".to_string(),
            BrowserSessionConfig::default(),
            diagnostics_b,
        );

        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let capture = build_volition_inspection_capture(
            "session-a".to_string(),
            0,
            time::OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            build_state_inspection(&state, &fixture),
            None,
        );

        runtime_a
            .volition_inspection_sender()
            .send_replace(Some(capture));

        let received_a = runtime_a.subscribe_volition_inspection().borrow().clone();
        let received_b = runtime_b.subscribe_volition_inspection().borrow().clone();
        assert!(received_a.is_some());
        assert!(received_b.is_none());
    }
}
