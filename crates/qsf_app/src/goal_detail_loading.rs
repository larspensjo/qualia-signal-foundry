use std::path::{Path, PathBuf};

use anyhow::Context;
use qsf_session::{
    ContinuitySessionResolution, continuity_session_dir, resolve_continuity_session_dir,
};
use qsf_volition::{
    GoalDetailReport, VolitionContinuitySnapshot, build_goal_detail_report, fixture_for_seed_id,
    known_seed_fixture_ids,
};

/// Snapshot metadata and the pure report ready for console rendering.
#[derive(Debug)]
pub struct LoadedGoalDetailReport {
    pub session_id: String,
    pub seed_fixture_id: String,
    pub recorded_at: String,
    pub report: GoalDetailReport,
}

/// Loads exactly one persisted continuity snapshot and builds its goal-detail report.
pub fn load_goal_detail_report(
    state_dir: &Path,
    requested_session: Option<&str>,
) -> anyhow::Result<LoadedGoalDetailReport> {
    let (continuity_dir, resolved_session_id) = resolve_session(state_dir, requested_session)?;
    let manifest_path = continuity_dir.join("continuity-manifest.json");
    if !manifest_path.exists() {
        anyhow::bail!(
            "continuity manifest for session `{resolved_session_id}` is absent at `{}`",
            manifest_path.display()
        );
    }

    let snapshot_path = crate::experiments::volition_continuity::snapshot_path_from_manifest(
        &manifest_path,
        &continuity_dir,
    )?;
    if !snapshot_path.exists() {
        anyhow::bail!(
            "volition snapshot for session `{resolved_session_id}` is absent at `{}`",
            snapshot_path.display()
        );
    }

    let snapshot =
        VolitionContinuitySnapshot::load_or_upgrade(&snapshot_path).with_context(|| {
            format!(
                "failed to load volition snapshot for session `{resolved_session_id}` at `{}`",
                snapshot_path.display()
            )
        })?;
    let Some(fixture) = fixture_for_seed_id(&snapshot.seed_fixture_id) else {
        anyhow::bail!(
            "unknown seed fixture id `{}` in snapshot `{}`; known fixture ids: {}",
            snapshot.seed_fixture_id,
            snapshot_path.display(),
            known_seed_fixture_ids().collect::<Vec<_>>().join(", ")
        );
    };

    engine_logging::engine_info!(
        "loaded goal detail session={} snapshot={} fixture={}",
        resolved_session_id,
        snapshot_path.display(),
        snapshot.seed_fixture_id
    );

    Ok(LoadedGoalDetailReport {
        session_id: if resolved_session_id.is_empty() {
            snapshot.qsf_session_id.clone()
        } else {
            resolved_session_id
        },
        seed_fixture_id: snapshot.seed_fixture_id.clone(),
        recorded_at: snapshot.recorded_at.clone(),
        report: build_goal_detail_report(&snapshot.state, &fixture),
    })
}

fn resolve_session(
    state_dir: &Path,
    requested_session: Option<&str>,
) -> anyhow::Result<(PathBuf, String)> {
    if let Some(session_id) = requested_session {
        return Ok((
            continuity_session_dir(state_dir, session_id),
            session_id.to_string(),
        ));
    }

    match resolve_continuity_session_dir(state_dir)? {
        ContinuitySessionResolution::Nested {
            session_dir,
            session_id,
        } => Ok((session_dir, session_id)),
        ContinuitySessionResolution::Flat(session_dir) => Ok((session_dir, String::new())),
        ContinuitySessionResolution::None => anyhow::bail!(
            "no continuity session manifest found under `{}`",
            state_dir.join("continuity").display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use qsf_session::ContinuityManifest;
    use qsf_volition::{
        REALTIME_SEED_FIXTURE_ID, VolitionState, build_state_inspection,
        persist_volition_continuity_snapshot, realtime_seed_fixture,
    };

    use super::*;

    fn write_snapshot_fixture(
        state_root: &Path,
        session_id: &str,
        seed_fixture_id: &str,
    ) -> anyhow::Result<PathBuf> {
        let continuity_dir = continuity_session_dir(state_root, session_id);
        std::fs::create_dir_all(&continuity_dir)?;
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let snapshot = VolitionContinuitySnapshot::new(
            session_id,
            "2026-07-19T00:00:00Z",
            seed_fixture_id,
            state.clone(),
            build_state_inspection(&state, &fixture),
        );
        let snapshot_path = continuity_dir.join("volition-state.json");
        persist_volition_continuity_snapshot(&snapshot, &snapshot_path)?;
        ContinuityManifest {
            current_volition_snapshot_path: Some(PathBuf::from("volition-state.json")),
            ..ContinuityManifest::default()
        }
        .persist(continuity_dir.join("continuity-manifest.json"))?;
        Ok(snapshot_path)
    }

    #[test]
    fn present_snapshot_resolves_and_builds_a_report() {
        let temp = tempfile::tempdir().unwrap();
        write_snapshot_fixture(temp.path(), "default", REALTIME_SEED_FIXTURE_ID).unwrap();

        let loaded = load_goal_detail_report(temp.path(), None).unwrap();

        assert_eq!(loaded.session_id, "default");
        assert_eq!(loaded.seed_fixture_id, REALTIME_SEED_FIXTURE_ID);
        assert!(!loaded.report.goals.is_empty());
    }

    #[test]
    fn absent_snapshot_error_names_resolved_path() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = continuity_session_dir(temp.path(), "chosen");
        std::fs::create_dir_all(&session_dir).unwrap();
        ContinuityManifest::default()
            .persist(session_dir.join("continuity-manifest.json"))
            .unwrap();

        let error = load_goal_detail_report(temp.path(), Some("chosen")).unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("volition-state.json"),
            "message: {message}"
        );
        assert!(message.contains("chosen"), "message: {message}");
    }

    #[test]
    fn unknown_fixture_error_names_unknown_and_known_ids() {
        let temp = tempfile::tempdir().unwrap();
        write_snapshot_fixture(temp.path(), "default", "other-fixture").unwrap();

        let error = load_goal_detail_report(temp.path(), None).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("other-fixture"), "message: {message}");
        assert!(
            message.contains(REALTIME_SEED_FIXTURE_ID),
            "message: {message}"
        );
    }
}
