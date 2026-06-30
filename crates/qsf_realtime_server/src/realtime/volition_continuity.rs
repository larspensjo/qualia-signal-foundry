use std::path::{Path, PathBuf};

use anyhow::Context;
use qsf_volition::{
    REALTIME_SEED_FIXTURE_ID, ReviewedVolitionSeed, VolitionContinuitySnapshot,
    apply_reviewed_seed, build_state_inspection, load_reviewed_volition_seed,
    persist_volition_continuity_snapshot,
};
use time::OffsetDateTime;

use crate::diagnostics::{DiagnosticRecord, DiagnosticWriter};

use super::volition::VolitionRuntimeState;

pub fn build_volition_continuity_snapshot(
    qsf_session_id: &str,
    runtime: &VolitionRuntimeState,
    recorded_at: OffsetDateTime,
) -> anyhow::Result<VolitionContinuitySnapshot> {
    let recorded_at = recorded_at
        .format(&time::format_description::well_known::Rfc3339)
        .context("failed to format volition snapshot timestamp as RFC3339")?;
    Ok(VolitionContinuitySnapshot::new(
        qsf_session_id.to_string(),
        recorded_at,
        REALTIME_SEED_FIXTURE_ID,
        runtime.state.clone(),
        build_state_inspection(&runtime.state, &runtime.fixture),
    ))
}

pub fn persist_snapshot(
    snapshot: &VolitionContinuitySnapshot,
    path: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    persist_volition_continuity_snapshot(snapshot, path)
}

pub fn load_reviewed_seed_or_note(
    qsf_session_id: &str,
    path: impl AsRef<Path>,
    diagnostics: &DiagnosticWriter,
) -> Option<ReviewedVolitionSeed> {
    let path = path.as_ref();
    match load_reviewed_volition_seed(path) {
        Ok(seed) => Some(seed),
        Err(error) if path.exists() => {
            let _ = diagnostics.write(&DiagnosticRecord::VolitionContinuityNote {
                qsf_session_id: qsf_session_id.to_string(),
                recorded_at: OffsetDateTime::now_utc(),
                note: format!(
                    "reviewed volition seed `{}` could not be loaded: {error}",
                    path.display()
                ),
                artifact_reference: path.display().to_string(),
            });
            None
        }
        Err(error) => {
            let _ = diagnostics.write(&DiagnosticRecord::VolitionContinuityNote {
                qsf_session_id: qsf_session_id.to_string(),
                recorded_at: OffsetDateTime::now_utc(),
                note: format!(
                    "reviewed volition seed `{}` is absent; using realtime fixture seed ({error})",
                    path.display()
                ),
                artifact_reference: path.display().to_string(),
            });
            None
        }
    }
}

pub fn apply_reviewed_seed_to_runtime(
    runtime: &mut VolitionRuntimeState,
    reviewed_seed: &ReviewedVolitionSeed,
) -> anyhow::Result<()> {
    runtime.state = apply_reviewed_seed(&runtime.fixture, reviewed_seed)?;
    Ok(())
}
