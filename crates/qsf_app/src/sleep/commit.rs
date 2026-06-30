use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::memory::store::MemoryStoreContents;
use crate::session::manifest::{ContinuityManifest, ResumeMode};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConsolidatedBrief {
    pub previous_session_summary: String,
    pub future_context_hints: Vec<String>,
    pub open_questions: Vec<String>,
    pub promoted_count: usize,
    pub new_associations_count: usize,
}

pub struct SleepCommit<'a> {
    pub state_dir: &'a Path,
    pub new_store_contents: MemoryStoreContents,
    pub brief: ConsolidatedBrief,
    pub sleep_run_id: String,
    pub consumed_session_id: String,
    pub brief_archive_name: String,
}

impl SleepCommit<'_> {
    pub fn write(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.state_dir).with_context(|| {
            format!(
                "failed to create sleep continuity state dir `{}`",
                self.state_dir.display()
            )
        })?;
        let archive_dir = self.state_dir.join("archive");
        std::fs::create_dir_all(&archive_dir).with_context(|| {
            format!(
                "failed to create sleep continuity archive dir `{}`",
                archive_dir.display()
            )
        })?;

        atomic_write_json(
            &self.state_dir.join("memory-store.json"),
            &self.new_store_contents,
        )?;
        atomic_write_json(&self.state_dir.join("consolidated-brief.json"), &self.brief)?;
        atomic_write_json(&archive_dir.join(&self.brief_archive_name), &self.brief)?;

        let manifest_path = self.state_dir.join("continuity-manifest.json");
        let mut manifest = ContinuityManifest::load_or_default(&manifest_path)?;
        manifest.last_sleep_run_id = Some(self.sleep_run_id.clone());
        manifest.last_sleep_brief_path = Some(PathBuf::from("consolidated-brief.json"));
        manifest.last_sleep_consumed_session_id = Some(self.consumed_session_id.clone());
        manifest.sleep_pending = false;
        manifest.resume_mode = ResumeMode::ConsolidatedBrief;
        manifest.persist(&manifest_path)?;

        Ok(())
    }
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent dir `{}`", parent.display()))?;
    let mut temp = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create temporary JSON file in `{}`",
            parent.display()
        )
    })?;
    temp.as_file_mut()
        .write_all(serde_json::to_string_pretty(value)?.as_bytes())
        .with_context(|| format!("failed to write temporary JSON `{}`", temp.path().display()))?;
    temp.as_file()
        .sync_all()
        .with_context(|| format!("failed to sync temporary JSON `{}`", temp.path().display()))?;
    temp.persist(path).map_err(|error| {
        anyhow::anyhow!("failed to persist `{}`: {}", path.display(), error.error)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn sample_brief() -> ConsolidatedBrief {
        ConsolidatedBrief {
            previous_session_summary: "Sample.".to_string(),
            future_context_hints: vec!["Resume topic.".to_string()],
            open_questions: vec!["What next?".to_string()],
            promoted_count: 1,
            new_associations_count: 0,
        }
    }

    fn sample_commit<'a>(state_dir: &'a Path) -> SleepCommit<'a> {
        SleepCommit {
            state_dir,
            new_store_contents: MemoryStoreContents::default(),
            brief: sample_brief(),
            sleep_run_id: "sleep-1".to_string(),
            consumed_session_id: "s-1".to_string(),
            brief_archive_name: "sleep-sleep-1.json".to_string(),
        }
    }

    #[test]
    fn commit_writes_memory_store_brief_archive_and_manifest() {
        let dir = TempDir::new().unwrap();
        sample_commit(dir.path()).write().unwrap();

        assert!(dir.path().join("memory-store.json").exists());
        assert!(dir.path().join("consolidated-brief.json").exists());
        assert!(dir.path().join("archive/sleep-sleep-1.json").exists());
        assert!(dir.path().join("continuity-manifest.json").exists());

        let manifest =
            ContinuityManifest::load_or_default(dir.path().join("continuity-manifest.json"))
                .unwrap();
        assert!(!manifest.sleep_pending);
        assert_eq!(manifest.last_sleep_run_id.as_deref(), Some("sleep-1"));
        assert_eq!(
            manifest.last_sleep_consumed_session_id.as_deref(),
            Some("s-1")
        );
        assert_eq!(manifest.resume_mode, ResumeMode::ConsolidatedBrief);
    }

    #[test]
    fn commit_twice_produces_byte_identical_state_files() {
        let dir = TempDir::new().unwrap();
        let commit = sample_commit(dir.path());
        commit.write().unwrap();
        let first_store = std::fs::read(dir.path().join("memory-store.json")).unwrap();
        let first_brief = std::fs::read(dir.path().join("consolidated-brief.json")).unwrap();
        let first_manifest = std::fs::read(dir.path().join("continuity-manifest.json")).unwrap();

        commit.write().unwrap();

        assert_eq!(
            first_store,
            std::fs::read(dir.path().join("memory-store.json")).unwrap()
        );
        assert_eq!(
            first_brief,
            std::fs::read(dir.path().join("consolidated-brief.json")).unwrap()
        );
        assert_eq!(
            first_manifest,
            std::fs::read(dir.path().join("continuity-manifest.json")).unwrap()
        );
    }

    #[test]
    fn partial_write_recovers_when_manifest_was_not_yet_flipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("memory-store.json"), b"stale store").unwrap();
        std::fs::write(dir.path().join("consolidated-brief.json"), b"stale brief").unwrap();
        std::fs::create_dir_all(dir.path().join("archive")).unwrap();
        std::fs::write(
            dir.path().join("archive/sleep-sleep-1.json"),
            b"stale archived brief",
        )
        .unwrap();
        ContinuityManifest {
            sleep_pending: true,
            current_volition_snapshot_path: None,
            ..ContinuityManifest::default()
        }
        .persist(dir.path().join("continuity-manifest.json"))
        .unwrap();

        let commit = sample_commit(dir.path());
        commit.write().unwrap();

        let manifest =
            ContinuityManifest::load_or_default(dir.path().join("continuity-manifest.json"))
                .unwrap();
        assert!(!manifest.sleep_pending);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("memory-store.json")).unwrap(),
            serde_json::to_string_pretty(&commit.new_store_contents).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("consolidated-brief.json")).unwrap(),
            serde_json::to_string_pretty(&commit.brief).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("archive/sleep-sleep-1.json")).unwrap(),
            serde_json::to_string_pretty(&commit.brief).unwrap()
        );
    }
}
