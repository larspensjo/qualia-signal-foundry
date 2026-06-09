use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tempfile::NamedTempFile;

use crate::state::{SESSION_STATE_SCHEMA_VERSION, SessionState};

pub fn persist_session_state(
    state: &SessionState,
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    let state_dir = state_dir.as_ref();
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("failed to create state dir `{}`", state_dir.display()))?;
    let path = state_dir.join("session-state.json");

    let mut temp = NamedTempFile::new_in(state_dir).with_context(|| {
        format!(
            "failed to create temporary session state file in `{}`",
            state_dir.display()
        )
    })?;
    temp.as_file_mut()
        .write_all(serde_json::to_string_pretty(state)?.as_bytes())
        .with_context(|| {
            format!(
                "failed to write temporary session state `{}`",
                temp.path().display()
            )
        })?;
    temp.as_file().sync_all().with_context(|| {
        format!(
            "failed to sync temporary session state `{}` before persist",
            temp.path().display()
        )
    })?;
    temp.persist(&path).map_err(|error| {
        anyhow::anyhow!(
            "failed to persist session state `{}`: {}",
            path.display(),
            error.error
        )
    })?;

    Ok(path)
}

pub fn load_session_state(path: impl AsRef<Path>) -> anyhow::Result<SessionState> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session state `{}`", path.display()))?;
    let parsed: SessionState = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse session state `{}`", path.display()))?;
    if parsed.schema_version > SESSION_STATE_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported session state schema_version: found {} expected <= {}",
            parsed.schema_version,
            SESSION_STATE_SCHEMA_VERSION
        );
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::exchange::Exchange;
    use crate::state::{MemorySourceConfig, SessionConfig};

    fn sample_state() -> SessionState {
        SessionState::new_with_id(
            "s-roundtrip".to_string(),
            SessionConfig {
                model_id: "mock".to_string(),
                max_turns: 10,
                warm_threshold: 2,
                allow_over_limit: false,
                memory_source: MemorySourceConfig {
                    source: "fixture".to_string(),
                    file: None,
                },
            },
        )
    }

    #[test]
    fn persist_then_load_roundtrips() {
        let dir = TempDir::new().unwrap();
        let state = sample_state();
        let path = persist_session_state(&state, dir.path()).unwrap();
        let reloaded = load_session_state(&path).unwrap();

        assert_eq!(reloaded.session_id, state.session_id);
    }

    #[test]
    fn persist_overwrites_existing_file() {
        let dir = TempDir::new().unwrap();
        let mut state = sample_state();
        persist_session_state(&state, dir.path()).unwrap();

        state.last_input = Some("second run".to_string());
        let path = persist_session_state(&state, dir.path()).unwrap();
        let reloaded = load_session_state(&path).unwrap();

        assert_eq!(reloaded.last_input.as_deref(), Some("second run"));
    }

    #[test]
    fn persist_keeps_completed_exchanges_in_memory_only() {
        let dir = TempDir::new().unwrap();
        let mut state = sample_state();
        state.live.completed_exchanges.push(Exchange::new_text(
            0,
            "hello",
            std::time::SystemTime::UNIX_EPOCH,
        ));

        let path = persist_session_state(&state, dir.path()).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let reloaded = load_session_state(&path).unwrap();

        assert!(!raw.contains("completed_exchanges"));
        assert!(reloaded.live.completed_exchanges.is_empty());
    }

    #[test]
    fn load_rejects_newer_schema_version() {
        let dir = TempDir::new().unwrap();
        let mut state = sample_state();
        state.schema_version = SESSION_STATE_SCHEMA_VERSION + 1;
        let path = dir.path().join("session-state.json");
        std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

        let error = load_session_state(&path).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported session state schema_version")
        );
    }
}
