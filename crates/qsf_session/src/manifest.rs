use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

pub const CONTINUITY_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeMode {
    ColdStart,
    AwakeContinuation,
    ConsolidatedBrief,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContinuityManifest {
    pub schema_version: u16,
    pub current_session_id: Option<String>,
    pub current_session_state_path: Option<PathBuf>,
    #[serde(default)]
    pub current_volition_snapshot_path: Option<PathBuf>,
    pub last_sleep_run_id: Option<String>,
    pub last_sleep_brief_path: Option<PathBuf>,
    pub last_sleep_consumed_session_id: Option<String>,
    pub sleep_pending: bool,
    pub resume_mode: ResumeMode,
}

impl Default for ContinuityManifest {
    fn default() -> Self {
        Self {
            schema_version: CONTINUITY_MANIFEST_SCHEMA_VERSION,
            current_session_id: None,
            current_session_state_path: None,
            current_volition_snapshot_path: None,
            last_sleep_run_id: None,
            last_sleep_brief_path: None,
            last_sleep_consumed_session_id: None,
            sleep_pending: false,
            resume_mode: ResumeMode::ColdStart,
        }
    }
}

impl ContinuityManifest {
    pub fn load_or_default(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest `{}`", path.display()))?;
        let parsed: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse manifest `{}`", path.display()))?;
        if parsed.schema_version != CONTINUITY_MANIFEST_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported manifest schema_version: found {} expected {}",
                parsed.schema_version,
                CONTINUITY_MANIFEST_SCHEMA_VERSION
            );
        }

        Ok(parsed)
    }

    pub fn persist(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create manifest parent dir `{}`",
                parent.display()
            )
        })?;

        let mut temp = NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "failed to create temporary manifest file in `{}`",
                parent.display()
            )
        })?;
        temp.as_file_mut()
            .write_all(serde_json::to_string_pretty(self)?.as_bytes())
            .with_context(|| {
                format!(
                    "failed to write temporary manifest `{}`",
                    temp.path().display()
                )
            })?;
        temp.as_file().sync_all().with_context(|| {
            format!(
                "failed to sync temporary manifest `{}` before persist",
                temp.path().display()
            )
        })?;
        temp.persist(path).map_err(|error| {
            anyhow::anyhow!(
                "failed to persist manifest `{}`: {}",
                path.display(),
                error.error
            )
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn load_or_default_returns_cold_start_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let manifest =
            ContinuityManifest::load_or_default(dir.path().join("continuity-manifest.json"))
                .unwrap();

        assert_eq!(manifest.resume_mode, ResumeMode::ColdStart);
        assert!(!manifest.sleep_pending);
        assert!(manifest.current_session_id.is_none());
    }

    #[test]
    fn persist_then_reload_preserves_all_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("continuity-manifest.json");
        let original = ContinuityManifest {
            schema_version: CONTINUITY_MANIFEST_SCHEMA_VERSION,
            current_session_id: Some("s-1".to_string()),
            current_session_state_path: Some(PathBuf::from("session-state.json")),
            current_volition_snapshot_path: Some(PathBuf::from("volition-state.json")),
            last_sleep_run_id: None,
            last_sleep_brief_path: None,
            last_sleep_consumed_session_id: None,
            sleep_pending: true,
            resume_mode: ResumeMode::AwakeContinuation,
        };

        original.persist(&path).unwrap();
        let reloaded = ContinuityManifest::load_or_default(&path).unwrap();

        assert_eq!(reloaded, original);
    }

    #[test]
    fn legacy_manifest_without_volition_snapshot_path_loads_with_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("continuity-manifest.json");
        std::fs::write(
            &path,
            r#"{"schema_version":1,"current_session_id":"s-1","current_session_state_path":"session-state.json","last_sleep_run_id":null,"last_sleep_brief_path":null,"last_sleep_consumed_session_id":null,"sleep_pending":true,"resume_mode":"awake_continuation"}"#,
        )
        .unwrap();

        let reloaded = ContinuityManifest::load_or_default(&path).unwrap();
        assert!(reloaded.current_volition_snapshot_path.is_none());
    }

    #[test]
    fn unsupported_schema_version_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("continuity-manifest.json");
        std::fs::write(
            &path,
            r#"{"schema_version":999,"current_session_id":null,"current_session_state_path":null,"last_sleep_run_id":null,"last_sleep_brief_path":null,"last_sleep_consumed_session_id":null,"sleep_pending":false,"resume_mode":"cold_start"}"#,
        )
        .unwrap();

        let err = ContinuityManifest::load_or_default(&path).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported manifest schema_version")
        );
    }
}
