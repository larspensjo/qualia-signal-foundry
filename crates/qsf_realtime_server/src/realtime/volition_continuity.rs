use std::path::{Path, PathBuf};

use anyhow::Context;
use qsf_session::{ContinuityManifest, ResumeMode, persist_session_state};
use qsf_volition::{
    REALTIME_SEED_FIXTURE_ID, ReviewedVolitionSeed, VolitionContinuitySnapshot,
    apply_reviewed_seed_in_place, build_state_inspection, load_reviewed_volition_seed,
    persist_volition_continuity_snapshot,
};
use time::OffsetDateTime;

use crate::diagnostics::{DiagnosticRecord, DiagnosticWriter};
use crate::state::{AppState, SessionRuntime};

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

/// Persist the canonical session state, volition snapshot, and continuity manifest together.
/// Promotion and the later end-of-run finalizer share this helper so detached formation results
/// use exactly the same artifact paths and manifest semantics as promoted turns.
pub(crate) fn persist_continuity_state_and_volition_snapshot(
    state: &AppState,
    runtime: &SessionRuntime,
) -> anyhow::Result<()> {
    let continuity_dir = state.continuity_session_dir(&runtime.qsf_session_id);
    let state_path = persist_session_state(&runtime.session_state, &continuity_dir)?;
    let snapshot = build_volition_continuity_snapshot(
        &runtime.qsf_session_id,
        &runtime.volition,
        OffsetDateTime::now_utc(),
    )?;
    let snapshot_path = persist_snapshot(
        &snapshot,
        state.continuity_volition_snapshot_path(&runtime.qsf_session_id),
    )?;
    let mut manifest = ContinuityManifest::load_or_default(
        state.continuity_manifest_path(&runtime.qsf_session_id),
    )?;
    manifest.current_session_id = Some(runtime.qsf_session_id.clone());
    manifest.current_session_state_path = Some(
        state_path
            .strip_prefix(&continuity_dir)
            .unwrap_or(&state_path)
            .to_path_buf(),
    );
    manifest.current_volition_snapshot_path = Some(
        snapshot_path
            .strip_prefix(&continuity_dir)
            .unwrap_or(&snapshot_path)
            .to_path_buf(),
    );
    manifest.sleep_pending = true;
    manifest.resume_mode = ResumeMode::AwakeContinuation;
    manifest.persist(state.continuity_manifest_path(&runtime.qsf_session_id))?;
    Ok(())
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
    apply_reviewed_seed_in_place(&mut runtime.state, &runtime.fixture, reviewed_seed)
}
