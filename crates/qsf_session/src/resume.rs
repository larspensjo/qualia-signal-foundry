use std::path::Path;

use crate::manifest::{ContinuityManifest, ResumeMode};
use crate::persistence;
use crate::state::SessionState;

#[derive(Clone, Debug, PartialEq)]
pub struct ResumeInputs {
    pub manifest: ContinuityManifest,
    pub previous_session: Option<SessionState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaUpgrade {
    pub session_id: String,
    pub from: u32,
    pub to: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadResumeInputsResult {
    pub inputs: ResumeInputs,
    pub schema_upgrade: Option<SchemaUpgrade>,
}

pub fn load_resume_inputs_with_upgrade(
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<LoadResumeInputsResult> {
    let state_dir = state_dir.as_ref();
    let manifest_path = state_dir.join("continuity-manifest.json");
    let manifest = ContinuityManifest::load_or_default(&manifest_path)?;

    let (previous_session, schema_upgrade) = match manifest.current_session_state_path.as_ref() {
        Some(path) => {
            let path = if path.is_absolute() {
                path.clone()
            } else {
                state_dir.join(path)
            };
            if path.exists() {
                let mut previous_session = persistence::load_session_state(path)?;
                let previous_schema_version = previous_session.schema_version;
                previous_session.upgrade_schema_version();
                let schema_upgrade = (previous_session.schema_version != previous_schema_version)
                    .then_some(SchemaUpgrade {
                        session_id: previous_session.session_id.clone(),
                        from: previous_schema_version,
                        to: previous_session.schema_version,
                    });
                (Some(previous_session), schema_upgrade)
            } else {
                (None, None)
            }
        }
        None => (None, None),
    };

    Ok(LoadResumeInputsResult {
        inputs: ResumeInputs {
            manifest,
            previous_session,
        },
        schema_upgrade,
    })
}

pub fn classify_resume_mode(inputs: &ResumeInputs) -> ResumeMode {
    match (
        &inputs.previous_session,
        inputs.manifest.sleep_pending,
        &inputs.manifest.last_sleep_run_id,
    ) {
        (None, _, _) => ResumeMode::ColdStart,
        (Some(_), true, _) => ResumeMode::AwakeContinuation,
        (Some(_), false, Some(_)) => ResumeMode::ConsolidatedBrief,
        (Some(_), false, None) => ResumeMode::ColdStart,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    use crate::manifest::ContinuityManifest;
    use crate::state::{MemorySourceConfig, SessionConfig};

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }

    fn inputs(prev: Option<SessionState>, pending: bool, last_sleep: Option<&str>) -> ResumeInputs {
        ResumeInputs {
            manifest: ContinuityManifest {
                sleep_pending: pending,
                last_sleep_run_id: last_sleep.map(str::to_string),
                current_volition_snapshot_path: None,
                ..ContinuityManifest::default()
            },
            previous_session: prev,
        }
    }

    #[test]
    fn no_previous_session_is_cold_start() {
        let r = classify_resume_mode(&inputs(None, false, None));
        assert_eq!(r, ResumeMode::ColdStart);
    }

    #[test]
    fn previous_session_with_sleep_pending_is_awake_continuation() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), true, None));

        assert_eq!(r, ResumeMode::AwakeContinuation);
    }

    #[test]
    fn previous_session_with_consumed_sleep_is_consolidated_brief() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), false, Some("sleep-1")));

        assert_eq!(r, ResumeMode::ConsolidatedBrief);
    }

    #[test]
    fn previous_session_with_no_sleep_history_is_cold_start_fallback() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), false, None));

        assert_eq!(r, ResumeMode::ColdStart);
    }

    #[test]
    fn load_resume_inputs_upgrades_legacy_session_schema_version() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("session-state.json"),
            include_str!("../tests/fixtures/pre_migration_session_state.json"),
        )
        .unwrap();

        ContinuityManifest {
            current_session_state_path: Some("session-state.json".into()),
            current_volition_snapshot_path: None,
            ..ContinuityManifest::default()
        }
        .persist(dir.path().join("continuity-manifest.json"))
        .unwrap();

        let result = load_resume_inputs_with_upgrade(dir.path()).unwrap();

        assert_eq!(
            result
                .inputs
                .previous_session
                .as_ref()
                .map(|state| state.schema_version),
            Some(crate::state::SESSION_STATE_SCHEMA_VERSION)
        );
        assert_eq!(
            result.schema_upgrade.as_ref().map(|upgrade| upgrade.from),
            Some(1)
        );
    }
}
