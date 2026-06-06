# Plan: Project-Doc Introspection

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a read-only project-document introspection channel so the
live-presence model can ground self-questions in actual project material
during human dialogue.

**Architecture:** Two new `Tool` implementations
(`search_project_docs`, `read_project_doc`) backed by a pure
`ProjectDocService`, registered through the existing `ToolRegistry`,
exposed to the `ConversationalResponder` role, with per-turn budget
enforcement at the dispatch layer and observability via the existing
`EventType::Tool*` lifecycle plus new `TraceRecord` operations.

**Tech Stack:** Rust, `anyhow`, `serde` + `serde_json`, `toml`, `time`,
`uuid`, existing `qsf_app` crate.

**Reference design:** `docs/Plans/Design.ProjectDocIntrospection.md` —
this plan implements the decisions there. Where the design defers a
choice ("plan-phase decision"), the plan picks one explicitly.

---

## Status

Phase 1 is the next implementation step.

## Background

The design at `docs/Plans/Design.ProjectDocIntrospection.md` specifies a
live-first introspection channel for project documents. This plan
implements it in nine sequential phases that each produce something
independently testable. Phases 1-6 are the minimum viable channel
(tools work end-to-end and the responder can call them). Phase 7
delivers the offline self-question battery promised by the design's
*Live-First Rationale*. Phase 8 adds the `influenced_reply` post-hoc
enrichment. Phase 9 lands the documentation updates required by
`docs/ProjectFrame/ProjectWorkflow.md`. A final external verification
step is recorded at the end.

## Current Anchors

Code anchors (existing, will be extended):

- `crates/qsf_app/src/tools/mod.rs` — re-exports tool surface; will
  add `project_docs` submodule.
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`,
  `Tool` trait, `ToolMetadata`, `ToolContext`. Adding two tools means
  extending the struct, `Default`, and the `match` arms in
  `metadata_for`, `dispatch`, and `model_tool_definitions_for`.
- `crates/qsf_app/src/tools/tool_request.rs` — `ToolPermission`
  (needs a `read_only()` constructor analogous to `compute_only()`).
- `crates/qsf_app/src/tools/calculator_tool.rs` and
  `crates/qsf_app/src/tools/recall_turn_tool.rs` — reference
  implementations of the `Tool` trait and custom `ToolContext`.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls`; this is where per-turn caps are
  enforced and where tool-result trace records are emitted.
- `crates/qsf_app/src/models/model_role.rs` — `ModelRole::predefined`
  for `ConversationalResponder`; `allowed_tools` is overridden by
  call sites (see `multi_turn_text_loop.rs`).
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs:495-511` —
  reference for how `ToolResult` becomes a `ModelMessage::tool_result`
  and is appended to the message list before the next provider turn.
- `crates/qsf_app/src/observability/trace.rs` — `TraceRecord` with
  rich `details: serde_json::Value` field; new operations
  (`project_doc_search`, `project_doc_read`) ride in `operation` and
  `details`, no schema change required.
- `crates/qsf_app/src/observability/event_log.rs` —
  `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed`.
  No new event type is added.
- `crates/qsf_app/src/runtime/run_context.rs` — `RunContext`
  exposes the event/trace writers.

Documentation anchors:

- `docs/Plans/Design.ProjectDocIntrospection.md` — the spec this
  plan implements.
- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md` — broader
  brainstorm (updated in Phase 9).
- `docs/ProjectFrame/DocumentStatus.md` — defines `kind` and
  `maturity_tag` taxonomies; updated in Phase 9 to reference the
  allowlist file.
- `docs/Architecture/Architecture.ToolSystem.md` — its *Implementation
  Status* section is refreshed in Phase 9 to move the two new tools
  from "Not yet implemented" to "Implemented today".

## Open Questions To Surface During Implementation

Per `Agents.md`, ambiguities should be surfaced rather than silently
resolved. The plan picks a default for each; if any plays out
differently, raise it before changing direction.

1. **Config file path.** This plan uses `config/project-doc-introspection.toml`
   at the repo root. If the repo already has another config-loading
   convention (search `crates/qsf_app` for existing config readers
   before Phase 1), align with that and update the path everywhere in
   this plan and in `Design.ProjectDocIntrospection.md`.
   *Path-resolution note:* `cargo test` runs with the working
   directory set to the package root (`crates/qsf_app`), **not** the
   workspace root, so tests and production code must never load the
   config via a bare relative path like
   `"config/project-doc-introspection.toml"`. Tests resolve it from
   `CARGO_MANIFEST_DIR` (see Task 1.2); production wiring (later
   phases) must construct `ProjectDocService` with an explicit
   absolute repo root and an explicit absolute allowlist path, rather
   than relying on the process working directory.
2. **`ProjectDocService` injection shape.** This plan uses a dedicated
   `ProjectDocToolContext` carrying a shared `Arc<ProjectDocService>`,
   parallel to `SessionToolContext`. If a different existing pattern
   is preferred, raise it before Phase 2.
3. **`influenced_reply` storage.** Phase 8 writes the marker as a
   follow-up `TraceRecord` referencing the original by `trace_id`.
   If an annotation on the original record is preferred, raise before
   Phase 8.
4. **Module naming.** Per `Agents.md`, name modules after stable
   behavior. This plan uses `project_docs` (not `project_doc_v1` or
   `introspection_phase_1`) and `ProjectDocService` (not
   `ProjectDocV1Service`). Keep this discipline through the work.
5. **Hard latency cap.** Decision 4 of the spec sets a 1500 ms hard
   cap. With lexical search over a small markdown corpus the cap is
   not expected to fire, so this plan **deliberately defers**
   cap-enforcement: the `ProjectDocService` exposes synchronous
   `search`/`read` with no deadline parameter. This is a conscious
   scope decision, not an oversight — record it as such in
   `Design.ProjectDocIntrospection.md` Decision 4 (a one-line note in
   Phase 9's documentation pass is sufficient) so the design and the
   implementation agree.
   A review raised that a synchronous API gives dispatch no clean way
   to interrupt a long filesystem walk. That risk is acceptable for
   the current corpus size, but if real-run traces ever show
   `latency_ms` over 1000, add enforcement **at the
   `ProjectDocService` boundary**: thread a deadline / max-elapsed
   budget through `search`/`read`, return partial results, and surface
   an `omitted_due_to_budget` signal that Phase 5's trace emission can
   record. Note the change in the diary entry when it happens.
6. **Test setup in Tasks 4.1 and 5.1.** Those tasks include test
   skeletons rather than fully-spelled integration tests, because
   wiring a `RunContext`, mock model client, `ProjectDocToolContext`,
   and `ModelRequest` for a unit test of `dispatch_model_tool_calls`
   is a lot of code that already has working examples in the file (or,
   if absent, can mirror `crates/qsf_app/tests/` patterns). The
   assertions in those skeletons are concrete; the harness wiring is
   not. If existing patterns are unclear, write the integration test
   under `crates/qsf_app/tests/project_doc_dispatch.rs` and treat the
   skeletons as the assertion contract.

## Target Shape

```text
user input
  -> ConversationalResponder advertises calculator + recall_turn +
     search_project_docs + read_project_doc
  -> model emits a search_project_docs tool call (optional)
  -> dispatch checks per-turn cap, runs SearchProjectDocsTool
  -> ProjectDocService consults the on-disk allowlist + corpus,
     returns ranked DocHits with kind/maturity metadata
  -> ToolResult formatted, ToolCompleted event + TraceRecord
     (operation = "project_doc_search") emitted
  -> provider-native tool message appended to messages list
  -> next provider call; model may then call read_project_doc
  -> dispatch checks per-turn cap, runs ReadProjectDocTool
  -> ProjectDocService returns focused DocRead under budget
  -> ToolResult, ToolCompleted, TraceRecord
     (operation = "project_doc_read") emitted
  -> provider produces the human-facing reply with kind/maturity hedging
  -> post-hoc enrichment pass marks influenced_reply on traces whose
     content overlapped the final reply
```

---

## Phase 1: `ProjectDocService` library

Pure, side-effect-free Rust module under
`crates/qsf_app/src/project_docs/` containing the allowlist loader,
metadata extraction, lexical search, and bounded read. No tool wiring
yet. All tests are unit tests driven by an in-tree fixture corpus
under `crates/qsf_app/src/project_docs/fixtures/`.

**Path-safety invariant for this phase:** any caller-supplied document
path that reaches the filesystem (the bounded read in Task 1.5) must be
normalized and confined to the repo root *before* the allowlist is
consulted. The allowlist operates on clean, repo-relative,
forward-slash paths; it must never see a raw string containing `..` or
an absolute prefix, because glob include/exclude evaluation against
such a string can admit material the exclude rules were meant to block.
Search (Task 1.4) derives its paths from a `walkdir` traversal of the
repo root, so those paths are already clean; only the read path takes a
raw caller string.

### Task 1.1: Module scaffold and types

**Files:**
- Create: `crates/qsf_app/src/project_docs/mod.rs`
- Create: `crates/qsf_app/src/project_docs/types.rs`
- Modify: `crates/qsf_app/src/lib.rs` (or wherever top-level modules
  are declared) to add `pub mod project_docs;`

- [ ] **Step 1: Write the types module.**

```rust
// crates/qsf_app/src/project_docs/types.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Frame,
    Concept,
    Research,
    Plan,
    Idea,
    Design,
    Architecture,
    ExperimentSpec,
    ExperimentReport,
    Diary,
    Decision,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturityTag {
    Brainstorm,
    Sketch,
    Candidate,
    Accepted,
    Implemented,
    Deprecated,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStrength {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocHit {
    pub path: String,
    pub kind: DocKind,
    pub maturity_tag: MaturityTag,
    pub last_reviewed: Option<String>, // ISO date, kept as string for trace stability
    pub snippet: String,
    pub section_hint: Option<String>,
    pub match_strength: MatchStrength,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocRead {
    pub path: String,
    pub kind: DocKind,
    pub maturity_tag: MaturityTag,
    pub last_reviewed: Option<String>,
    pub content: String,
    pub is_full: bool,
    pub omitted_sections: Vec<String>,
}
```

- [ ] **Step 2: Write the module scaffold.**

```rust
// crates/qsf_app/src/project_docs/mod.rs
//! Read-only project-document introspection: allowlist evaluation,
//! metadata extraction, lexical search, and bounded reads.

pub mod types;
// further submodules added in subsequent tasks:
//   pub mod allowlist;
//   pub mod metadata;
//   pub mod search;
//   pub mod read;
//   pub mod service;

pub use types::{DocHit, DocKind, DocRead, MatchStrength, MaturityTag};
```

- [ ] **Step 3: Wire the module into the crate.**

Open `crates/qsf_app/src/lib.rs` and add `pub mod project_docs;`
in alphabetical order with the other top-level modules.

- [ ] **Step 4: Build and verify.**

Run: `cargo build`
Expected: builds clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/project_docs crates/qsf_app/src/lib.rs
git commit -m "feat(project_docs): scaffold module and types"
```

### Task 1.2: Allowlist loader

**Files:**
- Create: `crates/qsf_app/src/project_docs/allowlist.rs`
- Create: `config/project-doc-introspection.toml`
- Create: `crates/qsf_app/src/project_docs/fixtures/allowlist_basic.toml`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`
- Modify: `crates/qsf_app/Cargo.toml` (add `toml`, `globset` if not
  already present; check first)

- [ ] **Step 1: Write the production allowlist file.**

```toml
# config/project-doc-introspection.toml
# Documents accessible to the project-doc introspection channel.
# Edit this file to add or remove material. Patterns are repo-root globs.

include = [
  "docs/ProjectFrame/**/*.md",
  "docs/Concepts/**/*.md",
  "docs/Architecture/**/*.md",
  "docs/Plans/**/*.md",
  "docs/Experiments/**/*.md",
  "docs/Research/**/*.md",
  "docs/DecisionLog.md",
  "README.md",
]

exclude = [
  "docs/Reviews/**",
  "docs/EngineeringDiary.md",
]
```

- [ ] **Step 2: Write a small fixture allowlist.**

```toml
# crates/qsf_app/src/project_docs/fixtures/allowlist_basic.toml
# Used by unit tests that walk the fixtures directory directly, so patterns
# are relative to that directory rather than to the repo root.
include = ["**/*.md"]
exclude = []
```

- [ ] **Step 3: Write the failing unit test.**

The production-allowlist test must resolve the workspace-root config
from a path anchored to `CARGO_MANIFEST_DIR`, because `cargo test` runs
with the working directory at the package root (`crates/qsf_app`), not
the workspace root. A bare relative string would resolve to
`crates/qsf_app/config/...` and fail to find the file.

```rust
// crates/qsf_app/src/project_docs/allowlist.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// `config/project-doc-introspection.toml` lives at the workspace root,
    /// two levels above this crate's manifest dir.
    fn workspace_config_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../config/project-doc-introspection.toml")
    }

    #[test]
    fn accepts_path_matching_include_only() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
               exclude=[]"#,
        )
        .unwrap();
        assert!(allowlist.allows("docs/ProjectFrame/ProjectVision.md"));
    }

    #[test]
    fn rejects_path_outside_include() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
               exclude=[]"#,
        )
        .unwrap();
        assert!(!allowlist.allows("crates/qsf_app/src/main.rs"));
    }

    #[test]
    fn exclude_overrides_include() {
        let allowlist = Allowlist::from_str(
            r#"include=["docs/**/*.md"]
               exclude=["docs/Reviews/**"]"#,
        )
        .unwrap();
        assert!(!allowlist.allows("docs/Reviews/Review.X.md"));
        assert!(allowlist.allows("docs/Architecture/Architecture.Overview.md"));
    }

    #[test]
    fn default_production_allowlist_excludes_diary_and_reviews() {
        let allowlist = Allowlist::from_file(workspace_config_path())
            .expect("production allowlist must load");
        assert!(!allowlist.allows("docs/EngineeringDiary.md"));
        assert!(!allowlist.allows("docs/Reviews/anything.md"));
        assert!(allowlist.allows("docs/ProjectFrame/ProjectVision.md"));
        assert!(allowlist.allows("docs/DecisionLog.md"));
    }
}
```

- [ ] **Step 4: Run tests; verify they fail.**

Run: `cargo test -p qsf_app project_docs::allowlist`
Expected: FAIL ("`Allowlist` not found" or similar).

- [ ] **Step 5: Implement `Allowlist`.**

```rust
// crates/qsf_app/src/project_docs/allowlist.rs
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Allowlist {
    include: GlobSet,
    exclude: GlobSet,
}

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

impl Allowlist {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("read allowlist `{}`", path.display()))?;
        Self::from_str(&raw)
    }

    pub fn from_str(raw: &str) -> Result<Self> {
        let parsed: AllowlistFile =
            toml::from_str(raw).context("parse allowlist toml")?;
        Ok(Self {
            include: build_globset(&parsed.include)?,
            exclude: build_globset(&parsed.exclude)?,
        })
    }

    /// Evaluates a clean, repo-relative, forward-slash path. Callers that
    /// accept raw paths from outside (the bounded read in Task 1.5) must
    /// normalize and confine the path before calling this.
    pub fn allows(&self, repo_relative_path: &str) -> bool {
        if self.exclude.is_match(repo_relative_path) {
            return false;
        }
        self.include.is_match(repo_relative_path)
    }
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(pattern)
                .with_context(|| format!("invalid glob `{pattern}`"))?,
        );
    }
    builder.build().context("compile glob set")
}
```

- [ ] **Step 6: Add `globset` and `toml` to the crate if absent.**

Inspect `crates/qsf_app/Cargo.toml`. If `globset` and `toml` are not
already present (or pinned at a workspace level), add them under
`[dependencies]`. Run `cargo build` to confirm the lockfile updates
cleanly.

- [ ] **Step 7: Re-export `Allowlist` from the module.**

In `crates/qsf_app/src/project_docs/mod.rs`, add:

```rust
pub mod allowlist;
pub use allowlist::Allowlist;
```

- [ ] **Step 8: Run tests; verify they pass.**

Run: `cargo test -p qsf_app project_docs::allowlist`
Expected: PASS.

- [ ] **Step 9: Commit.**

```bash
git add crates/qsf_app/src/project_docs/allowlist.rs \
        crates/qsf_app/src/project_docs/mod.rs \
        crates/qsf_app/src/project_docs/fixtures/allowlist_basic.toml \
        config/project-doc-introspection.toml \
        crates/qsf_app/Cargo.toml Cargo.lock
git commit -m "feat(project_docs): allowlist loader with include/exclude globs"
```

### Task 1.3: Metadata extraction

**Files:**
- Create: `crates/qsf_app/src/project_docs/metadata.rs`
- Create: `crates/qsf_app/src/project_docs/fixtures/sample_concept.md`
- Create: `crates/qsf_app/src/project_docs/fixtures/sample_architecture.md`
- Create: `crates/qsf_app/src/project_docs/fixtures/sample_design.md`
- Create: `crates/qsf_app/src/project_docs/fixtures/sample_unknown.md`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

- [ ] **Step 1: Write fixture documents.**

`sample_concept.md`:

```markdown
# Concept: Sample

## Maturity

Candidate

## Body

Concept body for testing.
```

`sample_architecture.md` (the trailing `## Notes` section gives the
bounded-read truncation test in Task 1.5 a body section to omit):

```markdown
# Architecture: Sample

## Maturity

Accepted

## Implementation Status

Implemented today: stuff.
Last reviewed: 2026-05-21 against the code on `main`.

## Notes

Additional notes for testing bounded reads.
```

`sample_design.md`:

```markdown
# Design: Sample

## Status

Candidate

## Body

Design body for testing.
```

`sample_unknown.md`:

```markdown
# Unstructured Notes

Some text without a recognized heading.
```

- [ ] **Step 2: Write the failing tests.**

These cover the design's metadata matrix plus the edge cases the
original skeleton omitted: `last_reviewed` must be scoped to the
`## Implementation Status` section (not matched anywhere in the body),
and a malformed date must yield `None` rather than a partial parse.

```rust
// crates/qsf_app/src/project_docs/metadata.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocKind, MaturityTag};

    #[test]
    fn kind_from_projectframe_path() {
        assert_eq!(kind_for_path("docs/ProjectFrame/ProjectVision.md"), DocKind::Frame);
    }

    #[test]
    fn kind_from_plan_idea_design_filenames() {
        assert_eq!(kind_for_path("docs/Plans/Plan.X.md"), DocKind::Plan);
        assert_eq!(kind_for_path("docs/Plans/Idea.X.md"), DocKind::Idea);
        assert_eq!(kind_for_path("docs/Plans/Design.X.md"), DocKind::Design);
    }

    #[test]
    fn kind_falls_back_to_unknown() {
        assert_eq!(kind_for_path("docs/Random/Other.md"), DocKind::Unknown);
    }

    #[test]
    fn maturity_from_concept_doc() {
        let body = include_str!("fixtures/sample_concept.md");
        assert_eq!(
            maturity_for(DocKind::Concept, body),
            MaturityTag::Candidate
        );
    }

    #[test]
    fn maturity_from_design_uses_status_heading() {
        let body = include_str!("fixtures/sample_design.md");
        assert_eq!(maturity_for(DocKind::Design, body), MaturityTag::Candidate);
    }

    #[test]
    fn maturity_unknown_when_heading_missing() {
        let body = include_str!("fixtures/sample_unknown.md");
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Unknown);
    }

    #[test]
    fn maturity_unknown_for_unrecognized_value() {
        let body = "## Maturity\n\nBananas\n";
        assert_eq!(maturity_for(DocKind::Concept, body), MaturityTag::Unknown);
    }

    #[test]
    fn maturity_not_applicable_for_frame() {
        assert_eq!(maturity_for(DocKind::Frame, "anything"), MaturityTag::NotApplicable);
    }

    #[test]
    fn last_reviewed_parsed_from_architecture() {
        let body = include_str!("fixtures/sample_architecture.md");
        assert_eq!(
            last_reviewed_for(body).as_deref(),
            Some("2026-05-21")
        );
    }

    #[test]
    fn last_reviewed_none_when_section_absent() {
        let body = include_str!("fixtures/sample_concept.md");
        assert_eq!(last_reviewed_for(body), None);
    }

    #[test]
    fn last_reviewed_ignored_outside_implementation_status_section() {
        // "Last reviewed:" appears in the body but there is no
        // `## Implementation Status` section to scope it to.
        let body = "# Doc\n\n## Body\n\nLast reviewed: 2020-01-01 somewhere.\n";
        assert_eq!(last_reviewed_for(body), None);
    }

    #[test]
    fn last_reviewed_none_for_malformed_date() {
        let body = "## Implementation Status\n\nLast reviewed: May 2026.\n";
        assert_eq!(last_reviewed_for(body), None);
    }
}
```

- [ ] **Step 3: Run tests; verify they fail.**

Run: `cargo test -p qsf_app project_docs::metadata`
Expected: FAIL (functions undefined).

- [ ] **Step 4: Implement extraction rules.**

`last_reviewed_for` is scoped to the `## Implementation Status` section
so a stray "Last reviewed:" elsewhere in the document is ignored; the
date regex enforces an ISO `YYYY-MM-DD` shape so malformed dates yield
`None`.

```rust
// crates/qsf_app/src/project_docs/metadata.rs
use once_cell::sync::Lazy;
use regex::Regex;

use super::{DocKind, MaturityTag};

pub fn kind_for_path(path: &str) -> DocKind {
    if path.starts_with("docs/ProjectFrame/") || path == "README.md" {
        return DocKind::Frame;
    }
    if path.starts_with("docs/Concepts/") {
        return DocKind::Concept;
    }
    if path.starts_with("docs/Research/") {
        return DocKind::Research;
    }
    if let Some(rest) = path.strip_prefix("docs/Plans/") {
        if rest.starts_with("Plan.") {
            return DocKind::Plan;
        }
        if rest.starts_with("Idea.") {
            return DocKind::Idea;
        }
        if rest.starts_with("Design.") {
            return DocKind::Design;
        }
    }
    if path.starts_with("docs/Architecture/") {
        return DocKind::Architecture;
    }
    if let Some(rest) = path.strip_prefix("docs/Experiments/") {
        if rest.starts_with("Experiment.") {
            return DocKind::ExperimentSpec;
        }
        if rest.starts_with("Report.") {
            return DocKind::ExperimentReport;
        }
    }
    if path == "docs/DecisionLog.md" {
        return DocKind::Decision;
    }
    if path == "docs/EngineeringDiary.md" {
        return DocKind::Diary;
    }
    DocKind::Unknown
}

pub fn maturity_for(kind: DocKind, body: &str) -> MaturityTag {
    match kind {
        DocKind::Concept | DocKind::Architecture => {
            maturity_from_heading(body, "Maturity")
        }
        DocKind::Design => maturity_from_heading(body, "Status"),
        _ => MaturityTag::NotApplicable,
    }
}

fn maturity_from_heading(body: &str, heading: &str) -> MaturityTag {
    let pattern = format!(r"(?m)^##\s+{heading}\s*\n+\s*(\S+)");
    let regex = Regex::new(&pattern).expect("static regex");
    let Some(captures) = regex.captures(body) else {
        return MaturityTag::Unknown;
    };
    match captures.get(1).map(|m| m.as_str()) {
        Some("Brainstorm") => MaturityTag::Brainstorm,
        Some("Sketch") => MaturityTag::Sketch,
        Some("Candidate") => MaturityTag::Candidate,
        Some("Accepted") => MaturityTag::Accepted,
        Some("Implemented") => MaturityTag::Implemented,
        Some("Deprecated") => MaturityTag::Deprecated,
        _ => MaturityTag::Unknown,
    }
}

static LAST_REVIEWED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^Last reviewed:\s*(\d{4}-\d{2}-\d{2})\b").expect("static regex")
});

/// Returns the slice of `body` covering the `## Implementation Status`
/// section, from its heading up to the next `## ` heading (or end of doc).
fn implementation_status_section(body: &str) -> Option<&str> {
    let start = body.find("## Implementation Status")?;
    let after = &body[start..];
    // Find the next top-level section heading after this one.
    let end = after[1..]
        .find("\n## ")
        .map(|i| i + 1)
        .unwrap_or(after.len());
    Some(&after[..end])
}

pub fn last_reviewed_for(body: &str) -> Option<String> {
    let section = implementation_status_section(body)?;
    LAST_REVIEWED_RE
        .captures(section)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}
```

If broader fixture coverage is desired (one document per `DocKind`,
status section without a date), add those fixtures here; they are a
welcome extension but not required to pass this task's contract.

- [ ] **Step 5: Re-export from `mod.rs` and add `regex` / `once_cell`
  if needed.**

```rust
// crates/qsf_app/src/project_docs/mod.rs
pub mod metadata;
pub use metadata::{kind_for_path, last_reviewed_for, maturity_for};
```

Confirm `regex` and `once_cell` are in `Cargo.toml`; add if missing.

- [ ] **Step 6: Run tests; verify they pass.**

Run: `cargo test -p qsf_app project_docs::metadata`
Expected: PASS.

- [ ] **Step 7: Commit.**

```bash
git add crates/qsf_app/src/project_docs/metadata.rs \
        crates/qsf_app/src/project_docs/fixtures \
        crates/qsf_app/src/project_docs/mod.rs \
        crates/qsf_app/Cargo.toml Cargo.lock
git commit -m "feat(project_docs): metadata extraction rules with fixture coverage"
```

### Task 1.4: Lexical search

**Files:**
- Create: `crates/qsf_app/src/project_docs/search.rs`
- Create: `crates/qsf_app/src/project_docs/fixtures/sample_body_heavy.md`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

Searches the in-scope corpus for a query, returning up to `max_results`
`DocHit`s. Ranking is **heading-first**: any document whose query match
falls inside a `## ` heading line outranks every document matched only
in body text, regardless of body occurrence count. Within the same
heading-match tier, documents are ordered by occurrence count, then by
a deterministic path tiebreaker. Snippet extraction returns a ~200
token excerpt around the strongest match offset.

Production search walks the provided root with `walkdir` and filters
each discovered path through the allowlist; the fixture allowlist uses
`include=["**/*.md"]` so the tests exercise the whole fixtures
directory. `walkdir` yields clean, repo-relative paths, so search needs
no parent-directory-traversal guard — that guard lives in the bounded
read (Task 1.5), which is the only path that consumes a raw
caller-supplied string.

- [ ] **Step 1: Add the body-heavy fixture and write the failing tests.**

`sample_body_heavy.md` (many body occurrences of "maturity", but no
`## ` heading containing it — used to prove heading matches win even
against a high body count):

```markdown
# Notes

Some text mentioning maturity. maturity maturity maturity maturity here.
```

```rust
// crates/qsf_app/src/project_docs/search.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::Allowlist;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn fixture_allowlist() -> Allowlist {
        Allowlist::from_file(fixtures_root().join("allowlist_basic.toml")).unwrap()
    }

    #[test]
    fn heading_match_outranks_body_match() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "Maturity", 6).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].section_hint.as_deref(), Some("Maturity"));
        assert_eq!(hits[0].match_strength, crate::project_docs::MatchStrength::High);
    }

    #[test]
    fn heading_match_outranks_many_body_matches() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "maturity", 6).unwrap();
        // The body-heavy doc has the most raw occurrences but no heading match;
        // it must rank below any document matched in a `## Maturity` heading.
        let first_heading_rank = hits
            .iter()
            .position(|h| h.section_hint.as_deref() == Some("Maturity"))
            .expect("a heading match should exist");
        let body_heavy_rank = hits
            .iter()
            .position(|h| h.path.contains("sample_body_heavy"))
            .expect("body-heavy doc should appear");
        assert!(first_heading_rank < body_heavy_rank);
    }

    #[test]
    fn returns_unknown_kind_for_unstructured_doc() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "Unstructured", 6).unwrap();
        assert!(hits.iter().any(|h| h.kind == crate::project_docs::DocKind::Unknown));
    }

    #[test]
    fn empty_results_when_no_match() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "xyzzyno-such-token", 6).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn empty_query_returns_no_results() {
        let hits = search(&fixtures_root(), &fixture_allowlist(), "   ", 6).unwrap();
        assert!(hits.is_empty());
    }
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app project_docs::search`
Expected: FAIL (`search` undefined).

- [ ] **Step 3: Implement `search`.**

The match analyzer scans the document line by line, tracking the
current `## ` heading. A match inside a heading line is recorded as a
heading match and pins `section_hint` to that heading; otherwise the
first body match is kept along with its enclosing heading (if any). The
final sort is a heading-first ordering tuple, never a summed score.

```rust
// crates/qsf_app/src/project_docs/search.rs
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::metadata::{kind_for_path, last_reviewed_for, maturity_for};
use super::{Allowlist, DocHit, MatchStrength};

const SNIPPET_BYTES: usize = 800; // ~200 tokens

struct DocScore {
    heading_match: bool,
    occurrences: usize,
    hit: DocHit,
}

pub fn search(
    repo_root: &Path,
    allowlist: &Allowlist,
    query: &str,
    max_results: usize,
) -> Result<Vec<DocHit>> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored: Vec<DocScore> = Vec::new();
    for entry in WalkDir::new(repo_root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !allowlist.allows(&rel) {
            continue;
        }
        let body = fs::read_to_string(entry.path())
            .with_context(|| format!("read `{rel}`"))?;
        let Some(analysis) = analyze_matches(&body, &needle) else {
            continue;
        };
        let kind = kind_for_path(&rel);
        let maturity = maturity_for(kind, &body);
        let last_reviewed = last_reviewed_for(&body);
        let snippet = extract_snippet(&body, analysis.best_offset, SNIPPET_BYTES);
        scored.push(DocScore {
            heading_match: analysis.heading_match,
            occurrences: analysis.occurrences,
            hit: DocHit {
                path: rel,
                kind,
                maturity_tag: maturity,
                last_reviewed,
                snippet,
                section_hint: analysis.section_hint,
                match_strength: classify_strength(analysis.heading_match, analysis.occurrences),
            },
        });
    }

    // Heading-first: heading matches win outright, then occurrence count,
    // then a deterministic path tiebreaker for stable ordering.
    scored.sort_by(|a, b| {
        b.heading_match
            .cmp(&a.heading_match)
            .then_with(|| b.occurrences.cmp(&a.occurrences))
            .then_with(|| a.hit.path.cmp(&b.hit.path))
    });
    Ok(scored.into_iter().take(max_results).map(|s| s.hit).collect())
}

struct MatchAnalysis {
    best_offset: usize,
    section_hint: Option<String>,
    heading_match: bool,
    occurrences: usize,
}

fn analyze_matches(body: &str, needle: &str) -> Option<MatchAnalysis> {
    let occurrences = body.to_ascii_lowercase().matches(needle).count();
    if occurrences == 0 {
        return None;
    }

    let mut current_heading: Option<String> = None;
    let mut best_heading: Option<(usize, String)> = None;
    let mut best_body: Option<(usize, Option<String>)> = None;
    let mut offset = 0usize;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_heading = trimmed.starts_with("## ");
        if is_heading {
            current_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
        }
        if let Some(pos) = line.to_ascii_lowercase().find(needle) {
            let abs = offset + pos;
            if is_heading {
                if best_heading.is_none() {
                    best_heading = Some((abs, current_heading.clone().unwrap_or_default()));
                }
            } else if best_body.is_none() {
                best_body = Some((abs, current_heading.clone()));
            }
        }
        offset += line.len();
    }

    if let Some((abs, heading)) = best_heading {
        Some(MatchAnalysis {
            best_offset: abs,
            section_hint: Some(heading),
            heading_match: true,
            occurrences,
        })
    } else {
        let (abs, heading) = best_body.expect("occurrences > 0 guarantees a body match");
        Some(MatchAnalysis {
            best_offset: abs,
            section_hint: heading,
            heading_match: false,
            occurrences,
        })
    }
}

fn extract_snippet(body: &str, around: usize, byte_budget: usize) -> String {
    let bytes = body.as_bytes();
    let start = around.saturating_sub(byte_budget / 2);
    let end = (around + byte_budget / 2).min(bytes.len());
    let start = floor_char_boundary(body, start);
    let end = ceil_char_boundary(body, end);
    body[start..end].to_string()
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn classify_strength(heading_match: bool, occurrences: usize) -> MatchStrength {
    if heading_match {
        MatchStrength::High
    } else if occurrences >= 3 {
        MatchStrength::Medium
    } else {
        MatchStrength::Low
    }
}
```

- [ ] **Step 4: Add `walkdir` to `Cargo.toml` if absent; re-export
  `search` from `mod.rs`.**

```rust
// crates/qsf_app/src/project_docs/mod.rs
pub mod search;
pub use search::search;
```

- [ ] **Step 5: Run tests; verify they pass.**

Run: `cargo test -p qsf_app project_docs::search`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/qsf_app/src/project_docs/search.rs \
        crates/qsf_app/src/project_docs/fixtures/sample_body_heavy.md \
        crates/qsf_app/src/project_docs/mod.rs \
        crates/qsf_app/Cargo.toml Cargo.lock
git commit -m "feat(project_docs): lexical search with heading-first ranking"
```

### Task 1.5: Bounded read with focus

**Files:**
- Create: `crates/qsf_app/src/project_docs/read.rs`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

This is the only entry point that takes a raw, caller-supplied path, so
it owns the path-safety invariant for the phase. Before the allowlist
is consulted, the path is normalized: absolute paths and any `..`
component are rejected outright, `.` components are dropped, and the
result is a clean forward-slash repo-relative string. Only that
normalized string is passed to `allowlist.allows`, and only
`repo_root.join(normalized)` is read. This closes the traversal hole
where a string like `docs/ProjectFrame/../../EngineeringDiary.md` could
satisfy an include glob, miss the literal `docs/EngineeringDiary.md`
exclude, and then resolve — after filesystem normalization — to the
excluded file.

Budget accounting is a single incremental pass. A document's preamble
(everything before the first `## ` heading) and its "head" sections
(`## Status`, `## Implementation Status`) are always emitted and are
excluded from later reconsideration so they are never duplicated. Every
remaining section that does not fit the byte budget (or, for a focused
read, does not match the focus) is recorded in `omitted_sections`, and
`is_full` is true only when nothing was omitted.

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/project_docs/read.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{Allowlist, DocKind, MaturityTag};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn allow_all() -> Allowlist {
        Allowlist::from_str(r#"include=["**/*.md"] exclude=[]"#).unwrap()
    }

    #[test]
    fn reads_whole_doc_when_under_budget() {
        let doc = read(&fixtures_root(), &allow_all(), "sample_concept.md", None, 10_000).unwrap();
        assert!(doc.is_full);
        assert!(doc.omitted_sections.is_empty());
        assert_eq!(doc.kind, DocKind::Unknown); // fixture path is not under docs/Concepts
        assert_eq!(doc.maturity_tag, MaturityTag::NotApplicable);
    }

    #[test]
    fn focused_read_returns_named_section_plus_head() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_architecture.md",
            Some("Implementation Status"),
            10_000,
        )
        .unwrap();
        assert!(doc.content.contains("Implementation Status"));
        assert!(doc.content.contains("Last reviewed"));
    }

    #[test]
    fn head_section_is_not_duplicated() {
        let doc = read(
            &fixtures_root(),
            &allow_all(),
            "sample_architecture.md",
            None,
            10_000,
        )
        .unwrap();
        // The `## Implementation Status` head section must appear exactly once.
        assert_eq!(doc.content.matches("## Implementation Status").count(), 1);
    }

    #[test]
    fn refuses_path_outside_allowlist() {
        let allow_none = Allowlist::from_str(r#"include=[] exclude=[]"#).unwrap();
        let err = read(&fixtures_root(), &allow_none, "sample_concept.md", None, 10_000)
            .unwrap_err();
        assert!(err.to_string().contains("not in allowlist"));
    }

    #[test]
    fn refuses_parent_directory_traversal() {
        // Even though the allowlist admits `**/*.md`, a `..` path must be
        // rejected before it can escape the repo root.
        let err = read(
            &fixtures_root(),
            &allow_all(),
            "../../docs/EngineeringDiary.md",
            None,
            10_000,
        )
        .unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn refuses_absolute_path() {
        let abs = if cfg!(windows) { r"C:\Windows\system.ini" } else { "/etc/passwd" };
        let err = read(&fixtures_root(), &allow_all(), abs, None, 10_000).unwrap_err();
        assert!(err.to_string().contains("repo-relative"));
    }

    #[test]
    fn omitted_sections_populated_when_truncated() {
        // A tiny budget: the preamble + head section are always emitted, so any
        // trailing body section (`## Maturity`, `## Notes`) overflows and is
        // recorded as omitted, making the read non-full.
        let doc = read(&fixtures_root(), &allow_all(), "sample_architecture.md", None, 8).unwrap();
        assert!(!doc.is_full);
        assert!(!doc.omitted_sections.is_empty());
    }
}
```

- [ ] **Step 2: Implement `read`.**

```rust
// crates/qsf_app/src/project_docs/read.rs
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};

use super::metadata::{kind_for_path, last_reviewed_for, maturity_for};
use super::{Allowlist, DocRead};

const HEAD_HEADINGS: [&str; 2] = ["Status", "Implementation Status"];

pub fn read(
    repo_root: &Path,
    allowlist: &Allowlist,
    relative_path: &str,
    focus: Option<&str>,
    max_tokens: usize,
) -> Result<DocRead> {
    let normalized = normalize_repo_relative(relative_path)?;
    if !allowlist.allows(&normalized) {
        bail!("path `{normalized}` not in allowlist");
    }
    let body = fs::read_to_string(repo_root.join(&normalized))
        .with_context(|| format!("read `{normalized}`"))?;
    let kind = kind_for_path(&normalized);
    let maturity = maturity_for(kind, &body);
    let last_reviewed = last_reviewed_for(&body);

    let byte_budget = max_tokens.saturating_mul(4);
    let preamble = preamble_of(&body);
    let sections = split_sections(&body);

    let head_indices: Vec<usize> = sections
        .iter()
        .enumerate()
        .filter(|(_, s)| HEAD_HEADINGS.contains(&s.heading.as_str()))
        .map(|(i, _)| i)
        .collect();

    let mut content = String::new();
    content.push_str(&preamble);
    for &i in &head_indices {
        content.push_str(&sections[i].text);
    }

    let mut omitted: Vec<String> = Vec::new();
    let focus_needle = focus.map(|f| f.to_ascii_lowercase());

    for (i, section) in sections.iter().enumerate() {
        if head_indices.contains(&i) {
            continue;
        }
        let matches_focus = match &focus_needle {
            Some(needle) => {
                section.heading.to_ascii_lowercase().contains(needle)
                    || section.text.to_ascii_lowercase().contains(needle)
            }
            None => true,
        };
        if matches_focus && content.len() + section.text.len() <= byte_budget {
            content.push_str(&section.text);
        } else {
            omitted.push(section.heading.clone());
        }
    }

    let is_full = omitted.is_empty();

    Ok(DocRead {
        path: normalized,
        kind,
        maturity_tag: maturity,
        last_reviewed,
        content,
        is_full,
        omitted_sections: omitted,
    })
}

/// Confines a caller-supplied path to a clean, repo-relative,
/// forward-slash string. Rejects absolute paths and any `..` component
/// so the allowlist and the filesystem read can never escape the root.
fn normalize_repo_relative(path: &str) -> Result<String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        bail!("path `{path}` must be repo-relative");
    }
    let mut parts: Vec<&str> = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(c) => {
                parts.push(c.to_str().context("non-utf8 path component")?)
            }
            Component::CurDir => {}
            Component::ParentDir => bail!("path `{path}` must not contain `..`"),
            Component::Prefix(_) | Component::RootDir => {
                bail!("path `{path}` must be repo-relative")
            }
        }
    }
    if parts.is_empty() {
        bail!("path `{path}` must name a document");
    }
    Ok(parts.join("/"))
}

struct Section {
    heading: String,
    text: String,
}

/// Everything before the first `## ` heading: title line(s) and intro.
fn preamble_of(body: &str) -> String {
    let mut out = String::new();
    for line in body.lines() {
        if line.starts_with("## ") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn split_sections(body: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            sections.push(Section {
                heading: heading.trim().to_string(),
                text: String::new(),
            });
        }
        if let Some(last) = sections.last_mut() {
            last.text.push_str(line);
            last.text.push('\n');
        }
        // Lines before the first heading belong to the preamble and are
        // intentionally dropped here.
    }
    sections
}
```

- [ ] **Step 3: Re-export `read` from `mod.rs`.**

```rust
pub mod read;
pub use read::read;
```

- [ ] **Step 4: Run tests; verify they pass.**

Run: `cargo test -p qsf_app project_docs::read`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/project_docs/read.rs \
        crates/qsf_app/src/project_docs/mod.rs
git commit -m "feat(project_docs): bounded read with path confinement and section budgeting"
```

### Task 1.6: `ProjectDocService` facade

**Files:**
- Create: `crates/qsf_app/src/project_docs/service.rs`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

A small struct that holds the repo root and a hot-reloaded `Allowlist`.
Re-reads the config file per call.

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/project_docs/service.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn repo_root_for_tests() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    #[test]
    fn hot_reloads_allowlist_between_calls() {
        let tmp = TempDir::new().unwrap();
        let config = tmp.path().join("allowlist.toml");
        std::fs::write(&config, r#"include=["sample_concept.md"] exclude=[]"#).unwrap();

        let service =
            ProjectDocService::new(repo_root_for_tests(), config.clone());

        assert!(service.allowlist().unwrap().allows("sample_concept.md"));

        std::fs::write(&config, r#"include=[] exclude=[]"#).unwrap();
        assert!(!service.allowlist().unwrap().allows("sample_concept.md"));
    }
}
```

- [ ] **Step 2: Implement the facade.**

```rust
// crates/qsf_app/src/project_docs/service.rs
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{read, search, Allowlist, DocHit, DocRead};

pub struct ProjectDocService {
    repo_root: PathBuf,
    allowlist_path: PathBuf,
}

impl ProjectDocService {
    pub fn new(repo_root: impl Into<PathBuf>, allowlist_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            allowlist_path: allowlist_path.into(),
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn allowlist(&self) -> Result<Allowlist> {
        Allowlist::from_file(&self.allowlist_path)
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<DocHit>> {
        let allowlist = self.allowlist()?;
        search(&self.repo_root, &allowlist, query, max_results)
    }

    pub fn read(
        &self,
        relative_path: &str,
        focus: Option<&str>,
        max_tokens: usize,
    ) -> Result<DocRead> {
        let allowlist = self.allowlist()?;
        read(&self.repo_root, &allowlist, relative_path, focus, max_tokens)
    }
}
```

- [ ] **Step 3: Add `tempfile` as a dev-dependency if not already.**

- [ ] **Step 4: Re-export from `mod.rs`.**

```rust
pub mod service;
pub use service::ProjectDocService;
```

- [ ] **Step 5: Run tests.**

Run: `cargo test -p qsf_app project_docs::service`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/qsf_app/src/project_docs \
        crates/qsf_app/Cargo.toml Cargo.lock
git commit -m "feat(project_docs): ProjectDocService facade with hot-reloaded allowlist"
```

### Phase 1 verification

Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`. Expect
both clean.

**Diary note:** No diary entry is written in this phase; the diary
entry in Phase 9 covers the whole logical change (Phases 1-8). This
deferral is only safe if Phases 1-8 land as a single grouped feature
submission. If Phase 1 (the library slice) is merged or reviewed as a
standalone deliverable, add a short diary entry for it at that point,
per the diary discipline in `Agents.md`.

Acceptance criteria for Phase 1:

- `cargo test -p qsf_app project_docs` passes, covering allowlist
  include/exclude precedence, kind/maturity/last-reviewed extraction
  (including last-reviewed scoping to the Implementation Status section
  and malformed-date rejection), lexical search heading-first ranking
  and empty-result/empty-query handling, bounded read with focus and
  truncation, and service-level allowlist hot-reload.
- The bounded read rejects absolute paths and any `..` component before
  consulting the allowlist or touching the filesystem, with regression
  coverage proving a traversal toward `docs/EngineeringDiary.md` is
  refused even under an `**/*.md` allowlist.
- The production allowlist `config/project-doc-introspection.toml`
  loads (resolved from `CARGO_MANIFEST_DIR`, not the process working
  directory) and provably excludes `docs/EngineeringDiary.md` and
  `docs/Reviews/**` while admitting `docs/ProjectFrame/**` and
  `docs/DecisionLog.md` (covered by the Task 1.2 test).
- The `project_docs` module is pure and side-effect-free apart from
  reading files under the repo root; no tool, registry, dispatch, or
  responder wiring is introduced in this phase.
- `crates/qsf_app/src/lib.rs` remains a thin module-declaration
  wrapper — the new `pub mod project_docs;` line is the only change
  there.

---

## Phase 2: Tool implementations

Two `Tool` impls plus a `ToolPermission::read_only()` constructor and a
new `ToolContext` variant. No registry wiring yet — that lands in
Phase 3.

### Task 2.1: `ToolPermission::read_only()`

**Files:**
- Modify: `crates/qsf_app/src/tools/tool_request.rs`

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/tools/tool_request.rs (extend the existing test block,
// or add a new one if absent)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_permission_allows_read_only_tools() {
        let permission = ToolPermission::read_only();
        assert!(permission.allows(ToolCategory::ReadOnly, ToolSideEffectLevel::ReadOnly));
    }

    #[test]
    fn read_only_permission_rejects_write_tools() {
        let permission = ToolPermission::read_only();
        assert!(!permission.allows(ToolCategory::WriteCapable, ToolSideEffectLevel::ExternalWrite));
    }
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::tool_request`
Expected: FAIL (`read_only` not defined).

- [ ] **Step 3: Implement the constructor.**

Add to the existing `impl ToolPermission` block:

```rust
pub fn read_only() -> Self {
    Self {
        allowed_categories: vec![ToolCategory::ReadOnly],
        max_side_effect_level: ToolSideEffectLevel::ReadOnly,
    }
}
```

- [ ] **Step 4: Run tests.**

Run: `cargo test -p qsf_app tools::tool_request`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/tools/tool_request.rs
git commit -m "feat(tools): add ToolPermission::read_only constructor"
```

### Task 2.2: Project-doc `ToolContext`

**Files:**
- Create: `crates/qsf_app/src/tools/project_doc_tool.rs`
- Modify: `crates/qsf_app/src/tools/tool_registry.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs`

Extends the `ToolContext` trait with a `project_doc_service()` accessor
and provides a concrete `ProjectDocToolContext` analogous to
`SessionToolContext`.

- [ ] **Step 1: Extend the trait.**

In `crates/qsf_app/src/tools/tool_registry.rs`, add to `trait ToolContext`:

```rust
fn project_doc_service(&self) -> Option<&crate::project_docs::ProjectDocService> {
    None
}
```

(Default returns `None`, so existing `SessionToolContext` and
`EmptyToolContext` keep compiling.)

- [ ] **Step 2: Write the context impl.**

```rust
// crates/qsf_app/src/tools/project_doc_tool.rs
use crate::project_docs::ProjectDocService;

use super::tool_registry::ToolContext;

pub struct ProjectDocToolContext<'a> {
    pub service: &'a ProjectDocService,
}

impl<'a> ToolContext for ProjectDocToolContext<'a> {
    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        Some(self.service)
    }
}
```

- [ ] **Step 3: Re-export from `mod.rs`.**

Add `pub mod project_doc_tool;` and
`pub use project_doc_tool::ProjectDocToolContext;`.

- [ ] **Step 4: Build.**

Run: `cargo build`
Expected: builds clean.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/tools/project_doc_tool.rs \
        crates/qsf_app/src/tools/tool_registry.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): ProjectDocToolContext and ToolContext accessor"
```

### Task 2.3: `SearchProjectDocsTool`

**Files:**
- Create: `crates/qsf_app/src/tools/search_project_docs_tool.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs`

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/tools/search_project_docs_tool.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::ProjectDocService;
    use crate::tools::{ProjectDocToolContext, Tool, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn make_request(query: &str) -> ToolRequest {
        ToolRequest {
            tool_name: SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            input: query.to_string(),
            structured: Some(serde_json::json!({ "query": query, "max_results": 6 })),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        }
    }

    #[test]
    fn search_returns_hits_with_metadata() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let ctx = ProjectDocToolContext { service: &service };
        let tool = SearchProjectDocsTool;
        let request = make_request("Maturity");

        let result = tool.execute(&request, &ctx).unwrap();

        assert!(result.observation_summary.contains("hits"));
    }

    #[test]
    fn search_fails_without_project_doc_context() {
        let tool = SearchProjectDocsTool;
        let request = make_request("anything");
        let err = tool
            .execute(&request, &crate::tools::EmptyToolContext)
            .unwrap_err();
        assert!(err.to_string().contains("ProjectDocToolContext"));
    }
}
```

- [ ] **Step 2: Implement the tool.**

```rust
// crates/qsf_app/src/tools/search_project_docs_tool.rs
use anyhow::{Context, Result};
use serde_json::json;

use crate::models::ModelToolDefinition;

use super::tool_registry::{Tool, ToolContext, ToolMetadata};
use super::tool_request::{ToolCategory, ToolRequest, ToolSideEffectLevel};
use super::tool_result::ToolResult;

pub const SEARCH_PROJECT_DOCS_TOOL_NAME: &str = "search_project_docs";

const DEFAULT_MAX_RESULTS: usize = 6;

pub struct SearchProjectDocsTool;

impl Tool for SearchProjectDocsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: SEARCH_PROJECT_DOCS_TOOL_NAME,
            description: "Search project documentation for material related to a query.",
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let service = ctx
            .project_doc_service()
            .context("search_project_docs requires ProjectDocToolContext")?;
        let args = request
            .structured
            .as_ref()
            .context("search_project_docs requires structured arguments")?;
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .context("search_project_docs requires `query`")?;
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .min(DEFAULT_MAX_RESULTS);

        let hits = service.search(query, max_results)?;

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::to_string(&hits)?,
            numeric_value: None,
            observation_summary: format!(
                "search_project_docs returned {} hits for query `{}`.",
                hits.len(),
                query
            ),
        })
    }

    fn model_tool_definition(&self) -> Option<ModelToolDefinition> {
        Some(ModelToolDefinition::new(
            SEARCH_PROJECT_DOCS_TOOL_NAME,
            "Search project documentation. Returns ranked hits with kind and maturity metadata; \
             follow up with read_project_doc to read a focused excerpt.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 6 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ))
    }
}
```

- [ ] **Step 3: Re-export from `mod.rs`.**

Add `pub mod search_project_docs_tool;` and re-export the name and
struct.

- [ ] **Step 4: Run tests.**

Run: `cargo test -p qsf_app tools::search_project_docs_tool`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/tools/search_project_docs_tool.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): SearchProjectDocsTool"
```

### Task 2.4: `ReadProjectDocTool`

**Files:**
- Create: `crates/qsf_app/src/tools/read_project_doc_tool.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs`

- [ ] **Step 1: Write the failing tests.**

The out-of-allowlist test uses a non-markdown path (`outside.txt`),
which normalizes cleanly but is not admitted by the fixture allowlist's
`**/*.md` include, so it exercises the "not in allowlist" branch.
Parent-directory traversal rejection is covered by the Phase 1 read
tests (Task 1.5), so it is not re-tested here.

```rust
// crates/qsf_app/src/tools/read_project_doc_tool.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::ProjectDocService;
    use crate::tools::{ProjectDocToolContext, Tool, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn make_request(path: &str, focus: Option<&str>, max_tokens: u64) -> ToolRequest {
        let mut args = serde_json::json!({ "path": path, "max_tokens": max_tokens });
        if let Some(f) = focus {
            args["focus"] = serde_json::Value::String(f.to_string());
        }
        ToolRequest {
            tool_name: READ_PROJECT_DOC_TOOL_NAME.to_string(),
            input: format!("read {path}"),
            structured: Some(args),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        }
    }

    #[test]
    fn read_returns_doc_content() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let ctx = ProjectDocToolContext { service: &service };
        let tool = ReadProjectDocTool;
        let request = make_request("sample_concept.md", None, 10_000);

        let result = tool.execute(&request, &ctx).unwrap();

        assert!(result.output_text.contains("Concept: Sample"));
    }

    #[test]
    fn read_refuses_out_of_allowlist() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let ctx = ProjectDocToolContext { service: &service };
        let tool = ReadProjectDocTool;
        // Normalizes cleanly but is not a `*.md` file, so the allowlist refuses it.
        let request = make_request("outside.txt", None, 10_000);

        let err = tool.execute(&request, &ctx).unwrap_err();
        assert!(err.to_string().contains("not in allowlist"));
    }
}
```

- [ ] **Step 2: Implement the tool.**

```rust
// crates/qsf_app/src/tools/read_project_doc_tool.rs
use anyhow::{Context, Result};
use serde_json::json;

use crate::models::ModelToolDefinition;

use super::tool_registry::{Tool, ToolContext, ToolMetadata};
use super::tool_request::{ToolCategory, ToolRequest, ToolSideEffectLevel};
use super::tool_result::ToolResult;

pub const READ_PROJECT_DOC_TOOL_NAME: &str = "read_project_doc";

const DEFAULT_MAX_TOKENS_FOCUSED: usize = 1200;
const DEFAULT_MAX_TOKENS_NO_FOCUS: usize = 2400;

pub struct ReadProjectDocTool;

impl Tool for ReadProjectDocTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: READ_PROJECT_DOC_TOOL_NAME,
            description: "Read a focused excerpt or bounded slice of a project document.",
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let service = ctx
            .project_doc_service()
            .context("read_project_doc requires ProjectDocToolContext")?;
        let args = request
            .structured
            .as_ref()
            .context("read_project_doc requires structured arguments")?;
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .context("read_project_doc requires `path`")?;
        let focus = args.get("focus").and_then(|v| v.as_str());
        let default_budget = if focus.is_some() {
            DEFAULT_MAX_TOKENS_FOCUSED
        } else {
            DEFAULT_MAX_TOKENS_NO_FOCUS
        };
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(default_budget);

        let doc = service.read(path, focus, max_tokens)?;
        let observation = format!(
            "read_project_doc returned {} bytes from `{}` (is_full={}, omitted_sections={}).",
            doc.content.len(),
            doc.path,
            doc.is_full,
            doc.omitted_sections.len()
        );

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: serde_json::to_string(&doc)?,
            numeric_value: None,
            observation_summary: observation,
        })
    }

    fn model_tool_definition(&self) -> Option<ModelToolDefinition> {
        Some(ModelToolDefinition::new(
            READ_PROJECT_DOC_TOOL_NAME,
            "Read a focused excerpt or bounded slice of a project document, with kind and \
             maturity metadata. Use after search_project_docs.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "focus": { "type": "string" },
                    "max_tokens": { "type": "integer", "minimum": 100, "maximum": 4000 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ))
    }
}
```

- [ ] **Step 3: Re-export and run tests.**

Add to `crates/qsf_app/src/tools/mod.rs`:

```rust
pub mod read_project_doc_tool;
pub use read_project_doc_tool::{READ_PROJECT_DOC_TOOL_NAME, ReadProjectDocTool};
```

Run: `cargo test -p qsf_app tools::read_project_doc_tool`
Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/tools/read_project_doc_tool.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): ReadProjectDocTool"
```

### Phase 2 verification

Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`. Expect
clean.

---

## Phase 3: `ToolRegistry` wiring

Extend the hand-coded registry to dispatch the two new tools. Per
`Agents.md`, keep shared constants DRY — the names already live in
their respective tool modules; the registry imports them.

### Task 3.1: Extend the registry

**Files:**
- Modify: `crates/qsf_app/src/tools/tool_registry.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs` (re-exports)

- [ ] **Step 1: Write the failing test.**

```rust
// add to crates/qsf_app/src/tools/tool_registry.rs tests
#[test]
fn registry_exposes_project_doc_tools() {
    let registry = ToolRegistry::default();
    let defs = registry.model_tool_definitions_for(&[
        crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
        crate::tools::READ_PROJECT_DOC_TOOL_NAME,
    ]);
    assert_eq!(defs.len(), 2);
}

#[test]
fn registry_metadata_for_project_doc_tools() {
    let registry = ToolRegistry::default();
    assert!(registry
        .metadata_for(crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME)
        .is_some());
    assert!(registry
        .metadata_for(crate::tools::READ_PROJECT_DOC_TOOL_NAME)
        .is_some());
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::tool_registry::tests::registry_exposes_project_doc_tools`

Expected: FAIL.

- [ ] **Step 3: Implement the extension.**

In `crates/qsf_app/src/tools/tool_registry.rs`:

```rust
use super::read_project_doc_tool::{READ_PROJECT_DOC_TOOL_NAME, ReadProjectDocTool};
use super::search_project_docs_tool::{SEARCH_PROJECT_DOCS_TOOL_NAME, SearchProjectDocsTool};
```

Extend the struct:

```rust
pub struct ToolRegistry {
    calculator: CalculatorTool,
    recall_turn: super::RecallTurnTool,
    search_project_docs: SearchProjectDocsTool,
    read_project_doc: ReadProjectDocTool,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            calculator: CalculatorTool,
            recall_turn: super::RecallTurnTool,
            search_project_docs: SearchProjectDocsTool,
            read_project_doc: ReadProjectDocTool,
        }
    }
}
```

Extend each match arm in `metadata_for`, `dispatch`, and
`model_tool_definitions_for` to route the two new names.

- [ ] **Step 4: Re-export the constants from `tools/mod.rs`.**

```rust
pub use search_project_docs_tool::{SEARCH_PROJECT_DOCS_TOOL_NAME, SearchProjectDocsTool};
pub use read_project_doc_tool::{READ_PROJECT_DOC_TOOL_NAME, ReadProjectDocTool};
```

- [ ] **Step 5: Run tests.**

Run: `cargo test -p qsf_app tools::tool_registry`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/qsf_app/src/tools/tool_registry.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): wire project-doc tools into ToolRegistry"
```

---

## Phase 4: Per-turn dispatch caps

`dispatch_model_tool_calls` currently iterates the batch and runs each
call unconditionally. Extend it to track how many `search_project_docs`
and `read_project_doc` calls a single batch (= one turn) has consumed,
and to fail the excess calls fast — with a `ToolFailed` event and a
`TraceRecord` recording the refusal — instead of running them.

### Task 4.1: Cap enforcement

**Files:**
- Modify: `crates/qsf_app/src/models/tool_dispatch.rs`

Caps per turn:
- `search_project_docs`: 2
- `read_project_doc`: 1

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/models/tool_dispatch.rs (extend or create the test block)
#[cfg(test)]
mod project_doc_cap_tests {
    use super::*;
    use crate::tools::{READ_PROJECT_DOC_TOOL_NAME, SEARCH_PROJECT_DOCS_TOOL_NAME, ToolRegistry};
    use crate::models::{ModelRole, ModelRoleId, ModelRequest, ModelToolCall};
    // exact setup helpers depend on the existing test harness in this file;
    // mirror the pattern used by any existing tool_dispatch tests.

    #[test]
    fn third_search_call_in_one_batch_is_refused() {
        // Build a ModelRequest whose role advertises both project-doc tools.
        // Emit three search calls. Expect the first two to succeed and the
        // third to produce a ToolFailed event with refusal_reason
        // "per_turn_cap" and a TraceRecord with refused = true.
        // Implementation of helpers: follow the existing patterns in this file.
        // Assertions:
        //   - results length == 3
        //   - third result observation_summary contains "per_turn_cap"
        //   - last ToolFailed event in context has "refusal_reason": "per_turn_cap"
    }

    #[test]
    fn second_read_call_in_one_batch_is_refused() {
        // Same shape, two read_project_doc calls.
    }
}
```

(The two tests are placeholders for the engineer; concrete fixture
setup mirrors the existing test patterns in the same file. If the file
has no existing test infrastructure yet, write a focused integration
test under `crates/qsf_app/tests/` that builds a `RunContext`, a
`ModelRequest`, a `ToolRegistry`, and a `ProjectDocToolContext`, then
calls `dispatch_model_tool_calls` directly.)

- [ ] **Step 2: Run tests; verify they fail.**

Expected: FAIL.

- [ ] **Step 3: Implement the cap.**

Inside `dispatch_model_tool_calls`, before the per-tool dispatch:

```rust
let mut search_count = 0usize;
let mut read_count = 0usize;
const SEARCH_CAP: usize = 2;
const READ_CAP: usize = 1;

for tool_call in tool_calls {
    // ... existing allowed_tools check ...

    let over_cap = match tool_call.name.as_str() {
        SEARCH_PROJECT_DOCS_TOOL_NAME => {
            search_count += 1;
            search_count > SEARCH_CAP
        }
        READ_PROJECT_DOC_TOOL_NAME => {
            read_count += 1;
            read_count > READ_CAP
        }
        _ => false,
    };

    if over_cap {
        let reason = "per_turn_cap";
        context.record_event(
            EventType::ToolFailed,
            json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "tool_name": &tool_call.name,
                "call_id": &tool_call.call_id,
                "error": "per-turn budget exhausted",
                "refusal_reason": reason,
            }),
            None,
        )?;
        context.record_trace(
            TraceRecord::new(
                context.experiment_id(),
                if tool_call.name == SEARCH_PROJECT_DOCS_TOOL_NAME {
                    "project_doc_search"
                } else {
                    "project_doc_read"
                },
                "(refused)",
                "per_turn_cap",
            )
            .with_details(json!({
                "refused": true,
                "refusal_reason": reason,
                "role_id": request.role.role_id,
            })),
        )?;
        results.push(ToolResult {
            tool_name: tool_call.name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: String::new(),
            output_text: String::new(),
            numeric_value: None,
            observation_summary: format!(
                "{} refused: per_turn_cap (max {} calls per turn).",
                tool_call.name,
                if tool_call.name == SEARCH_PROJECT_DOCS_TOOL_NAME {
                    SEARCH_CAP
                } else {
                    READ_CAP
                }
            ),
        });
        continue;
    }

    // ... existing tool_request_from_model_tool_call + dispatch path ...
}
```

Imports to add to the file:

```rust
use crate::observability::trace::TraceRecord;
use crate::tools::{
    READ_PROJECT_DOC_TOOL_NAME, SEARCH_PROJECT_DOCS_TOOL_NAME, ToolCategory, ToolSideEffectLevel,
};
```

- [ ] **Step 4: Run tests.**

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/models/tool_dispatch.rs
git commit -m "feat(dispatch): enforce per-turn caps for project-doc tools"
```

---

## Phase 5: TraceRecord emission for successful project-doc calls

Phase 4 added refusal traces. This phase adds traces for the *successful*
search and read paths, so a researcher can replay every call.

### Task 5.1: Emit success traces

**Files:**
- Modify: `crates/qsf_app/src/models/tool_dispatch.rs`

In the success path of the dispatch loop, after the
`ToolCompleted` event is written, emit a `TraceRecord` for the two
project-doc operations. Calculator and recall_turn continue to behave
as today.

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/models/tool_dispatch.rs (tests)
#[test]
fn successful_search_emits_project_doc_search_trace() {
    // Run one search_project_docs call through dispatch_model_tool_calls.
    // Read the trace artifact (via RunContext's trace writer, or by
    // capturing into a Vec<TraceRecord> in test harness mode).
    // Assert there is a TraceRecord with operation == "project_doc_search"
    // and details containing the hits count.
}

#[test]
fn successful_read_emits_project_doc_read_trace() {
    // Same shape for read_project_doc.
}
```

- [ ] **Step 2: Implement the emission.**

After the existing `ToolCompleted` event write in
`dispatch_model_tool_calls`, branch on tool name:

```rust
match tool_request.tool_name.as_str() {
    SEARCH_PROJECT_DOCS_TOOL_NAME => {
        let parsed_hits: serde_json::Value =
            serde_json::from_str(&result.output_text).unwrap_or_else(|_| json!([]));
        let hit_count = parsed_hits.as_array().map(|a| a.len()).unwrap_or(0);
        context.record_trace(
            TraceRecord::new(
                context.experiment_id(),
                "project_doc_search",
                tool_call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                format!("{hit_count} hit(s)"),
            )
            .with_details(json!({
                "role_id": request.role.role_id,
                "hits": parsed_hits,
                "refused": false,
            }))
            .with_latency_ms(tool_latency_ms),
        )?;
    }
    READ_PROJECT_DOC_TOOL_NAME => {
        let parsed: serde_json::Value =
            serde_json::from_str(&result.output_text).unwrap_or_else(|_| json!({}));
        context.record_trace(
            TraceRecord::new(
                context.experiment_id(),
                "project_doc_read",
                tool_call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                parsed
                    .get("is_full")
                    .map(|v| format!("is_full={v}"))
                    .unwrap_or_else(|| "?".to_string()),
            )
            .with_details(json!({
                "role_id": request.role.role_id,
                "focus": tool_call.arguments.get("focus"),
                "max_tokens": tool_call.arguments.get("max_tokens"),
                "is_full": parsed.get("is_full"),
                "omitted_sections": parsed.get("omitted_sections"),
                "refused": false,
            }))
            .with_latency_ms(tool_latency_ms),
        )?;
    }
    _ => {}
}
```

The success traces complement the `ToolCompleted` event; they do not
replace it.

- [ ] **Step 3: Run tests.**

Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/models/tool_dispatch.rs
git commit -m "feat(dispatch): emit project_doc_search/read trace records on success"
```

---

## Phase 6: Wire the responder role

Adds the two tools to the `ConversationalResponder` allowed-tools list
used by the multi-turn text loop (and, by extension, the unified
text/voice path once that lands), and adds the always-on prompt block
that teaches the model when and how to use them.

### Task 6.1: Extend `allowed_tools` for the responder

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
  (and any other call site that constructs a `ConversationalResponder`
  request with explicit `allowed_tools` — grep for
  `allowed_tools` to find them all).

- [ ] **Step 1: Grep for current advertising patterns.**

```bash
grep -rn "allowed_tools" crates/qsf_app/src
```

Identify every call site that builds a request for the responder.
The multi-turn loop currently advertises `calculator` and
`recall_turn`; extend each such list to include
`SEARCH_PROJECT_DOCS_TOOL_NAME` and `READ_PROJECT_DOC_TOOL_NAME`.

- [ ] **Step 2: Write a test confirming the responder advertises the
  tools.**

```rust
// in the appropriate experiments test module, or a new one
#[test]
fn responder_advertises_project_doc_tools() {
    let role = build_conversational_responder_with_tools();
    assert!(role
        .allowed_tools
        .iter()
        .any(|n| n == crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME));
    assert!(role
        .allowed_tools
        .iter()
        .any(|n| n == crate::tools::READ_PROJECT_DOC_TOOL_NAME));
}
```

- [ ] **Step 3: Update the call site(s).**

Extend each existing `vec![...]` of tool names to include the two new
constants. Keep the constants imported from `crate::tools`.

- [ ] **Step 4: Run tests.**

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/experiments
git commit -m "feat(responder): advertise project-doc tools in multi-turn loop"
```

### Task 6.2: Always-on prompt block

**Files:**
- Modify: `crates/qsf_app/src/conversation/prompt.rs` (or whichever
  module assembles the system prompt for the responder; grep for
  `ConversationalResponder` and `system` to find it).

The block is the verbatim text from `Design.ProjectDocIntrospection.md`
Decision 3, *Voicing prompt*. It is appended to the responder's system
prompt whenever the responder advertises the two tools.

- [ ] **Step 1: Add a constant.**

```rust
// in crates/qsf_app/src/conversation/prompt.rs (or new sibling module)
pub const PROJECT_DOC_INTROSPECTION_PROMPT: &str = "\
You can consult the project's own documents to ground questions about \
Qualia Signal Foundry. Use search_project_docs to find relevant material, \
then read_project_doc to pull a focused excerpt or a bounded slice from \
the most promising one.\n\
\n\
Every result carries a kind (Frame, Concept, Research, Plan, Idea, Design, \
Architecture, ExperimentSpec, ExperimentReport, Decision, Diary, or \
Unknown) and, where applicable, a maturity tag (Brainstorm, Sketch, \
Candidate, Accepted, Implemented, Deprecated, or Unknown).\n\
\n\
Attribute lightly in your reply, using kind and maturity to hedge:\n\
  - \"The project's accepted framing says...\"         (Frame, or Accepted Concept)\n\
  - \"An accepted decision records that...\"           (DecisionLog entry)\n\
  - \"There's a candidate architecture sketch for...\" (Candidate Architecture)\n\
  - \"A brainstorm idea explores...\"                  (Idea, or Brainstorm Concept)\n\
  - \"I found a document but couldn't classify it...\" (Unknown kind or maturity)\n\
\n\
Do not claim current behavior from a Plan, Idea, or Concept; those describe \
intent. Source code is the only authority for what runs today, and is not \
available to this channel. If a read was truncated or limited to a single \
section, mention that. When nothing relevant comes back, or when the \
metadata is Unknown, say so plainly rather than improvising.";
```

- [ ] **Step 2: Append the block when the responder has the tools.**

In the prompt-assembly path that builds the responder's system message,
append `PROJECT_DOC_INTROSPECTION_PROMPT` if (and only if) the role's
`allowed_tools` contains both `SEARCH_PROJECT_DOCS_TOOL_NAME` and
`READ_PROJECT_DOC_TOOL_NAME`. Conditioning on tool presence keeps the
prompt block out of contexts where it would be misleading.

- [ ] **Step 3: Write a test.**

```rust
#[test]
fn responder_system_prompt_includes_introspection_block_when_tools_present() {
    let role = role_with_project_doc_tools();
    let prompt = build_system_prompt(&role, /* other args */);
    assert!(prompt.contains("search_project_docs"));
    assert!(prompt.contains("kind and maturity"));
}

#[test]
fn responder_system_prompt_omits_block_when_tools_absent() {
    let role = role_without_project_doc_tools();
    let prompt = build_system_prompt(&role, /* other args */);
    assert!(!prompt.contains("search_project_docs"));
}
```

- [ ] **Step 4: Run tests.**

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/conversation
git commit -m "feat(prompt): append project-doc voicing block when tools advertised"
```

### Phase 6 verification

Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`. At
this point the responder can call the tools end-to-end against the
real `docs/` tree. A short manual smoke test (run the multi-turn
text loop, ask "what are you?") is optional here; the full battery
arrives in Phase 7.

---

## Phase 7: Self-question battery fixture test

A small structured offline test that exercises the responder with a
fixed list of self-questions and asserts on the calls made and the
hedging language used. Runs as a normal `cargo test` so it is part of
CI.

### Task 7.1: Battery fixture and harness

**Files:**
- Create: `crates/qsf_app/tests/project_doc_self_question_battery.rs`
- Create:
  `crates/qsf_app/tests/fixtures/self_question_battery.json`

The harness uses a mock provider (mirror the existing `MockResponder`
test pattern) to produce predetermined tool calls and replies, then
asserts on the recorded events and traces. The intent is to verify
plumbing and voicing rules, not to test the model's natural-language
choices.

- [ ] **Step 1: Encode the battery.**

```json
{
  "questions": [
    {
      "id": "what_are_you",
      "prompt": "What are you?",
      "expected_calls": [{ "tool": "search_project_docs", "query_contains": "vision" }],
      "expected_reply_contains": ["accepted framing"]
    },
    {
      "id": "sleep_phase_implemented",
      "prompt": "Is the sleep phase implemented?",
      "expected_calls": [
        { "tool": "search_project_docs", "query_contains": "sleep" },
        { "tool": "read_project_doc", "path_contains": "Architecture.SleepPhase.md" }
      ],
      "expected_reply_must_not_contain": ["I do", "I have"],
      "expected_reply_contains": ["the project"]
    },
    {
      "id": "goal_system",
      "prompt": "Tell me about the goal system.",
      "expected_calls": [{ "tool": "search_project_docs", "query_contains": "goal" }],
      "expected_reply_contains": ["brainstorm"]
    },
    {
      "id": "off_topic",
      "prompt": "What's the capital of France?",
      "expected_calls": [],
      "expected_reply_must_not_contain": ["search_project_docs"]
    }
  ]
}
```

- [ ] **Step 2: Write the harness.**

```rust
// crates/qsf_app/tests/project_doc_self_question_battery.rs
//! Offline self-question battery for the project-doc introspection channel.
//!
//! Replays a fixed list of self-questions against a stubbed responder that
//! emits predetermined tool calls, then asserts on the recorded events and
//! traces and on the hedging language present in the final reply.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Battery {
    questions: Vec<Question>,
}

#[derive(Debug, Deserialize)]
struct Question {
    id: String,
    prompt: String,
    expected_calls: Vec<ExpectedCall>,
    #[serde(default)]
    expected_reply_contains: Vec<String>,
    #[serde(default)]
    expected_reply_must_not_contain: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedCall {
    tool: String,
    #[serde(default)]
    query_contains: Option<String>,
    #[serde(default)]
    path_contains: Option<String>,
}

#[test]
fn battery_runs_against_stubbed_responder() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/self_question_battery.json");
    let raw = std::fs::read_to_string(&path).unwrap();
    let battery: Battery = serde_json::from_str(&raw).unwrap();

    for question in &battery.questions {
        let outcome = run_question_through_stubbed_responder(&question.prompt);

        assert_eq!(
            outcome.calls.len(),
            question.expected_calls.len(),
            "question {}: expected {} calls, got {}",
            question.id,
            question.expected_calls.len(),
            outcome.calls.len()
        );

        for (expected, actual) in question.expected_calls.iter().zip(&outcome.calls) {
            assert_eq!(actual.tool, expected.tool, "question {}", question.id);
            if let Some(needle) = &expected.query_contains {
                let query = actual.arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
                assert!(
                    query.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()),
                    "question {}: query `{query}` missing `{needle}`",
                    question.id
                );
            }
            if let Some(needle) = &expected.path_contains {
                let path = actual.arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
                assert!(path.contains(needle), "question {}", question.id);
            }
        }

        for needle in &question.expected_reply_contains {
            assert!(
                outcome.reply.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()),
                "question {}: reply missing `{needle}`",
                question.id
            );
        }
        for forbidden in &question.expected_reply_must_not_contain {
            assert!(
                !outcome.reply.contains(forbidden),
                "question {}: reply contained forbidden `{forbidden}`",
                question.id
            );
        }
    }
}

// run_question_through_stubbed_responder is implemented in this same file
// using a small stub model client. Implementation mirrors the test patterns
// in crates/qsf_app/src/models/openai_tool_client.rs which already use
// MockResponder for deterministic outputs.
```

The stub model client is the work of the task — it should issue the
expected tool calls for each prompt and produce a canned reply that
exercises the assertions. Use the existing `MockResponder`
infrastructure as a starting point.

- [ ] **Step 3: Run the battery.**

Run: `cargo test -p qsf_app --test project_doc_self_question_battery`
Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/tests/project_doc_self_question_battery.rs \
        crates/qsf_app/tests/fixtures
git commit -m "test(project_docs): self-question battery against stubbed responder"
```

---

## Phase 8: `influenced_reply` post-hoc enrichment

A small, deterministic pass that joins each `project_doc_*` trace
record in a run's `traces.jsonl` to the same-turn final assistant
reply and writes a follow-up `TraceRecord` (operation =
`project_doc_influence`) marking whether the reply substantively
overlapped the returned content.

### Task 8.1: Overlap check

**Files:**
- Create: `crates/qsf_app/src/project_docs/influence.rs`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/project_docs/influence.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_reply_is_marked_influenced() {
        let excerpt = "The project's accepted framing says X about Y.";
        let reply = "Well, X about Y is what the project's framing says.";
        assert!(reply_overlaps_excerpt(reply, excerpt));
    }

    #[test]
    fn unrelated_reply_is_not_influenced() {
        let excerpt = "The project's accepted framing says X about Y.";
        let reply = "The capital of France is Paris.";
        assert!(!reply_overlaps_excerpt(reply, excerpt));
    }
}
```

- [ ] **Step 2: Implement the check.**

```rust
// crates/qsf_app/src/project_docs/influence.rs
//! Best-effort overlap check used to mark whether a tool-returned excerpt
//! influenced the final assistant reply. False negatives are acceptable;
//! false positives are guarded against by requiring multi-word overlap.

const MIN_NGRAM_SIZE: usize = 4;

pub fn reply_overlaps_excerpt(reply: &str, excerpt: &str) -> bool {
    let reply_lower = reply.to_ascii_lowercase();
    let words: Vec<&str> = excerpt
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .collect();
    if words.len() < MIN_NGRAM_SIZE {
        return false;
    }
    words.windows(MIN_NGRAM_SIZE).any(|window| {
        let phrase = window.join(" ").to_ascii_lowercase();
        reply_lower.contains(&phrase)
    })
}
```

- [ ] **Step 3: Re-export and run tests.**

```rust
// crates/qsf_app/src/project_docs/mod.rs
pub mod influence;
pub use influence::reply_overlaps_excerpt;
```

Run: `cargo test -p qsf_app project_docs::influence`
Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/project_docs/influence.rs \
        crates/qsf_app/src/project_docs/mod.rs
git commit -m "feat(project_docs): post-hoc reply-overlap check"
```

### Task 8.2: Enrichment writer

**Files:**
- Create: `crates/qsf_app/src/project_docs/enrichment.rs`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

A function that, given a run's `traces.jsonl` path, reads the trace
records, pairs each `project_doc_*` operation with the same-turn final
assistant reply, computes the overlap signal, and appends new
`project_doc_influence` records.

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/project_docs/enrichment.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn enrichment_appends_influence_records() {
        let mut file = NamedTempFile::new().unwrap();
        // Write two records:
        //   1. project_doc_search with hits containing an excerpt
        //   2. assistant reply trace that quotes the excerpt
        // (Schema: full TraceRecord JSON lines)
        // Call enrich(file.path()).
        // Re-read; assert a project_doc_influence record was appended
        // with details.influenced_reply = true.
    }
}
```

- [ ] **Step 2: Implement `enrich`.**

The implementation reads `traces.jsonl` line by line, parses each
`TraceRecord`, groups them by `turn_id` (carried in `details.role_id`
or equivalent — confirm against the actual trace shape during
implementation), pairs each `project_doc_*` record with the final
`assistant_reply` trace in the same turn, computes
`reply_overlaps_excerpt`, and appends one
`project_doc_influence` record per pair.

This is plumbing work whose precise shape depends on existing trace
conventions; follow the pattern of any other post-hoc analysis tool
already in `crates/qsf_app/src/`. Surface naming choices as open
questions if existing conventions are unclear.

- [ ] **Step 3: Run tests.**

Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/project_docs/enrichment.rs \
        crates/qsf_app/src/project_docs/mod.rs
git commit -m "feat(project_docs): traces.jsonl post-hoc influenced_reply enrichment"
```

---

## Phase 9: Documentation updates

Per `Agents.md` and `docs/ProjectFrame/ProjectWorkflow.md`. These are
documentation changes only; no application code changes. Per the
diary discipline, a diary entry covers the *application* work from
Phases 1-8; this phase does not need its own diary entry beyond that.

### Task 9.1: Update the brainstorm idea doc

**Files:**
- Modify: `docs/Plans/Idea.SelfReflectionProjectIntrospection.md`

Add a short pointer near the top:

> The documentation-introspection slice of this idea is now in design at
> `docs/Plans/Design.ProjectDocIntrospection.md` and implementation at
> `docs/Plans/Plan.ProjectDocIntrospection.md`. The rest of this
> document is preserved as future-scope brainstorm.

- [ ] Commit.

```bash
git add docs/Plans/Idea.SelfReflectionProjectIntrospection.md
git commit -m "docs(idea): point self-reflection idea at design and plan"
```

### Task 9.2: Record the decision

**Files:**
- Modify: `docs/DecisionLog.md`

Add a new entry:

```text
Decision:
  Project-doc introspection v1 is framed-self only, exposed to the
  ConversationalResponder role only, with no source-code access, no
  write effects, and a default allowlist that excludes
  docs/Reviews/** and docs/EngineeringDiary.md.

Context:
  Self-reflection design (`docs/Plans/Design.ProjectDocIntrospection.md`)
  and review (`docs/Reviews/Review.ProjectDocIntrospectionDesign.md`).

Consequences:
  Active-self, episodic-self, pattern-self, meta-memory, source-code,
  write-capable, and non-live-role introspection are deferred to
  follow-on designs.
```

- [ ] Commit.

```bash
git add docs/DecisionLog.md
git commit -m "docs(decision): commit project-doc introspection v1 scope"
```

### Task 9.3: Refresh ToolSystem Implementation Status

**Files:**
- Modify: `docs/Architecture/Architecture.ToolSystem.md`

Move `search_project_docs` and `read_project_doc` from "Not yet
implemented" to "Implemented today" with code-module refs to
`crates/qsf_app/src/tools/search_project_docs_tool.rs` and
`crates/qsf_app/src/tools/read_project_doc_tool.rs`. Refresh
`Last reviewed:` to today's date.

- [ ] Commit.

```bash
git add docs/Architecture/Architecture.ToolSystem.md
git commit -m "docs(architecture): mark project-doc tools implemented"
```

### Task 9.4: Pointer in DocumentStatus, and record the deferred latency cap

**Files:**
- Modify: `docs/ProjectFrame/DocumentStatus.md`
- Modify: `docs/Plans/Design.ProjectDocIntrospection.md`

In `DocumentStatus.md`'s *Implications For Introspection* section, add a
one-line pointer:

> The set of documents accessible to the introspection channel is
> defined by `config/project-doc-introspection.toml`.

In `Design.ProjectDocIntrospection.md` Decision 4, add a one-line note
recording that the 1500 ms hard cap is **deliberately not enforced in
v1** (lexical search over a small markdown corpus), and that if it is
ever needed it will be added at the `ProjectDocService` boundary as a
deadline/budget parameter with partial-result reporting. This keeps the
design and the implementation in agreement per Open Question #5.

- [ ] Commit.

```bash
git add docs/ProjectFrame/DocumentStatus.md docs/Plans/Design.ProjectDocIntrospection.md
git commit -m "docs(frame): pointer to allowlist; record deferred latency cap"
```

### Task 9.5: Engineering diary entry

**Files:**
- Modify: `docs/EngineeringDiary.md`

Per the *Instructions how to use* at the top of the diary, add one
entry at the end of the file covering the application work landed in
Phases 1-8. Keep it short, reference concrete artifacts, do not
reference planning documents.

Template:

```markdown
## YYYY-MM-DD - Project-doc introspection channel

The `ConversationalResponder` can now call `search_project_docs` and
`read_project_doc` mid-dialogue to ground self-questions in actual
project material, with per-turn budget enforcement, kind/maturity
hedging, and trace records.

What changed:
- New `project_docs` module: allowlist loader, metadata extraction,
  lexical search, bounded read (path-confined against traversal),
  post-hoc reply-overlap check.
- New tools `search_project_docs` and `read_project_doc` wired into
  `ToolRegistry`.
- `dispatch_model_tool_calls` enforces per-turn caps (2 search, 1 read)
  and emits `project_doc_search` / `project_doc_read` trace records.
- `ToolPermission::read_only()` constructor.
- Responder system prompt appends a kind/maturity voicing block when
  the tools are advertised.
- Self-question battery test exercises the responder end-to-end against
  the in-tree fixture corpus.

Refs: crates/qsf_app/src/project_docs, crates/qsf_app/src/tools,
crates/qsf_app/src/models/tool_dispatch.rs,
config/project-doc-introspection.toml; implements: Project-doc
introspection v1 scope (DecisionLog.md).
```

- [ ] Commit.

```bash
git add docs/EngineeringDiary.md
git commit -m "docs(diary): project-doc introspection channel"
```

---

## Phase 10: Manual live verification (external human testing recommended)

**External testing recommended:** this phase requires a live model
provider and judgement about reply quality. Treat the fixture battery
in Phase 7 as the regression gate and this phase as the qualitative
acceptance gate.

### Task 10.1: Run a live session

- [ ] Run the multi-turn text loop end-to-end against the production
  allowlist. Suggested prompts (mirrors the design's Testing section):
  - "What are you?"
  - "Is the sleep phase implemented?"
  - "What's your stance on autonomous agency?"
  - "Tell me about the goal system."
  - "What's the capital of France?" (control: no introspection
    expected)
- [ ] Open the run's `runs/<run-id>/traces.jsonl`. For each reply,
  confirm:
  - Searches and reads are present where expected.
  - `kind` and `maturity_tag` in trace details match the documents
    fetched.
  - Hedging in the reply text matches the maturity tag (e.g.
    "brainstorm idea" language only for Idea/Brainstorm material).
  - No claim of current behavior is made from a Plan, Idea, or
    Concept.
  - The control question made no introspection calls.
  - Recorded `latency_ms` values stay well under 1000 ms; if any
    exceed it, follow Open Question #5 and add a cap-enforcement task
    at the `ProjectDocService` boundary.
- [ ] If anything fails, do **not** patch the prompt to mask it —
  open a new diary entry describing the failure and add a follow-on
  ticket in the experiment backlog.

---

## Self-Review

Run after all phases land:

- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt`
- [ ] `cargo test -p qsf_app`
- [ ] Verify the production allowlist excludes `docs/Reviews/**` and
  `docs/EngineeringDiary.md` (Task 1.2 test should already cover this
  in CI).
- [ ] Verify the bounded read rejects `..` traversal and absolute paths
  (Task 1.5 tests should already cover this in CI).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  if Phase 1 was merged independently, a standalone library-slice entry
  plus the Phases 2-8 entry).