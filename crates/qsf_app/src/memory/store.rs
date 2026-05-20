use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::association::{Association, ensure_current_association_schema};
use super::memory_record::{MemoryRecord, ensure_current_memory_schema};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemoryStoreContents {
    pub records: Vec<MemoryRecord>,
    pub associations: Vec<Association>,
}

#[derive(Clone, Debug)]
pub struct MemoryStore {
    path: PathBuf,
    contents: MemoryStoreContents,
}

impl MemoryStore {
    pub fn load_or_empty(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let contents = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read memory store `{}`", path.display()))?;
            let parsed: MemoryStoreContents = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse memory store `{}`", path.display()))?;
            ensure_current_memory_schema(&parsed.records)?;
            ensure_current_association_schema(&parsed.associations)?;
            parsed
        } else {
            MemoryStoreContents::default()
        };

        Ok(Self { path, contents })
    }

    pub fn contents(&self) -> &MemoryStoreContents {
        &self.contents
    }

    pub fn contents_mut(&mut self) -> &mut MemoryStoreContents {
        &mut self.contents
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append_records(&mut self, records: impl IntoIterator<Item = MemoryRecord>) {
        self.contents.records.extend(records);
    }

    pub fn append_associations(&mut self, associations: impl IntoIterator<Item = Association>) {
        self.contents.associations.extend(associations);
    }

    pub fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create memory store parent dir `{}`",
                    parent.display()
                )
            })?;
        }

        ensure_current_memory_schema(&self.contents.records)?;
        ensure_current_association_schema(&self.contents.associations)?;

        let json = serde_json::to_string_pretty(&self.contents)
            .context("failed to serialize memory store")?;
        let parent = self.path.parent().unwrap_or(Path::new("."));
        let mut temp = NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "failed to create temporary memory store file in `{}`",
                parent.display()
            )
        })?;
        temp.as_file_mut()
            .write_all(json.as_bytes())
            .with_context(|| {
                format!(
                    "failed to write temporary memory store `{}`",
                    temp.path().display()
                )
            })?;
        temp.as_file().sync_all().with_context(|| {
            format!(
                "failed to sync temporary memory store `{}` before persist",
                temp.path().display()
            )
        })?;
        temp.persist(&self.path).map_err(|error| {
            anyhow::anyhow!(
                "failed to persist memory store `{}`: {}",
                self.path.display(),
                error.error
            )
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use crate::memory::MemoryRecordKind;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    fn record(id: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            "Title",
            "Summary text.",
            vec!["topic"],
            ts(),
            0.5,
            0,
            "tests",
            10,
        )
    }

    #[test]
    fn load_or_empty_returns_empty_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");

        let store = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(store.path(), path.as_path());
        assert!(store.contents().records.is_empty());
        assert!(store.contents().associations.is_empty());
    }

    #[test]
    fn persist_then_reload_roundtrips_record_and_association() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();

        store.append_records([record("memory.test")]);
        store.append_associations([Association::new(
            "memory.a",
            "memory.b",
            0.4,
            "related",
            ts(),
        )]);
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents(), store.contents());
    }

    #[test]
    fn persist_overwrites_existing_file_on_second_call() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();
        store.persist().unwrap();

        store.append_records([record("memory.second")]);
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents().records.len(), 1);
        assert_eq!(reloaded.contents().records[0].id, "memory.second");
    }

    #[test]
    fn load_rejects_wrong_record_schema_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        std::fs::write(
            &path,
            r#"{"records":[{"schema_version":999,"id":"memory.bad","kind":"observation","title":"Bad","summary":"Bad","tags":[],"created_at":"2026-05-19T00:00:00Z","importance":0.5,"reinforcement_count":0,"source_reference":"tests","estimated_tokens":10}],"associations":[]}"#,
        )
        .unwrap();

        let error = MemoryStore::load_or_empty(&path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported memory record schema version")
        );
    }

    #[test]
    fn load_rejects_wrong_association_schema_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        std::fs::write(
            &path,
            r#"{"records":[],"associations":[{"schema_version":999,"from_memory_id":"memory.a","to_memory_id":"memory.b","weight":0.5,"reason":"bad","last_reinforced_at":"2026-05-19T00:00:00Z"}]}"#,
        )
        .unwrap();

        let error = MemoryStore::load_or_empty(&path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported association schema version")
        );
    }
}
