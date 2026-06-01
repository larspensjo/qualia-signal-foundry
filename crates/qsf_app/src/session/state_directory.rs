use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateDirectoryResolution {
    pub resume_state_dir: PathBuf,
    pub persist_state_dir: PathBuf,
    pub legacy_fallback_used: bool,
}

pub fn resolve_shared_state_directory_from_env() -> StateDirectoryResolution {
    if let Ok(path) = std::env::var("QSF_STATE_DIR") {
        return resolve_shared_state_directory(Some(PathBuf::from(path)), None, None);
    }

    let shared = PathBuf::from("state/session");
    let legacy = PathBuf::from("state/text-loop");
    resolve_shared_state_directory(
        None,
        shared.exists().then_some(shared),
        legacy.exists().then_some(legacy),
    )
}

fn resolve_shared_state_directory(
    env_override: Option<PathBuf>,
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

    let default_shared = PathBuf::from("state/session");
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
            Some(path("state/session")),
            Some(path("state/text-loop")),
        );

        assert_eq!(resolution.resume_state_dir, path("state/session"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(!resolution.legacy_fallback_used);
    }

    #[test]
    fn legacy_directory_is_read_only_fallback_to_shared_persist() {
        let resolution = resolve_shared_state_directory(None, None, Some(path("state/text-loop")));

        assert_eq!(resolution.resume_state_dir, path("state/text-loop"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(resolution.legacy_fallback_used);
    }

    #[test]
    fn absent_state_uses_fresh_shared_directory() {
        let resolution = resolve_shared_state_directory(None, None, None);

        assert_eq!(resolution.resume_state_dir, path("state/session"));
        assert_eq!(resolution.persist_state_dir, path("state/session"));
        assert!(!resolution.legacy_fallback_used);
    }
}
