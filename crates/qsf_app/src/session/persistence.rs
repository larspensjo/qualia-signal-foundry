use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tempfile::NamedTempFile;

use crate::session::SessionState;

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
    let parsed = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse session state `{}`", path.display()))?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::session::{MemorySourceConfig, SessionConfig};

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
}
