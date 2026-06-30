use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::realtime::turn_integrity::TurnPhase;
use crate::realtime::volition_initiative::RealtimeBoundedInitiativeTrace;
use crate::realtime::volition_injection::VolitionContextInjectionTrace;
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
        at: OffsetDateTime,
    },
    NoSecretEvidence {
        qsf_session_id: String,
        at: OffsetDateTime,
        note: String,
    },
    CallBound {
        qsf_session_id: String,
        call_id: String,
        bound_at: OffsetDateTime,
    },
    CallInvalidated {
        qsf_session_id: String,
        call_id: String,
        invalidated_at: OffsetDateTime,
        reason: String,
    },
    RelayEventReceived {
        qsf_session_id: String,
        event_id: String,
        event_kind: String,
        at: OffsetDateTime,
    },
    LatencyObservation {
        qsf_session_id: String,
        label: String,
        started_at: OffsetDateTime,
        finished_at: OffsetDateTime,
        latency_ms: i64,
    },
    VolitionContextInjected {
        qsf_session_id: String,
        exchange_index: usize,
        recorded_at: OffsetDateTime,
        trace: VolitionContextInjectionTrace,
    },
    RealtimeBoundedInitiative {
        qsf_session_id: String,
        exchange_index: usize,
        recorded_at: OffsetDateTime,
        trace: RealtimeBoundedInitiativeTrace,
    },
    VolitionContinuityNote {
        qsf_session_id: String,
        recorded_at: OffsetDateTime,
        note: String,
        artifact_reference: String,
    },
    DiagnosticExchangeRecorded {
        qsf_session_id: String,
        source: String,
        trust: DiagnosticTrust,
        recorded_at: OffsetDateTime,
        exchange: Exchange,
    },
    IgnoredContinuationTranscript {
        qsf_session_id: String,
        transcript: String,
        turn_phase: TurnPhase,
        response_id: Option<String>,
        at: OffsetDateTime,
    },
    StaleProviderEvent {
        qsf_session_id: String,
        response_id: Option<String>,
        status: Option<String>,
        exchange_index: Option<usize>,
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
