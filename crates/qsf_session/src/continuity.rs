//! Continuity directory layout shared by the realtime server (which writes per-session
//! continuity state) and the sleep/consolidation command (which reads it back).
//!
//! Two layouts exist in the wild:
//! - **Flat**: `continuity-manifest.json` sits directly in the state directory. The
//!   text-loop experiments persist and resume this way.
//! - **Nested**: the realtime server organizes state under
//!   `<state_dir>/continuity/<session_id>/`, keeping one manifest per session.
//!
//! Keeping the layout constants and the resolver here is the single source of truth so the
//! writer (realtime server) and the reader (sleep) cannot drift apart — the drift that made
//! `qsf sleep` report `no_persisted_session` against a real realtime session.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// The stable session id used when the realtime server is not in random-session mode.
/// This is the source of truth; `qsf_realtime_server` derives its own alias from it.
pub const DEFAULT_SESSION_ID: &str = "default";

/// Subdirectory under a realtime state root that holds per-session continuity dirs.
pub const CONTINUITY_SUBDIR: &str = "continuity";

const CONTINUITY_MANIFEST_FILE: &str = "continuity-manifest.json";

/// `<state_dir>/continuity`
pub fn continuity_root_dir(state_dir: impl AsRef<Path>) -> PathBuf {
    state_dir.as_ref().join(CONTINUITY_SUBDIR)
}

/// `<state_dir>/continuity/<session_id>`
pub fn continuity_session_dir(state_dir: impl AsRef<Path>, session_id: &str) -> PathBuf {
    continuity_root_dir(state_dir).join(session_id)
}

/// How a state directory resolved to a concrete continuity session directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContinuitySessionResolution {
    /// The state dir itself holds `continuity-manifest.json` (flat layout).
    Flat(PathBuf),
    /// A nested `<state_dir>/continuity/<session_id>` dir holds the manifest (realtime layout).
    Nested {
        session_dir: PathBuf,
        session_id: String,
    },
    /// No continuity manifest was found under the state dir in either layout.
    None,
}

impl ContinuitySessionResolution {
    /// The directory a caller should load/persist continuity state in, if one was found.
    pub fn session_dir(&self) -> Option<&Path> {
        match self {
            Self::Flat(dir) => Some(dir),
            Self::Nested { session_dir, .. } => Some(session_dir),
            Self::None => None,
        }
    }
}

/// Resolve the continuity session directory to load/persist for a given state root.
///
/// Supports both the flat layout (manifest directly under `state_dir`) and the realtime
/// nested layout (`state_dir/continuity/<session_id>/`). When multiple nested sessions
/// exist, the stable [`DEFAULT_SESSION_ID`] session wins; otherwise a single session is
/// used. An ambiguous set (multiple sessions, none named `default`) is reported as an error
/// so the caller can ask the operator to pass an explicit directory rather than guessing.
pub fn resolve_continuity_session_dir(
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<ContinuitySessionResolution> {
    let state_dir = state_dir.as_ref();

    // Flat layout wins when the manifest is right here: this preserves the text-loop
    // experiments that persist and resume against a single directory.
    if state_dir.join(CONTINUITY_MANIFEST_FILE).exists() {
        return Ok(ContinuitySessionResolution::Flat(state_dir.to_path_buf()));
    }

    let root = continuity_root_dir(state_dir);
    if !root.is_dir() {
        return Ok(ContinuitySessionResolution::None);
    }

    let mut sessions: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&root)
        .with_context(|| format!("failed to read continuity root `{}`", root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if !entry.path().join(CONTINUITY_MANIFEST_FILE).exists() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            sessions.push(name.to_string());
        }
    }
    sessions.sort();

    let selected = if sessions.iter().any(|session| session == DEFAULT_SESSION_ID) {
        Some(DEFAULT_SESSION_ID.to_string())
    } else if sessions.len() == 1 {
        Some(sessions[0].clone())
    } else {
        None
    };

    match selected {
        Some(session_id) => Ok(ContinuitySessionResolution::Nested {
            session_dir: root.join(&session_id),
            session_id,
        }),
        None if sessions.is_empty() => Ok(ContinuitySessionResolution::None),
        None => anyhow::bail!(
            "multiple continuity sessions found under `{}` ({}); \
             pass an explicit --state-dir <state_dir>/{}/<session_id> to pick one",
            root.display(),
            sessions.join(", "),
            CONTINUITY_SUBDIR,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(CONTINUITY_MANIFEST_FILE), b"{}").unwrap();
    }

    #[test]
    fn flat_layout_resolves_to_the_state_dir_itself() {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(temp.path());

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(
            resolution,
            ContinuitySessionResolution::Flat(temp.path().to_path_buf())
        );
    }

    #[test]
    fn nested_default_session_is_resolved() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = continuity_session_dir(temp.path(), DEFAULT_SESSION_ID);
        write_manifest(&session_dir);

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(resolution.session_dir(), Some(session_dir.as_path()));
        assert!(matches!(
            resolution,
            ContinuitySessionResolution::Nested { session_id, .. } if session_id == DEFAULT_SESSION_ID
        ));
    }

    #[test]
    fn single_nested_session_is_resolved_even_without_default_name() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = continuity_session_dir(temp.path(), "abc-123");
        write_manifest(&session_dir);

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(resolution.session_dir(), Some(session_dir.as_path()));
    }

    #[test]
    fn default_wins_when_multiple_nested_sessions_exist() {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(&continuity_session_dir(temp.path(), "abc-123"));
        write_manifest(&continuity_session_dir(temp.path(), DEFAULT_SESSION_ID));

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(
            resolution.session_dir(),
            Some(continuity_session_dir(temp.path(), DEFAULT_SESSION_ID).as_path())
        );
    }

    #[test]
    fn multiple_nested_sessions_without_default_are_ambiguous() {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(&continuity_session_dir(temp.path(), "abc-123"));
        write_manifest(&continuity_session_dir(temp.path(), "def-456"));

        let error = resolve_continuity_session_dir(temp.path()).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("abc-123"), "message: {message}");
        assert!(message.contains("def-456"), "message: {message}");
    }

    #[test]
    fn absent_continuity_state_resolves_to_none() {
        let temp = tempfile::tempdir().unwrap();

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(resolution, ContinuitySessionResolution::None);
        assert_eq!(resolution.session_dir(), None);
    }

    #[test]
    fn empty_continuity_root_resolves_to_none() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(continuity_root_dir(temp.path())).unwrap();

        let resolution = resolve_continuity_session_dir(temp.path()).unwrap();

        assert_eq!(resolution, ContinuitySessionResolution::None);
    }
}
