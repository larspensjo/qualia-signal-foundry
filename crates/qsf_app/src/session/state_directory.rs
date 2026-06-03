use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateDirectoryResolution {
    pub resume_state_dir: PathBuf,
    pub persist_state_dir: PathBuf,
    pub legacy_fallback_used: bool,
}

pub fn resolve_shared_state_directory_from_env() -> StateDirectoryResolution {
    if let Ok(path) = std::env::var("QSF_STATE_DIR") {
        return resolve_shared_state_directory(
            Some(PathBuf::from(path)),
            PathBuf::from("state/session"),
            None,
            None,
        );
    }

    resolve_shared_state_directory_from_default_paths(
        PathBuf::from("state/session"),
        PathBuf::from("state/text-loop"),
    )
}

fn resolve_shared_state_directory_from_default_paths(
    default_shared: PathBuf,
    legacy: PathBuf,
) -> StateDirectoryResolution {
    let shared_exists = default_shared.exists();
    let legacy_exists = legacy.exists();
    let shared_is_resumable = default_shared.join("continuity-manifest.json").exists();
    resolve_shared_state_directory(
        None,
        default_shared.clone(),
        (shared_is_resumable || (shared_exists && !legacy_exists)).then_some(default_shared),
        legacy_exists.then_some(legacy),
    )
}

fn resolve_shared_state_directory(
    env_override: Option<PathBuf>,
    default_shared: PathBuf,
    existing_shared: Option<PathBuf>,
    existing_legacy: Option<PathBuf>,
) -> StateDirectoryResolution {
    if let Some(path) = env_override {
        return StateDirectoryResolution {
            resume_state_dir: path.clone(),
            persist_state_dir: path,
            legacy_fallback_used: false,
        };
    }

    if let Some(shared) = existing_shared {
        return StateDirectoryResolution {
            resume_state_dir: shared.clone(),
            persist_state_dir: shared,
            legacy_fallback_used: false,
        };
    }

    if let Some(legacy) = existing_legacy {
        return StateDirectoryResolution {
            resume_state_dir: legacy,
            persist_state_dir: default_shared,
            legacy_fallback_used: true,
        };
    }

    StateDirectoryResolution {
        resume_state_dir: default_shared.clone(),
        persist_state_dir: default_shared,
        legacy_fallback_used: false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn path(value: &str) -> PathBuf {
        Path::new(value).to_path_buf()
    }

    #[test]
    fn explicit_env_uses_one_directory_for_resume_and_persist() {
        let resolution = resolve_shared_state_directory(
            Some(path("custom-state")),
            path("state/session"),
            Some(path("state/session")),
            Some(path("state/text-loop")),
        );

        assert_eq!(resolution.resume_state_dir, path("custom-state"));
        assert_eq!(resolution.persist_state_dir, path("custom-state"));
        assert!(!resolution.legacy_fallback_used);
    }

    #[test]
    fn existing_shared_directory_wins_over_legacy() {
        let resolution = resolve_shared_state_directory(
            None,
            path("state/session"),
            Some(path("state/session")),
            Some(path("state/text-loop")),
        );

        assert_eq!(resolution.resume_state_dir, path("state/session"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(!resolution.legacy_fallback_used);
    }

    #[test]
    fn incomplete_shared_directory_does_not_mask_legacy_fallback() {
        let dir = tempfile::TempDir::new().unwrap();
        let shared = dir.path().join("state").join("session");
        let legacy = dir.path().join("state").join("text-loop");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();

        let resolution =
            resolve_shared_state_directory_from_default_paths(shared.clone(), legacy.clone());

        assert_eq!(resolution.resume_state_dir, legacy);
        assert_eq!(resolution.persist_state_dir, shared);
        assert!(resolution.legacy_fallback_used);
    }

    #[test]
    fn legacy_directory_is_read_only_fallback_to_shared_persist() {
        let resolution = resolve_shared_state_directory(
            None,
            path("state/session"),
            None,
            Some(path("state/text-loop")),
        );

        assert_eq!(resolution.resume_state_dir, path("state/text-loop"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(resolution.legacy_fallback_used);
    }

    #[test]
    fn absent_state_uses_fresh_shared_directory() {
        let resolution = resolve_shared_state_directory(None, path("state/session"), None, None);

        assert_eq!(resolution.resume_state_dir, path("state/session"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(!resolution.legacy_fallback_used);
    }
}
