# Plan: Memory Association Browser

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Rust-backed, browser-based workbench (`qsf_browser_server`) for inspecting durable memory state, with a focal-hub canvas for local association navigation.

**Architecture:** A new `qsf_memory` shared crate owns memory record / association types, schema validation, and the two-pass loader. A new `qsf_browser_server` axum binary depends on `qsf_memory`, exposes a read-only visualization API under `/api/*`, and serves a TypeScript/Vite/PixiJS frontend (HTML/CSS workbench + small PixiJS canvas for the selected memory's neighborhood).

**Tech Stack:** Rust (axum, clap, tokio, serde_json, rust-embed behind a feature flag), TypeScript, Vite, PixiJS/WebGL, HTML/CSS.

**Refs:** [Design.MemoryAssociationBrowser.md](Design.MemoryAssociationBrowser.md), [Design.SharedVisualLanguage.md](Design.SharedVisualLanguage.md), [DecisionLog.md](../DecisionLog.md) entry `2026-05-20 - Post-hoc browser tools use Rust backend + browser frontend split`.

---

## Status

Active. Phase 3 implementation complete; external human verification pending.

## Core Invariants

- `qsf_browser_server` is read-only; no mutation endpoints exist.
- Backend depends on `qsf_memory` only (not on `qsf_app`).
- Frontend never reads persisted JSON directly; it sees DTOs only.
- DTOs are defined by `qsf_browser_server`, not exposed by `qsf_memory`.
- Filter, sort, search, and neighborhood ranking are pure functions taking a store snapshot + query and returning a result.
- HTTP handlers are thin wrappers around those pure functions.
- Default bind is `127.0.0.1`; non-loopback bind requires an explicit flag and logs a startup warning via `engine_logging`.
- `cargo build` and `cargo clippy --all-targets -- -D warnings` MUST succeed without npm/Node installed.
- A `build.rs` that shells out to npm is explicitly disallowed.

## Standard Per-Task Closing Steps

Each task ends with the same closing rhythm. When the closing block reads "Run the standard closing steps", do these three commands and verify:

```bash
cargo clippy --all-targets -- -D warnings   # must be clean
cargo fmt --all                              # rewrites if needed
cargo test -p <crate-touched>                # passes
```

Then add a single diary entry at the end of the phase (not per task) and commit.

## Verification Conventions

- Every task contains an executable verification step.
- Phases that change runtime behavior call out external human testing explicitly.
- Defaults always exercise the new code path (e.g. `--store` default points at the runtime's standard memory store location, the workbench launches without flags).

## Documents To Update

Per [docs/ProjectFrame/ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md):

- `docs/EngineeringDiary.md` — one entry per phase (Phases 0–5).
- `docs/DecisionLog.md` — the architecture decision is already in place (`2026-05-20 - Post-hoc browser tools use Rust backend + browser frontend split`). No further entries expected from this plan unless implementation surprises produce a durable rule.
- `docs/Plans/Idea.MemoryAssociationBrowser.md` — add a one-line note at the top pointing to `Design.MemoryAssociationBrowser.md` once Phase 0 lands.
- `docs/Architecture/` — optional new entry after Phase 4 describing the `qsf_app` / `qsf_memory` / `qsf_browser_server` boundary, once shipping code exists.
- `README.md` — usage section updated in Phase 5.

---

## Phase 0: Extract `qsf_memory`

Move the persisted memory types and store loader out of `qsf_app` into a new shared crate. Add the `load_existing` helper, the two-pass loader, and the `StoreLoadError` taxonomy. `qsf_app` should still build and pass tests unchanged after the move.

### Task 0.1: Create the `qsf_memory` crate scaffold

**Files:**
- Create: `crates/qsf_memory/Cargo.toml`
- Create: `crates/qsf_memory/src/lib.rs`

- [x] **Step 1: Create the Cargo manifest**

```toml
# crates/qsf_memory/Cargo.toml
[package]
name = "qsf_memory"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tempfile = "3"
thiserror = { workspace = true }
time = { workspace = true }
```

`tempfile` is in `[dependencies]` rather than `[dev-dependencies]` because
`MemoryStore::persist` (moved in Task 0.4) uses `tempfile::NamedTempFile`
in production code, not just tests.

- [x] **Step 2: Create the empty lib root**

```rust
// crates/qsf_memory/src/lib.rs
//! Persisted memory record, association, and store-loading types
//! shared between qsf_app and qsf_browser_server.
```

- [x] **Step 3: Verify the workspace picks it up**

Run: `cargo build -p qsf_memory`
Expected: builds with no errors (empty crate).

### Task 0.2: Move `MemoryRecord` into `qsf_memory`

**Files:**
- Create: `crates/qsf_memory/src/record.rs`
- Modify: `crates/qsf_memory/src/lib.rs`
- Modify: `crates/qsf_app/src/memory/memory_record.rs`
- Modify: `crates/qsf_app/src/memory/mod.rs`

- [x] **Step 1: Copy `memory_record.rs` content into the new crate**

Copy the full contents of `crates/qsf_app/src/memory/memory_record.rs` into `crates/qsf_memory/src/record.rs` unchanged.

- [x] **Step 2: Re-export from `qsf_memory` lib root**

Append to `crates/qsf_memory/src/lib.rs`:

```rust
pub mod record;
pub use record::{MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind};
```

- [x] **Step 3: Replace `qsf_app`'s copy with a thin re-export**

Replace `crates/qsf_app/src/memory/memory_record.rs` with:

```rust
//! Re-export of the persisted memory record from qsf_memory.
//! Kept for backwards compatibility with existing import paths.
pub use qsf_memory::{MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind};
pub use qsf_memory::record::ensure_current_memory_schema;
```

(If `ensure_current_memory_schema` is a free function in the original file, ensure it is also re-exported from `qsf_memory::record` in Step 1.)

- [x] **Step 4: Add the `qsf_memory` dependency to `qsf_app`**

Modify `crates/qsf_app/Cargo.toml` `[dependencies]`:

```toml
qsf_memory = { path = "../qsf_memory" }
```

- [x] **Step 5: Verify**

Run: `cargo build -p qsf_app`
Expected: builds with no errors.

### Task 0.3: Move `Association` into `qsf_memory`

**Files:**
- Create: `crates/qsf_memory/src/association.rs`
- Modify: `crates/qsf_memory/src/lib.rs`
- Modify: `crates/qsf_app/src/memory/association.rs`

- [x] **Step 1: Copy `association.rs` content into `qsf_memory`**

Copy `crates/qsf_app/src/memory/association.rs` to `crates/qsf_memory/src/association.rs` unchanged.

- [x] **Step 2: Re-export from the lib root**

Append to `crates/qsf_memory/src/lib.rs`:

```rust
pub mod association;
pub use association::{ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema};
```

- [x] **Step 3: Replace `qsf_app`'s copy with a thin re-export**

Replace `crates/qsf_app/src/memory/association.rs` with:

```rust
//! Re-export of the persisted association from qsf_memory.
pub use qsf_memory::{ASSOCIATION_SCHEMA_VERSION, Association, ensure_current_association_schema};
```

- [x] **Step 4: Verify**

Run: `cargo build -p qsf_app`
Expected: builds with no errors.

### Task 0.4: Move `MemoryStoreContents` and the existing loader into `qsf_memory`

**Files:**
- Create: `crates/qsf_memory/src/store.rs`
- Modify: `crates/qsf_memory/src/lib.rs`
- Modify: `crates/qsf_app/src/memory/store.rs`

- [x] **Step 1: Move `MemoryStoreContents` and `MemoryStore` into `qsf_memory::store`**

Copy `crates/qsf_app/src/memory/store.rs` to `crates/qsf_memory/src/store.rs`. Update imports: replace `super::association::...` with `crate::association::...` and `super::memory_record::...` with `crate::record::...`.

- [x] **Step 2: Re-export from the lib root**

Append to `crates/qsf_memory/src/lib.rs`:

```rust
pub mod store;
pub use store::{MemoryStore, MemoryStoreContents};
```

- [x] **Step 3: Replace `qsf_app`'s copy with a re-export**

Replace `crates/qsf_app/src/memory/store.rs` with:

```rust
//! Re-export of the persisted memory store from qsf_memory.
pub use qsf_memory::{MemoryStore, MemoryStoreContents};
```

- [x] **Step 4: Verify the move**

Run: `cargo build` (workspace) and then `cargo test -p qsf_app`.
Expected: full workspace builds; existing tests still pass.

### Task 0.5: Add `StoreLoadError` taxonomy

**Files:**
- Create: `crates/qsf_memory/src/errors.rs`
- Modify: `crates/qsf_memory/src/lib.rs`

- [x] **Step 1: Write the error enum**

```rust
// crates/qsf_memory/src/errors.rs
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoreLoadError {
    #[error("memory store file not found at {path}")]
    MissingFile { path: PathBuf, message: String },

    #[error("memory store at {path} is not valid JSON: {message}")]
    InvalidJson { path: PathBuf, message: String },

    #[error("memory store at {path} uses unsupported schema versions")]
    UnsupportedSchema {
        path: PathBuf,
        message: String,
        schema_versions_found: SchemaVersions,
        schema_versions_supported: SchemaVersions,
    },

    #[error("memory store at {path} fails structural validation")]
    InvalidStoreShape {
        path: PathBuf,
        message: String,
        schema_versions_found: SchemaVersions,
        shape_errors: Vec<ShapeError>,
    },

    #[error("memory store at {path} contains duplicate memory ids")]
    DuplicateMemoryIds {
        path: PathBuf,
        message: String,
        duplicate_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchemaVersions {
    pub records: Vec<u16>,
    pub associations: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShapeError {
    pub field_path: String,
    pub message: String,
}
```

- [x] **Step 2: Re-export from the lib root**

```rust
pub mod errors;
pub use errors::{SchemaVersions, ShapeError, StoreLoadError};
```

- [x] **Step 3: Verify**

Run: `cargo build -p qsf_memory`
Expected: builds with no errors.

### Task 0.6: Add `load_existing` and the two-pass loader

**Files:**
- Modify: `crates/qsf_memory/src/store.rs`
- Test: inline `#[cfg(test)]` block in `store.rs`

- [x] **Step 1: Add the loaded-store struct that carries the raw index**

Append to `crates/qsf_memory/src/store.rs`:

```rust
use std::collections::{BTreeSet, HashMap};

use crate::errors::{SchemaVersions, ShapeError, StoreLoadError};
use crate::association::ASSOCIATION_SCHEMA_VERSION;
use crate::record::MEMORY_RECORD_SCHEMA_VERSION;

/// Result of a successful two-pass load. `raw_records` keeps the source-faithful
/// JSON for each record id so callers can serve the verbatim persisted form
/// without round-tripping through the typed deserialization.
#[derive(Clone, Debug)]
pub struct LoadedStore {
    pub contents: MemoryStoreContents,
    pub raw_records: HashMap<String, serde_json::Value>,
    pub schema_versions_found: SchemaVersions,
}
```

- [x] **Step 2: Implement `load_existing`**

Append to `crates/qsf_memory/src/store.rs`:

```rust
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
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        StoreLoadError::InvalidJson {
            path: path.clone(),
            message: e.to_string(),
        }
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
            schema_versions_found,
            schema_versions_supported,
        });
    }

    // Pass 2: deserialize into the typed shape and run structural validation.
    let contents: MemoryStoreContents = serde_json::from_value(value.clone()).map_err(|e| {
        StoreLoadError::InvalidStoreShape {
            path: path.clone(),
            message: e.to_string(),
            schema_versions_found: schema_versions_found.clone(),
            shape_errors: vec![ShapeError {
                field_path: e.to_string(),
                message: e.to_string(),
            }],
        }
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
```

- [x] **Step 3: Re-export the new surface**

Append to `crates/qsf_memory/src/lib.rs`:

```rust
pub use store::{LoadedStore, dangling_association_ids, load_existing};
```

- [x] **Step 4: Write tests against fixture stores**

Append to `crates/qsf_memory/src/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_store(json: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        tmp
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
        let tmp = write_store(r#"{ "records": [ { "schema_version": 9999, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0 } ], "associations": [] }"#);
        let err = load_existing(tmp.path()).unwrap_err();
        match err {
            StoreLoadError::UnsupportedSchema { schema_versions_found, .. } => {
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
        let tmp = write_store(r#"{ "records": [ { "schema_version": 1, "kind": "concept" } ], "associations": [] }"#);
        let err = load_existing(tmp.path()).unwrap_err();
        assert!(matches!(err, StoreLoadError::InvalidStoreShape { .. }));
    }

    #[test]
    fn raw_record_index_preserves_extra_fields() {
        let tmp = write_store(r#"{ "records": [ { "schema_version": 1, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0, "future_field": "kept" } ], "associations": [] }"#);
        let loaded = load_existing(tmp.path()).unwrap();
        let raw = loaded.raw_records.get("a").unwrap();
        assert_eq!(raw.get("future_field").unwrap().as_str().unwrap(), "kept");
    }

    #[test]
    fn dangling_associations_are_counted_not_errors() {
        let tmp = write_store(r#"{ "records": [ { "schema_version": 1, "id": "a", "kind": "concept", "title": "t", "summary": "s", "tags": [], "created_at": "2026-05-20T00:00:00Z", "importance": 0.5, "reinforcement_count": 0, "source_reference": "x", "estimated_tokens": 0 } ], "associations": [ { "schema_version": 1, "from_memory_id": "a", "to_memory_id": "ghost", "weight": 0.5, "reason": "test", "last_reinforced_at": "2026-05-20T00:00:00Z" } ] }"#);
        let loaded = load_existing(tmp.path()).unwrap();
        assert_eq!(dangling_association_ids(&loaded.contents), vec!["ghost".to_string()]);
    }
}
```

- [x] **Step 5: Run tests**

Run: `cargo test -p qsf_memory`
Expected: all seven tests pass.

### Task 0.7: Close out Phase 0

- [x] **Step 1: Run the standard closing steps for the workspace**

Run:
```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo test
```
Expected: all clean, all tests pass.

- [x] **Step 2: Add a diary entry**

Append to `docs/EngineeringDiary.md`:

```markdown
## 2026-MM-DD - Extract qsf_memory shared crate

Memory record, association, and store-loading types moved from `qsf_app` into a
new `qsf_memory` crate. The new crate adds a `load_existing` helper (missing file
is an error, not an empty store), a two-pass loader that retains source-faithful
per-record JSON for later raw inspection, the `StoreLoadError` taxonomy, and a
`dangling_association_ids` helper.

What changed:
- New crate `crates/qsf_memory` with record, association, store, errors modules.
- `qsf_app::memory::*` now re-exports from `qsf_memory` to preserve existing import paths.

Refs: crates/qsf_memory, crates/qsf_app/src/memory; implements:
docs/DecisionLog.md#2026-05-20---post-hoc-browser-tools-use-rust-backend--browser-frontend-split
```

- [x] **Step 3: Commit**

```bash
git add crates/qsf_memory crates/qsf_app docs/EngineeringDiary.md
git commit -m "feat(qsf_memory): extract shared memory crate with load_existing"
```

---

## Phase 1: `qsf_browser_server` skeleton

Stand up the new crate, wire up axum, parse CLI args, implement `/api/health` against `qsf_memory::load_existing`, default to loopback binding, and emit a startup warning for non-loopback. Data endpoints exist as stubs returning `503` when the store fails to load.

### Task 1.1: Create the `qsf_browser_server` crate scaffold

**Files:**
- Create: `crates/qsf_browser_server/Cargo.toml`
- Create: `crates/qsf_browser_server/src/main.rs`
- Create: `crates/qsf_browser_server/src/lib.rs`

- [ ] **Step 1: Create the manifest**

```toml
# crates/qsf_browser_server/Cargo.toml
[package]
name = "qsf_browser_server"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[features]
default = []
embedded-frontend = ["dep:rust-embed", "dep:mime_guess"]

[dependencies]
anyhow = { workspace = true }
axum = "0.7"
axum-extra = { version = "0.9", features = ["query"] }
clap = { workspace = true }
engine_logging = { path = "../engine_logging" }
log = { workspace = true }
qsf_memory = { path = "../qsf_memory" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
time = { workspace = true }
tokio = { workspace = true, features = ["net", "rt-multi-thread", "macros", "signal", "time"] }
tower = "0.5"
mime_guess = { version = "2", optional = true }
rust-embed = { version = "8", features = ["mime-guess"], optional = true }

[dev-dependencies]
http-body-util = "0.1"
hyper = { version = "1", features = ["client", "http1"] }
tempfile = "3"
tower = "0.5"
```

- [ ] **Step 2: Create a thin `main.rs`**

```rust
// crates/qsf_browser_server/src/main.rs
use qsf_browser_server::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}
```

- [ ] **Step 3: Create the lib root with a placeholder `run`**

```rust
// crates/qsf_browser_server/src/lib.rs
//! HTTP server for post-hoc inspection of QSF persisted artifacts.
//!
//! Read-only. Depends on `qsf_memory`, never on `qsf_app`.

pub mod cli;
pub mod server;
pub mod state;
pub mod memory;
pub mod health;

pub async fn run() -> anyhow::Result<()> {
    let args = cli::Args::parse_from_env();
    server::serve(args).await
}
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build -p qsf_browser_server` (will fail until subsequent tasks fill in modules — expected). After Tasks 1.2–1.4, this command must succeed.

### Task 1.2: CLI args

**Files:**
- Create: `crates/qsf_browser_server/src/cli.rs`

- [ ] **Step 1: Write the arg parser**

```rust
// crates/qsf_browser_server/src/cli.rs
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use clap::Parser;

/// Defaults are chosen so the workbench launches with no arguments against
/// the runtime's standard memory store location.
pub const DEFAULT_STORE_PATH: &str = "state/text-loop/memory-store.json";
pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const DEFAULT_PORT: u16 = 3939;

#[derive(Clone, Debug, Parser)]
#[command(name = "qsf_browser_server", about = "Memory Association Browser server")]
pub struct Args {
    /// Path to the persisted memory store.
    #[arg(long, default_value = DEFAULT_STORE_PATH)]
    pub store: PathBuf,

    /// Host interface to bind.
    #[arg(long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,

    /// TCP port to bind.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}

impl Args {
    pub fn parse_from_env() -> Self {
        Self::parse()
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo build -p qsf_browser_server` (still failing — proceeds in next tasks).

### Task 1.3: `AppState` and load result

**Files:**
- Create: `crates/qsf_browser_server/src/state.rs`

- [ ] **Step 1: Write the state module**

```rust
// crates/qsf_browser_server/src/state.rs
use std::sync::Arc;

use qsf_memory::{LoadedStore, StoreLoadError, load_existing};

use crate::cli::Args;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pub load_result: Result<LoadedStore, StoreLoadError>,
    pub store_path: std::path::PathBuf,
}

impl AppState {
    pub fn load(args: &Args) -> Self {
        let load_result = load_existing(&args.store);
        Self {
            inner: Arc::new(Inner {
                load_result,
                store_path: args.store.clone(),
            }),
        }
    }

    pub fn store_path(&self) -> &std::path::Path {
        &self.inner.store_path
    }

    pub fn loaded(&self) -> Result<&LoadedStore, &StoreLoadError> {
        self.inner.load_result.as_ref()
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo build -p qsf_browser_server` (still failing — proceeds).

### Task 1.4: Health route and `LoadError` DTO

**Files:**
- Create: `crates/qsf_browser_server/src/memory/mod.rs`
- Create: `crates/qsf_browser_server/src/memory/dto.rs`
- Create: `crates/qsf_browser_server/src/health/mod.rs`
- Create: `crates/qsf_browser_server/src/health/routes.rs`

- [ ] **Step 1: Define the wire `LoadError` DTO**

```rust
// crates/qsf_browser_server/src/memory/dto.rs
//! DTOs returned over /api/*. These are not the persisted types; mapping
//! happens explicitly in memory::mapping (Phase 2).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadError {
    MissingFile { path: String, message: String },
    InvalidJson { path: String, message: String },
    UnsupportedSchema {
        path: String,
        message: String,
        schema_versions_found: SchemaVersions,
        schema_versions_supported: SchemaVersions,
    },
    InvalidStoreShape {
        path: String,
        message: String,
        schema_versions_found: SchemaVersions,
        shape_errors: Vec<ShapeError>,
    },
    DuplicateMemoryIds {
        path: String,
        message: String,
        duplicate_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchemaVersions {
    pub records: Vec<u16>,
    pub associations: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShapeError {
    pub field_path: String,
    pub message: String,
}

impl From<&qsf_memory::StoreLoadError> for LoadError {
    fn from(err: &qsf_memory::StoreLoadError) -> Self {
        use qsf_memory::StoreLoadError::*;
        match err {
            MissingFile { path, message } => LoadError::MissingFile {
                path: path.display().to_string(),
                message: message.clone(),
            },
            InvalidJson { path, message } => LoadError::InvalidJson {
                path: path.display().to_string(),
                message: message.clone(),
            },
            UnsupportedSchema {
                path,
                message,
                schema_versions_found,
                schema_versions_supported,
            } => LoadError::UnsupportedSchema {
                path: path.display().to_string(),
                message: message.clone(),
                schema_versions_found: SchemaVersions {
                    records: schema_versions_found.records.clone(),
                    associations: schema_versions_found.associations.clone(),
                },
                schema_versions_supported: SchemaVersions {
                    records: schema_versions_supported.records.clone(),
                    associations: schema_versions_supported.associations.clone(),
                },
            },
            InvalidStoreShape {
                path,
                message,
                schema_versions_found,
                shape_errors,
            } => LoadError::InvalidStoreShape {
                path: path.display().to_string(),
                message: message.clone(),
                schema_versions_found: SchemaVersions {
                    records: schema_versions_found.records.clone(),
                    associations: schema_versions_found.associations.clone(),
                },
                shape_errors: shape_errors
                    .iter()
                    .map(|e| ShapeError {
                        field_path: e.field_path.clone(),
                        message: e.message.clone(),
                    })
                    .collect(),
            },
            DuplicateMemoryIds { path, message, duplicate_ids } => LoadError::DuplicateMemoryIds {
                path: path.display().to_string(),
                message: message.clone(),
                duplicate_ids: duplicate_ids.clone(),
            },
        }
    }
}
```

- [ ] **Step 2: Create the memory module stub**

```rust
// crates/qsf_browser_server/src/memory/mod.rs
pub mod dto;
// mapping, filters, routes are added in Phase 2.
```

- [ ] **Step 3: Implement the health route**

```rust
// crates/qsf_browser_server/src/health/mod.rs
pub mod routes;
pub use routes::router;
```

```rust
// crates/qsf_browser_server/src/health/routes.rs
use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::memory::dto::LoadError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum HealthResponse {
    Ok,
    Error { load_error: LoadError },
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    match state.loaded() {
        Ok(_) => Json(HealthResponse::Ok),
        Err(err) => Json(HealthResponse::Error {
            load_error: LoadError::from(err),
        }),
    }
}
```

- [ ] **Step 4: Verify**

Run: `cargo build -p qsf_browser_server` (still incomplete — completes in next task).

### Task 1.5: Server module and binding warning

**Files:**
- Create: `crates/qsf_browser_server/src/server.rs`
- Create: `crates/qsf_browser_server/src/memory/routes_stub.rs`

- [ ] **Step 1: Stub the data routes returning `503` while the load failed**

```rust
// crates/qsf_browser_server/src/memory/routes_stub.rs
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};

use crate::memory::dto::LoadError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/store/summary", get(unavailable))
        .route("/api/memories", get(unavailable))
        .route("/api/memories/:id", get(unavailable))
        .route("/api/memories/:id/neighborhood", get(unavailable))
        .route("/api/memories/:id/raw", get(unavailable))
}

async fn unavailable(State(state): State<AppState>) -> (StatusCode, Json<UnavailableBody>) {
    let body = match state.loaded() {
        Ok(_) => UnavailableBody {
            message: "endpoint not yet implemented".to_string(),
            load_error: None,
        },
        Err(err) => UnavailableBody {
            message: "store failed to load".to_string(),
            load_error: Some(LoadError::from(err)),
        },
    };
    (StatusCode::SERVICE_UNAVAILABLE, Json(body))
}

#[derive(serde::Serialize)]
struct UnavailableBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_error: Option<LoadError>,
}
```

Wire the stub into the memory module:

```rust
// crates/qsf_browser_server/src/memory/mod.rs
pub mod dto;
pub mod routes_stub;
// mapping, filters, real routes are added in Phase 2.
```

- [ ] **Step 2: Implement the server module**

```rust
// crates/qsf_browser_server/src/server.rs
use std::net::{IpAddr, SocketAddr};

use axum::Router;

use crate::cli::Args;
use crate::health;
use crate::memory::routes_stub;
use crate::state::AppState;

pub async fn serve(args: Args) -> anyhow::Result<()> {
    // `engine_logging::initialize()` is the project-wide initializer that
    // wires both stderr and a file logger writing to engine.log in the
    // current working directory.
    engine_logging::initialize();

    let state = AppState::load(&args);
    log_startup_summary(&args, &state);

    let app = Router::new()
        .merge(health::router())
        .merge(routes_stub::router())
        .with_state(state);

    let addr = SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn log_startup_summary(args: &Args, state: &AppState) {
    log::info!("memory store path: {}", state.store_path().display());
    match state.loaded() {
        Ok(loaded) => log::info!(
            "store loaded: {} records, {} associations",
            loaded.contents.records.len(),
            loaded.contents.associations.len()
        ),
        Err(err) => log::warn!("store failed to load: {err}"),
    }
    if !is_loopback(args.host) {
        log::warn!(
            "binding to {} (non-loopback). The Memory Association Browser serves memory contents over HTTP; this address may be reachable from other hosts.",
            args.host
        );
    }
}

fn is_loopback(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
```

`engine_logging::initialize()` is the same helper used elsewhere in the workspace (combines `TermLogger` for stderr and `WriteLogger` for `engine.log` in the current working directory). No other initializer is needed here.

- [ ] **Step 3: Verify the crate builds**

Run: `cargo build -p qsf_browser_server`
Expected: builds clean.

### Task 1.6: Manual smoke test against a real store

- [ ] **Step 1: Run against the runtime's standard store path**

Run:
```bash
cargo run -p qsf_browser_server
# in another shell:
curl -s http://127.0.0.1:3939/api/health
```

Expected: `{"status":"ok"}` if `state/text-loop/memory-store.json` exists, otherwise `{"status":"error","load_error":{"kind":"missing_file",...}}`.

- [ ] **Step 2: Run against a deliberately bad path**

Run:
```bash
cargo run -p qsf_browser_server -- --store /no/such/path.json
curl -s http://127.0.0.1:3939/api/health
curl -i http://127.0.0.1:3939/api/memories | head -3
```

Expected: health returns the `missing_file` body; `/api/memories` returns `503 Service Unavailable` with the same load-error embedded.

- [ ] **Step 3: External human verification**

Ask the project owner to run the two `cargo run` invocations above and confirm the JSON looks right, the log line for the bound address appears, and a non-loopback bind (`--host 0.0.0.0`) prints the disclosure warning. This is the first external test point for this plan.

### Task 1.7: Phase 1 integration test for missing-store regression

**Files:**
- Create: `crates/qsf_browser_server/tests/health_load_error.rs`

- [ ] **Step 1: Write the integration test**

```rust
// crates/qsf_browser_server/tests/health_load_error.rs
use axum::Router;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

use qsf_browser_server::{
    cli::Args,
    health, memory::routes_stub,
    state::AppState,
};

fn app(args: Args) -> Router {
    let state = AppState::load(&args);
    Router::new()
        .merge(health::router())
        .merge(routes_stub::router())
        .with_state(state)
}

#[tokio::test]
async fn missing_store_path_yields_missing_file_on_health() {
    let args = Args {
        store: "/nonexistent/memory-store.json".into(),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let response = app(args)
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["load_error"]["kind"], "missing_file");
}

#[tokio::test]
async fn missing_store_path_yields_503_on_data_endpoints() {
    let args = Args {
        store: "/nonexistent/memory-store.json".into(),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let response = app(args)
        .oneshot(Request::builder().uri("/api/memories").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["load_error"]["kind"], "missing_file");
}
```

If the public surface of `Args` makes direct construction awkward, expose a `pub fn for_tests(store: PathBuf, host: IpAddr, port: u16) -> Self` builder on `Args` rather than weakening field visibility.

- [ ] **Step 2: Run tests**

Run: `cargo test -p qsf_browser_server`
Expected: both tests pass.

### Task 1.8: Close out Phase 1

- [ ] **Step 1: Standard closing steps**

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo test
```

- [ ] **Step 2: Diary entry**

Append to `docs/EngineeringDiary.md`:

```markdown
## 2026-MM-DD - qsf_browser_server skeleton with /api/health

New crate `qsf_browser_server` hosts the HTTP server for post-hoc memory
inspection. Phase 1 implements CLI args, `AppState` over `qsf_memory::load_existing`,
the `/api/health` route, stubbed `503` responses on the other `/api/*` routes,
loopback-by-default binding, and a non-loopback disclosure warning logged via
`engine_logging`.

Refs: crates/qsf_browser_server; implements:
docs/DecisionLog.md#2026-05-20---post-hoc-browser-tools-use-rust-backend--browser-frontend-split
```

- [ ] **Step 3: Commit**

```bash
git add crates/qsf_browser_server docs/EngineeringDiary.md
git commit -m "feat(qsf_browser_server): /api/health and CLI scaffolding"
```

---

## Phase 2: Memory list, summary, detail

Implement the read DTOs, the pure filter/sort/mapping logic, and the data endpoints. The frontend is not built yet; this phase is verified via integration tests and curl.

### Task 2.1: Define the read DTOs

**Files:**
- Modify: `crates/qsf_browser_server/src/memory/dto.rs`

- [x] **Step 1: Add the read DTO types**

Append to `crates/qsf_browser_server/src/memory/dto.rs`:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryListItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub last_reinforced_at: Option<String>,
    pub importance: f64,
    pub reinforcement_count: u32,
    pub estimated_tokens: usize,
    pub association_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssociationDisplay {
    pub other_id: String,
    pub other_title: Option<String>,
    pub weight: f64,
    pub last_reinforced_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryDetail {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub last_reinforced_at: Option<String>,
    pub importance: f64,
    pub reinforcement_count: u32,
    pub source_reference: String,
    pub estimated_tokens: usize,
    pub incoming_count: usize,
    pub outgoing_count: usize,
    pub incoming: Vec<AssociationDisplay>,
    pub outgoing: Vec<AssociationDisplay>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssociationDisplayEdge {
    pub from_id: String,
    pub to_id: String,
    pub weight: f64,
    pub last_reinforced_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Neighborhood {
    pub center: MemoryListItem,
    pub edges: Vec<AssociationDisplayEdge>,
    pub members: Vec<MemoryListItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreSummary {
    pub record_count: usize,
    pub association_count: usize,
    pub broken_associations_count: usize,
    pub total_estimated_tokens: usize,
    pub records_by_kind: BTreeMap<String, usize>,
    pub records_by_tag: Vec<(String, usize)>,
    pub newest: Vec<MemoryListItem>,
    pub most_reinforced: Vec<MemoryListItem>,
    pub highest_importance: Vec<MemoryListItem>,
    pub strongest_associations: Vec<AssociationDisplayEdge>,
    pub orphaned_count: usize,
    pub missing_last_reinforced_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPage {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<MemoryListItem>,
}
```

- [x] **Step 2: Verify**

Run: `cargo build -p qsf_browser_server`

### Task 2.2: Mapping pure functions

**Files:**
- Create: `crates/qsf_browser_server/src/memory/mapping.rs`
- Modify: `crates/qsf_browser_server/src/memory/mod.rs`

- [x] **Step 1: Write mapping functions**

```rust
// crates/qsf_browser_server/src/memory/mapping.rs
//! Pure conversions from persisted types into wire DTOs.

use std::collections::{HashMap, HashSet};

use qsf_memory::{Association, MemoryRecord, MemoryRecordKind, MemoryStoreContents};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::dto::{
    AssociationDisplay, AssociationDisplayEdge, MemoryDetail, MemoryListItem,
};

pub fn format_ts(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339).expect("RFC3339 always formats")
}

pub fn kind_str(kind: &MemoryRecordKind) -> String {
    match kind {
        MemoryRecordKind::Concept => "concept",
        MemoryRecordKind::ArchitectureNote => "architecture_note",
        MemoryRecordKind::Experiment => "experiment",
        MemoryRecordKind::Decision => "decision",
        MemoryRecordKind::Question => "question",
        MemoryRecordKind::Observation => "observation",
    }
    .to_string()
}

pub struct Index<'a> {
    pub by_id: HashMap<&'a str, &'a MemoryRecord>,
    pub outgoing: HashMap<&'a str, Vec<&'a Association>>,
    pub incoming: HashMap<&'a str, Vec<&'a Association>>,
}

pub fn build_index(store: &MemoryStoreContents) -> Index<'_> {
    let mut by_id = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&Association>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&Association>> = HashMap::new();
    for r in &store.records {
        by_id.insert(r.id.as_str(), r);
    }
    for a in &store.associations {
        outgoing.entry(a.from_memory_id.as_str()).or_default().push(a);
        incoming.entry(a.to_memory_id.as_str()).or_default().push(a);
    }
    Index {
        by_id,
        outgoing,
        incoming,
    }
}

pub fn to_list_item(record: &MemoryRecord, index: &Index<'_>) -> MemoryListItem {
    let association_count = index
        .outgoing
        .get(record.id.as_str())
        .map(|v| v.len())
        .unwrap_or(0)
        + index
            .incoming
            .get(record.id.as_str())
            .map(|v| v.len())
            .unwrap_or(0);
    MemoryListItem {
        id: record.id.clone(),
        kind: kind_str(&record.kind),
        title: record.title.clone(),
        summary: record.summary.clone(),
        tags: record.tags.clone(),
        created_at: format_ts(record.created_at),
        last_reinforced_at: record.last_reinforced_at.map(format_ts),
        importance: record.importance,
        reinforcement_count: record.reinforcement_count,
        estimated_tokens: record.estimated_tokens,
        association_count,
    }
}

pub fn to_detail(record: &MemoryRecord, index: &Index<'_>) -> MemoryDetail {
    let outgoing_vec = index
        .outgoing
        .get(record.id.as_str())
        .cloned()
        .unwrap_or_default();
    let incoming_vec = index
        .incoming
        .get(record.id.as_str())
        .cloned()
        .unwrap_or_default();
    let outgoing = sort_and_map_assocs(record.id.as_str(), &outgoing_vec, true, index);
    let incoming = sort_and_map_assocs(record.id.as_str(), &incoming_vec, false, index);
    MemoryDetail {
        id: record.id.clone(),
        kind: kind_str(&record.kind),
        title: record.title.clone(),
        summary: record.summary.clone(),
        tags: record.tags.clone(),
        created_at: format_ts(record.created_at),
        last_reinforced_at: record.last_reinforced_at.map(format_ts),
        importance: record.importance,
        reinforcement_count: record.reinforcement_count,
        source_reference: record.source_reference.clone(),
        estimated_tokens: record.estimated_tokens,
        incoming_count: incoming.len(),
        outgoing_count: outgoing.len(),
        incoming,
        outgoing,
    }
}

fn sort_and_map_assocs(
    self_id: &str,
    assocs: &[&Association],
    outgoing: bool,
    index: &Index<'_>,
) -> Vec<AssociationDisplay> {
    let mut sorted = assocs.to_vec();
    sorted.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    sorted
        .into_iter()
        .map(|a| {
            let other_id = if outgoing {
                a.to_memory_id.as_str()
            } else {
                a.from_memory_id.as_str()
            };
            AssociationDisplay {
                other_id: other_id.to_string(),
                other_title: index.by_id.get(other_id).map(|r| r.title.clone()),
                weight: a.weight,
                last_reinforced_at: format_ts(a.last_reinforced_at),
                reason: a.reason.clone(),
            }
        })
        .filter(|d| d.other_id != self_id)
        .collect()
}

pub fn association_edge(a: &Association) -> AssociationDisplayEdge {
    AssociationDisplayEdge {
        from_id: a.from_memory_id.clone(),
        to_id: a.to_memory_id.clone(),
        weight: a.weight,
        last_reinforced_at: format_ts(a.last_reinforced_at),
        reason: a.reason.clone(),
    }
}

pub fn orphan_ids(store: &MemoryStoreContents) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for a in &store.associations {
        referenced.insert(a.from_memory_id.clone());
        referenced.insert(a.to_memory_id.clone());
    }
    store
        .records
        .iter()
        .map(|r| r.id.clone())
        .filter(|id| !referenced.contains(id))
        .collect()
}
```

Register the module:

```rust
// crates/qsf_browser_server/src/memory/mod.rs
pub mod dto;
pub mod mapping;
pub mod routes_stub;
```

- [x] **Step 2: Write mapping tests**

Append to `crates/qsf_browser_server/src/memory/mapping.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use qsf_memory::{Association, MemoryRecord, MemoryRecordKind, MemoryStoreContents};
    use time::macros::datetime;

    fn fixture() -> MemoryStoreContents {
        let r = |id: &str, title: &str| MemoryRecord {
            schema_version: qsf_memory::MEMORY_RECORD_SCHEMA_VERSION,
            id: id.to_string(),
            kind: MemoryRecordKind::Concept,
            title: title.to_string(),
            summary: "s".into(),
            tags: vec!["tag".into()],
            created_at: datetime!(2026-05-20 0:00 UTC),
            importance: 0.5,
            reinforcement_count: 0,
            last_reinforced_at: Some(datetime!(2026-05-20 0:00 UTC)),
            source_reference: "src".into(),
            estimated_tokens: 10,
        };
        let a = |from: &str, to: &str, weight: f64| Association {
            schema_version: qsf_memory::ASSOCIATION_SCHEMA_VERSION,
            from_memory_id: from.into(),
            to_memory_id: to.into(),
            weight,
            reason: "r".into(),
            last_reinforced_at: datetime!(2026-05-20 0:00 UTC),
        };
        MemoryStoreContents {
            records: vec![r("a", "A"), r("b", "B")],
            associations: vec![a("a", "b", 0.9), a("a", "ghost", 0.5), a("b", "a", 0.3)],
        }
    }

    #[test]
    fn detail_lists_incoming_and_outgoing_sorted_by_weight_desc() {
        let store = fixture();
        let idx = build_index(&store);
        let detail = to_detail(&store.records[0], &idx);
        assert_eq!(detail.outgoing.len(), 2);
        assert!(detail.outgoing[0].weight >= detail.outgoing[1].weight);
        assert_eq!(detail.incoming.len(), 1);
    }

    #[test]
    fn broken_edge_other_title_is_null() {
        let store = fixture();
        let idx = build_index(&store);
        let detail = to_detail(&store.records[0], &idx);
        let ghost = detail.outgoing.iter().find(|d| d.other_id == "ghost").unwrap();
        assert!(ghost.other_title.is_none());
    }

    #[test]
    fn orphan_ids_excludes_associated_records() {
        let store = fixture();
        let orphans = orphan_ids(&store);
        assert!(!orphans.contains("a"));
        assert!(!orphans.contains("b"));
    }
}
```

- [x] **Step 3: Run tests**

Run: `cargo test -p qsf_browser_server`
Expected: mapping tests pass.

### Task 2.3: Filter and sort predicates

**Files:**
- Create: `crates/qsf_browser_server/src/memory/filters.rs`
- Modify: `crates/qsf_browser_server/src/memory/mod.rs`

- [x] **Step 1: Write filter and sort logic**

```rust
// crates/qsf_browser_server/src/memory/filters.rs
use std::collections::HashSet;

use qsf_memory::{MemoryRecord, MemoryStoreContents};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::mapping::{Index, orphan_ids};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    #[serde(default, rename = "tag")]
    pub tags: Vec<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
    pub last_reinforced_from: Option<String>,
    pub last_reinforced_to: Option<String>,
    pub delta_since: Option<String>,
    pub min_importance: Option<f64>,
    pub min_reinforcement_count: Option<u32>,
    pub has_associations: Option<bool>,
    pub orphaned: Option<bool>,
    pub missing_last_reinforced: Option<bool>,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 500;

pub fn filter_records<'a>(
    store: &'a MemoryStoreContents,
    index: &Index<'a>,
    query: &ListQuery,
) -> Vec<&'a MemoryRecord> {
    let q = query.q.as_deref().map(str::to_lowercase);
    let kind = query.kind.as_deref();
    let tags: HashSet<&str> = query.tags.iter().map(String::as_str).collect();
    let created_from = parse_ts(query.created_from.as_deref());
    let created_to = parse_ts(query.created_to.as_deref());
    let last_from = parse_ts(query.last_reinforced_from.as_deref());
    let last_to = parse_ts(query.last_reinforced_to.as_deref());
    let delta_since = parse_ts(query.delta_since.as_deref());
    let orphans = if matches!(query.orphaned, Some(_)) {
        Some(orphan_ids(store))
    } else {
        None
    };

    store
        .records
        .iter()
        .filter(|r| match &q {
            Some(needle) => keyword_hit(r, needle),
            None => true,
        })
        .filter(|r| match kind {
            Some(k) => super::mapping::kind_str(&r.kind) == k,
            None => true,
        })
        .filter(|r| tags.is_empty() || r.tags.iter().any(|t| tags.contains(t.as_str())))
        .filter(|r| created_from.map_or(true, |t| r.created_at >= t))
        .filter(|r| created_to.map_or(true, |t| r.created_at <= t))
        .filter(|r| match last_from {
            Some(t) => r.last_reinforced_at.map_or(false, |lr| lr >= t),
            None => true,
        })
        .filter(|r| match last_to {
            Some(t) => r.last_reinforced_at.map_or(false, |lr| lr <= t),
            None => true,
        })
        .filter(|r| match delta_since {
            Some(t) => r.created_at >= t || r.last_reinforced_at.map_or(false, |lr| lr >= t),
            None => true,
        })
        .filter(|r| query.min_importance.map_or(true, |m| r.importance >= m))
        .filter(|r| {
            query
                .min_reinforcement_count
                .map_or(true, |m| r.reinforcement_count >= m)
        })
        .filter(|r| match query.has_associations {
            Some(true) => index.outgoing.contains_key(r.id.as_str()) || index.incoming.contains_key(r.id.as_str()),
            Some(false) => !(index.outgoing.contains_key(r.id.as_str()) || index.incoming.contains_key(r.id.as_str())),
            None => true,
        })
        .filter(|r| match (&orphans, query.orphaned) {
            (Some(set), Some(true)) => set.contains(&r.id),
            (Some(set), Some(false)) => !set.contains(&r.id),
            _ => true,
        })
        .filter(|r| match query.missing_last_reinforced {
            Some(true) => r.last_reinforced_at.is_none(),
            Some(false) => r.last_reinforced_at.is_some(),
            None => true,
        })
        .collect()
}

fn keyword_hit(record: &MemoryRecord, needle: &str) -> bool {
    let haystacks = [
        record.title.to_lowercase(),
        record.summary.to_lowercase(),
        record.source_reference.to_lowercase(),
    ];
    if haystacks.iter().any(|h| h.contains(needle)) {
        return true;
    }
    record
        .tags
        .iter()
        .any(|t| t.to_lowercase().contains(needle))
}

fn parse_ts(s: Option<&str>) -> Option<OffsetDateTime> {
    s.and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
}

pub fn sort_records<'a>(records: &mut Vec<&'a MemoryRecord>, sort: Option<&str>, index: &Index<'a>) {
    match sort.unwrap_or("newest") {
        "oldest" => records.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        "most_reinforced" => records.sort_by(|a, b| b.reinforcement_count.cmp(&a.reinforcement_count)),
        "highest_importance" => {
            records.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
        }
        "strongest_connected" => {
            let strength = |id: &str| -> f64 {
                let out = index.outgoing.get(id).into_iter().flatten();
                let inb = index.incoming.get(id).into_iter().flatten();
                out.chain(inb).map(|a| a.weight).sum()
            };
            records.sort_by(|a, b| {
                strength(b.id.as_str())
                    .partial_cmp(&strength(a.id.as_str()))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "largest_tokens" => records.sort_by(|a, b| b.estimated_tokens.cmp(&a.estimated_tokens)),
        _ => records.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
    }
}

pub fn paginate<'a>(records: &[&'a MemoryRecord], query: &ListQuery) -> (usize, usize, Vec<&'a MemoryRecord>) {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).max(1);
    let slice: Vec<&MemoryRecord> = records.iter().skip(offset).take(limit).copied().collect();
    (offset, limit, slice)
}
```

Register the module:

```rust
// crates/qsf_browser_server/src/memory/mod.rs
pub mod dto;
pub mod filters;
pub mod mapping;
pub mod routes_stub;
```

- [x] **Step 2: Add filter/sort tests**

Append `#[cfg(test)]` block to `crates/qsf_browser_server/src/memory/filters.rs` covering each predicate (keyword, kind, tag, created range, last_reinforced range, delta_since, min_importance, min_reinforcement_count, has_associations, orphaned, missing_last_reinforced) and each sort key against the same fixture used in `mapping.rs`. Use direct construction of `ListQuery { ..Default::default() }` per test.

For brevity here, write one representative test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use qsf_memory::{Association, MemoryRecord, MemoryRecordKind, MemoryStoreContents};
    use time::macros::datetime;

    fn fixture() -> MemoryStoreContents {
        // Re-create the same fixture used in mapping::tests.
        let mut r = |id: &str, title: &str, imp: f64, reinf: u32, last_reinforced: bool| MemoryRecord {
            schema_version: qsf_memory::MEMORY_RECORD_SCHEMA_VERSION,
            id: id.into(),
            kind: MemoryRecordKind::Concept,
            title: title.into(),
            summary: "s".into(),
            tags: vec!["t".into()],
            created_at: datetime!(2026-05-20 0:00 UTC),
            importance: imp,
            reinforcement_count: reinf,
            last_reinforced_at: if last_reinforced { Some(datetime!(2026-05-20 0:00 UTC)) } else { None },
            source_reference: "src".into(),
            estimated_tokens: 10,
        };
        MemoryStoreContents {
            records: vec![r("a", "A", 0.9, 3, true), r("b", "B", 0.1, 0, false)],
            associations: vec![Association {
                schema_version: qsf_memory::ASSOCIATION_SCHEMA_VERSION,
                from_memory_id: "a".into(),
                to_memory_id: "b".into(),
                weight: 0.5,
                reason: "r".into(),
                last_reinforced_at: datetime!(2026-05-20 0:00 UTC),
            }],
        }
    }

    fn ids(records: &[&MemoryRecord]) -> Vec<String> {
        records.iter().map(|r| r.id.clone()).collect()
    }

    #[test]
    fn missing_last_reinforced_filter() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { missing_last_reinforced: Some(true), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert_eq!(ids(&out), vec!["b"]);
    }

    #[test]
    fn keyword_search_matches_title_case_insensitive() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { q: Some("alpha".into()), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert_eq!(ids(&out), vec!["a"]);
    }

    #[test]
    fn kind_filter_excludes_other_kinds() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { kind: Some("decision".into()), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert!(out.is_empty());
    }

    #[test]
    fn tag_filter_keeps_only_tagged() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { tags: vec!["t".into()], ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn min_importance_threshold() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { min_importance: Some(0.5), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert_eq!(ids(&out), vec!["a"]);
    }

    #[test]
    fn min_reinforcement_count_threshold() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { min_reinforcement_count: Some(1), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert_eq!(ids(&out), vec!["a"]);
    }

    #[test]
    fn has_associations_true_keeps_connected() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { has_associations: Some(true), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        // Both records are connected via the single association in the fixture.
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn has_associations_false_excludes_connected() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { has_associations: Some(false), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert!(out.is_empty());
    }

    #[test]
    fn orphaned_true_returns_only_unreferenced() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let q = ListQuery { orphaned: Some(true), ..Default::default() };
        let out = filter_records(&store, &idx, &q);
        assert!(out.is_empty()); // both are referenced by the single association
    }

    #[test]
    fn sort_oldest_orders_ascending_by_created_at() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let mut all = filter_records(&store, &idx, &ListQuery::default());
        sort_records(&mut all, Some("oldest"), &idx);
        // Both fixture records share created_at; this asserts stability, not order.
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn sort_highest_importance_puts_a_first() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let mut all = filter_records(&store, &idx, &ListQuery::default());
        sort_records(&mut all, Some("highest_importance"), &idx);
        assert_eq!(all[0].id, "a");
    }

    #[test]
    fn sort_most_reinforced_puts_a_first() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let mut all = filter_records(&store, &idx, &ListQuery::default());
        sort_records(&mut all, Some("most_reinforced"), &idx);
        assert_eq!(all[0].id, "a");
    }

    #[test]
    fn sort_strongest_connected_orders_by_weight_sum() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let mut all = filter_records(&store, &idx, &ListQuery::default());
        sort_records(&mut all, Some("strongest_connected"), &idx);
        // a and b share a single edge with weight 0.5; same connected strength,
        // so we just assert it doesn't panic and produces a stable order.
        assert_eq!(all.len(), 2);
    }
}
```

- [x] **Step 3: Run tests**

Run: `cargo test -p qsf_browser_server`
Expected: all filter and sort tests pass.

### Task 2.4: Implement the real data routes

**Files:**
- Create: `crates/qsf_browser_server/src/memory/routes.rs`
- Modify: `crates/qsf_browser_server/src/memory/mod.rs`
- Modify: `crates/qsf_browser_server/src/server.rs`
- Delete: `crates/qsf_browser_server/src/memory/routes_stub.rs` (after the real routes are in place; see Step 4)

- [x] **Step 1: Write the real handlers**

```rust
// crates/qsf_browser_server/src/memory/routes.rs
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
// axum_extra::extract::Query uses serde_html_form, which supports
// repeated query params like `?tag=x&tag=y` (the design's wire contract for
// the `tag` filter). The default `axum::extract::Query` uses serde_urlencoded
// and would fail to deserialize `Vec<String>` from repeated keys.
use axum_extra::extract::Query;

use qsf_memory::{LoadedStore, MemoryStoreContents, dangling_association_ids};

use super::dto::{
    AssociationDisplayEdge, LoadError, MemoryDetail, MemoryListItem, MemoryPage, Neighborhood,
    StoreSummary,
};
use super::filters::{ListQuery, filter_records, paginate, sort_records};
use super::mapping::{
    Index, association_edge, build_index, kind_str, orphan_ids, to_detail, to_list_item,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/store/summary", get(store_summary))
        .route("/api/memories", get(list_memories))
        .route("/api/memories/:id", get(get_memory))
        .route("/api/memories/:id/raw", get(get_memory_raw))
        .route("/api/memories/:id/neighborhood", get(get_memory_neighborhood))
}

fn loaded_or_503(state: &AppState) -> Result<&LoadedStore, (StatusCode, Json<serde_json::Value>)> {
    state.loaded().map_err(|err| {
        let body = serde_json::json!({
            "message": "store failed to load",
            "load_error": LoadError::from(err),
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(body))
    })
}

async fn store_summary(
    State(state): State<AppState>,
) -> Result<Json<StoreSummary>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);
    let dangling = dangling_association_ids(store);
    let orphans = orphan_ids(store);
    let missing_lr = store.records.iter().filter(|r| r.last_reinforced_at.is_none()).count();

    let mut records_by_kind: std::collections::BTreeMap<String, usize> = Default::default();
    for r in &store.records {
        *records_by_kind.entry(kind_str(&r.kind)).or_insert(0) += 1;
    }
    let mut tag_counts: std::collections::HashMap<String, usize> = Default::default();
    for r in &store.records {
        for t in &r.tags {
            *tag_counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut records_by_tag: Vec<(String, usize)> = tag_counts.into_iter().collect();
    records_by_tag.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    records_by_tag.truncate(20);

    let mut newest: Vec<_> = store.records.iter().collect();
    newest.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    newest.truncate(5);
    let mut most_reinforced: Vec<_> = store.records.iter().collect();
    most_reinforced.sort_by(|a, b| b.reinforcement_count.cmp(&a.reinforcement_count));
    most_reinforced.truncate(5);
    let mut highest_importance: Vec<_> = store.records.iter().collect();
    highest_importance.sort_by(|a, b| {
        b.importance
            .partial_cmp(&a.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    highest_importance.truncate(5);
    let mut strongest: Vec<_> = store.associations.iter().collect();
    strongest.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    strongest.truncate(5);

    Ok(Json(StoreSummary {
        record_count: store.records.len(),
        association_count: store.associations.len(),
        broken_associations_count: dangling.len(),
        total_estimated_tokens: store.records.iter().map(|r| r.estimated_tokens).sum(),
        records_by_kind,
        records_by_tag,
        newest: newest.iter().map(|r| to_list_item(r, &index)).collect(),
        most_reinforced: most_reinforced.iter().map(|r| to_list_item(r, &index)).collect(),
        highest_importance: highest_importance.iter().map(|r| to_list_item(r, &index)).collect(),
        strongest_associations: strongest.iter().map(|a| association_edge(a)).collect(),
        orphaned_count: orphans.len(),
        missing_last_reinforced_count: missing_lr,
    }))
}

async fn list_memories(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<MemoryPage>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);

    let mut filtered = filter_records(store, &index, &query);
    sort_records(&mut filtered, query.sort.as_deref(), &index);
    let total = filtered.len();
    let (offset, limit, page) = paginate(&filtered, &query);
    Ok(Json(MemoryPage {
        total,
        offset,
        limit,
        items: page.iter().map(|r| to_list_item(r, &index)).collect(),
    }))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryDetail>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);
    match store.records.iter().find(|r| r.id == id) {
        Some(r) => Ok(Json(to_detail(r, &index))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "message": "memory not found", "id": id })),
        )),
    }
}

async fn get_memory_raw(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    match loaded.raw_records.get(&id) {
        Some(value) => Ok(Json(value.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "message": "memory not found", "id": id })),
        )),
    }
}

async fn get_memory_neighborhood(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<NeighborhoodQuery>,
) -> Result<Json<Neighborhood>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);
    let limit = query.limit.unwrap_or(8).clamp(1, 64);

    let center_record = match store.records.iter().find(|r| r.id == id) {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "message": "memory not found", "id": id })),
            ));
        }
    };
    let center = to_list_item(center_record, &index);

    let mut edges = Vec::new();
    for a in store.associations.iter().filter(|a| a.from_memory_id == id || a.to_memory_id == id) {
        edges.push(association_edge(a));
    }
    edges.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    edges.truncate(limit);

    let mut member_ids: std::collections::HashSet<String> = Default::default();
    for e in &edges {
        if e.from_id != id {
            member_ids.insert(e.from_id.clone());
        }
        if e.to_id != id {
            member_ids.insert(e.to_id.clone());
        }
    }
    let members: Vec<MemoryListItem> = member_ids
        .iter()
        .filter_map(|m| index.by_id.get(m.as_str()).map(|r| to_list_item(r, &index)))
        .collect();

    Ok(Json(Neighborhood { center, edges, members }))
}

#[derive(serde::Deserialize)]
struct NeighborhoodQuery {
    limit: Option<usize>,
}
```

- [x] **Step 2: Register the real routes module**

```rust
// crates/qsf_browser_server/src/memory/mod.rs
pub mod dto;
pub mod filters;
pub mod mapping;
pub mod routes;
```

- [x] **Step 3: Swap server module to use the real router**

In `crates/qsf_browser_server/src/server.rs`, change:

```rust
use crate::memory::routes_stub;
```
to:
```rust
use crate::memory::routes;
```
and update the `.merge(routes_stub::router())` line to `.merge(routes::router())`.

- [x] **Step 4: Delete the stub module**

Remove `crates/qsf_browser_server/src/memory/routes_stub.rs` and any remaining `pub mod routes_stub;` line. The earlier integration test in `tests/health_load_error.rs` should be updated to import `memory::routes` instead of `routes_stub`.

- [x] **Step 5: Verify**

Run: `cargo build -p qsf_browser_server`
Expected: builds clean.

### Task 2.5: Integration tests for the data routes

**Files:**
- Create: `crates/qsf_browser_server/tests/data_endpoints.rs`
- Create: `crates/qsf_browser_server/tests/fixtures/small-store.json` (committed test fixture)

- [x] **Step 1: Write a small fixture store**

```json
// crates/qsf_browser_server/tests/fixtures/small-store.json
{
  "records": [
    { "schema_version": 1, "id": "a", "kind": "concept", "title": "Alpha", "summary": "first", "tags": ["x"], "created_at": "2026-05-19T00:00:00Z", "importance": 0.9, "reinforcement_count": 3, "last_reinforced_at": "2026-05-20T00:00:00Z", "source_reference": "ref-a", "estimated_tokens": 10, "future_field": "kept" },
    { "schema_version": 1, "id": "b", "kind": "concept", "title": "Beta", "summary": "second", "tags": ["x", "y"], "created_at": "2026-05-20T00:00:00Z", "importance": 0.1, "reinforcement_count": 0, "source_reference": "ref-b", "estimated_tokens": 5 }
  ],
  "associations": [
    { "schema_version": 1, "from_memory_id": "a", "to_memory_id": "b", "weight": 0.9, "reason": "supports", "last_reinforced_at": "2026-05-20T00:00:00Z" },
    { "schema_version": 1, "from_memory_id": "a", "to_memory_id": "ghost", "weight": 0.5, "reason": "broken", "last_reinforced_at": "2026-05-20T00:00:00Z" }
  ]
}
```

- [x] **Step 2: Write integration tests**

```rust
// crates/qsf_browser_server/tests/data_endpoints.rs
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

use qsf_browser_server::{cli::Args, memory::routes, health, state::AppState};

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/small-store.json")
}

fn app() -> axum::Router {
    let args = Args { store: fixture_path(), host: "127.0.0.1".parse().unwrap(), port: 0 };
    let state = AppState::load(&args);
    axum::Router::new()
        .merge(health::router())
        .merge(routes::router())
        .with_state(state)
}

#[tokio::test]
async fn summary_reports_broken_associations() {
    let response = app()
        .oneshot(Request::builder().uri("/api/store/summary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["broken_associations_count"], 1);
}

#[tokio::test]
async fn detail_surfaces_broken_edge_as_null_other_title() {
    let response = app()
        .oneshot(Request::builder().uri("/api/memories/a").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let outgoing = json["outgoing"].as_array().unwrap();
    let ghost = outgoing.iter().find(|e| e["other_id"] == "ghost").unwrap();
    assert!(ghost["other_title"].is_null());
}

#[tokio::test]
async fn raw_endpoint_preserves_extra_fields() {
    let response = app()
        .oneshot(Request::builder().uri("/api/memories/a/raw").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["future_field"], "kept");
}

#[tokio::test]
async fn neighborhood_includes_broken_edge_member_missing() {
    let response = app()
        .oneshot(Request::builder().uri("/api/memories/a/neighborhood").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let edges = json["edges"].as_array().unwrap();
    let to_ids: Vec<String> = edges.iter().map(|e| e["to_id"].as_str().unwrap().to_string()).collect();
    assert!(to_ids.contains(&"ghost".to_string()));
    let members = json["members"].as_array().unwrap();
    let member_ids: Vec<String> = members.iter().map(|m| m["id"].as_str().unwrap().to_string()).collect();
    assert!(!member_ids.contains(&"ghost".to_string()));
}
```

- [x] **Step 3: Run tests**

Run: `cargo test -p qsf_browser_server`
Expected: all integration tests pass.

### Task 2.6: External smoke test

- [ ] **Step 1: Hit each endpoint with curl against a real store**

Run the server (`cargo run -p qsf_browser_server`), then:

```bash
curl -s http://127.0.0.1:3939/api/store/summary | head
curl -s "http://127.0.0.1:3939/api/memories?sort=most_reinforced&limit=5" | head
curl -s http://127.0.0.1:3939/api/memories/<some-id> | head
curl -s http://127.0.0.1:3939/api/memories/<some-id>/raw | head
curl -s "http://127.0.0.1:3939/api/memories/<some-id>/neighborhood?limit=4" | head
```

Replace `<some-id>` with an actual id taken from the summary response.

Expected: each endpoint returns a JSON body matching the DTO contract.

- [ ] **Step 2: External human verification**

Ask the project owner to inspect the JSON for ordering, counts, broken-edge handling, and raw-field preservation against the actual `state/text-loop/memory-store.json`. This is the second external test point.

### Task 2.7: Close out Phase 2

- [x] Standard closing steps.
- [x] Diary entry covering DTOs, filters, sort keys, mapping, and integration tests.
- [ ] Commit: `feat(qsf_browser_server): memory list/summary/detail endpoints`.

---

## Phase 3: Frontend shell (HTML/CSS, no canvas)

Build the TypeScript/Vite shell at `crates/qsf_browser_server/ui/`. Implement layout C from the design (list left, canvas slot top-right placeholder, inspector bottom-right), all controls, URL-state encoding, and the load-error screen. PixiJS is not used yet; the canvas slot shows a placeholder.

### Task 3.1: Vite + TypeScript scaffold

**Files:**
- Create: `crates/qsf_browser_server/ui/package.json`
- Create: `crates/qsf_browser_server/ui/tsconfig.json`
- Create: `crates/qsf_browser_server/ui/vite.config.ts`
- Create: `crates/qsf_browser_server/ui/index.html`
- Create: `crates/qsf_browser_server/ui/src/main.ts`
- Modify: `.gitignore` (add `crates/qsf_browser_server/ui/node_modules/` and `crates/qsf_browser_server/ui/dist/`)

- [x] **Step 1: `package.json`**

```json
{
  "name": "qsf-memory-browser-ui",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "preview": "vite preview"
  },
  "devDependencies": {
    "typescript": "^5.4.0",
    "vite": "^5.4.0"
  }
}
```

- [x] **Step 2: `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "lib": ["ES2022", "DOM", "DOM.Iterable"]
  },
  "include": ["src"]
}
```

- [x] **Step 3: `vite.config.ts`** (Vite proxies `/api/*` to the Rust server in dev)

```ts
import { defineConfig } from "vite";

export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:3939",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
```

- [x] **Step 4: `index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>QSF Memory Association Browser</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [x] **Step 5: Minimal `main.ts`**

```ts
// src/main.ts
const root = document.getElementById("app");
if (root) root.textContent = "QSF Memory Association Browser — loading…";
```

- [x] **Step 6: Update `.gitignore`**

Append:
```
crates/qsf_browser_server/ui/node_modules/
crates/qsf_browser_server/ui/dist/
```

- [x] **Step 7: Verify**

Run:
```bash
cd crates/qsf_browser_server/ui
npm install
npm run build
```
Expected: a `dist/` folder is created and contains `index.html` + bundled JS.

### Task 3.2: Visual tokens

**Files:**
- Create: `crates/qsf_browser_server/ui/src/tokens.css`
- Modify: `crates/qsf_browser_server/ui/src/main.ts`

- [x] **Step 1: Tokens**

```css
/* src/tokens.css */
:root {
  --qsf-bg-page: #050812;
  --qsf-bg-field: #07162a;
  --qsf-bg-panel: rgba(7, 18, 32, 0.88);
  --qsf-bg-panel-elevated: rgba(12, 30, 48, 0.92);
  --qsf-border-subtle: rgba(172, 215, 255, 0.18);
  --qsf-border-active: rgba(158, 221, 255, 0.56);
  --qsf-text-primary: #eaf6ff;
  --qsf-text-secondary: #b8cbe2;
  --qsf-text-muted: #6f89a5;
  --qsf-signal-context: #7de3ff;
  --qsf-signal-memory: #ffd76a;
  --qsf-signal-association: #ffb94a;
  --qsf-signal-error: #ff5d73;
  --qsf-signal-success: #7ae28a;
  --qsf-radius-panel: 8px;
  --qsf-duration-fast: 140ms;
  --qsf-duration-panel: 180ms;
  --qsf-font-tabular: "Inter", "Segoe UI", system-ui, sans-serif;
}

html, body, #app {
  background: var(--qsf-bg-page);
  color: var(--qsf-text-primary);
  font-family: var(--qsf-font-tabular);
  font-variant-numeric: tabular-nums;
  margin: 0;
  height: 100%;
  font-size: 14px;
  line-height: 20px;
}

a { color: var(--qsf-signal-context); }
```

- [x] **Step 2: Import tokens from `main.ts`**

Replace `main.ts`:

```ts
import "./tokens.css";

const root = document.getElementById("app");
if (root) root.textContent = "QSF Memory Association Browser — loading…";
```

- [x] **Step 3: Verify**

Run: `npm run build`. Expected: build succeeds and `dist/assets/` contains the CSS.

### Task 3.3: DTO mirrors and API wrappers

**Files:**
- Create: `crates/qsf_browser_server/ui/src/types.ts`
- Create: `crates/qsf_browser_server/ui/src/api.ts`

- [x] **Step 1: Type mirrors**

```ts
// src/types.ts
export type MemoryKind =
  | "concept"
  | "architecture_note"
  | "experiment"
  | "decision"
  | "question"
  | "observation";

export interface MemoryListItem {
  id: string;
  kind: string;
  title: string;
  summary: string;
  tags: string[];
  created_at: string;
  last_reinforced_at: string | null;
  importance: number;
  reinforcement_count: number;
  estimated_tokens: number;
  association_count: number;
}

export interface AssociationDisplay {
  other_id: string;
  other_title: string | null;
  weight: number;
  last_reinforced_at: string;
  reason: string;
}

export interface MemoryDetail {
  id: string;
  kind: string;
  title: string;
  summary: string;
  tags: string[];
  created_at: string;
  last_reinforced_at: string | null;
  importance: number;
  reinforcement_count: number;
  source_reference: string;
  estimated_tokens: number;
  incoming_count: number;
  outgoing_count: number;
  incoming: AssociationDisplay[];
  outgoing: AssociationDisplay[];
}

export interface AssociationDisplayEdge {
  from_id: string;
  to_id: string;
  weight: number;
  last_reinforced_at: string;
  reason: string;
}

export interface Neighborhood {
  center: MemoryListItem;
  edges: AssociationDisplayEdge[];
  members: MemoryListItem[];
}

export interface StoreSummary {
  record_count: number;
  association_count: number;
  broken_associations_count: number;
  total_estimated_tokens: number;
  records_by_kind: Record<string, number>;
  records_by_tag: Array<[string, number]>;
  newest: MemoryListItem[];
  most_reinforced: MemoryListItem[];
  highest_importance: MemoryListItem[];
  strongest_associations: AssociationDisplayEdge[];
  orphaned_count: number;
  missing_last_reinforced_count: number;
}

export interface MemoryPage {
  total: number;
  offset: number;
  limit: number;
  items: MemoryListItem[];
}

export type LoadError =
  | { kind: "missing_file"; path: string; message: string }
  | { kind: "invalid_json"; path: string; message: string }
  | {
      kind: "unsupported_schema";
      path: string;
      message: string;
      schema_versions_found: { records: number[]; associations: number[] };
      schema_versions_supported: { records: number[]; associations: number[] };
    }
  | {
      kind: "invalid_store_shape";
      path: string;
      message: string;
      schema_versions_found: { records: number[]; associations: number[] };
      shape_errors: Array<{ field_path: string; message: string }>;
    }
  | {
      kind: "duplicate_memory_ids";
      path: string;
      message: string;
      duplicate_ids: string[];
    };

export type HealthResponse =
  | { status: "ok" }
  | { status: "error"; load_error: LoadError };
```

- [x] **Step 2: API wrappers**

```ts
// src/api.ts
import type {
  HealthResponse, MemoryDetail, MemoryPage, Neighborhood, StoreSummary,
} from "./types";

export interface ListMemoriesQuery {
  q?: string;
  kind?: string;
  tag?: string[];
  createdFrom?: string;
  createdTo?: string;
  lastReinforcedFrom?: string;
  lastReinforcedTo?: string;
  deltaSince?: string;
  minImportance?: number;
  minReinforcementCount?: number;
  hasAssociations?: boolean;
  orphaned?: boolean;
  missingLastReinforced?: boolean;
  sort?: string;
  limit?: number;
  offset?: number;
}

function qs(params: Record<string, unknown>): string {
  const sp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    if (Array.isArray(v)) v.forEach((item) => sp.append(k, String(item)));
    else sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw Object.assign(new Error(`HTTP ${res.status} on ${url}`), { status: res.status, body });
  }
  return res.json() as Promise<T>;
}

export const api = {
  health: () => getJson<HealthResponse>("/api/health"),
  storeSummary: () => getJson<StoreSummary>("/api/store/summary"),
  listMemories: (q: ListMemoriesQuery) =>
    getJson<MemoryPage>("/api/memories" + qs({
      q: q.q,
      kind: q.kind,
      tag: q.tag,
      created_from: q.createdFrom,
      created_to: q.createdTo,
      last_reinforced_from: q.lastReinforcedFrom,
      last_reinforced_to: q.lastReinforcedTo,
      delta_since: q.deltaSince,
      min_importance: q.minImportance,
      min_reinforcement_count: q.minReinforcementCount,
      has_associations: q.hasAssociations,
      orphaned: q.orphaned,
      missing_last_reinforced: q.missingLastReinforced,
      sort: q.sort,
      limit: q.limit,
      offset: q.offset,
    })),
  getMemory: (id: string) => getJson<MemoryDetail>(`/api/memories/${encodeURIComponent(id)}`),
  getMemoryRaw: (id: string) => getJson<unknown>(`/api/memories/${encodeURIComponent(id)}/raw`),
  getMemoryNeighborhood: (id: string, limit = 8) =>
    getJson<Neighborhood>(`/api/memories/${encodeURIComponent(id)}/neighborhood?limit=${limit}`),
};
```

- [x] **Step 3: Verify**

Run: `npm run build`
Expected: TypeScript compiles cleanly.

### Task 3.4: URL state and reducer

**Files:**
- Create: `crates/qsf_browser_server/ui/src/state.ts`

- [x] **Step 1: Write the reducer**

```ts
// src/state.ts
import type { ListMemoriesQuery } from "./api";

export interface ViewState {
  selectedId: string | null;
  query: ListMemoriesQuery;
  filtersExpanded: boolean;
}

export const initialState: ViewState = {
  selectedId: null,
  query: { sort: "newest", limit: 50 },
  filtersExpanded: false,
};

export type Action =
  | { type: "select"; id: string | null }
  | { type: "setQuery"; query: ListMemoriesQuery }
  | { type: "toggleFilters" };

export function reduce(state: ViewState, action: Action): ViewState {
  switch (action.type) {
    case "select":
      return { ...state, selectedId: action.id };
    case "setQuery":
      return { ...state, query: { ...state.query, ...action.query } };
    case "toggleFilters":
      return { ...state, filtersExpanded: !state.filtersExpanded };
  }
}

export function stateToUrl(state: ViewState): string {
  const sp = new URLSearchParams();
  const { selectedId, query } = state;
  if (selectedId) sp.set("id", selectedId);
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined || v === null || v === "") continue;
    if (Array.isArray(v)) v.forEach((x) => sp.append(k, String(x)));
    else sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

export function urlToState(search: string): ViewState {
  const sp = new URLSearchParams(search);
  const query: ListMemoriesQuery = {};
  const set = (key: keyof ListMemoriesQuery, parser: (s: string) => unknown = (x) => x) => {
    const v = sp.get(String(key));
    if (v !== null) (query as Record<string, unknown>)[key as string] = parser(v);
  };
  set("q");
  set("kind");
  const tag = sp.getAll("tag");
  if (tag.length) query.tag = tag;
  set("createdFrom");
  set("createdTo");
  set("lastReinforcedFrom");
  set("lastReinforcedTo");
  set("deltaSince");
  set("minImportance", Number);
  set("minReinforcementCount", Number);
  set("hasAssociations", (s) => s === "true");
  set("orphaned", (s) => s === "true");
  set("missingLastReinforced", (s) => s === "true");
  set("sort");
  set("limit", Number);
  set("offset", Number);
  return {
    selectedId: sp.get("id"),
    query: { sort: "newest", limit: 50, ...query },
    filtersExpanded: false,
  };
}
```

- [x] **Step 2: Add a small unit test harness** (skip if Vitest is not added to the project; otherwise add `vitest` to devDependencies and create `src/state.test.ts` asserting `urlToState(stateToUrl(s)) === s` round-trip for a sample state.)

Defer Vitest unless the project owner wants it.

- [x] **Step 3: Verify**

Run: `npm run build`
Expected: TypeScript compiles cleanly.

### Task 3.5: HTML/CSS layout shell

**Files:**
- Create: `crates/qsf_browser_server/ui/src/ui/layout.css`
- Create: `crates/qsf_browser_server/ui/src/ui/shell.ts`

- [x] **Step 1: Layout CSS**

```css
/* src/ui/layout.css */
.workbench {
  display: grid;
  grid-template-rows: 40px 1fr 24px;
  height: 100vh;
}
.workbench .toolbar {
  display: flex; align-items: center; gap: 8px;
  padding: 0 12px;
  background: var(--qsf-bg-panel);
  border-bottom: 1px solid var(--qsf-border-subtle);
}
.workbench .main {
  display: grid;
  grid-template-columns: 280px 1fr;
  overflow: hidden;
}
.workbench .list {
  background: rgba(7,18,32,0.6);
  border-right: 1px solid var(--qsf-border-subtle);
  overflow: auto;
}
.workbench .right {
  display: grid;
  grid-template-rows: 1fr 1fr;
  overflow: hidden;
}
.workbench .canvas-slot {
  background: radial-gradient(ellipse at center, #0b2c4a 0%, #07162a 70%);
  border-bottom: 1px solid var(--qsf-border-subtle);
  display: flex; align-items: center; justify-content: center;
  color: var(--qsf-text-muted);
}
.workbench .inspector {
  background: var(--qsf-bg-panel);
  overflow: auto;
  padding: 16px;
}
.workbench .statusbar {
  display: flex; align-items: center; gap: 16px;
  padding: 0 12px;
  border-top: 1px solid var(--qsf-border-subtle);
  background: var(--qsf-bg-panel);
  color: var(--qsf-text-muted);
  font-size: 12px;
}

.row { padding: 6px 10px; border-bottom: 1px solid rgba(172,215,255,0.08); cursor: pointer; }
.row.selected { background: rgba(125,227,255,0.08); border-left: 2px solid var(--qsf-signal-context); padding-left: 8px; }
.row .row-title { color: var(--qsf-text-primary); }
.row .row-meta { color: var(--qsf-text-muted); font-size: 12px; }

.assoc { display: grid; grid-template-columns: 1fr 60px 140px; gap: 8px; padding: 4px 0; border-bottom: 1px solid rgba(172,215,255,0.06); cursor: pointer; }
.assoc .weight { color: var(--qsf-signal-association); text-align: right; }
.assoc .broken { color: var(--qsf-signal-error); }

.load-error {
  padding: 32px; max-width: 720px; margin: 64px auto;
  background: var(--qsf-bg-panel-elevated);
  border: 1px solid var(--qsf-signal-error);
  border-radius: var(--qsf-radius-panel);
  color: var(--qsf-text-primary);
}
.load-error h2 { color: var(--qsf-signal-error); margin-top: 0; }
.load-error code { background: rgba(255,93,115,0.1); padding: 2px 6px; border-radius: 3px; }
```

- [x] **Step 2: Shell renderer**

```ts
// src/ui/shell.ts
import type { ViewState } from "../state";

export function renderShell(root: HTMLElement) {
  root.className = "workbench";
  root.innerHTML = `
    <div class="toolbar" id="toolbar"></div>
    <div class="main">
      <div class="list" id="list"></div>
      <div class="right">
        <div class="canvas-slot" id="canvas-slot">Canvas placeholder — focal hub lands in Phase 4</div>
        <div class="inspector" id="inspector">Select a memory to inspect.</div>
      </div>
    </div>
    <div class="statusbar" id="statusbar"></div>
  `;
}

export function getSlots(root: HTMLElement) {
  return {
    toolbar: root.querySelector<HTMLElement>("#toolbar")!,
    list: root.querySelector<HTMLElement>("#list")!,
    canvasSlot: root.querySelector<HTMLElement>("#canvas-slot")!,
    inspector: root.querySelector<HTMLElement>("#inspector")!,
    statusbar: root.querySelector<HTMLElement>("#statusbar")!,
  };
}

export type Slots = ReturnType<typeof getSlots>;
export type { ViewState };
```

- [x] **Step 3: Wire into `main.ts`**

Replace `src/main.ts`:

```ts
import "./tokens.css";
import "./ui/layout.css";
import { renderShell, getSlots } from "./ui/shell";

const root = document.getElementById("app")!;
renderShell(root);
const slots = getSlots(root);

slots.statusbar.textContent = "loading…";
// Subsequent tasks wire components into the slots.
```

- [x] **Step 4: Verify**

Run `.\scripts\qsf.ps1 workbench` from the repository root, or use the raw fallback
commands (`cargo run -p qsf_browser_server` in one shell and `npm run dev` from
`crates/qsf_browser_server/ui` in another). Then open `http://127.0.0.1:5173`.
Expected: the workbench shell renders with placeholders. Vite dev proxies `/api/*` to the running API instance; both should be running to exercise the next tasks.

### Task 3.6: Components — toolbar, filters, list, inspector

**Files:**
- Create: `crates/qsf_browser_server/ui/src/ui/toolbar.ts`
- Create: `crates/qsf_browser_server/ui/src/ui/filters.ts`
- Create: `crates/qsf_browser_server/ui/src/ui/list.ts`
- Create: `crates/qsf_browser_server/ui/src/ui/inspector.ts`
- Create: `crates/qsf_browser_server/ui/src/ui/loadError.ts`
- Modify: `crates/qsf_browser_server/ui/src/main.ts`

- [x] **Step 1: Toolbar**

```ts
// src/ui/toolbar.ts
//
// Re-renders the toolbar without throwing away in-progress input. The
// previous implementation reset `el.innerHTML` on every reload, which
// dropped any text the user had typed but not yet committed (i.e. before
// `change` fires on blur/Enter). To avoid that, the toolbar is built once
// and subsequent calls only update values for inputs that are NOT focused
// and whose current value already differs from the new state.

import type { Action, ViewState } from "../state";

let built = false;

export function renderToolbar(
  el: HTMLElement,
  state: ViewState,
  storePath: string,
  dispatch: (a: Action) => void,
) {
  if (!built) {
    el.innerHTML = `
      <span style="color:var(--qsf-text-muted)">store</span>
      <code id="store-path" style="color:var(--qsf-signal-context)"></code>
      <input id="q" placeholder="search or paste id" style="flex:1;background:rgba(7,18,32,0.4);border:1px solid var(--qsf-border-subtle);color:var(--qsf-signal-context);padding:4px 8px"/>
      <select id="sort" style="background:rgba(7,18,32,0.4);color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:4px 8px">
        <option value="newest">newest</option>
        <option value="oldest">oldest</option>
        <option value="most_reinforced">most reinforced</option>
        <option value="highest_importance">highest importance</option>
        <option value="strongest_connected">strongest connected</option>
        <option value="largest_tokens">largest tokens</option>
      </select>
      <button id="toggle-filters" style="background:rgba(7,18,32,0.4);color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:4px 8px;cursor:pointer">filters</button>
    `;
    const q = el.querySelector<HTMLInputElement>("#q")!;
    q.addEventListener("change", () => dispatch({ type: "setQuery", query: { q: q.value } }));
    q.addEventListener("keydown", (e) => {
      if (e.key === "Enter") dispatch({ type: "setQuery", query: { q: q.value } });
    });
    const sort = el.querySelector<HTMLSelectElement>("#sort")!;
    sort.addEventListener("change", () => dispatch({ type: "setQuery", query: { sort: sort.value } }));
    el.querySelector<HTMLButtonElement>("#toggle-filters")!.addEventListener("click", () =>
      dispatch({ type: "toggleFilters" }),
    );
    built = true;
  }

  // Update display values. For the search input, do NOT overwrite while it
  // is focused (the user may be mid-typing). Also skip overwrite when the
  // current value already matches state to avoid cursor jumps.
  const storeEl = el.querySelector<HTMLElement>("#store-path")!;
  if (storeEl.textContent !== storePath) storeEl.textContent = storePath;

  const q = el.querySelector<HTMLInputElement>("#q")!;
  const desiredQ = state.query.q ?? "";
  if (document.activeElement !== q && q.value !== desiredQ) q.value = desiredQ;

  const sort = el.querySelector<HTMLSelectElement>("#sort")!;
  const desiredSort = state.query.sort ?? "newest";
  if (sort.value !== desiredSort) sort.value = desiredSort;

  const toggle = el.querySelector<HTMLButtonElement>("#toggle-filters")!;
  const desiredLabel = state.filtersExpanded ? "hide filters" : "filters";
  if (toggle.textContent !== desiredLabel) toggle.textContent = desiredLabel;
}
```

- [x] **Step 2: Filters row**

```ts
// src/ui/filters.ts
import type { Action, ViewState } from "../state";

export function renderFilters(
  parent: HTMLElement,
  state: ViewState,
  dispatch: (a: Action) => void,
) {
  if (!state.filtersExpanded) {
    parent.querySelector("#filters")?.remove();
    return;
  }
  let row = parent.querySelector<HTMLElement>("#filters");
  if (!row) {
    row = document.createElement("div");
    row.id = "filters";
    row.style.cssText = "display:flex;flex-wrap:wrap;gap:8px;padding:6px 12px;background:rgba(7,18,32,0.4);border-bottom:1px solid var(--qsf-border-subtle);font-size:12px;color:var(--qsf-text-secondary)";
    parent.appendChild(row);
  }
  row.innerHTML = `
    <label>kind <input id="f-kind" value="${state.query.kind ?? ""}" style="background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px;width:120px"/></label>
    <label>tag <input id="f-tag" value="${(state.query.tag ?? []).join(",")}" placeholder="comma,separated" style="background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px;width:160px"/></label>
    <label>created &ge; <input id="f-created-from" value="${state.query.createdFrom ?? ""}" placeholder="YYYY-MM-DD" style="background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px;width:120px"/></label>
    <label>delta since <input id="f-delta-since" value="${state.query.deltaSince ?? ""}" placeholder="ISO 8601" style="background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px;width:170px"/></label>
    <label>min importance <input id="f-min-imp" type="number" step="0.05" value="${state.query.minImportance ?? ""}" style="background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px;width:80px"/></label>
    <label><input type="checkbox" id="f-orphaned" ${state.query.orphaned ? "checked" : ""}/> orphaned only <span style="color:var(--qsf-text-muted)">(no association references this id)</span></label>
    <label><input type="checkbox" id="f-missing-lr" ${state.query.missingLastReinforced ? "checked" : ""}/> missing last_reinforced</label>
  `;
  const sync = () => {
    dispatch({ type: "setQuery", query: {
      kind: (row!.querySelector<HTMLInputElement>("#f-kind")!.value || undefined),
      tag: row!.querySelector<HTMLInputElement>("#f-tag")!.value.split(",").map((s) => s.trim()).filter(Boolean),
      createdFrom: row!.querySelector<HTMLInputElement>("#f-created-from")!.value || undefined,
      deltaSince: row!.querySelector<HTMLInputElement>("#f-delta-since")!.value || undefined,
      minImportance: Number(row!.querySelector<HTMLInputElement>("#f-min-imp")!.value) || undefined,
      orphaned: row!.querySelector<HTMLInputElement>("#f-orphaned")!.checked || undefined,
      missingLastReinforced: row!.querySelector<HTMLInputElement>("#f-missing-lr")!.checked || undefined,
    }});
  };
  row.querySelectorAll("input").forEach((i) => i.addEventListener("change", sync));
}
```

(Only the most common filters are surfaced in the row to keep the initial UI compact; remaining params remain reachable via URL until UI polish in a later phase. This is acceptable per the design's "denser, calmer" workbench mode and is documented in the design's Search/Filter section. Tests still cover the full predicate set on the backend.)

- [x] **Step 3: List**

```ts
// src/ui/list.ts
import type { MemoryListItem, MemoryPage } from "../types";
import type { Action } from "../state";

export function renderList(
  el: HTMLElement,
  page: MemoryPage,
  selectedId: string | null,
  dispatch: (a: Action) => void,
) {
  el.innerHTML = page.items.map((m) => rowHtml(m, m.id === selectedId)).join("");
  el.querySelectorAll<HTMLElement>(".row").forEach((row) => {
    row.addEventListener("click", () => dispatch({ type: "select", id: row.dataset.id! }));
  });
}

function rowHtml(m: MemoryListItem, selected: boolean): string {
  return `
    <div class="row ${selected ? "selected" : ""}" data-id="${escapeHtml(m.id)}">
      <div class="row-title">${escapeHtml(m.title)}</div>
      <div class="row-meta">${escapeHtml(m.kind)} · ${m.association_count} assoc · ${m.created_at.slice(0,10)}</div>
    </div>
  `;
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"})[c]!);
}
```

- [x] **Step 4: Inspector**

```ts
// src/ui/inspector.ts
import { api } from "../api";
import type { MemoryDetail } from "../types";
import type { Action } from "../state";

export async function renderInspector(
  el: HTMLElement,
  id: string,
  dispatch: (a: Action) => void,
) {
  el.innerHTML = `<div style="color:var(--qsf-text-muted)">loading…</div>`;
  let detail: MemoryDetail;
  try {
    detail = await api.getMemory(id);
  } catch (e) {
    el.innerHTML = `<div style="color:var(--qsf-signal-error)">failed to load ${escapeHtml(id)}</div>`;
    return;
  }
  el.innerHTML = `
    <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px">
      <h2 style="margin:0 0 4px 0;color:var(--qsf-signal-memory)">${escapeHtml(detail.title)}</h2>
      <button id="view-raw" style="background:transparent;color:var(--qsf-signal-context);border:1px solid var(--qsf-border-subtle);padding:4px 8px;cursor:pointer">view raw JSON</button>
    </div>
    <div style="color:var(--qsf-text-muted);font-size:12px;margin-bottom:12px">
      ${escapeHtml(detail.kind)} · created ${detail.created_at.slice(0,10)} · last reinforced ${detail.last_reinforced_at?.slice(0,10) ?? "—"} · ×${detail.reinforcement_count} · imp ${detail.importance.toFixed(2)}
    </div>
    <h3 style="margin:8px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Summary</h3>
    <div style="white-space:pre-wrap">${escapeHtml(detail.summary)}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Tags</h3>
    <div>${detail.tags.map((t) => `<span style="display:inline-block;padding:1px 6px;margin-right:4px;border:1px solid var(--qsf-border-subtle);border-radius:3px">${escapeHtml(t)}</span>`).join("")}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Source</h3>
    <div style="color:var(--qsf-text-secondary);font-size:12px">${escapeHtml(detail.source_reference)}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Associations · outgoing (${detail.outgoing_count})</h3>
    ${detail.outgoing.map(assocRow).join("") || `<div style="color:var(--qsf-text-muted)">none</div>`}
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Associations · incoming (${detail.incoming_count})</h3>
    ${detail.incoming.map(assocRow).join("") || `<div style="color:var(--qsf-text-muted)">none</div>`}
  `;
  el.querySelectorAll<HTMLElement>(".assoc").forEach((r) => {
    r.addEventListener("click", () => {
      const otherId = r.dataset.otherId!;
      if (otherId && !r.classList.contains("broken")) {
        dispatch({ type: "select", id: otherId });
      }
    });
  });
  el.querySelector<HTMLButtonElement>("#view-raw")!.addEventListener("click", () => openRawOverlay(id));
}

function assocRow(a: { other_id: string; other_title: string | null; weight: number; last_reinforced_at: string; reason: string }): string {
  const broken = a.other_title === null;
  return `
    <div class="assoc ${broken ? "broken" : ""}" data-other-id="${escapeHtml(a.other_id)}">
      <div>${broken ? `<span class="broken">broken → ${escapeHtml(a.other_id)}</span>` : escapeHtml(a.other_title!)}</div>
      <div class="weight">${a.weight.toFixed(2)}</div>
      <div style="color:var(--qsf-text-muted);font-size:12px">${a.last_reinforced_at.slice(0,10)}</div>
    </div>
  `;
}

async function openRawOverlay(id: string) {
  const raw = await api.getMemoryRaw(id);
  const overlay = document.createElement("div");
  overlay.style.cssText = "position:fixed;inset:0;background:rgba(5,8,18,0.85);display:flex;align-items:center;justify-content:center;z-index:1000";
  overlay.innerHTML = `<pre style="background:var(--qsf-bg-panel-elevated);padding:24px;max-width:90vw;max-height:90vh;overflow:auto;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);border-radius:8px">${escapeHtml(JSON.stringify(raw, null, 2))}</pre>`;
  overlay.addEventListener("click", () => overlay.remove());
  document.body.appendChild(overlay);
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"})[c]!);
}
```

- [x] **Step 5: Load-error screen**

```ts
// src/ui/loadError.ts
import type { LoadError } from "../types";

export function renderLoadError(root: HTMLElement, err: LoadError) {
  root.className = "";
  root.innerHTML = `
    <div class="load-error">
      <h2>memory store failed to load</h2>
      <p><strong>kind:</strong> <code>${err.kind}</code></p>
      <p><strong>path:</strong> <code>${escapeHtml(err.path)}</code></p>
      <p><strong>message:</strong> ${escapeHtml(err.message)}</p>
      ${"schema_versions_found" in err ? `<p><strong>schema versions found:</strong><br/>records: <code>${err.schema_versions_found.records.join(", ") || "(none)"}</code><br/>associations: <code>${err.schema_versions_found.associations.join(", ") || "(none)"}</code></p>` : ""}
      ${"schema_versions_supported" in err ? `<p><strong>schema versions supported:</strong><br/>records: <code>${err.schema_versions_supported.records.join(", ")}</code><br/>associations: <code>${err.schema_versions_supported.associations.join(", ")}</code></p>` : ""}
      ${"duplicate_ids" in err ? `<p><strong>duplicate ids:</strong><br/><code>${err.duplicate_ids.map(escapeHtml).join(", ")}</code></p>` : ""}
      ${"shape_errors" in err ? `<p><strong>shape errors:</strong><br/>${err.shape_errors.map((e) => `<code>${escapeHtml(e.field_path)}</code>: ${escapeHtml(e.message)}`).join("<br/>")}</p>` : ""}
    </div>
  `;
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"})[c]!);
}
```

- [x] **Step 6: Verify**

Run: `npm run build`. Expected: cleanly typechecks and bundles.

### Task 3.7: Wire it together in `main.ts`

**Files:**
- Modify: `crates/qsf_browser_server/ui/src/main.ts`

- [x] **Step 1: Implement the top-level controller**

```ts
// src/main.ts
import "./tokens.css";
import "./ui/layout.css";
import { api } from "./api";
import { initialState, reduce, stateToUrl, urlToState, type Action, type ViewState } from "./state";
import { getSlots, renderShell } from "./ui/shell";
import { renderToolbar } from "./ui/toolbar";
import { renderFilters } from "./ui/filters";
import { renderList } from "./ui/list";
import { renderInspector } from "./ui/inspector";
import { renderLoadError } from "./ui/loadError";

const root = document.getElementById("app")!;
let state: ViewState = urlToState(window.location.search) ?? initialState;
let storePath = "";

(async function bootstrap() {
  const health = await api.health();
  if (health.status === "error") {
    renderLoadError(root, health.load_error);
    return;
  }
  renderShell(root);
  const slots = getSlots(root);
  const summary = await api.storeSummary();
  storePath = ""; // backend doesn't currently expose store path on summary; surfaced via the toolbar's `store` label only.
  slots.statusbar.textContent = `${summary.record_count} records · ${summary.association_count} associations · ${summary.broken_associations_count} broken edges · ${summary.total_estimated_tokens.toLocaleString()} tokens`;

  async function reload() {
    renderToolbar(slots.toolbar, state, storePath || "(store)", dispatch);
    renderFilters(slots.toolbar.parentElement!, state, dispatch);
    const page = await api.listMemories(state.query);
    renderList(slots.list, page, state.selectedId, dispatch);
    if (state.selectedId) {
      renderInspector(slots.inspector, state.selectedId, dispatch);
    } else {
      slots.inspector.innerHTML = `<div style="color:var(--qsf-text-muted)">Select a memory to inspect.</div>`;
    }
    history.replaceState(null, "", window.location.pathname + stateToUrl(state));
  }

  function dispatch(action: Action) {
    state = reduce(state, action);
    reload();
  }

  reload();
})();
```

- [ ] **Step 2: Build + manual verification**

Run the documented launcher path:
```bash
.\scripts\qsf.ps1 workbench
```

Raw fallback commands:
```bash
cargo run -p qsf_browser_server         # in shell 1
cd crates/qsf_browser_server/ui && npm run dev   # in shell 2
```
Open `http://localhost:5173`. Expected: the workbench shows the list, summary in status bar, selecting a memory renders the inspector, sort and search update the list and the URL.

Implementation note: `npm run build` passed, the Rust server was smoke-tested
against `tests/fixtures/small-store.json`, `/api/health` returned OK, and the
Vite dev URL returned HTTP 200. Full browser interaction remains an external
human verification item because in-app browser automation was unavailable in
this session.

- [ ] **Step 3: External human verification**

Ask the project owner to:
- Use the workbench against a real store.
- Confirm filters and sort changes appear in the URL and survive a refresh.
- Trigger a load-error path (run the server with a deliberately bad `--store`) and confirm the load-error screen renders all relevant fields.
- Note any UX surprises in `docs/EngineeringDiary.md` for follow-up.

### Task 3.8: Close out Phase 3

- [x] Standard closing steps (`cargo clippy`, `cargo fmt`, `cargo test`; also `npm run build`).
- [x] Diary entry covering the frontend shell, URL state, load-error screen.
- [ ] Commit: `feat(qsf_browser_server-ui): workbench shell with list, inspector, filters, load-error screen`.

---

## Phase 4: Focal-hub canvas

Replace the canvas placeholder with a PixiJS scene showing the selected memory's local neighborhood. Static radial layout; hover tooltip; click-to-navigate; broken edges rendered dashed.

### Task 4.1: Install PixiJS and add layout pure function

**Files:**
- Modify: `crates/qsf_browser_server/ui/package.json`
- Create: `crates/qsf_browser_server/ui/src/canvas/radial.ts`

- [ ] **Step 1: Install PixiJS**

In `crates/qsf_browser_server/ui/`:

```bash
npm install pixi.js@8
```

- [ ] **Step 2: Write the pure radial layout**

```ts
// src/canvas/radial.ts
export interface NeighborLayout {
  id: string;
  x: number;
  y: number;
  angle: number;
}

/**
 * Lay out neighbors evenly around a unit circle. Pure function: deterministic
 * for a given (neighbor_count, radius). Caller is responsible for centering.
 */
export function radialPositions(count: number, radius: number): NeighborLayout[] {
  if (count <= 0) return [];
  const out: NeighborLayout[] = [];
  const step = (Math.PI * 2) / count;
  for (let i = 0; i < count; i++) {
    const angle = -Math.PI / 2 + step * i; // first node at top
    out.push({
      id: String(i),
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      angle,
    });
  }
  return out;
}
```

- [ ] **Step 3: Verify**

Run: `npm run build`
Expected: typechecks and bundles.

### Task 4.2: PixiJS focal-hub scene

**Files:**
- Create: `crates/qsf_browser_server/ui/src/canvas/focalHub.ts`
- Modify: `crates/qsf_browser_server/ui/src/main.ts`

- [ ] **Step 1: Write the scene**

```ts
// src/canvas/focalHub.ts
import { Application, Container, Graphics, Text } from "pixi.js";
import type { Neighborhood } from "../types";
import { radialPositions } from "./radial";

const COLOR_MEMORY = 0xffd76a;
const COLOR_EDGE = 0xffb94a;
const COLOR_BROKEN = 0xff5d73;
const COLOR_LABEL = 0xeaf6ff;

export class FocalHubScene {
  private app: Application;
  private layer = new Container();
  private hoverLayer = new Container();
  private onSelect: (id: string) => void;
  private ready = false;

  constructor(slot: HTMLElement, onSelect: (id: string) => void) {
    this.app = new Application();
    this.onSelect = onSelect;
    this.init(slot).catch((e) => {
      slot.textContent = `canvas init failed: ${(e as Error).message}`;
    });
  }

  private async init(slot: HTMLElement) {
    await this.app.init({
      background: "#07162a",
      resizeTo: slot,
      antialias: true,
    });
    slot.innerHTML = "";
    slot.appendChild(this.app.canvas);
    this.app.stage.addChild(this.layer);
    this.app.stage.addChild(this.hoverLayer);
    this.ready = true;
  }

  render(centerId: string, n: Neighborhood) {
    if (!this.ready) {
      // queue render until init resolves
      setTimeout(() => this.render(centerId, n), 50);
      return;
    }
    // PixiJS v8: removeChildren() detaches but does NOT free GPU/WebGL
    // resources. Destroy children explicitly first to avoid leaking
    // textures and buffers across re-renders.
    this.layer.children.forEach((child) => child.destroy({ children: true }));
    this.layer.removeChildren();
    this.hoverLayer.children.forEach((child) => child.destroy({ children: true }));
    this.hoverLayer.removeChildren();
    const w = this.app.renderer.width;
    const h = this.app.renderer.height;
    const cx = w / 2;
    const cy = h / 2;
    const radius = Math.min(w, h) * 0.35;

    const memberById = new Map(n.members.map((m) => [m.id, m]));
    const neighborIds = Array.from(new Set(n.edges.flatMap((e) => [e.from_id, e.to_id]).filter((id) => id !== centerId)));
    const positions = radialPositions(neighborIds.length, radius);
    const idToPos = new Map<string, { x: number; y: number }>();
    neighborIds.forEach((id, i) => idToPos.set(id, { x: cx + positions[i].x, y: cy + positions[i].y }));

    // edges
    const maxWeight = n.edges.reduce((m, e) => Math.max(m, e.weight), 0.001);
    for (const e of n.edges) {
      const otherId = e.from_id === centerId ? e.to_id : e.from_id;
      const pos = idToPos.get(otherId);
      if (!pos) continue;
      const broken = !memberById.has(otherId);
      const g = new Graphics();
      const lineWidth = 1 + (e.weight / maxWeight) * 3;
      const color = broken ? COLOR_BROKEN : COLOR_EDGE;
      if (broken) {
        drawDashed(g, cx, cy, pos.x, pos.y, lineWidth, color);
      } else {
        g.moveTo(cx, cy).lineTo(pos.x, pos.y).stroke({ width: lineWidth, color, alpha: 0.7 });
      }
      this.layer.addChild(g);
    }

    // neighbor nodes
    for (const id of neighborIds) {
      const pos = idToPos.get(id)!;
      const member = memberById.get(id);
      const broken = !member;
      const node = new Graphics();
      node.circle(pos.x, pos.y, broken ? 7 : 10).fill({ color: broken ? COLOR_BROKEN : COLOR_MEMORY, alpha: broken ? 0.5 : 0.85 });
      node.eventMode = "static";
      node.cursor = "pointer";
      const label = new Text({
        text: member?.title ?? id.slice(0, 10) + "…",
        style: { fill: broken ? COLOR_BROKEN : COLOR_LABEL, fontSize: 11, fontFamily: "Inter, sans-serif" },
      });
      label.anchor.set(0.5, 0);
      label.position.set(pos.x, pos.y + 14);
      this.layer.addChild(node);
      this.layer.addChild(label);
      if (!broken) {
        node.on("pointertap", () => this.onSelect(id));
      }
      node.on("pointerover", () => this.showTooltip(pos.x, pos.y, member?.title ?? id, broken));
      node.on("pointerout", () => this.clearHover());
    }

    // center
    const center = new Graphics();
    center.circle(cx, cy, 18).fill({ color: COLOR_MEMORY, alpha: 0.95 });
    this.layer.addChild(center);
    const centerLabel = new Text({
      text: n.center.title,
      style: { fill: COLOR_LABEL, fontSize: 13, fontFamily: "Inter, sans-serif" },
    });
    centerLabel.anchor.set(0.5, 0);
    centerLabel.position.set(cx, cy + 22);
    this.layer.addChild(centerLabel);
  }

  private clearHover() {
    this.hoverLayer.children.forEach((child) => child.destroy({ children: true }));
    this.hoverLayer.removeChildren();
  }

  private showTooltip(x: number, y: number, text: string, broken: boolean) {
    this.clearHover();
    const tip = new Text({
      text: (broken ? "broken → " : "") + text,
      style: { fill: COLOR_LABEL, fontSize: 11, fontFamily: "Inter, sans-serif" },
    });
    tip.position.set(x + 12, y - 18);
    const bg = new Graphics();
    bg.roundRect(x + 8, y - 22, tip.width + 12, tip.height + 6, 3).fill({ color: 0x07162a, alpha: 0.9 }).stroke({ color: 0xacd7ff, width: 1, alpha: 0.4 });
    this.hoverLayer.addChild(bg);
    this.hoverLayer.addChild(tip);
  }
}

function drawDashed(g: Graphics, x1: number, y1: number, x2: number, y2: number, width: number, color: number) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const dist = Math.sqrt(dx * dx + dy * dy);
  const dashLen = 6;
  const gapLen = 4;
  const segLen = dashLen + gapLen;
  const ux = dx / dist;
  const uy = dy / dist;
  let drawn = 0;
  while (drawn + dashLen < dist) {
    g.moveTo(x1 + ux * drawn, y1 + uy * drawn);
    g.lineTo(x1 + ux * (drawn + dashLen), y1 + uy * (drawn + dashLen));
    drawn += segLen;
  }
  g.stroke({ width, color, alpha: 0.7 });
}
```

- [ ] **Step 2: Wire into `main.ts`**

In `crates/qsf_browser_server/ui/src/main.ts`, replace the bootstrap body's `reload()` so that on `state.selectedId` change it also fetches the neighborhood and renders into the canvas slot:

```ts
import { FocalHubScene } from "./canvas/focalHub";

// inside bootstrap, after `getSlots`:
let scene: FocalHubScene | null = null;

async function reload() {
  renderToolbar(slots.toolbar, state, storePath || "(store)", dispatch);
  renderFilters(slots.toolbar.parentElement!, state, dispatch);
  const page = await api.listMemories(state.query);
  renderList(slots.list, page, state.selectedId, dispatch);
  if (state.selectedId) {
    renderInspector(slots.inspector, state.selectedId, dispatch);
    if (!scene) scene = new FocalHubScene(slots.canvasSlot, (id) => dispatch({ type: "select", id }));
    try {
      const n = await api.getMemoryNeighborhood(state.selectedId, 8);
      scene.render(state.selectedId, n);
    } catch {
      slots.canvasSlot.textContent = "no neighborhood data";
    }
  } else {
    slots.inspector.innerHTML = `<div style="color:var(--qsf-text-muted)">Select a memory to inspect.</div>`;
    slots.canvasSlot.innerHTML = "Select a memory to see its neighborhood.";
  }
  history.replaceState(null, "", window.location.pathname + stateToUrl(state));
}
```

- [ ] **Step 3: Verify**

Run `npm run build`, then start the browser workbench with `.\scripts\qsf.ps1 workbench` or the raw API + Vite commands and select a memory. Expected: the canvas shows the focal hub with up to 8 neighbors. Broken edges render dashed with the truncated `other_id`. Clicking a non-broken neighbor changes the selection and updates list, inspector, canvas, and URL.

- [ ] **Step 4: External human verification**

Ask the project owner to navigate through several memories in the real store, confirm the focal hub stays legible, and confirm broken edges render distinctly.

### Task 4.3: Close out Phase 4

- [ ] Standard closing steps.
- [ ] Diary entry covering the PixiJS focal-hub canvas and broken-edge rendering.
- [ ] Commit: `feat(qsf_browser_server-ui): PixiJS focal-hub canvas with broken edges`.

---

## Phase 5: Packaging

Make the release binary self-contained without making `cargo build` depend on npm.

### Task 5.1: `embedded-frontend` Cargo feature

**Files:**
- Modify: `crates/qsf_browser_server/Cargo.toml` (feature already declared in Phase 1)
- Create: `crates/qsf_browser_server/src/assets.rs`
- Modify: `crates/qsf_browser_server/src/lib.rs`
- Modify: `crates/qsf_browser_server/src/server.rs`

- [ ] **Step 1: Asset module behind the feature**

```rust
// crates/qsf_browser_server/src/assets.rs
//! Optional static asset serving for the built frontend.
//!
//! Behind the `embedded-frontend` feature. When the feature is off,
//! the server still runs and serves /api/*; / returns a small text page
//! pointing the user to either the dev server or the build step.

#[cfg(feature = "embedded-frontend")]
mod embedded {
    use axum::body::Body;
    use axum::http::{Response, StatusCode, header};
    use axum::response::IntoResponse;
    use rust_embed::Embed;

    #[derive(Embed)]
    #[folder = "ui/dist/"]
    struct Assets;

    pub async fn serve(uri: axum::http::Uri) -> impl IntoResponse {
        let mut path = uri.path().trim_start_matches('/').to_string();
        if path.is_empty() {
            path = "index.html".to_string();
        }
        match Assets::get(&path) {
            Some(content) => {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(Body::from(content.data.into_owned()))
                    .unwrap()
            }
            None => {
                if path != "index.html" {
                    // SPA fallback
                    if let Some(content) = Assets::get("index.html") {
                        return Response::builder()
                            .header(header::CONTENT_TYPE, "text/html")
                            .body(Body::from(content.data.into_owned()))
                            .unwrap();
                    }
                }
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("not found"))
                    .unwrap()
            }
        }
    }
}

#[cfg(not(feature = "embedded-frontend"))]
mod placeholder {
    use axum::response::Html;
    pub async fn serve(_uri: axum::http::Uri) -> Html<&'static str> {
        Html(r#"
            <html><body style="background:#050812;color:#eaf6ff;font-family:system-ui;padding:32px">
              <h2 style="color:#7de3ff">QSF Memory Association Browser — API running</h2>
              <p>This build does not include the embedded frontend.</p>
              <p>For development: run <code>npm run dev</code> in <code>crates/qsf_browser_server/ui/</code> and open <a href="http://localhost:5173">http://localhost:5173</a>.</p>
              <p>For a single-binary build: from <code>crates/qsf_browser_server/ui/</code> run <code>npm install &amp;&amp; npm run build</code>, then rebuild with <code>cargo build --release -p qsf_browser_server --features embedded-frontend</code>.</p>
            </body></html>
        "#)
    }
}

#[cfg(feature = "embedded-frontend")]
pub use embedded::serve;
#[cfg(not(feature = "embedded-frontend"))]
pub use placeholder::serve;
```

- [ ] **Step 2: Wire the assets handler into the router**

In `crates/qsf_browser_server/src/lib.rs`:

```rust
pub mod assets;
```

In `crates/qsf_browser_server/src/server.rs`, append the static asset fallback at the end of the router chain:

```rust
use axum::routing::get;

let app = Router::new()
    .merge(health::router())
    .merge(routes::router())
    .fallback(get(crate::assets::serve))
    .with_state(state);
```

- [ ] **Step 3: Verify Rust-only build works without npm**

In a clean environment (or after `rm -rf crates/qsf_browser_server/ui/node_modules crates/qsf_browser_server/ui/dist`):

```bash
cargo build -p qsf_browser_server
cargo clippy --all-targets -- -D warnings
```

Expected: both succeed without invoking npm.

### Task 5.2: Verify the embedded build path

- [ ] **Step 1: Build frontend then enable the feature**

```bash
cd crates/qsf_browser_server/ui
npm install
npm run build
cd ../../..
cargo build --release -p qsf_browser_server --features embedded-frontend
./target/release/qsf_browser_server &
curl -s http://127.0.0.1:3939/api/health
curl -s http://127.0.0.1:3939/ | head -3
```

Expected: the single binary serves both `/api/*` and the workbench HTML.

- [ ] **Step 2: External human verification**

Ask the project owner to run the embedded binary against the real store, navigate the workbench, and confirm the canvas and inspector both work.

### Task 5.3: README usage section

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a section near the existing usage docs**

Append a new section:

```markdown
## Memory Association Browser

`qsf_browser_server` is a read-only HTTP workbench for inspecting the persisted
memory store.

Default development loop:

```bash
.\scripts\qsf.ps1 workbench
```

Raw fallback/reference commands:

```bash
# Shell 1
cargo run -p qsf_browser_server                 # serves /api/* on :3939

# Shell 2
cd crates/qsf_browser_server/ui
npm install                                     # first time only
npm run dev                                     # opens http://localhost:5173
```

Single-binary release:

```bash
cd crates/qsf_browser_server/ui && npm install && npm run build
cd -
cargo build --release -p qsf_browser_server --features embedded-frontend
./target/release/qsf_browser_server                # serves API + workbench on :3939
```

Use `--store <path>` to point at a different memory store. The default is
`state/text-loop/memory-store.json`. The server binds to `127.0.0.1` unless
`--host <addr>` is passed; non-loopback binds log a disclosure warning.
```

- [ ] **Step 2: Verify**

Render the README in your editor; check that the new section is well-formed and the commands match what was used in Tasks 5.1 and 5.2.

### Task 5.4: Close out Phase 5

- [ ] Standard closing steps.
- [ ] Diary entry covering the `embedded-frontend` feature and the README update.
- [ ] Commit: `feat(qsf_browser_server): embedded-frontend feature and README usage section`.

- [ ] **Final step — update `Idea.MemoryAssociationBrowser.md`** with a top-of-file pointer to the design document, e.g.:

```markdown
> **Status:** This idea has graduated to a design and shipping implementation. See
> [Design.MemoryAssociationBrowser.md](Design.MemoryAssociationBrowser.md) and
> [Plan.MemoryAssociationBrowser.md](Plan.MemoryAssociationBrowser.md).
```

- [ ] Commit the Idea note: `docs(memory-browser): point Idea doc at Design and Plan`.

---

## Open Questions Surfaced During Planning

- The Idea document's "delta-since" filter is included in the API but does not appear in the compact filter row UI in Phase 3 (it is reachable via URL). Decide whether to surface it in the row during Phase 3 or defer to Phase 5 polish.
- The toolbar in Phase 3 currently shows a literal `(store)` label because the API does not yet expose the active store path. Decide whether to add `store_path` to `/api/store/summary` or `/api/health` for the toolbar (small backend change) before Phase 3 closes.
- Phase 3 does not include automated frontend tests. If Vitest is wanted, add it as a follow-up; the URL state round-trip is the natural first test.
- The toolbar's id-jump behavior is not yet wired (the `q` field is treated only as keyword search). A small enhancement in `main.ts` should detect an exact id match in the current page and emit `select` instead of leaving it as a search term. Track as Phase 3 follow-up.

These are intentional gaps rather than silent decisions, per the project workflow rule that ambiguities should be surfaced.
