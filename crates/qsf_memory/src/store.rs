use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::association::{
    ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema,
};
use crate::errors::{SchemaVersions, ShapeError, StoreLoadError};
use crate::processed_range::ProcessedRange;
use crate::record::{MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, ensure_current_memory_schema};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemoryStoreContents {
    pub records: Vec<MemoryRecord>,
    pub associations: Vec<Association>,
    #[serde(default, deserialize_with = "deserialize_processed_ranges")]
    pub processed_ranges: Vec<ProcessedRange>,
}

fn deserialize_processed_ranges<'de, D>(deserializer: D) -> Result<Vec<ProcessedRange>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<ProcessedRange>>::deserialize(deserializer)?.unwrap_or_default())
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
            // Keep this compatibility loader behavior stable for qsf_app. The
            // browser-facing `load_existing` path below owns the richer error
            // taxonomy and should be kept aligned when schema versions change.
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

/// Result of a successful two-pass load. `raw_records` keeps the source-faithful
/// JSON for each record id so callers can serve the verbatim persisted form
/// without round-tripping through the typed deserialization.
#[derive(Clone, Debug)]
pub struct LoadedStore {
    pub contents: MemoryStoreContents,
    pub raw_records: HashMap<String, serde_json::Value>,
    pub schema_versions_found: SchemaVersions,
}

/// Load a memory store from `path`. Unlike `MemoryStore::load_or_empty`,
/// a missing file is a `MissingFile` error rather than an empty store.
pub fn load_existing(path: impl AsRef<std::path::Path>) -> Result<LoadedStore, StoreLoadError> {
    let path = path.as_ref().to_path_buf();
    if !path.exists() {
        return Err(StoreLoadError::MissingFile {
            path: path.clone(),
            message: format!("no file at {}", path.display()),
        });
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| StoreLoadError::InvalidJson {
        path: path.clone(),
        message: format!("read error: {e}"),
    })?;

    // Pass 1: parse to serde_json::Value to capture observed schema versions
    // and to retain source-faithful per-record JSON for the raw endpoint.
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| StoreLoadError::InvalidJson {
            path: path.clone(),
            message: e.to_string(),
        })?;

    let schema_versions_found = collect_schema_versions(&value);
    let schema_versions_supported = SchemaVersions {
        records: vec![MEMORY_RECORD_SCHEMA_VERSION],
        associations: vec![ASSOCIATION_SCHEMA_VERSION],
    };

    if !schema_versions_compatible(&schema_versions_found, &schema_versions_supported) {
        return Err(StoreLoadError::UnsupportedSchema {
            path,
            message: "store contains record or association schema_version values not supported by this build".to_string(),
            schema_versions_found: Box::new(schema_versions_found),
            schema_versions_supported: Box::new(schema_versions_supported),
        });
    }

    // Pass 2: deserialize into the typed shape and run structural validation.
    let contents: MemoryStoreContents =
        serde_json::from_value(value.clone()).map_err(|e| StoreLoadError::InvalidStoreShape {
            path: path.clone(),
            message: e.to_string(),
            schema_versions_found: Box::new(schema_versions_found.clone()),
            // Serde's error does not expose a stable JSON pointer here. Browser
            // endpoints can refine this once user-facing shape diagnostics land.
            shape_errors: vec![ShapeError {
                field_path: e.to_string(),
                message: e.to_string(),
            }],
        })?;

    let duplicate_ids = find_duplicate_memory_ids(&contents);
    if !duplicate_ids.is_empty() {
        return Err(StoreLoadError::DuplicateMemoryIds {
            path,
            message: format!("{} duplicate memory id(s)", duplicate_ids.len()),
            duplicate_ids,
        });
    }

    let raw_records = build_raw_record_index(&value);

    Ok(LoadedStore {
        contents,
        raw_records,
        schema_versions_found,
    })
}

fn collect_schema_versions(value: &serde_json::Value) -> SchemaVersions {
    fn collect_field(value: &serde_json::Value, key: &str) -> Vec<u16> {
        let mut set = BTreeSet::new();
        if let Some(arr) = value.get(key).and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(v) = item.get("schema_version").and_then(|v| v.as_u64()) {
                    set.insert(v as u16);
                }
            }
        }
        set.into_iter().collect()
    }
    SchemaVersions {
        records: collect_field(value, "records"),
        associations: collect_field(value, "associations"),
    }
}

fn schema_versions_compatible(found: &SchemaVersions, supported: &SchemaVersions) -> bool {
    found.records.iter().all(|v| supported.records.contains(v))
        && found
            .associations
            .iter()
            .all(|v| supported.associations.contains(v))
}

fn find_duplicate_memory_ids(contents: &MemoryStoreContents) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for record in &contents.records {
        if !seen.insert(record.id.clone()) {
            duplicates.insert(record.id.clone());
        }
    }
    duplicates.into_iter().collect()
}

fn build_raw_record_index(value: &serde_json::Value) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::new();
    if let Some(records) = value.get("records").and_then(|v| v.as_array()) {
        for record in records {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                out.insert(id.to_string(), record.clone());
            }
        }
    }
    out
}

/// Return the set of memory ids referenced by any association but not present
/// in the record set. Dangling references are not load errors but are surfaced
/// elsewhere as broken edges.
pub fn dangling_association_ids(contents: &MemoryStoreContents) -> Vec<String> {
    let known: BTreeSet<&str> = contents.records.iter().map(|r| r.id.as_str()).collect();
    let mut dangling = BTreeSet::new();
    for a in &contents.associations {
        if !known.contains(a.from_memory_id.as_str()) {
            dangling.insert(a.from_memory_id.clone());
        }
        if !known.contains(a.to_memory_id.as_str()) {
            dangling.insert(a.to_memory_id.clone());
        }
    }
    dangling.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use crate::MemoryRecordKind;

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

    fn write_store(json: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        tmp
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
    fn processed_ranges_roundtrip_through_persist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();
        store.contents_mut().processed_ranges.push(ProcessedRange {
            session_id: "s".into(),
            first_turn_index: 0,
            last_turn_index: 2,
            kind: crate::processed_range::ProcessedRangeKind::LiveBatch,
            at: ts(),
        });
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents().processed_ranges.len(), 1);
        assert_eq!(reloaded.contents().processed_ranges[0].session_id, "s");
    }

    #[test]
    fn legacy_store_without_processed_ranges_loads_via_serde_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        std::fs::write(&path, r#"{"records":[],"associations":[]}"#).unwrap();

        let store = MemoryStore::load_or_empty(&path).unwrap();
        assert!(store.contents().processed_ranges.is_empty());
    }

    #[test]
    fn store_with_null_processed_ranges_loads_as_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        std::fs::write(
            &path,
            r#"{"records":[],"associations":[],"processed_ranges":null}"#,
        )
        .unwrap();

        let store = MemoryStore::load_or_empty(&path).unwrap();
        assert!(store.contents().processed_ranges.is_empty());
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

    #[test]
    fn missing_file_returns_missing_file_error() {
        let err = load_existing("/nonexistent/memory-store.json").unwrap_err();
        assert!(matches!(err, StoreLoadError::MissingFile { .. }));
    }

    #[test]
    fn invalid_json_returns_invalid_json_error() {
        let tmp = write_store("{not json");
        let err = load_existing(tmp.path()).unwrap_err();
        assert!(matches!(err, StoreLoadError::InvalidJson { .. }));
    }

    #[test]
    fn unsupported_schema_returns_unsupported_schema_error() {
        let tmp = write_store(
            r#"{ "records": [ { "schema_version": 9999, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0 } ], "associations": [] }"#,
        );
        let err = load_existing(tmp.path()).unwrap_err();
        match err {
            StoreLoadError::UnsupportedSchema {
                schema_versions_found,
                ..
            } => {
                assert_eq!(schema_versions_found.records, vec![9999]);
            }
            other => panic!("expected UnsupportedSchema, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_ids_return_duplicate_memory_ids_error() {
        let one = r#"{ "schema_version": 1, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0 }"#;
        let json = format!(r#"{{ "records": [{one}, {one}], "associations": [] }}"#);
        let tmp = write_store(&json);
        let err = load_existing(tmp.path()).unwrap_err();
        match err {
            StoreLoadError::DuplicateMemoryIds { duplicate_ids, .. } => {
                assert_eq!(duplicate_ids, vec!["a".to_string()]);
            }
            other => panic!("expected DuplicateMemoryIds, got {other:?}"),
        }
    }

    #[test]
    fn invalid_shape_returns_invalid_store_shape_error() {
        // Missing required `id` field.
        let tmp = write_store(
            r#"{ "records": [ { "schema_version": 1, "kind": "concept" } ], "associations": [] }"#,
        );
        let err = load_existing(tmp.path()).unwrap_err();
        assert!(matches!(err, StoreLoadError::InvalidStoreShape { .. }));
    }

    #[test]
    fn raw_record_index_preserves_extra_fields() {
        let tmp = write_store(
            r#"{ "records": [ { "schema_version": 1, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0, "future_field": "kept" } ], "associations": [] }"#,
        );
        let loaded = load_existing(tmp.path()).unwrap();
        assert_eq!(loaded.contents.records.len(), 1);
        assert_eq!(
            loaded.raw_records["a"]["future_field"],
            serde_json::json!("kept")
        );
    }

    #[test]
    fn dangling_associations_are_counted_not_errors() {
        let tmp = write_store(
            r#"{ "records": [ { "schema_version": 1, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0 } ], "associations": [ { "schema_version": 1, "from_memory_id": "a", "to_memory_id": "ghost", "weight": 0.5, "reason": "test", "last_reinforced_at": "2026-05-20T00:00:00Z" } ] }"#,
        );
        let loaded = load_existing(tmp.path()).unwrap();
        assert_eq!(
            dangling_association_ids(&loaded.contents),
            vec!["ghost".to_string()]
        );
    }
}
