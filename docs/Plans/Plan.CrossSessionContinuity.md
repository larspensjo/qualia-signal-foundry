# Cross-Session Continuity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist `SessionState` across runs of the multi-turn text loop, let sleep auto-promote routine memory candidates and form cross-turn associations, and let the live loop form co-retrieval associations during a turn — so a returning session feels different from a cold start.

**Architecture:** A new gitignored `state/text-loop/` directory holds the continuity manifest, persisted SessionState, cross-session memory store, and consolidated brief. Sleep is a pure function of `(SessionState, memory-store)` and uses the manifest as its commit record. Live-loop writes are pure delta events handled in isolated effect handlers, preserving reducer purity.

**Tech Stack:** Rust 2024 edition, Cargo workspace, `serde`/`serde_json` for persistence, `time` crate (already used) for timestamps, `tempfile::NamedTempFile::persist` for Windows-safe atomic writes, `cargo test` for tests, `cargo clippy --all-targets -- -D warnings` and `cargo fmt` as the per-task verification rhythm (per `Agents.md`).

**Design reference:** [docs/Plans/Design.CrossSessionContinuity.md](Design.CrossSessionContinuity.md). When the plan and design disagree, the design wins — open an issue rather than diverging.

---

## Stage 1 — `openai` feature removal

**Goal:** Eliminate the optional `openai` Cargo feature so the real-provider path is unconditional. The runtime still defaults to mock via `QSF_MODEL_PROVIDER`; only the build-time gate disappears.

**Stage exit criteria:**
- `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings` all pass with no feature flags
- `cargo run -p qsf_app -- list-experiments` lists every experiment
- README's setup section no longer mentions `--features openai`
- A Decision-Log entry records the removal

### Task 1.1: Remove the feature from the workspace manifest

**Files:**
- Modify: `crates/qsf_app/Cargo.toml`

- [ ] **Step 1:** Open `crates/qsf_app/Cargo.toml`. Find the `[features]` table that defines `openai = [...]`. Identify every dependency listed in that feature's array (these become unconditional).

- [ ] **Step 2:** Move every dependency the `openai` feature pulls in (e.g. `tokio`, `reqwest`, `tokio-tungstenite`, `openai_provider_kit`, `cpal`, etc. — the actual set lives in the manifest) from any `optional = true` entry to a plain unconditional dependency. Remove `default-features = false` qualifiers that existed solely to gate the feature.

- [ ] **Step 3:** Delete the entire `[features]` table entry for `openai` (and the `default = []` line if it only existed to support `openai`).

- [ ] **Step 4:** Run `cargo build` to confirm the manifest still resolves.

```bash
cargo build
```

Expected: build succeeds. If it fails because a dep was incorrectly marked optional, fix the declaration.

- [ ] **Step 5:** Do not commit yet — gate-removal in source is the next task, and they belong together.

### Task 1.2: Strip every `#[cfg(feature = "openai")]` gate

**Files:**
- Modify: `crates/qsf_app/src/audio/voice_session_provider.rs`
- Modify: `crates/qsf_app/src/audio/transcript_provider.rs`
- Modify: `crates/qsf_app/src/models/openai_provider.rs`
- Modify: `crates/qsf_app/src/models/mod.rs`

- [ ] **Step 1:** Run a grep to enumerate every cfg site.

```bash
rg 'feature\s*=\s*"openai"' crates/qsf_app/src
```

Expected: matches across the four files above, ~50 sites total.

- [ ] **Step 2:** In each file, delete every `#[cfg(feature = "openai")]` attribute line (the line above the item it gates becomes unconditional). For `#[cfg(not(feature = "openai"))]` blocks, delete the entire gated item — those are the fallback stubs and no longer needed. For `#[cfg(any(feature = "openai", test))]`, replace with `#[cfg(any(target_pointer_width = "64", test))]` only if the item should remain test-only; otherwise drop the gate entirely so the item is unconditional. Walk file-by-file; do not skip.

- [ ] **Step 3:** Run `cargo build` and fix any unresolved import / dead-code warnings revealed by the unconditional code. If a use statement was only valid behind the feature, it's now unconditional too.

```bash
cargo build
```

Expected: build succeeds.

- [ ] **Step 4:** Run `cargo test`.

```bash
cargo test
```

Expected: all tests pass. Tests that previously only ran with `--features openai` now run unconditionally.

- [ ] **Step 5:** Run lints and formatter per `Agents.md`.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected: clippy clean, no formatting diffs.

- [ ] **Step 6:** Verify the experiment list.

```bash
cargo run -p qsf_app -- list-experiments
```

Expected: every experiment listed, including the previously feature-gated voice experiments.

- [ ] **Step 7:** Commit Tasks 1.1 + 1.2 together.

```bash
git add crates/qsf_app/Cargo.toml crates/qsf_app/src
git commit -m "refactor(qsf_app): remove openai feature gate; real-provider path is unconditional"
```

### Task 1.3: Update README and add Decision-Log entry

**Files:**
- Modify: `README.md`
- Modify: `docs/DecisionLog.md`
- Modify: `docs/EngineeringDiary.md`

- [ ] **Step 1:** In `README.md`, remove the "Optional `openai` feature" subsection. Update any `cargo run` examples that use `--features openai` so the flag is gone.

- [ ] **Step 2:** Append a Decision-Log entry. Replace `YYYY-MM-DD` with today's date.

```markdown
## YYYY-MM-DD - openai Cargo feature removed
Decision: The `openai` Cargo feature is removed from `qsf_app`. Real-provider
code (OpenAI Chat Completions, realtime transcription, realtime voice session)
compiles unconditionally. Provider selection at runtime remains explicit via
`QSF_MODEL_PROVIDER` / `QSF_TRANSCRIPT_PROVIDER` / `QSF_VOICE_SESSION_PROVIDER`
per the 2026-05-11 decision.
Context: The feature gate was an early hedge from when real-provider code was
experimental. It now adds CI complexity, hides drift behind a flag, and conflicts
with the cross-session continuity work that touches code in feature-gated paths.
Removing the gate also unblocks the planned voice/text loop unification.
Consequences: `cargo build` / `cargo test` exercise the full path. API keys
still do not switch the runtime away from mocks — provider selection is the
single decision point.
Refs: crates/qsf_app/Cargo.toml, crates/qsf_app/src/models, crates/qsf_app/src/audio,
docs/DecisionLog.md#2026-05-11---model-access-uses-explicit-roles-and-optional-provider-adapters
```

- [ ] **Step 3:** Add a diary entry for the change in `docs/EngineeringDiary.md` following the existing entry template (one entry per logical change). Reference the new Decision-Log entry and this plan.

- [ ] **Step 4:** Commit.

```bash
git add README.md docs/DecisionLog.md docs/EngineeringDiary.md
git commit -m "docs: record openai feature removal in DecisionLog and diary"
```

---

## Stage 2 — Memory store, decay field, and retrieval integration

**Goal:** Add `last_reinforced_at` to `MemoryRecord`, introduce a `MemoryStore` module that reads/writes the JSON store atomically, and replace the rank-based recency in `score_record` with time-based decay against `last_reinforced_at`. No persistence behavior changes for live runs yet — the cross-session store is only created by Stage 4's sleep work.

**Stage exit criteria:**
- `MemoryRecord` carries `last_reinforced_at: Option<OffsetDateTime>` and existing v1 fixtures still deserialize
- `memory::store::MemoryStore` exposes atomic load / append-records / append-associations operations
- `retrieval::score_record` uses time-based decay for the `recency` component, falling back to `created_at` when `last_reinforced_at` is `None`
- Existing multi-turn text loop runs unchanged on default config (`ColdStart` path is still in effect)

### Task 2.1: Add `last_reinforced_at` to `MemoryRecord` as an additive field

**Files:**
- Modify: `crates/qsf_app/src/memory/memory_record.rs`
- Test: `crates/qsf_app/src/memory/memory_record.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing regression test.** Append to the existing inline tests module in `memory_record.rs`.

```rust
#[test]
fn deserializes_v1_record_without_last_reinforced_at_with_none_fallback() {
    let v1_json = r#"{
        "schema_version": 1,
        "id": "memory.legacy",
        "kind": "observation",
        "title": "Legacy",
        "summary": "An old record predating last_reinforced_at.",
        "tags": [],
        "created_at": "2026-05-09T12:00:00Z",
        "importance": 0.5,
        "reinforcement_count": 0,
        "source_reference": "tests",
        "estimated_tokens": 10
    }"#;

    let record: MemoryRecord = serde_json::from_str(v1_json).unwrap();
    assert_eq!(record.last_reinforced_at, None);
    assert!(record.ensure_current_schema().is_ok());
}
```

- [ ] **Step 2: Run the test to verify it fails.**

```bash
cargo test -p qsf_app memory::memory_record::tests::deserializes_v1_record_without_last_reinforced_at_with_none_fallback
```

Expected: FAIL (`last_reinforced_at` field does not exist).

- [ ] **Step 3: Add the field with serde defaults.** In the `MemoryRecord` struct definition, add the field after `reinforcement_count`:

```rust
#[serde(default, with = "time::serde::rfc3339::option")]
pub last_reinforced_at: Option<OffsetDateTime>,
```

Also update the `MemoryRecord::new` signature so it does not require the new field. Add a separate `with_last_reinforced_at` setter on `MemoryRecord`:

```rust
impl MemoryRecord {
    pub fn with_last_reinforced_at(mut self, at: OffsetDateTime) -> Self {
        self.last_reinforced_at = Some(at);
        self
    }
}
```

In `MemoryRecord::new`, initialize the field to `None`:

```rust
last_reinforced_at: None,
```

- [ ] **Step 4: Run the regression test plus existing tests.**

```bash
cargo test -p qsf_app memory::memory_record
```

Expected: all pass, including the new test.

- [ ] **Step 5:** Run clippy and fmt.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 6:** Commit.

```bash
git add crates/qsf_app/src/memory/memory_record.rs
git commit -m "feat(memory): add MemoryRecord.last_reinforced_at as additive optional field"
```

### Task 2.2: Replace rank-based recency with time-based decay in `retrieval::score_record`

**Files:**
- Modify: `crates/qsf_app/src/memory/retrieval.rs`
- Test: `crates/qsf_app/src/memory/retrieval.rs` (inline tests)

- [ ] **Step 1: Add a new test asserting the recency component uses time-based decay.** Append:

```rust
#[test]
fn recency_uses_time_based_decay_from_last_reinforced_at() {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    let now = OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap();
    let recent = MemoryRecord::new(
        "memory.recent", MemoryRecordKind::Observation, "Recent", "Recent",
        vec![], now - time::Duration::days(1), 0.5, 0, "tests", 10,
    )
    .with_last_reinforced_at(now - time::Duration::days(1));
    let stale = MemoryRecord::new(
        "memory.stale", MemoryRecordKind::Observation, "Stale", "Stale",
        vec![], now - time::Duration::days(120), 0.5, 0, "tests", 10,
    )
    .with_last_reinforced_at(now - time::Duration::days(120));

    let recent_score = super::compute_recency_decay(&recent, now);
    let stale_score = super::compute_recency_decay(&stale, now);

    assert!(recent_score > 0.9, "recent score was {recent_score}");
    assert!(stale_score < 0.1, "stale score was {stale_score}");
}

#[test]
fn recency_falls_back_to_created_at_when_last_reinforced_at_is_none() {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    let now = OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339).unwrap();
    let record = MemoryRecord::new(
        "memory.legacy", MemoryRecordKind::Observation, "Legacy", "Legacy",
        vec![], now - time::Duration::days(10), 0.5, 0, "tests", 10,
    );
    assert_eq!(record.last_reinforced_at, None);

    let score = super::compute_recency_decay(&record, now);
    assert!(score > 0.5 && score < 1.0, "fallback score was {score}");
}
```

Add imports at the top of the test module as needed (`use super::*;` plus `use crate::memory::{MemoryRecord, MemoryRecordKind};` if not already present).

- [ ] **Step 2: Run the tests to verify they fail.**

```bash
cargo test -p qsf_app memory::retrieval::tests::recency_uses_time_based_decay
cargo test -p qsf_app memory::retrieval::tests::recency_falls_back_to_created_at
```

Expected: FAIL (no `compute_recency_decay` function).

- [ ] **Step 3: Add the decay constant and function.** In `retrieval.rs`, add:

```rust
pub(crate) const DECAY_HALFLIFE_DAYS: f64 = 30.0;

pub(crate) fn compute_recency_decay(record: &MemoryRecord, now: OffsetDateTime) -> f64 {
    let reference = record.last_reinforced_at.unwrap_or(record.created_at);
    let age_seconds = (now - reference).whole_seconds().max(0) as f64;
    let age_days = age_seconds / 86_400.0;
    (-age_days / DECAY_HALFLIFE_DAYS).exp()
}
```

Add the `use time::OffsetDateTime;` import at the top of the file if not present.

- [ ] **Step 4: Replace the rank-based recency path.** Locate `recency_scores` (the function returning a `HashMap<String, f64>` keyed by rank) and remove its callers. Change `retrieve_memories` to compute recency per-record via `compute_recency_decay(record, now)`, where `now = OffsetDateTime::now_utc()`. The `score` function signature changes to accept `now: OffsetDateTime` rather than a pre-computed `recency: f64`. Plumb `now` from `retrieve_memories` into `score_record`.

Concretely, change the `score_record` signature:

```rust
fn score_record(
    record: &MemoryRecord,
    strategy: RetrievalStrategy,
    now: OffsetDateTime,
    matched_terms: &[String],
    association_paths: &[AssociationPath],
) -> RetrievalScore {
    let recency = compute_recency_decay(record, now);
    // ... rest unchanged
}
```

Update `retrieve_memories`:

```rust
let now = OffsetDateTime::now_utc();
// remove `let recency_by_id = recency_scores(records);`
// in the map, pass `now` instead of looking up by id
```

Delete `recency_scores` entirely. Adjust any other call sites.

- [ ] **Step 5: Run all retrieval tests.**

```bash
cargo test -p qsf_app memory::retrieval
```

Expected: all pass. The pre-existing `recency_only_prefers_newest_records` test still passes because newer `created_at` still yields higher decay scores when `last_reinforced_at` is `None`.

- [ ] **Step 6:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/memory/retrieval.rs
git commit -m "feat(memory): replace rank-based recency with time-based decay against last_reinforced_at"
```

### Task 2.3: Introduce `memory::store::MemoryStore` with atomic load/save

**Files:**
- Create: `crates/qsf_app/src/memory/store.rs`
- Modify: `crates/qsf_app/src/memory/mod.rs`
- Modify: `crates/qsf_app/Cargo.toml` (add `tempfile` as a direct dependency if not already)
- Test: `crates/qsf_app/src/memory/store.rs` (inline tests)

- [ ] **Step 1: Promote `tempfile` to a direct dependency.** Inspect `crates/qsf_app/Cargo.toml`. If `tempfile` is absent or only present as `dev-dependencies`, add it under `[dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 2: Write the failing tests.** Create `crates/qsf_app/src/memory/store.rs` containing the test module first:

```rust
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

    pub fn contents(&self) -> &MemoryStoreContents { &self.contents }
    pub fn contents_mut(&mut self) -> &mut MemoryStoreContents { &mut self.contents }
    pub fn path(&self) -> &Path { &self.path }

    pub fn persist(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create memory store parent dir `{}`", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(&self.contents)?;
        let parent = self.path.parent().unwrap_or(Path::new("."));
        let temp = NamedTempFile::new_in(parent)?;
        std::fs::write(temp.path(), json.as_bytes())?;
        temp.persist(&self.path).map_err(|e| anyhow::anyhow!(
            "failed to persist memory store `{}`: {}", self.path.display(), e.error
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryRecordKind, association::Association};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn load_or_empty_returns_empty_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");

        let store = MemoryStore::load_or_empty(&path).unwrap();
        assert!(store.contents().records.is_empty());
        assert!(store.contents().associations.is_empty());
    }

    #[test]
    fn persist_then_reload_roundtrips_record_and_association() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();

        store.contents_mut().records.push(MemoryRecord::new(
            "memory.test", MemoryRecordKind::Observation, "Title",
            "Summary text.", vec!["topic"], ts(), 0.5, 0, "tests", 10,
        ));
        store.contents_mut().associations.push(Association::new(
            "memory.a", "memory.b", 0.4, "related", ts(),
        ));
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents(), store.contents());
    }

    #[test]
    fn persist_overwrites_existing_file_atomically_on_windows_and_posix() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();
        store.persist().unwrap();

        // Second persist over an existing file must succeed (the platform-atomic replace path).
        store.contents_mut().records.push(MemoryRecord::new(
            "memory.second", MemoryRecordKind::Observation, "Second",
            "Second summary.", vec![], ts(), 0.5, 0, "tests", 10,
        ));
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents().records.len(), 1);
    }
}
```

In `crates/qsf_app/src/memory/mod.rs`, add `pub mod store;`.

- [ ] **Step 3: Run the tests to verify they pass (this task's tests and impl are paired).**

```bash
cargo test -p qsf_app memory::store
```

Expected: PASS — all three tests green.

- [ ] **Step 4:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/Cargo.toml crates/qsf_app/src/memory
git commit -m "feat(memory): add MemoryStore with atomic temp-file-then-rename persistence"
```

---

## Stage 3 — SessionState persistence, continuity manifest, boot resolver, AwakeContinuation

**Goal:** Persist `SessionState` at session end. Add the continuity manifest. Boot resolver classifies the resume mode purely. `AwakeContinuation` carries forward the previous session via `prepare_awake_continuation`. `ConsolidatedBrief` is *not* yet wired (Stage 4) — the resolver classifies it but the brief-injection path remains a TODO until Stage 4 lands. `ColdStart` runs the loop today.

**Stage exit criteria:**
- A multi-turn run, quit, re-run cycle resumes the previous session's turns
- `state/text-loop/` survives a fresh checkout (deleted directory triggers `ColdStart`)
- All four `prepare_awake_continuation` scenarios (quit, EOF, model error, session-limit reached) are unit-tested
- `cargo test`, clippy, fmt all clean

### Task 3.1: Add `session_id` and `previous_session_id` to `SessionState`

**Files:**
- Modify: `crates/qsf_app/src/session/mod.rs`
- Test: `crates/qsf_app/src/session/mod.rs` (inline tests if absent — add `#[cfg(test)] mod tests` block)

- [ ] **Step 1: Write the failing test.** Append (or create) at the bottom of `session/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_state_carries_session_id_and_no_previous() {
        let config = SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        };
        let state = SessionState::new_with_id("session-abc".to_string(), config);

        assert_eq!(state.session_id, "session-abc");
        assert_eq!(state.previous_session_id, None);
    }

    #[test]
    fn session_state_serde_roundtrips_with_new_fields() {
        let config = SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        };
        let state = SessionState::new_with_id("session-roundtrip".to_string(), config);

        let json = serde_json::to_string(&state).unwrap();
        let parsed: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "session-roundtrip");
    }
}
```

- [ ] **Step 2:** Run the tests to verify they fail.

```bash
cargo test -p qsf_app session::tests
```

Expected: FAIL — `session_id` field does not exist.

- [ ] **Step 3:** Add the new fields to `SessionState`:

```rust
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionState {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub previous_session_id: Option<String>,
    pub started_at: SystemTime,
    pub config: SessionConfig,
    pub turns: Vec<Turn>,
    pub summarized_turns: Vec<TurnSummary>,
    pub ended_reason: Option<SessionEndReason>,
    pub last_input: Option<String>,
    pub last_prompt_hash: Option<ContentHash>,
    pub prefix_invalidated_since_last_prompt: bool,
    pub last_model_error: Option<String>,
    pub limit_reached: Option<SessionLimit>,
}
```

Add a constructor that takes a session id:

```rust
impl SessionState {
    pub fn new_with_id(session_id: String, config: SessionConfig) -> Self {
        Self {
            session_id,
            previous_session_id: None,
            started_at: SystemTime::now(),
            config,
            turns: vec![],
            summarized_turns: vec![],
            ended_reason: None,
            last_input: None,
            last_prompt_hash: None,
            prefix_invalidated_since_last_prompt: false,
            last_model_error: None,
            limit_reached: None,
        }
    }
}
```

Keep the existing `SessionState::new` but make it call `new_with_id` with a generated id:

```rust
pub fn new(config: SessionConfig) -> Self {
    Self::new_with_id(uuid::Uuid::new_v4().to_string(), config)
}
```

If `uuid` is not yet a dependency of qsf_app, add it (it is — already used in `observability/event_log.rs`).

- [ ] **Step 4:** Run tests, lints, format.

```bash
cargo test -p qsf_app session
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 5:** Commit.

```bash
git add crates/qsf_app/src/session/mod.rs
git commit -m "feat(session): add session_id and previous_session_id to SessionState"
```

### Task 3.2: Create the continuity manifest type

**Files:**
- Create: `crates/qsf_app/src/session/manifest.rs`
- Modify: `crates/qsf_app/src/session/mod.rs` (add `pub mod manifest;`)
- Test: `crates/qsf_app/src/session/manifest.rs` (inline tests)

- [ ] **Step 1: Write the failing tests first.** In a new file `crates/qsf_app/src/session/manifest.rs`:

```rust
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
        if !path.exists() { return Ok(Self::default()); }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest `{}`", path.display()))?;
        let parsed: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse manifest `{}`", path.display()))?;
        if parsed.schema_version != CONTINUITY_MANIFEST_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported manifest schema_version: found {} expected {}",
                parsed.schema_version, CONTINUITY_MANIFEST_SCHEMA_VERSION
            );
        }
        Ok(parsed)
    }

    pub fn persist(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let temp = NamedTempFile::new_in(parent)?;
        std::fs::write(temp.path(), serde_json::to_string_pretty(self)?.as_bytes())?;
        temp.persist(path).map_err(|e| anyhow::anyhow!(
            "failed to persist manifest `{}`: {}", path.display(), e.error
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_or_default_returns_cold_start_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("continuity-manifest.json");

        let manifest = ContinuityManifest::load_or_default(&path).unwrap();
        assert_eq!(manifest.resume_mode, ResumeMode::ColdStart);
        assert_eq!(manifest.sleep_pending, false);
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
    fn unsupported_schema_version_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("continuity-manifest.json");
        std::fs::write(&path, r#"{"schema_version":999,"current_session_id":null,"current_session_state_path":null,"last_sleep_run_id":null,"last_sleep_brief_path":null,"last_sleep_consumed_session_id":null,"sleep_pending":false,"resume_mode":"cold_start"}"#).unwrap();

        let err = ContinuityManifest::load_or_default(&path).unwrap_err();
        assert!(err.to_string().contains("unsupported manifest schema_version"));
    }
}
```

In `crates/qsf_app/src/session/mod.rs`, add at the top:

```rust
pub mod manifest;
```

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app session::manifest
```

Expected: PASS — all three tests green (tests and impl are paired in this task).

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/session
git commit -m "feat(session): add ContinuityManifest type with atomic persistence"
```

### Task 3.3: Add `prepare_awake_continuation` pure function

**Files:**
- Create: `crates/qsf_app/src/session/continuation.rs`
- Modify: `crates/qsf_app/src/session/mod.rs` (add `pub mod continuation;`)
- Test: `crates/qsf_app/src/session/continuation.rs` (inline tests)

- [ ] **Step 1: Write the failing tests in `continuation.rs`.**

```rust
use crate::session::{SessionConfig, SessionEndReason, SessionLimit, SessionState};

/// Prepare a persisted `SessionState` for awake continuation in a new run.
/// Pure function — does not consult I/O or the clock.
pub fn prepare_awake_continuation(
    previous: SessionState,
    new_config: &SessionConfig,
) -> SessionState {
    // If the config changed in any field that affects observable behavior, the
    // caller should treat this as ColdStart-equivalent instead. Carrying forward
    // turns under a mismatched config would silently change behavior. We keep this
    // check explicit and conservative: any config mismatch -> caller decides.
    let mut state = previous;
    state.config = new_config.clone();
    state.ended_reason = None;
    state.last_input = None;
    state.last_model_error = None;
    state.last_prompt_hash = None;
    state.prefix_invalidated_since_last_prompt = true;
    state.previous_session_id = None;
    state.limit_reached = recompute_limit(&state, new_config);
    state
}

fn recompute_limit(state: &SessionState, new_config: &SessionConfig) -> Option<SessionLimit> {
    let current = state.turns.len();
    if current >= new_config.max_turns {
        Some(SessionLimit {
            current,
            max: new_config.max_turns,
            override_active: new_config.allow_over_limit,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MemorySourceConfig;

    fn config(max_turns: usize) -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        }
    }

    fn previous_with(end_reason: Option<SessionEndReason>, turns: usize) -> SessionState {
        let mut state = SessionState::new_with_id("prev-1".to_string(), config(10));
        for _ in 0..turns { state.turns.push(crate::session::tests::fake_turn(state.turns.len())); }
        state.ended_reason = end_reason;
        state.last_input = Some("hello".to_string());
        state.last_model_error = Some("network blip".to_string());
        state
    }

    #[test]
    fn after_quit_command_clears_end_reason_and_last_input_and_error() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 2);
        let cont = prepare_awake_continuation(prev, &config(10));

        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_input, None);
        assert_eq!(cont.last_model_error, None);
        assert_eq!(cont.session_id, "prev-1");
        assert_eq!(cont.turns.len(), 2);
    }

    #[test]
    fn after_eof_clears_state_same_way() {
        let prev = previous_with(Some(SessionEndReason::Eof), 1);
        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_model_error, None);
    }

    #[test]
    fn after_model_error_clears_error_field() {
        let prev = previous_with(Some(SessionEndReason::Error), 3);
        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(cont.ended_reason, None);
        assert_eq!(cont.last_model_error, None);
    }

    #[test]
    fn limit_reached_recomputes_against_new_config_under_limit() {
        let mut prev = previous_with(Some(SessionEndReason::QuitCommand), 5);
        prev.limit_reached = Some(SessionLimit { current: 5, max: 5, override_active: false });

        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(cont.limit_reached, None);
    }

    #[test]
    fn limit_reached_recomputes_when_still_over_new_limit() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 12);
        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(
            cont.limit_reached,
            Some(SessionLimit { current: 12, max: 10, override_active: false })
        );
    }

    #[test]
    fn prefix_cache_is_invalidated_in_new_run() {
        let prev = previous_with(Some(SessionEndReason::QuitCommand), 1);
        let cont = prepare_awake_continuation(prev, &config(10));
        assert_eq!(cont.last_prompt_hash, None);
        assert!(cont.prefix_invalidated_since_last_prompt);
    }
}
```

You will also need a small `fake_turn` helper for tests. Add to `session/mod.rs` inside the test module:

```rust
#[cfg(test)]
pub(crate) fn fake_turn(index: usize) -> Turn {
    use crate::context::ContextAssembly;
    use crate::conversation::ContentHash;
    Turn {
        index,
        started_at: SystemTime::UNIX_EPOCH,
        completed_at: SystemTime::UNIX_EPOCH,
        user_input: format!("turn-{index}-input"),
        context_assembly: ContextAssembly::default(),
        retrieved_memory_block: String::new(),
        assistant_response: format!("turn-{index}-response"),
        recalled_turns: vec![],
        model_id: "mock".to_string(),
        model_latency_ms: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        full_request_hash: ContentHash::default(),
        message_count: 0,
    }
}
```

Add `pub mod continuation;` to `session/mod.rs`.

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app session::continuation
```

Expected: PASS. If `ContextAssembly::default()` or `ContentHash::default()` does not exist, derive `Default` on those types, or hand-construct the fields explicitly in `fake_turn`.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/session
git commit -m "feat(session): add prepare_awake_continuation pure function with field-reset semantics"
```

### Task 3.4: Add `session::resume` boot resolver (I/O wrapper + pure classifier)

**Files:**
- Create: `crates/qsf_app/src/session/resume.rs`
- Modify: `crates/qsf_app/src/session/mod.rs` (add `pub mod resume;`)
- Test: `crates/qsf_app/src/session/resume.rs` (inline tests)

- [ ] **Step 1: Write the failing tests in `resume.rs`.**

```rust
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::session::SessionState;
use crate::session::manifest::{ContinuityManifest, ResumeMode};

#[derive(Clone, Debug, PartialEq)]
pub struct ResumeInputs {
    pub manifest: ContinuityManifest,
    pub previous_session: Option<SessionState>,
}

pub fn load_resume_inputs(state_dir: impl AsRef<Path>) -> anyhow::Result<ResumeInputs> {
    let state_dir = state_dir.as_ref();
    let manifest_path = state_dir.join("continuity-manifest.json");
    let manifest = ContinuityManifest::load_or_default(&manifest_path)?;

    let previous_session = match manifest.current_session_state_path.as_ref() {
        Some(rel) => {
            let abs = if rel.is_absolute() { rel.clone() } else { state_dir.join(rel) };
            if abs.exists() {
                let raw = std::fs::read_to_string(&abs).with_context(|| {
                    format!("failed to read session state `{}`", abs.display())
                })?;
                let parsed: SessionState = serde_json::from_str(&raw).with_context(|| {
                    format!("failed to parse session state `{}`", abs.display())
                })?;
                Some(parsed)
            } else { None }
        }
        None => None,
    };

    Ok(ResumeInputs { manifest, previous_session })
}

/// Pure classifier — no I/O.
pub fn classify_resume_mode(inputs: &ResumeInputs) -> ResumeMode {
    match (&inputs.previous_session, inputs.manifest.sleep_pending,
           &inputs.manifest.last_sleep_run_id) {
        (None, _, _) => ResumeMode::ColdStart,
        (Some(_), true, _) => ResumeMode::AwakeContinuation,
        (Some(_), false, Some(_)) => ResumeMode::ConsolidatedBrief,
        (Some(_), false, None) => ResumeMode::ColdStart,
    }
}

pub fn state_dir_from_env() -> PathBuf {
    std::env::var("QSF_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("state/text-loop"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{MemorySourceConfig, SessionConfig};

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        }
    }

    fn inputs(prev: Option<SessionState>, pending: bool, last_sleep: Option<&str>) -> ResumeInputs {
        ResumeInputs {
            manifest: ContinuityManifest {
                sleep_pending: pending,
                last_sleep_run_id: last_sleep.map(str::to_string),
                ..ContinuityManifest::default()
            },
            previous_session: prev,
        }
    }

    #[test]
    fn no_previous_session_is_cold_start() {
        let r = classify_resume_mode(&inputs(None, false, None));
        assert_eq!(r, ResumeMode::ColdStart);
    }

    #[test]
    fn previous_session_with_sleep_pending_is_awake_continuation() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), true, None));
        assert_eq!(r, ResumeMode::AwakeContinuation);
    }

    #[test]
    fn previous_session_with_consumed_sleep_is_consolidated_brief() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), false, Some("sleep-1")));
        assert_eq!(r, ResumeMode::ConsolidatedBrief);
    }

    #[test]
    fn previous_session_with_no_sleep_history_is_cold_start_fallback() {
        let state = SessionState::new_with_id("s-1".to_string(), config());
        let r = classify_resume_mode(&inputs(Some(state), false, None));
        // No sleep ever ran and the prior run somehow flipped sleep_pending to false:
        // we treat that as malformed history and fall back to a fresh start.
        assert_eq!(r, ResumeMode::ColdStart);
    }
}
```

Add `pub mod resume;` to `session/mod.rs`.

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app session::resume
```

Expected: PASS.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/session
git commit -m "feat(session): add resume boot resolver split into I/O wrapper and pure classifier"
```

### Task 3.5: Add `session::persistence` module to write SessionState at session end

**Files:**
- Create: `crates/qsf_app/src/session/persistence.rs`
- Modify: `crates/qsf_app/src/session/mod.rs` (add `pub mod persistence;`)
- Test: `crates/qsf_app/src/session/persistence.rs` (inline tests)

- [ ] **Step 1: Write the failing test and impl together.**

```rust
use std::path::{Path, PathBuf};

use anyhow::Context;
use tempfile::NamedTempFile;

use crate::session::SessionState;

pub fn persist_session_state(
    state: &SessionState,
    state_dir: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    let state_dir = state_dir.as_ref();
    std::fs::create_dir_all(state_dir)?;
    let path = state_dir.join("session-state.json");

    let json = serde_json::to_string_pretty(state)?;
    let temp = NamedTempFile::new_in(state_dir)?;
    std::fs::write(temp.path(), json.as_bytes())?;
    temp.persist(&path).map_err(|e| anyhow::anyhow!(
        "failed to persist session state `{}`: {}", path.display(), e.error
    ))?;
    Ok(path)
}

pub fn load_session_state(path: impl AsRef<Path>) -> anyhow::Result<SessionState> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session state `{}`", path.display()))?;
    let parsed: SessionState = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse session state `{}`", path.display()))?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{MemorySourceConfig, SessionConfig};
    use tempfile::TempDir;

    fn sample_state() -> SessionState {
        SessionState::new_with_id("s-roundtrip".to_string(), SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        })
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
```

Add `pub mod persistence;` to `session/mod.rs`.

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app session::persistence
```

Expected: PASS.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/session
git commit -m "feat(session): add persistence module with atomic SessionState write"
```

### Task 3.6: Wire the boot resolver into the multi-turn text loop

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- Modify: `crates/qsf_app/src/observability/event_log.rs` (add `SessionResumed` variant)

- [ ] **Step 1:** Add the new event variant. In `observability/event_log.rs`, locate the `EventType` enum and add:

```rust
SessionResumed,
```

(immediately after the existing `SessionStarted` line). Run `cargo build` to confirm no exhaustive-match call sites break.

- [ ] **Step 2:** In `multi_turn_text_loop.rs`, at the top of `run`, replace the current `SessionConfig::from_env()` plus `SessionState::new(...)` pair with:

```rust
let config = SessionConfig::from_env();
let state_dir = crate::session::resume::state_dir_from_env();
let resume_inputs = crate::session::resume::load_resume_inputs(&state_dir)?;
let resume_mode = crate::session::resume::classify_resume_mode(&resume_inputs);

let mut state = match resume_mode {
    crate::session::manifest::ResumeMode::ColdStart => SessionState::new(config.clone()),
    crate::session::manifest::ResumeMode::AwakeContinuation => {
        let previous = resume_inputs.previous_session.clone().unwrap();
        crate::session::continuation::prepare_awake_continuation(previous, &config)
    }
    crate::session::manifest::ResumeMode::ConsolidatedBrief => {
        // Stage 4 wires brief injection; for Stage 3, behave like ColdStart but
        // record previous_session_id for traceability.
        let mut fresh = SessionState::new(config.clone());
        fresh.previous_session_id = resume_inputs.previous_session
            .as_ref()
            .map(|s| s.session_id.clone());
        fresh
    }
};

// Emit SessionResumed before SessionStarted so observability captures the boot decision.
context.event_log_mut()?.append(
    crate::observability::event_log::EventRecord::new(
        context.experiment_id().to_string(),
        crate::observability::event_log::EventType::SessionResumed,
        serde_json::json!({
            "mode": resume_mode,
            "previous_session_id": state.previous_session_id,
            "brief_path": resume_inputs.manifest.last_sleep_brief_path,
        }),
        None,
    ),
)?;
```

Adjust the exact API to match `RunContext`'s event-log accessor signature in the codebase — the appender may be named differently, but the JSON payload shape stays.

- [ ] **Step 3:** At the end of the run loop, persist the SessionState and update the manifest with `sleep_pending = true`. Insert just before the loop returns its `ExperimentOutcome`:

```rust
let state_path = crate::session::persistence::persist_session_state(&state, &state_dir)?;
let mut manifest = resume_inputs.manifest.clone();
manifest.current_session_id = Some(state.session_id.clone());
manifest.current_session_state_path = Some(state_path.strip_prefix(&state_dir)
    .unwrap_or(&state_path)
    .to_path_buf());
manifest.sleep_pending = true;
manifest.resume_mode = crate::session::manifest::ResumeMode::AwakeContinuation;
manifest.persist(state_dir.join("continuity-manifest.json"))?;
```

- [ ] **Step 4:** Add `state/` to `.gitignore` if not already present.

```bash
cat .gitignore | rg '^state/?$' || echo 'state/' >> .gitignore
```

- [ ] **Step 5:** Build, test, lint, format.

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected: all green. If the experiment's existing tests assume no persistence, they may need to set `QSF_STATE_DIR` to a per-test temp dir — fix any test fallout by adding that env override in `Experiment.run` test harness setup.

- [ ] **Step 6: Human golden-path test.**

```powershell
$env:QSF_STATE_DIR = "$env:TEMP\qsf-continuity-test"
Remove-Item -Recurse -Force "$env:TEMP\qsf-continuity-test" -ErrorAction SilentlyContinue
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Type a couple of turns, then ":quit"
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Verify: turn index resumes at the previous count + 1; SessionResumed in events.jsonl logs mode=AwakeContinuation
```

Expected: the second run's first turn is numbered after the first run's last turn, and the run's `events.jsonl` contains a `SessionResumed` entry with `mode: "awake_continuation"`.

- [ ] **Step 7:** Commit.

```bash
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs crates/qsf_app/src/observability/event_log.rs .gitignore
git commit -m "feat(text-loop): persist SessionState and resume via continuity manifest"
```

---

## Stage 4 — Sleep auto-promotion, consolidated brief, ConsolidatedBrief resume

**Goal:** Sleep consumes the persisted SessionState and the current memory store, promotes routine memory candidates as `Observation` records, builds cross-turn associations, emits a `ReviewedMemoryDraft` for `Decision`-kind candidates, writes the consolidated brief, and commits the manifest last. Sleep is byte-idempotent on the same input.

**Stage exit criteria:**
- Running sleep twice in a row on the same SessionState produces byte-identical `memory-store.json`, `consolidated-brief.json`, manifest, and archive entries
- A partial-write crash between derived-file writes and the manifest commit recovers cleanly on the next sleep run
- Decision-kind candidates land in a `ReviewedMemoryDraft` consumable by the existing `accept-reviewed-memory` experiment
- A session → sleep → session sequence logs `mode=consolidated_brief` on the second run and injects the brief into turn 1's context

### Task 4.1: Add `sleep::auto_promote` pure module

**Files:**
- Create: `crates/qsf_app/src/sleep/auto_promote.rs`
- Modify: `crates/qsf_app/src/sleep/mod.rs` (add `pub mod auto_promote;`)
- Test: `crates/qsf_app/src/sleep/auto_promote.rs` (inline tests)

- [ ] **Step 1: Write the failing tests for the promotion filter.**

```rust
use std::time::SystemTime;

use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::sleep_report::{SleepMemoryCandidate, SleepReport};

pub const CROSS_TURN_ASSOCIATION_WINDOW: usize = 3;
pub const SLEEP_ASSOCIATION_INITIAL_WEIGHT: f64 = 0.35;
pub const SLEEP_ASSOCIATION_STRENGTHEN_DELTA: f64 = 0.05;

#[derive(Clone, Debug, PartialEq)]
pub struct PromotionPlan {
    pub new_records: Vec<MemoryRecord>,
    pub new_associations: Vec<Association>,
    pub strengthened_associations: Vec<(String, String, f64)>, // (from, to, new_weight)
    pub skipped_duplicates: Vec<String>, // titles of skipped candidates
}

pub fn build_promotion_plan(
    report: &SleepReport,
    session: &SessionState,
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
    sleep_run_id: &str,
) -> PromotionPlan {
    let mut new_records = Vec::new();
    let mut skipped_duplicates = Vec::new();

    for (index, candidate) in report.memory_candidates.iter().enumerate() {
        if candidate.summary.trim().is_empty() { continue; }
        let title = first_sentence(&candidate.summary).to_string();
        let summary = candidate.summary.trim().to_string();
        let normalized = normalize_for_dedup(&title, &summary);

        let duplicate = current_store.records.iter().any(|r| {
            normalize_for_dedup(&r.title, &r.summary) == normalized
        }) || new_records.iter().any(|r: &MemoryRecord| {
            normalize_for_dedup(&r.title, &r.summary) == normalized
        });
        if duplicate { skipped_duplicates.push(title); continue; }

        let id = format!("memory.sleep.{}.{:03}", sanitize(sleep_run_id), index + 1);
        let record = MemoryRecord::new(
            id, MemoryRecordKind::Observation, title, summary, vec![],
            as_of, candidate.importance.unwrap_or(0.3).clamp(0.0, 1.0),
            0,
            candidate.source_reference.clone().unwrap_or_else(|| {
                format!("sleep-run:{sleep_run_id}#memory_candidates[{:03}]", index + 1)
            }),
            estimated_tokens(&candidate.summary),
        )
        .with_last_reinforced_at(as_of);
        new_records.push(record);
    }

    let (new_associations, strengthened_associations) =
        build_cross_turn_associations(session, current_store, &new_records, as_of);

    PromotionPlan { new_records, new_associations, strengthened_associations, skipped_duplicates }
}

fn build_cross_turn_associations(
    session: &SessionState,
    current_store: &MemoryStoreContents,
    new_records: &[MemoryRecord],
    as_of: OffsetDateTime,
) -> (Vec<Association>, Vec<(String, String, f64)>) {
    let mut new_assocs = Vec::new();
    let mut strengthened = Vec::new();
    let window = CROSS_TURN_ASSOCIATION_WINDOW;

    // Gather retrieved-memory-id sets per turn from the context assembly.
    let retrievals: Vec<Vec<String>> = session.turns.iter()
        .map(|t| t.context_assembly.retrieved_memory_ids())
        .collect();

    for i in 0..retrievals.len() {
        for j in (i + 1)..(i + 1 + window).min(retrievals.len()) {
            for from_id in &retrievals[i] {
                for to_id in &retrievals[j] {
                    if from_id == to_id { continue; }
                    let (a, b) = if from_id < to_id {
                        (from_id.clone(), to_id.clone())
                    } else {
                        (to_id.clone(), from_id.clone())
                    };

                    let existing = current_store.associations.iter()
                        .position(|x| (x.from_memory_id == a && x.to_memory_id == b)
                                   || (x.from_memory_id == b && x.to_memory_id == a));
                    let already_proposed = new_assocs.iter()
                        .any(|x: &Association| (x.from_memory_id == a && x.to_memory_id == b)
                                            || (x.from_memory_id == b && x.to_memory_id == a));
                    if already_proposed { continue; }

                    if let Some(_existing_idx) = existing {
                        let new_weight = (current_store.associations[_existing_idx].weight
                            + SLEEP_ASSOCIATION_STRENGTHEN_DELTA).min(1.0);
                        strengthened.push((a, b, new_weight));
                    } else {
                        new_assocs.push(Association::new(
                            a, b, SLEEP_ASSOCIATION_INITIAL_WEIGHT,
                            format!("cross-turn co-occurrence in session {}", session.session_id),
                            as_of,
                        ));
                    }
                    let _ = new_records; // suppress unused warning until association on new records is wired
                }
            }
        }
    }

    (new_assocs, strengthened)
}

fn normalize_for_dedup(title: &str, summary: &str) -> String {
    let mut s = format!("{title}|{summary}").to_lowercase();
    s.retain(|c| !c.is_whitespace());
    s
}

fn sanitize(value: &str) -> String {
    value.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>()
        .trim_matches('-').to_string()
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    for (i, c) in trimmed.char_indices() {
        if matches!(c, '.' | '!' | '?' | '\n') {
            return trimmed[..=i.min(63)].trim().to_string();
        }
    }
    trimmed.chars().take(64).collect::<String>()
}

fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sleep::sleep_report::{SleepAssociationCandidate, SleepReport};
    use time::format_description::well_known::Rfc3339;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    fn empty_session() -> SessionState {
        use crate::session::{MemorySourceConfig, SessionConfig};
        SessionState::new_with_id("s-test".to_string(), SessionConfig {
            model_id: "mock".to_string(), max_turns: 10, warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig { source: "fixture".to_string(), file: None },
        })
    }

    fn report_with_candidates(candidates: Vec<&str>) -> SleepReport {
        SleepReport {
            session_summary: "summary".to_string(),
            memory_candidates: candidates.into_iter().map(|s| SleepMemoryCandidate {
                summary: s.to_string(), importance: Some(0.5), source_reference: None,
            }).collect(),
            association_candidates: vec![],
            open_questions: vec![],
            decision_candidates: vec![],
            future_context_hints: vec![],
            review_notes: vec![],
        }
    }

    #[test]
    fn promotes_each_candidate_as_observation() {
        let report = report_with_candidates(vec![
            "Reducers stay pure.",
            "Tools are perception extensions.",
        ]);
        let plan = build_promotion_plan(
            &report, &empty_session(), &MemoryStoreContents::default(), ts(), "sleep-1",
        );
        assert_eq!(plan.new_records.len(), 2);
        assert!(plan.new_records.iter().all(|r| r.kind == MemoryRecordKind::Observation));
        assert!(plan.new_records.iter().all(|r| r.last_reinforced_at == Some(ts())));
    }

    #[test]
    fn skips_duplicates_of_existing_store_records() {
        let report = report_with_candidates(vec!["Reducers stay pure."]);
        let store = MemoryStoreContents {
            records: vec![MemoryRecord::new(
                "memory.existing", MemoryRecordKind::Observation,
                "Reducers stay pure", "Reducers stay pure.",
                vec![], ts(), 0.5, 0, "src", 10,
            )],
            associations: vec![],
        };
        let plan = build_promotion_plan(&report, &empty_session(), &store, ts(), "sleep-1");
        assert_eq!(plan.new_records.len(), 0);
        assert_eq!(plan.skipped_duplicates.len(), 1);
    }

    #[test]
    fn promotion_is_byte_idempotent_on_same_inputs() {
        let report = report_with_candidates(vec!["Reducers stay pure."]);
        let plan_a = build_promotion_plan(&report, &empty_session(), &MemoryStoreContents::default(), ts(), "sleep-1");
        let plan_b = build_promotion_plan(&report, &empty_session(), &MemoryStoreContents::default(), ts(), "sleep-1");
        let a = serde_json::to_string(&plan_a.new_records).unwrap();
        let b = serde_json::to_string(&plan_b.new_records).unwrap();
        assert_eq!(a, b);
    }
}
```

Add `pub mod auto_promote;` to `crates/qsf_app/src/sleep/mod.rs`.

You may need to derive `Serialize` on `PromotionPlan` and its substructures for the idempotency test — change the derive line to include `Serialize`. Make sure `MemoryStoreContents` already derives both `Serialize` and `Deserialize` from Task 2.3.

If `ContextAssembly` does not have a `retrieved_memory_ids()` method, add a simple accessor:

In `crates/qsf_app/src/context/mod.rs` (or wherever `ContextAssembly` is defined):

```rust
impl ContextAssembly {
    pub fn retrieved_memory_ids(&self) -> Vec<String> {
        // Walk the selected memory fragments; return their ids.
        // Exact body depends on the existing ContextAssembly shape.
        self.selected_memories.iter().map(|m| m.memory.id.clone()).collect()
    }
}
```

Adapt to the real field names. If the existing struct exposes ids already, skip this and use that path directly in `auto_promote.rs`.

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app sleep::auto_promote
```

Expected: PASS.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/sleep crates/qsf_app/src/context
git commit -m "feat(sleep): add auto_promote pure module for memory candidate promotion and cross-turn associations"
```

### Task 4.2: Add `sleep::commit` multi-file commit protocol helper

**Files:**
- Create: `crates/qsf_app/src/sleep/commit.rs`
- Modify: `crates/qsf_app/src/sleep/mod.rs` (add `pub mod commit;`)
- Test: `crates/qsf_app/src/sleep/commit.rs` (inline tests)

- [ ] **Step 1: Write the failing test for the commit protocol.**

```rust
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::memory::store::{MemoryStore, MemoryStoreContents};
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

impl<'a> SleepCommit<'a> {
    pub fn write(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.state_dir)?;
        let archive_dir = self.state_dir.join("archive");
        std::fs::create_dir_all(&archive_dir)?;

        // 1. Derived file: memory-store.json
        let store_path = self.state_dir.join("memory-store.json");
        atomic_write_json(&store_path, &self.new_store_contents)?;

        // 2. Derived file: consolidated-brief.json
        let brief_path = self.state_dir.join("consolidated-brief.json");
        atomic_write_json(&brief_path, &self.brief)?;

        // 3. Archive copy of the brief
        let archive_path = archive_dir.join(&self.brief_archive_name);
        atomic_write_json(&archive_path, &self.brief)?;

        // 4. Manifest commit — last write
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
    std::fs::create_dir_all(parent)?;
    let temp = NamedTempFile::new_in(parent)?;
    std::fs::write(temp.path(), serde_json::to_string_pretty(value)?.as_bytes())?;
    temp.persist(path).map_err(|e| anyhow::anyhow!(
        "failed to persist `{}`: {}", path.display(), e.error
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_brief() -> ConsolidatedBrief {
        ConsolidatedBrief {
            previous_session_summary: "Sample.".to_string(),
            future_context_hints: vec!["Resume topic.".to_string()],
            open_questions: vec![],
            promoted_count: 1,
            new_associations_count: 0,
        }
    }

    #[test]
    fn commit_writes_memory_store_brief_archive_and_manifest() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStoreContents::default();
        let commit = SleepCommit {
            state_dir: dir.path(),
            new_store_contents: store,
            brief: sample_brief(),
            sleep_run_id: "sleep-1".to_string(),
            consumed_session_id: "s-1".to_string(),
            brief_archive_name: "sleep-sleep-1.json".to_string(),
        };
        commit.write().unwrap();

        assert!(dir.path().join("memory-store.json").exists());
        assert!(dir.path().join("consolidated-brief.json").exists());
        assert!(dir.path().join("archive/sleep-sleep-1.json").exists());
        assert!(dir.path().join("continuity-manifest.json").exists());

        let manifest = ContinuityManifest::load_or_default(
            dir.path().join("continuity-manifest.json")).unwrap();
        assert_eq!(manifest.sleep_pending, false);
        assert_eq!(manifest.last_sleep_run_id.as_deref(), Some("sleep-1"));
        assert_eq!(manifest.last_sleep_consumed_session_id.as_deref(), Some("s-1"));
    }

    #[test]
    fn commit_twice_produces_byte_identical_state_files() {
        let dir = TempDir::new().unwrap();
        let commit = SleepCommit {
            state_dir: dir.path(),
            new_store_contents: MemoryStoreContents::default(),
            brief: sample_brief(),
            sleep_run_id: "sleep-1".to_string(),
            consumed_session_id: "s-1".to_string(),
            brief_archive_name: "sleep-sleep-1.json".to_string(),
        };
        commit.write().unwrap();
        let first = std::fs::read(dir.path().join("memory-store.json")).unwrap();
        let first_brief = std::fs::read(dir.path().join("consolidated-brief.json")).unwrap();
        let first_manifest = std::fs::read(dir.path().join("continuity-manifest.json")).unwrap();
        commit.write().unwrap();
        assert_eq!(first, std::fs::read(dir.path().join("memory-store.json")).unwrap());
        assert_eq!(first_brief, std::fs::read(dir.path().join("consolidated-brief.json")).unwrap());
        assert_eq!(first_manifest, std::fs::read(dir.path().join("continuity-manifest.json")).unwrap());
    }

    #[test]
    fn partial_write_recovers_when_manifest_was_not_yet_flipped() {
        // Simulate: derived files written, manifest still says sleep_pending=true.
        // A re-run of write() with the same inputs should produce the same final state.
        let dir = TempDir::new().unwrap();
        let initial_manifest = ContinuityManifest { sleep_pending: true, ..ContinuityManifest::default() };
        initial_manifest.persist(dir.path().join("continuity-manifest.json")).unwrap();

        let commit = SleepCommit {
            state_dir: dir.path(),
            new_store_contents: MemoryStoreContents::default(),
            brief: sample_brief(),
            sleep_run_id: "sleep-1".to_string(),
            consumed_session_id: "s-1".to_string(),
            brief_archive_name: "sleep-sleep-1.json".to_string(),
        };
        commit.write().unwrap();
        let manifest = ContinuityManifest::load_or_default(
            dir.path().join("continuity-manifest.json")).unwrap();
        assert_eq!(manifest.sleep_pending, false);
    }
}
```

Add `pub mod commit;` to `sleep/mod.rs`.

- [ ] **Step 2:** Run the tests.

```bash
cargo test -p qsf_app sleep::commit
```

Expected: PASS.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/sleep
git commit -m "feat(sleep): add commit protocol with manifest-last writes and idempotency tests"
```

### Task 4.3: Extend `sleep_phase_session_summary` to consume SessionState and produce promotion plan + brief

**Files:**
- Modify: `crates/qsf_app/src/experiments/sleep_phase_session_summary.rs`
- Modify: `crates/qsf_app/src/observability/event_log.rs` (verify no new variants needed yet; payload-only changes go on existing `SleepPhaseCompleted`)

- [ ] **Step 1:** In the sleep experiment's `run`, after producing the `SleepReport`, do the following in order:

```rust
// 1. Load persisted SessionState if it exists; otherwise treat sleep as
//    operating on the per-run transcript and skip cross-session writes
//    (preserves current behavior for sleep runs that have no prior session).
let state_dir = crate::session::resume::state_dir_from_env();
let resume_inputs = crate::session::resume::load_resume_inputs(&state_dir)?;
let Some(session) = resume_inputs.previous_session.clone() else {
    // Legacy single-run sleep path: nothing to consolidate; the report stays
    // available in the sleep run dir. Return the existing outcome unchanged.
    return Ok(existing_outcome);
};

// 2. Skip if this session was already consolidated (idempotency at the
//    experiment boundary).
if resume_inputs.manifest.last_sleep_consumed_session_id.as_deref()
    == Some(session.session_id.as_str())
    && !resume_inputs.manifest.sleep_pending
{
    return Ok(existing_outcome);
}

// 3. Derive the deterministic as_of timestamp from the session's last turn.
let as_of = session.turns.iter()
    .map(|t| t.completed_at)
    .max()
    .map(|st| time::OffsetDateTime::from(st))
    .unwrap_or_else(time::OffsetDateTime::now_utc);

// 4. Load the current store and build the promotion plan.
let store_path = state_dir.join("memory-store.json");
let mut store = crate::memory::store::MemoryStore::load_or_empty(&store_path)?;
let plan = crate::sleep::auto_promote::build_promotion_plan(
    &report, &session, store.contents(), as_of, &sleep_run_id,
);

// 5. Apply the plan to the store.
store.contents_mut().records.extend(plan.new_records.clone());
store.contents_mut().associations.extend(plan.new_associations.clone());
for (a, b, new_weight) in &plan.strengthened_associations {
    if let Some(existing) = store.contents_mut().associations.iter_mut()
        .find(|x| (x.from_memory_id == *a && x.to_memory_id == *b)
                || (x.from_memory_id == *b && x.to_memory_id == *a)) {
        existing.weight = *new_weight;
        existing.last_reinforced_at = as_of;
    }
}

// 6. Build the brief.
let brief = crate::sleep::commit::ConsolidatedBrief {
    previous_session_summary: report.session_summary.clone(),
    future_context_hints: report.future_context_hints.clone(),
    open_questions: report.open_questions.clone(),
    promoted_count: plan.new_records.len(),
    new_associations_count: plan.new_associations.len(),
};

// 7. Emit a ReviewedMemoryDraft with kind=Decision for decision_candidates,
//    using the existing pipeline so accept-reviewed-memory consumes it unchanged.
//    See Task 4.4 for the conversion call.
crate::memory::reviewed_memory_draft::write_decision_candidates_draft(
    &report.decision_candidates, &sleep_run_id, context.run_dir(), as_of,
)?;

// 8. Commit: derived files first (memory-store, brief, archive), manifest last.
let commit = crate::sleep::commit::SleepCommit {
    state_dir: &state_dir,
    new_store_contents: store.contents().clone(),
    brief,
    sleep_run_id: sleep_run_id.clone(),
    consumed_session_id: session.session_id.clone(),
    brief_archive_name: format!("sleep-{sleep_run_id}.json"),
};
commit.write()?;
```

Adapt the exact variable names (e.g. `existing_outcome`, `report`, `sleep_run_id`, `context`) to whatever the experiment uses today. The structure is: load → idempotency-check → as_of → plan → apply → brief → decision-draft → commit.

- [ ] **Step 2:** Build to verify compile.

```bash
cargo build
```

Expected: build succeeds. `write_decision_candidates_draft` does not exist yet — Task 4.4 adds it. Until then, comment out the call or stub the function to return `Ok(())`.

- [ ] **Step 3: Stub the missing function temporarily.** Add to `crates/qsf_app/src/memory/reviewed_memory_draft.rs`:

```rust
pub fn write_decision_candidates_draft(
    _candidates: &[String],
    _sleep_run_id: &str,
    _run_dir: &Path,
    _as_of: OffsetDateTime,
) -> anyhow::Result<()> {
    // Stub. Implemented in Task 4.4.
    Ok(())
}
```

- [ ] **Step 4:** Build, clippy, fmt.

```bash
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
```

- [ ] **Step 5:** Commit.

```bash
git add crates/qsf_app/src/experiments/sleep_phase_session_summary.rs crates/qsf_app/src/memory/reviewed_memory_draft.rs
git commit -m "feat(sleep): wire SessionState consumption, promotion plan, and commit protocol into sleep experiment"
```

### Task 4.4: Route `decision_candidates` through the existing reviewed-memory-draft pipeline

**Files:**
- Modify: `crates/qsf_app/src/memory/reviewed_memory_draft.rs`
- Test: same file, inline tests

- [ ] **Step 1: Write the failing test.** Append to the existing test module:

```rust
#[test]
fn write_decision_candidates_draft_emits_decision_kind_records() {
    let dir = TempDir::new().unwrap();
    let candidates = vec![
        "Tools should remain read-only until permissions matured.".to_string(),
        "Voice loop unification waits until SessionState is shared.".to_string(),
    ];
    write_decision_candidates_draft(
        &candidates,
        "sleep-test",
        dir.path(),
        timestamp(),
    ).unwrap();

    let json_path = dir.path().join(REVIEWED_MEMORY_DRAFT_JSON);
    assert!(json_path.exists());
    let raw = std::fs::read_to_string(&json_path).unwrap();
    let fixture: MemoryFixture = serde_json::from_str(&raw).unwrap();
    assert_eq!(fixture.records.len(), 2);
    assert!(fixture.records.iter().all(|r| r.kind == MemoryRecordKind::Decision));
}
```

Add `use tempfile::TempDir;` to the test module if not present.

- [ ] **Step 2:** Run the test to verify it fails.

```bash
cargo test -p qsf_app memory::reviewed_memory_draft::tests::write_decision_candidates_draft_emits_decision_kind_records
```

Expected: FAIL (stubbed function does nothing).

- [ ] **Step 3:** Replace the stub with the real implementation:

```rust
pub fn write_decision_candidates_draft(
    candidates: &[String],
    sleep_run_id: &str,
    run_dir: &Path,
    as_of: OffsetDateTime,
) -> anyhow::Result<()> {
    if candidates.is_empty() {
        return Ok(());
    }
    let sanitized = sanitize_memory_id_segment(sleep_run_id);
    let records: Vec<MemoryRecord> = candidates.iter().enumerate().map(|(i, summary)| {
        let summary = summary.trim().to_string();
        let title = title_from_summary(&summary, i);
        MemoryRecord::new(
            format!("memory.decision.{sanitized}.{:03}", i + 1),
            MemoryRecordKind::Decision,
            title,
            summary.clone(),
            vec![],
            as_of,
            DEFAULT_DRAFT_IMPORTANCE,
            0,
            format!("sleep-run:{sleep_run_id}#decision_candidates[{:03}]", i + 1),
            estimated_tokens(&summary),
        )
    }).collect();

    let draft = ReviewedMemoryDraft {
        source_sleep_run_id: sleep_run_id.to_string(),
        source_sleep_report_path: run_dir.join("sleep-report.json"),
        fixture: MemoryFixture { records, associations: vec![] },
        association_reviews: vec![],
    };
    write_reviewed_memory_draft(run_dir, &draft)?;
    Ok(())
}
```

- [ ] **Step 4:** Run the test.

```bash
cargo test -p qsf_app memory::reviewed_memory_draft
```

Expected: PASS.

- [ ] **Step 5:** In the sleep experiment file, uncomment the `write_decision_candidates_draft` call from Task 4.3 if it was commented out.

- [ ] **Step 6:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/memory/reviewed_memory_draft.rs crates/qsf_app/src/experiments/sleep_phase_session_summary.rs
git commit -m "feat(memory): emit Decision-kind reviewed-memory draft from sleep for manual review"
```

### Task 4.5: Wire ConsolidatedBrief injection into the multi-turn text loop boot

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

The brief is **not** stored on `SessionState` — that would create a module
cycle (`session` ↔ `sleep::commit`). Instead, the experiment driver holds the
brief in a local `Option<ConsolidatedBrief>` and consumes it exactly once on
turn 0.

- [ ] **Step 1:** Replace the placeholder `ConsolidatedBrief` arm from Task 3.6
  with a version that loads the brief into a driver-local variable:

```rust
// Driver-local: holds the brief until turn 0 consumes it. Not persisted.
let mut pending_boot_brief: Option<crate::sleep::commit::ConsolidatedBrief> = None;

let mut state = match resume_mode {
    crate::session::manifest::ResumeMode::ColdStart => SessionState::new(config.clone()),
    crate::session::manifest::ResumeMode::AwakeContinuation => {
        let previous = resume_inputs.previous_session.clone().unwrap();
        crate::session::continuation::prepare_awake_continuation(previous, &config)
    }
    crate::session::manifest::ResumeMode::ConsolidatedBrief => {
        let previous = resume_inputs.previous_session.as_ref().unwrap();
        let mut fresh = SessionState::new(config.clone());
        fresh.previous_session_id = Some(previous.session_id.clone());

        if let Some(brief_path) = &resume_inputs.manifest.last_sleep_brief_path {
            let abs = if brief_path.is_absolute() {
                brief_path.clone()
            } else {
                state_dir.join(brief_path)
            };
            if abs.exists() {
                let raw = std::fs::read_to_string(&abs)?;
                pending_boot_brief = Some(serde_json::from_str(&raw)?);
            }
        }
        fresh
    }
};
```

- [ ] **Step 2:** At the start of the turn loop, before turn 0's context is
  assembled, take the brief and convert it into a context fragment string that
  the existing context-assembly path can prepend to the memory block:

```rust
let boot_brief_fragment: Option<String> = pending_boot_brief
    .take()
    .map(|brief| format_boot_brief_for_context(&brief));
```

Where the existing code computes the memory block string for turn 0, prepend
the fragment if present:

```rust
let memory_block_for_turn = match (&boot_brief_fragment, retrieved_block.as_str()) {
    (Some(brief_text), "") => brief_text.clone(),
    (Some(brief_text), retrieved) => format!("{brief_text}\n\n{retrieved}"),
    (None, retrieved) => retrieved.to_string(),
};
```

(`retrieved_block` is whatever variable the loop uses for the assembled memory
block today.) The brief is consumed exactly once on turn 0.

- [ ] **Step 3:** Add the formatter near the bottom of the experiment file:

```rust
fn format_boot_brief_for_context(brief: &crate::sleep::commit::ConsolidatedBrief) -> String {
    let mut s = String::new();
    s.push_str("Previous session summary:\n");
    s.push_str(&brief.previous_session_summary);
    s.push('\n');
    if !brief.future_context_hints.is_empty() {
        s.push_str("\nFuture context hints:\n");
        for hint in &brief.future_context_hints { s.push_str(&format!("- {hint}\n")); }
    }
    if !brief.open_questions.is_empty() {
        s.push_str("\nOpen questions:\n");
        for q in &brief.open_questions { s.push_str(&format!("- {q}\n")); }
    }
    s
}
```

Add `format_boot_brief_for_context` near the bottom of the experiment file:

```rust
fn format_boot_brief_for_context(brief: &crate::sleep::commit::ConsolidatedBrief) -> String {
    let mut s = String::new();
    s.push_str("Previous session summary:\n");
    s.push_str(&brief.previous_session_summary);
    s.push('\n');
    if !brief.future_context_hints.is_empty() {
        s.push_str("\nFuture context hints:\n");
        for hint in &brief.future_context_hints { s.push_str(&format!("- {hint}\n")); }
    }
    if !brief.open_questions.is_empty() {
        s.push_str("\nOpen questions:\n");
        for q in &brief.open_questions { s.push_str(&format!("- {q}\n")); }
    }
    s
}
```

- [ ] **Step 4:** Build to make sure it compiles end-to-end.

```bash
cargo build
```

Expected: build succeeds.

- [ ] **Step 5:** Run the full test suite.

```bash
cargo test
```

Expected: PASS.

- [ ] **Step 6:** Human golden-path test (session → sleep → session).

```powershell
$env:QSF_STATE_DIR = "$env:TEMP\qsf-continuity-stage4"
Remove-Item -Recurse -Force "$env:TEMP\qsf-continuity-stage4" -ErrorAction SilentlyContinue
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Two turns, then ":quit"
cargo run -p qsf_app -- experiment sleep-phase-session-summary
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Inspect events.jsonl: first event after experiment-started must be SessionResumed with mode=consolidated_brief
# Inspect state/text-loop/memory-store.json: contains records under id memory.sleep.<sleep-run-id>.NNN
# Inspect the sleep run directory: contains reviewed-memory-draft.json with Decision-kind records if there were decision_candidates
```

- [ ] **Step 7:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app
git commit -m "feat(text-loop): inject ConsolidatedBrief into turn 1 context after sleep"
```

---

## Stage 5 — Live-loop co-retrieval associations and reinforcement

**Goal:** When retrieval returns ≥2 memories in a turn, the live loop creates new associations or strengthens existing ones, with deterministic ordering. Each retrieved memory has its `reinforcement_count` bumped and `last_reinforced_at` set. Writes go to the persisted memory store; if no persistent store exists, writes are skipped (no-op with trace).

**Stage exit criteria:**
- A multi-turn run with ≥2 memories retrieved per turn produces associations in `memory-store.json` whose reasons include `co-retrieved in turn N of session X`
- The reducer is pure and deterministic; replaying the same retrieval input produces identical association deltas
- Reinforcement events emit `MemoryReinforced` with `timestamp_source = "live_now"`
- A cold-start run (no `state/`) emits a trace event noting that writes are disabled, and does not crash

### Task 5.1: Add `memory::co_retrieval` pure delta generator

**Files:**
- Create: `crates/qsf_app/src/memory/co_retrieval.rs`
- Modify: `crates/qsf_app/src/memory/mod.rs` (add `pub mod co_retrieval;`)
- Test: `crates/qsf_app/src/memory/co_retrieval.rs` (inline tests)

- [ ] **Step 1: Write the failing tests.**

```rust
use crate::memory::association::Association;
use time::OffsetDateTime;

pub const CO_RETRIEVAL_INITIAL_WEIGHT: f64 = 0.3;
pub const CO_RETRIEVAL_STRENGTHEN_DELTA: f64 = 0.05;
pub const MAX_NEW_ASSOCIATIONS_PER_TURN: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub enum CoRetrievalDelta {
    Create { from: String, to: String, weight: f64, reason: String, at: OffsetDateTime },
    Strengthen { from: String, to: String, new_weight: f64, at: OffsetDateTime },
}

/// `retrieved` is a slice of (memory_id, retrieval_score) tuples.
/// `existing` is the current set of associations in the store.
/// Returns deterministically ordered deltas. At most MAX_NEW_ASSOCIATIONS_PER_TURN
/// Create deltas; excess pairs that have no existing association are dropped.
pub fn generate_deltas(
    retrieved: &[(String, f64)],
    existing: &[Association],
    turn_index: usize,
    session_id: &str,
    now: OffsetDateTime,
) -> Vec<CoRetrievalDelta> {
    if retrieved.len() < 2 { return vec![]; }

    // Build unordered candidate pairs with joint score.
    let mut pairs: Vec<((String, String), f64)> = Vec::new();
    for i in 0..retrieved.len() {
        for j in (i + 1)..retrieved.len() {
            let (a_id, a_score) = &retrieved[i];
            let (b_id, b_score) = &retrieved[j];
            if a_id == b_id { continue; }
            let (lo, hi) = if a_id < b_id {
                (a_id.clone(), b_id.clone())
            } else {
                (b_id.clone(), a_id.clone())
            };
            pairs.push(((lo, hi), a_score + b_score));
        }
    }
    // Deterministic order: joint score desc, then lexicographic on (lo, hi).
    pairs.sort_by(|x, y| {
        y.1.total_cmp(&x.1).then_with(|| x.0.cmp(&y.0))
    });

    let mut deltas = Vec::new();
    let mut creates_used = 0;
    for ((lo, hi), _score) in pairs {
        let existing_idx = existing.iter().position(|a| {
            (a.from_memory_id == lo && a.to_memory_id == hi)
                || (a.from_memory_id == hi && a.to_memory_id == lo)
        });
        if let Some(idx) = existing_idx {
            let new_weight = (existing[idx].weight + CO_RETRIEVAL_STRENGTHEN_DELTA).min(1.0);
            deltas.push(CoRetrievalDelta::Strengthen {
                from: lo, to: hi, new_weight, at: now,
            });
        } else if creates_used < MAX_NEW_ASSOCIATIONS_PER_TURN {
            deltas.push(CoRetrievalDelta::Create {
                from: lo, to: hi, weight: CO_RETRIEVAL_INITIAL_WEIGHT,
                reason: format!("co-retrieved in turn {turn_index} of session {session_id}"),
                at: now,
            });
            creates_used += 1;
        }
        // else: drop the pair (no create budget, no existing to strengthen).
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::format_description::well_known::Rfc3339;

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn no_deltas_when_fewer_than_two_memories() {
        let deltas = generate_deltas(&[("a".to_string(), 1.0)], &[], 0, "s", now());
        assert!(deltas.is_empty());
    }

    #[test]
    fn creates_one_delta_per_pair_within_cap() {
        let retrieved = vec![
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.8),
            ("c".to_string(), 0.7),
        ];
        let deltas = generate_deltas(&retrieved, &[], 0, "s", now());
        assert_eq!(deltas.len(), 3);
        assert!(deltas.iter().all(|d| matches!(d, CoRetrievalDelta::Create { .. })));
    }

    #[test]
    fn strengthens_existing_associations_instead_of_creating() {
        let retrieved = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let existing = vec![Association::new("a", "b", 0.5, "earlier", now())];
        let deltas = generate_deltas(&retrieved, &existing, 0, "s", now());
        assert_eq!(deltas.len(), 1);
        assert!(matches!(deltas[0], CoRetrievalDelta::Strengthen { ref from, ref to, new_weight, .. }
            if from == "a" && to == "b" && (new_weight - 0.55).abs() < 1e-9));
    }

    #[test]
    fn caps_creates_at_five_per_turn() {
        // 6 memories -> 15 pairs -> only 5 creates, rest dropped.
        let retrieved: Vec<(String, f64)> = (0..6)
            .map(|i| (format!("memory.{:02}", i), 1.0 - 0.01 * i as f64))
            .collect();
        let deltas = generate_deltas(&retrieved, &[], 0, "s", now());
        let creates = deltas.iter().filter(|d| matches!(d, CoRetrievalDelta::Create { .. })).count();
        assert_eq!(creates, MAX_NEW_ASSOCIATIONS_PER_TURN);
    }

    #[test]
    fn deterministic_ordering_under_tied_scores() {
        let retrieved = vec![
            ("z".to_string(), 1.0),
            ("a".to_string(), 1.0),
            ("m".to_string(), 1.0),
        ];
        let deltas_first = generate_deltas(&retrieved, &[], 0, "s", now());
        let deltas_second = generate_deltas(&retrieved, &[], 0, "s", now());
        assert_eq!(deltas_first, deltas_second);
        // First pair lexicographically must be (a, m).
        match &deltas_first[0] {
            CoRetrievalDelta::Create { from, to, .. } => {
                assert_eq!(from.as_str(), "a");
                assert_eq!(to.as_str(), "m");
            }
            _ => panic!("expected Create"),
        }
    }
}
```

Add `pub mod co_retrieval;` to `memory/mod.rs`.

- [ ] **Step 2:** Run tests.

```bash
cargo test -p qsf_app memory::co_retrieval
```

Expected: PASS.

- [ ] **Step 3:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app/src/memory
git commit -m "feat(memory): add co_retrieval pure delta generator with deterministic ordering"
```

### Task 5.2: Wire deltas into the multi-turn text loop turn pipeline

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- Modify: `crates/qsf_app/src/observability/event_log.rs` (add `CoRetrievalAssociationsProposed`, `MemoryReinforced`, `MemoryStorePersisted` variants)

- [ ] **Step 1: Add the three new event variants.** In `event_log.rs`'s `EventType` enum, append:

```rust
CoRetrievalAssociationsProposed,
MemoryReinforced,
MemoryStorePersisted,
```

Build to confirm exhaustive matches still resolve.

- [ ] **Step 2: In the text loop, after `MemoryRetrieved`, compute and apply deltas.** Locate the turn body where retrieval result is available. Insert:

```rust
let memory_store_path = state_dir.join("memory-store.json");
let store_exists = memory_store_path.exists();

if store_exists {
    let mut store = crate::memory::store::MemoryStore::load_or_empty(&memory_store_path)?;
    let now = time::OffsetDateTime::now_utc();
    let retrieved_pairs: Vec<(String, f64)> = retrieval_result.selected.iter()
        .map(|m| (m.memory.id.clone(), m.score.total))
        .collect();
    let deltas = crate::memory::co_retrieval::generate_deltas(
        &retrieved_pairs, &store.contents().associations,
        turn_index, &state.session_id, now,
    );

    let mut created = 0;
    let mut strengthened = 0;
    for delta in &deltas {
        match delta {
            crate::memory::co_retrieval::CoRetrievalDelta::Create { from, to, weight, reason, at } => {
                store.contents_mut().associations.push(crate::memory::association::Association::new(
                    from.clone(), to.clone(), *weight, reason.clone(), *at,
                ));
                created += 1;
            }
            crate::memory::co_retrieval::CoRetrievalDelta::Strengthen { from, to, new_weight, at } => {
                if let Some(existing) = store.contents_mut().associations.iter_mut().find(|a|
                    (a.from_memory_id == *from && a.to_memory_id == *to)
                        || (a.from_memory_id == *to && a.to_memory_id == *from)
                ) {
                    existing.weight = *new_weight;
                    existing.last_reinforced_at = *at;
                    strengthened += 1;
                }
            }
        }
    }

    // Reinforce each retrieved memory: bump count, set last_reinforced_at.
    let retrieved_ids: Vec<String> = retrieved_pairs.iter().map(|(id, _)| id.clone()).collect();
    for record in store.contents_mut().records.iter_mut() {
        if retrieved_ids.iter().any(|id| id == &record.id) {
            record.reinforcement_count = record.reinforcement_count.saturating_add(1);
            record.last_reinforced_at = Some(now);
        }
    }

    store.persist()?;

    // Events.
    context.event_log_mut()?.append(EventRecord::new(
        context.experiment_id().to_string(),
        EventType::CoRetrievalAssociationsProposed,
        serde_json::json!({
            "turn_index": turn_index,
            "proposed_count": deltas.len(),
            "created_count": created,
            "strengthened_count": strengthened,
            "dropped_count": (retrieved_pairs.len().saturating_sub(1) * retrieved_pairs.len() / 2)
                - deltas.len(),
        }),
        None,
    ))?;
    context.event_log_mut()?.append(EventRecord::new(
        context.experiment_id().to_string(),
        EventType::MemoryReinforced,
        serde_json::json!({
            "ids": retrieved_ids,
            "count": retrieved_ids.len(),
            "timestamp_source": "live_now",
        }),
        None,
    ))?;
    if !deltas.is_empty() || !retrieved_ids.is_empty() {
        context.event_log_mut()?.append(EventRecord::new(
            context.experiment_id().to_string(),
            EventType::MemoryStorePersisted,
            serde_json::json!({
                "path": memory_store_path.display().to_string(),
                "records_count": store.contents().records.len(),
                "associations_count": store.contents().associations.len(),
            }),
            None,
        ))?;
    }
} else {
    // Cold-start: no persistent store, writes are no-op.
    context.event_log_mut()?.append(EventRecord::new(
        context.experiment_id().to_string(),
        EventType::MemoryReinforced,
        serde_json::json!({
            "ids": Vec::<String>::new(),
            "count": 0,
            "timestamp_source": "live_now",
            "skipped_reason": "no persistent memory store on cold start",
        }),
        None,
    ))?;
}
```

Adjust variable names to match the experiment's locals (`turn_index`, `retrieval_result`, `context`).

- [ ] **Step 3:** Build.

```bash
cargo build
```

Expected: success.

- [ ] **Step 4:** Run the full test suite.

```bash
cargo test
```

Expected: PASS.

- [ ] **Step 5:** Human golden-path test (live association formation after Stage 4 sleep produced a real store).

```powershell
$env:QSF_STATE_DIR = "$env:TEMP\qsf-continuity-stage5"
Remove-Item -Recurse -Force "$env:TEMP\qsf-continuity-stage5" -ErrorAction SilentlyContinue
$env:QSF_SESSION_MEMORY_SOURCE = "phase_four_fixture"
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Two turns, ":quit"
cargo run -p qsf_app -- experiment sleep-phase-session-summary
cargo run -p qsf_app -- experiment multi-turn-text-loop
# Ask questions designed to retrieve >=2 memories; quit.
# Inspect state/text-loop/memory-store.json: contains associations with reason "co-retrieved in turn N of session ..."
```

- [ ] **Step 6:** Lints, format, commit.

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt
git add crates/qsf_app
git commit -m "feat(text-loop): emit co-retrieval associations and reinforcement during live turns"
```

---

## Stage 6 — Documentation, diary, decision-log entries

**Goal:** Bring the workflow documents into alignment with what now exists in code (per `Agents.md` and `ProjectWorkflow.md`).

### Task 6.1: Update Architecture Implementation Status sections

**Files:**
- Modify: `docs/Architecture/Architecture.MemorySystem.md`
- Modify: `docs/Architecture/Architecture.SleepPhase.md`
- Modify: `docs/Architecture/Architecture.StateAndObservability.md`

- [x] **Step 1:** In each file's *Implementation Status* section, move the relevant items from "not yet implemented" to "implemented today" with code-module refs. For example, in `Architecture.MemorySystem.md`:

```markdown
**Implemented today:**
- ...existing items...
- Cross-session memory store via `MemoryStore`
  ([memory/store.rs](../../crates/qsf_app/src/memory/store.rs)) backed by
  `state/text-loop/memory-store.json`
- Time-based decay against `MemoryRecord.last_reinforced_at`
  ([memory/retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs))
- Live-loop co-retrieval association formation and reinforcement
  ([memory/co_retrieval.rs](../../crates/qsf_app/src/memory/co_retrieval.rs))
- Sleep-side auto-promotion of memory candidates and cross-turn associations
  ([sleep/auto_promote.rs](../../crates/qsf_app/src/sleep/auto_promote.rs))
```

Bump `Last reviewed:` to today's date.

Make analogous edits in `Architecture.SleepPhase.md` (auto-promotion, commit protocol) and `Architecture.StateAndObservability.md` (cross-session SessionState lifetime, new event types).

- [ ] **Step 2:** Commit.

```bash
git add docs/Architecture
git commit -m "docs(architecture): update implementation-status sections for cross-session continuity"
```

### Task 6.2: Append the 2026-05-16 refinement Decision-Log entry

**Files:**
- Modify: `docs/DecisionLog.md`

- [x] **Step 1:** Append a new entry. Replace `YYYY-MM-DD` with today's date.

```markdown
## YYYY-MM-DD - Sleep auto-promotes routine memory candidates
Decision: Sleep promotes `SleepReport.memory_candidates` into the cross-session
memory store as `Observation` records automatically, with structural validation
and normalized-string dedup. `SleepReport.decision_candidates` are emitted as a
`ReviewedMemoryDraft` with `kind = Decision` for manual review through the
existing `accept-reviewed-memory` experiment. This refines the 2026-05-16
"Sleep-to-memory conversion is explicit and separate" decision: explicit review
remains the path for the highest-stakes record category (`Decision`), while
routine observations flow through automatically so cross-session continuity is
observable in normal use.
Context: The 2026-05-16 boundary blocked cross-session continuity, which is the
project's core thesis. Per Design.CrossSessionContinuity.md, `Decision`-kind
items retain the manual boundary; everything else under `memory_candidates`
auto-promotes. Decay is time-based at retrieval, computed from
`last_reinforced_at`, with a 30-day half-life as the starting default.
Consequences: Sleep writes through a commit protocol where the manifest is the
last file written, with idempotent re-execution recovering from partial writes.
Live loops can reinforce retrieved memories and form co-retrieval associations.
Refs: docs/Plans/Design.CrossSessionContinuity.md,
docs/Plans/Plan.CrossSessionContinuity.md,
crates/qsf_app/src/sleep/auto_promote.rs,
crates/qsf_app/src/sleep/commit.rs,
docs/DecisionLog.md#2026-05-16---sleep-to-memory-conversion-is-explicit-and-separate
```

- [ ] **Step 2:** Commit.

```bash
git add docs/DecisionLog.md
git commit -m "docs(decision-log): refine 2026-05-16 boundary; sleep auto-promotes routine candidates"
```

### Task 6.3: Add Engineering Diary entry and follow-up Idea stub

**Files:**
- Modify: `docs/EngineeringDiary.md`
- Create: `docs/Plans/Idea.VoiceLoopUnification.md`

- [x] **Step 1:** Append a diary entry following the file's existing template. Reference this plan, the design doc, and the new decision-log entry.

- [x] **Step 2:** Create `docs/Plans/Idea.VoiceLoopUnification.md` as a thin stub:

```markdown
# Idea: Voice Loop Unification with Multi-Turn SessionState

Status: Idea — not yet planned.

## Motivation

The multi-turn text loop has cross-session continuity. The voice loop today has
no `SessionState` module and cannot resume. The next plan should design a
shared session model that handles voice's event-driven shape (interrupts,
partial transcripts, partial responses) and then have the voice loop participate
in the same continuity manifest the text loop uses today.

## Prerequisite

`Plan.CrossSessionContinuity.md` must be complete. This idea consumes its
`SessionState`, `ContinuityManifest`, `MemoryStore`, and `SleepCommit`
abstractions.

## Open problems

- Whether `Turn` is the right unit for voice (or whether voice needs a finer
  `Utterance`/`Exchange` boundary)
- How interruption state participates in `prepare_awake_continuation`
- Whether the consolidated brief is read at session start or streamed as the
  user begins speaking
```

- [ ] **Step 3:** Commit.

```bash
git add docs/EngineeringDiary.md docs/Plans/Idea.VoiceLoopUnification.md
git commit -m "docs: diary entry for cross-session continuity; stub voice-loop unification follow-up"
```

---

## Final verification

After all stages land:

- [ ] `cargo build`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo run -p qsf_app -- list-experiments` returns the full list
- [ ] End-to-end session → sleep → session golden-path: second session boot logs `mode=consolidated_brief`, memory store contains promoted observations and co-retrieval associations
- [ ] Idempotency: running `sleep-phase-session-summary` twice in a row against the same `state/` produces byte-identical `memory-store.json`, `consolidated-brief.json`, and `continuity-manifest.json`

Once these all pass, the plan is complete. The voice-loop unification follow-up (`Idea.VoiceLoopUnification.md`) is the next planning effort.
