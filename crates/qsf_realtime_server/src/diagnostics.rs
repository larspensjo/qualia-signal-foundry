use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::realtime::live_goal_formation::LiveGoalFormationTrace;
use crate::realtime::turn_integrity::TurnPhase;
use crate::realtime::volition_initiative::RealtimeBoundedInitiativeTrace;
use crate::realtime::volition_injection::VolitionContextInjectionTrace;
use crate::realtime::world_consultation::WorldConsultationTrace;
use qsf_session::Exchange;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticTrust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiagnosticRecord {
    SessionAllocated {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    NoSecretEvidence {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
        note: String,
    },
    CallBound {
        qsf_session_id: String,
        call_id: String,
        #[serde(with = "time::serde::rfc3339")]
        bound_at: OffsetDateTime,
    },
    CallInvalidated {
        qsf_session_id: String,
        call_id: String,
        #[serde(with = "time::serde::rfc3339")]
        invalidated_at: OffsetDateTime,
        reason: String,
    },
    RelayEventReceived {
        qsf_session_id: String,
        event_id: String,
        event_kind: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    LatencyObservation {
        qsf_session_id: String,
        label: String,
        #[serde(with = "time::serde::rfc3339")]
        started_at: OffsetDateTime,
        #[serde(with = "time::serde::rfc3339")]
        finished_at: OffsetDateTime,
        latency_ms: i64,
    },
    VolitionContextInjected {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: VolitionContextInjectionTrace,
    },
    RealtimeBoundedInitiative {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: RealtimeBoundedInitiativeTrace,
    },
    /// Authoritative external-effect boundary for a successful or deferred world-corpus read.
    WorldConsultationPerformed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: WorldConsultationTrace,
    },
    LiveGoalFormationPerformed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: LiveGoalFormationTrace,
    },
    /// Persistent record of a formation attempt that errored (provider failure, non-JSON
    /// output, or judge-output validation failure) — without this, "formation failed" was
    /// indistinguishable in the diagnostics from "formation never attempted".
    LiveGoalFormationFailed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        error: String,
    },
    /// Persistent record of a formation attempt that did not apply because it was skipped or
    /// discarded by runtime guards.
    LiveGoalFormationSkipped {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        reason: String,
    },
    VolitionContinuityNote {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        note: String,
        artifact_reference: String,
    },
    /// Verbatim model-visible request sequence for one trusted turn, persisted so
    /// experiments can verify what was actually sent to the provider.
    TurnContextCaptured {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        request_hash: String,
        messages: Vec<serde_json::Value>,
    },
    DiagnosticExchangeRecorded {
        qsf_session_id: String,
        source: String,
        trust: DiagnosticTrust,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        exchange: Exchange,
    },
    IgnoredContinuationTranscript {
        qsf_session_id: String,
        transcript: String,
        turn_phase: TurnPhase,
        response_id: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    StaleProviderEvent {
        qsf_session_id: String,
        response_id: Option<String>,
        status: Option<String>,
        exchange_index: Option<usize>,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
}

#[derive(Clone)]
pub struct DiagnosticWriter {
    path: PathBuf,
    file: std::sync::Arc<Mutex<BufWriter<File>>>,
}

impl DiagnosticWriter {
    pub fn create(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create diagnostics dir `{}`", parent.display())
            })?;
        }
        let file = File::options()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open diagnostics log `{}`", path.display()))?;
        Ok(Self {
            path,
            file: std::sync::Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, record: &DiagnosticRecord) -> anyhow::Result<()> {
        let mut guard = self.file.lock().expect("diagnostic writer mutex poisoned");
        serde_json::to_writer(&mut *guard, record)
            .context("failed to serialize diagnostic record")?;
        guard
            .write_all(b"\n")
            .context("failed to append newline to diagnostic record")?;
        guard.flush().context("failed to flush diagnostic record")?;
        Ok(())
    }
}
