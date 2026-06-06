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

Phase 1 (the `ProjectDocService` library) has landed and is committed.
**Phase 2 (the two `Tool` implementations) is the next implementation
step.**

## Background

The design at `docs/Plans/Design.ProjectDocIntrospection.md` specifies a
live-first introspection channel for project documents. This plan
implements the v1 channel in sequential implementation and
documentation phases that each produce something independently testable.
Phases 1-6 are the minimum viable channel (tools work end-to-end and
the responder can call them). Phase 7 delivers the offline
self-question battery promised by the design's *Live-First Rationale*.
Phase 8 adds the `influenced_reply` post-hoc enrichment. Phase 9 lands
the documentation updates required by
`docs/ProjectFrame/ProjectWorkflow.md`. Phase 10 records the live
external verification step. Phase 11 is a future planning handoff for
associative project-doc context pointers; it is not part of the v1
tool implementation.

## Current Anchors

Code anchors (existing, will be extended):

- `crates/qsf_app/src/project_docs/` — **landed in Phase 1.** Pure
  library: `Allowlist`, metadata extraction, lexical `search`, bounded
  `read`, and the `ProjectDocService` facade. Phase 2 consumes this; it
  does not modify it.
- `crates/qsf_app/src/tools/mod.rs` — re-exports tool surface; Phase 2
  adds the `project_doc_tool`, `search_project_docs_tool`, and
  `read_project_doc_tool` submodules and their re-exports.
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`,
  `Tool` trait, `ToolMetadata`, `ToolContext`, `EmptyToolContext`.
  Adding two tools means extending the struct, `Default`, and the
  `match` arms in `metadata_for`, `dispatch`, and
  `model_tool_definitions_for` (the registry wiring lands in Phase 3;
  Phase 2 only extends the `ToolContext` trait with a defaulted
  accessor).
- `crates/qsf_app/src/tools/tool_request.rs` — `ToolPermission`
  (has `compute_only()`; Phase 2 adds a `read_only()` constructor),
  `ToolRequest`, `ToolCategory`, `ToolSideEffectLevel`.
- `crates/qsf_app/src/tools/tool_result.rs` — `ToolResult` with fields
  `tool_name`, `category`, `side_effect_level`, `input`, `output_text`,
  `numeric_value`, `observation_summary`.
- `crates/qsf_app/src/tools/calculator_tool.rs` and
  `crates/qsf_app/src/tools/recall_turn_tool.rs` — reference
  implementations of the `Tool` trait and custom `ToolContext`
  (`SessionToolContext`). Phase 2's tools mirror these exactly.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls`; this is where per-turn caps are
  enforced (Phase 4) and where tool-result trace records are emitted
  (Phase 5).
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
   at the repo root. Phase 1 settled this path; no other config-loading
   convention was found to conflict. *Path-resolution note (still
   binding on later phases):* `cargo test` runs with the working
   directory set to the package root (`crates/qsf_app`), **not** the
   workspace root, so tests and production code must never load the
   config via a bare relative path like
   `"config/project-doc-introspection.toml"`. Tests resolve it from
   `CARGO_MANIFEST_DIR`; production wiring (Phase 6 onward) must
   construct `ProjectDocService` with an explicit absolute repo root
   and an explicit absolute allowlist path, rather than relying on the
   process working directory.
2. **`ProjectDocService` injection shape.** This plan uses a dedicated
   `ProjectDocToolContext` carrying a borrowed `&ProjectDocService`,
   parallel to `SessionToolContext`, surfaced through a new defaulted
   `ToolContext::project_doc_service()` accessor. **Phase 2 makes this
   decision concrete (Task 2.2).** A review raised that the live
   `ConversationalResponder` advertises `recall_turn` (which needs
   `session_state()`) alongside the project-doc tools (which need
   `project_doc_service()`), and `dispatch_model_tool_calls` threads a
   single `ToolContext` per batch. The standalone `ProjectDocToolContext`
   is therefore sufficient only for Phase 2's **isolated unit tests**;
   the production wiring in Phases 3-6 must supply **one** combined
   context that can answer *both* accessors — otherwise `recall_turn`
   fails under a project-doc-only context, or the project-doc tools fail
   under the session-only context. The defaulted accessors keep this
   composable (extend `SessionToolContext` to also carry an optional
   `&ProjectDocService`, or introduce a dedicated combined context
   implementing both accessors). If a different pattern is preferred,
   raise it before Phase 3 hardens the registry/dispatch wiring.
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

## Phase 1: `ProjectDocService` library — completed

**Status: landed and committed** (git: "ProjectDocIntrospection phase 1:
feat(project_docs): add read-only project document service"). Summarized
here; the source of truth is the code under
`crates/qsf_app/src/project_docs/`.

### What shipped

A pure, side-effect-free module at `crates/qsf_app/src/project_docs/`
(declared by a one-line `pub mod project_docs;` in `lib.rs`), with these
submodules and a public surface that later phases depend on:

- `types` — `DocKind`, `MaturityTag`, `MatchStrength`, `DocHit`,
  `DocRead` (all `Serialize`/`Deserialize`, re-exported from
  `crate::project_docs`).
- `allowlist` — `Allowlist::from_file` / `from_str`, with
  `allows(repo_relative_path)` evaluating exclude-then-include globs.
- `metadata` — `kind_for_path`, `maturity_for`, `last_reviewed_for`
  (the last scoped to the `## Implementation Status` section and
  enforcing an ISO `YYYY-MM-DD` shape).
- `search` — `search(repo_root, allowlist, query, max_results)`,
  walking the corpus with `walkdir`, returning ranked `DocHit`s.
- `read` — `read(repo_root, allowlist, relative_path, focus, max_tokens)`,
  returning a bounded `DocRead`.
- `service` — the facade other phases construct:
  - `ProjectDocService::new(repo_root, allowlist_path)`
  - `.search(query, max_results) -> Result<Vec<DocHit>>`
  - `.read(path, focus, max_tokens) -> Result<DocRead>`
  - `.allowlist() -> Result<Allowlist>` (re-read per call, so the
    on-disk allowlist is hot-reloaded)
  - `.repo_root() -> &Path`

Dependencies added to `crates/qsf_app/Cargo.toml`: `globset`, `toml`,
`regex`, `once_cell`, `walkdir`, and `tempfile` (dev). The production
allowlist lives at `config/project-doc-introspection.toml`.

### Lessons and constraints that bind later phases

- **Path resolution (Open Question #1).** Tests resolve paths from
  `CARGO_MANIFEST_DIR`. Production wiring (Phase 6 onward) must
  construct `ProjectDocService` with an explicit **absolute** repo root
  and an explicit **absolute** allowlist path — never a bare relative
  path, because the test/runtime working directory is the package root,
  not the workspace root.
- **Path-safety lives in the library, not the tool.** The bounded
  `read` normalizes and confines any caller-supplied path *before* the
  allowlist or filesystem is touched: absolute paths and any `..`
  component are rejected, `.` is dropped, and the result is a clean
  forward-slash repo-relative string. Phase 2's `read_project_doc` tool
  therefore must **not** re-implement traversal guards — it forwards the
  raw `path` straight to `service.read(...)` and relies on this
  invariant (the out-of-allowlist tool test in Task 2.4 uses a
  non-`.md` path, since traversal rejection is already proven in the
  library tests).
- **Allowlist hot-reload + production defaults.** The production
  allowlist excludes `docs/EngineeringDiary.md` and `docs/Reviews/**`
  while admitting `docs/ProjectFrame/**` and `docs/DecisionLog.md`. The
  channel will pick up edits to that file without a rebuild.
- **Latency cap deferred (Open Question #5).** The service API is
  synchronous with no deadline parameter; if real traces show
  `latency_ms` over 1000, enforcement is added at the service boundary,
  not in the tools or dispatch.
- **Purity.** The module is side-effect-free apart from reading files
  under the repo root. No tool, registry, dispatch, or responder wiring
  was introduced in Phase 1.

### Acceptance outcome (met)

`cargo test -p qsf_app project_docs` passes, covering allowlist
include/exclude precedence, kind/maturity/last-reviewed extraction
(including the Implementation-Status scoping and malformed-date
rejection), heading-first lexical search with empty-result/empty-query
handling, bounded read with focus and truncation, the traversal/absolute
path refusals, and service-level allowlist hot-reload. `cargo clippy
--all-targets -- -D warnings` and `cargo fmt` are clean.

### Diary follow-up constraint

Phase 1 was committed as a standalone deliverable. The Phase 9 diary
pass must therefore account for it explicitly: either fold Phase 1 into
the Phases 1-8 entry (acceptable since it is part of the same logical
feature) or add a separate library-slice entry. Do not silently skip it.

---

## Phase 2: Tool implementations

Two `Tool` impls plus a `ToolPermission::read_only()` constructor and a
new `ToolContext` variant. **No registry wiring yet — that lands in
Phase 3.** This phase is implementable and reviewable on its own: the
tools are exercised in unit tests by constructing them directly with a
`ProjectDocToolContext` built over the Phase 1 fixture corpus, without
touching `ToolRegistry` or `dispatch`.

This phase resolves Open Question #2: the injection shape is a dedicated
`ProjectDocToolContext<'a>` holding a borrowed `&'a ProjectDocService`,
parallel to the existing `SessionToolContext`, surfaced through a new
defaulted `ToolContext::project_doc_service()` accessor. Raise it before
starting if a different pattern is preferred — Phase 3's registry wiring
hardens this choice.

**Mixed-batch dispatch (raised in review — plan now, build in Phase 3).**
The live `ConversationalResponder` advertises `recall_turn` (needs
`session_state()`) *alongside* the two project-doc tools (need
`project_doc_service()`), and `dispatch_model_tool_calls` threads a single
`ToolContext` per batch. The standalone `ProjectDocToolContext` introduced
in Task 2.2 is therefore sufficient **only** for unit-testing the tools in
isolation. Before Phase 3 hardens the registry/dispatch wiring, decide and
implement a **combined** context that can answer *both* accessors — either
extend `SessionToolContext` to also carry an optional `&ProjectDocService`
(returning it from `project_doc_service()`), or add a dedicated combined
context implementing both accessors. The defaulted accessors make this
composable, so adding the second accessor in Task 2.2 does not force the
decision now; but do not leave the Phase 3/6 call sites to infer the
combined-context requirement. This is the concrete follow-through on Open
Question #2.

Implement the tasks in order (2.1 → 2.4); each ends in its own commit so
the phase can be reviewed incrementally. Follow
`superpowers:test-driven-development`: the failing test precedes the
implementation in every task.

### Task 2.1: `ToolPermission::read_only()`

**Files:**
- Modify: `crates/qsf_app/src/tools/tool_request.rs`

The reference is the existing `compute_only()` constructor in the same
`impl ToolPermission` block. The new constructor grants the `ReadOnly`
category and a `ReadOnly` maximum side-effect level — matching the
metadata the two tools advertise so `ToolRegistry::validate_request`
will admit them once Phase 3 wires them in.

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/tools/tool_request.rs — add a test block (the file
// currently has no inline tests).
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

    #[test]
    fn read_only_permission_rejects_compute_only_category() {
        // Guards against an over-broad allow-list: read_only must not also
        // admit the compute_only category.
        let permission = ToolPermission::read_only();
        assert!(!permission.allows(ToolCategory::ComputeOnly, ToolSideEffectLevel::None));
    }
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::tool_request`
Expected: FAIL (`read_only` not defined).

- [ ] **Step 3: Implement the constructor.**

Add to the existing `impl ToolPermission` block, next to `compute_only`:

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
- Modify: `crates/qsf_app/src/tools/tool_registry.rs` (extend the
  `ToolContext` trait)
- Create: `crates/qsf_app/src/tools/project_doc_tool.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs`

The current `ToolContext` trait has one defaulted accessor
(`session_state`). Add a second defaulted accessor returning `None`, so
`EmptyToolContext` and `SessionToolContext` keep compiling untouched,
then add a concrete context that returns the service. Per the phase's
TDD rule, the new accessor behavior gets a failing test *before* the
implementation — `cargo build` alone is not sufficient verification.

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/tools/project_doc_tool.rs (test block, added with the
// module in Step 2 — written first so it fails to compile/assert).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::ProjectDocService;
    use crate::tools::EmptyToolContext;
    use crate::tools::tool_registry::ToolContext;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    #[test]
    fn context_exposes_service() {
        let service = ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        );
        let ctx = ProjectDocToolContext { service: &service };
        assert!(ctx.project_doc_service().is_some());
    }

    #[test]
    fn empty_context_returns_none() {
        // The defaulted accessor must leave existing contexts unchanged.
        assert!(EmptyToolContext.project_doc_service().is_none());
    }
}
```

- [ ] **Step 2: Extend the trait and write the context impl.**

In `crates/qsf_app/src/tools/tool_registry.rs`, add to `trait ToolContext`:

```rust
fn project_doc_service(&self) -> Option<&crate::project_docs::ProjectDocService> {
    None
}
```

Create the context:

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
`pub use project_doc_tool::ProjectDocToolContext;` to
`crates/qsf_app/src/tools/mod.rs`.

- [ ] **Step 4: Run tests and build.**

Run: `cargo test -p qsf_app tools::project_doc_tool`
Expected: PASS (the two accessor tests).
Run: `cargo build`
Expected: builds clean (the defaulted trait method means no existing
`ToolContext` impl needs to change).

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

Mirrors `calculator_tool.rs` exactly: a unit struct implementing `Tool`
with `metadata`, `execute`, and `model_tool_definition`. The tool reads
its arguments from `ToolRequest::structured`, calls
`service.search(query, max_results)`, and serializes the `Vec<DocHit>`
into `output_text`. It **normalizes** `max_results` into the
`1..=DEFAULT_MAX_RESULTS` range — capping the upper bound so a model
cannot request an unbounded page, and treating an out-of-schema `0` as
the default rather than silently returning an empty page.

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/tools/search_project_docs_tool.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocHit, ProjectDocService};
    use crate::tools::{EmptyToolContext, ProjectDocToolContext, Tool, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn service() -> ProjectDocService {
        ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        )
    }

    fn make_request_with_max(query: &str, max_results: u64) -> ToolRequest {
        ToolRequest {
            tool_name: SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
            input: query.to_string(),
            structured: Some(serde_json::json!({ "query": query, "max_results": max_results })),
            permission: ToolPermission::read_only(),
            requested_by: "test".to_string(),
        }
    }

    fn make_request(query: &str) -> ToolRequest {
        make_request_with_max(query, 6)
    }

    #[test]
    fn search_returns_hits_with_metadata() {
        let service = service();
        let ctx = ProjectDocToolContext { service: &service };
        let result = SearchProjectDocsTool.execute(&make_request("Maturity"), &ctx).unwrap();

        assert_eq!(result.category, ToolCategory::ReadOnly);
        assert!(result.observation_summary.contains("hits"));
        // output_text is the serialized Vec<DocHit>; it must parse back.
        let hits: Vec<DocHit> = serde_json::from_str(&result.output_text).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn search_treats_zero_max_results_as_default() {
        // max_results = 0 violates the schema minimum; the executor normalizes
        // it to the default instead of returning an empty page.
        let service = service();
        let ctx = ProjectDocToolContext { service: &service };
        let result = SearchProjectDocsTool
            .execute(&make_request_with_max("Maturity", 0), &ctx)
            .unwrap();
        let hits: Vec<DocHit> = serde_json::from_str(&result.output_text).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn search_fails_without_project_doc_context() {
        let err = SearchProjectDocsTool
            .execute(&make_request("anything"), &EmptyToolContext)
            .unwrap_err();
        assert!(err.to_string().contains("ProjectDocToolContext"));
    }
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::search_project_docs_tool`
Expected: FAIL (module/struct not defined).

- [ ] **Step 3: Implement the tool.**

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
        // Normalize into 1..=DEFAULT_MAX_RESULTS: clamp the upper bound and
        // treat a missing or out-of-schema 0 value as the default.
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .filter(|&n| n >= 1)
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

- [ ] **Step 4: Re-export from `mod.rs`.**

Add `pub mod search_project_docs_tool;` and
`pub use search_project_docs_tool::{SEARCH_PROJECT_DOCS_TOOL_NAME, SearchProjectDocsTool};`.

- [ ] **Step 5: Run tests.**

Run: `cargo test -p qsf_app tools::search_project_docs_tool`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/qsf_app/src/tools/search_project_docs_tool.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): SearchProjectDocsTool"
```

### Task 2.4: `ReadProjectDocTool`

**Files:**
- Create: `crates/qsf_app/src/tools/read_project_doc_tool.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs`

Same shape as Task 2.3. The default token budget is smaller for a
focused read than for a whole-document read. Crucially, the tool does
**not** trust the model- or dispatch-supplied `max_tokens`: the value is
clamped to a hard cap (`MAX_TOKENS_HARD_CAP`, matching the
`model_tool_definition` schema maximum) before it reaches
`service.read(...)`, so a request that ignores the schema cannot produce
an unbounded read. This mirrors the upper-bound clamp the search tool
applies to `max_results`. Path safety is **not** re-implemented here —
the raw `path` is forwarded to `service.read(...)`, which enforces the
Phase 1 traversal/absolute-path invariant. The out-of-allowlist test
therefore uses a clean-but-non-`.md` path so it exercises the allowlist
refusal branch rather than the traversal branch.

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/tools/read_project_doc_tool.rs (test block)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::{DocRead, ProjectDocService};
    use crate::tools::{EmptyToolContext, ProjectDocToolContext, Tool, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    fn service() -> ProjectDocService {
        ProjectDocService::new(
            fixtures_root(),
            fixtures_root().join("allowlist_basic.toml"),
        )
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

    fn read_output(path: &str, focus: Option<&str>, max_tokens: u64) -> String {
        let service = service();
        let ctx = ProjectDocToolContext { service: &service };
        ReadProjectDocTool
            .execute(&make_request(path, focus, max_tokens), &ctx)
            .unwrap()
            .output_text
    }

    #[test]
    fn read_returns_doc_content() {
        // In-range budget; output_text must round-trip back to a DocRead.
        let doc: DocRead =
            serde_json::from_str(&read_output("sample_concept.md", None, 4000)).unwrap();
        assert!(doc.content.contains("Concept: Sample"));
    }

    #[test]
    fn read_clamps_max_tokens_to_hard_cap() {
        // A request above the advertised schema maximum must behave identically
        // to one at the cap — the tool does not trust the supplied budget.
        let at_cap = read_output("sample_concept.md", None, MAX_TOKENS_HARD_CAP as u64);
        let over_cap = read_output("sample_concept.md", None, 10_000);
        assert_eq!(at_cap, over_cap);
    }

    #[test]
    fn read_refuses_out_of_allowlist() {
        let service = service();
        let ctx = ProjectDocToolContext { service: &service };
        // Normalizes cleanly but is not a `*.md` file, so the allowlist refuses it.
        let err = ReadProjectDocTool
            .execute(&make_request("outside.txt", None, 4000), &ctx)
            .unwrap_err();
        assert!(err.to_string().contains("not in allowlist"));
    }

    #[test]
    fn read_fails_without_project_doc_context() {
        let err = ReadProjectDocTool
            .execute(&make_request("sample_concept.md", None, 4000), &EmptyToolContext)
            .unwrap_err();
        assert!(err.to_string().contains("ProjectDocToolContext"));
    }
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::read_project_doc_tool`
Expected: FAIL (module/struct not defined).

- [ ] **Step 3: Implement the tool.**

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
/// Hard ceiling, identical to the `model_tool_definition` schema maximum. The
/// model and dispatch are not trusted to honor the schema, so the tool clamps
/// any supplied `max_tokens` to this value before calling `service.read`.
const MAX_TOKENS_HARD_CAP: usize = 4000;

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
        // Clamp to the hard cap so a model that ignores the schema maximum
        // cannot request an unbounded read.
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(default_budget)
            .min(MAX_TOKENS_HARD_CAP);

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

Keep `MAX_TOKENS_HARD_CAP` and the schema `maximum` in sync — they are
two faces of the same contract.

- [ ] **Step 4: Re-export and run tests.**

Add to `crates/qsf_app/src/tools/mod.rs`:

```rust
pub mod read_project_doc_tool;
pub use read_project_doc_tool::{READ_PROJECT_DOC_TOOL_NAME, ReadProjectDocTool};
```

Run: `cargo test -p qsf_app tools::read_project_doc_tool`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/tools/read_project_doc_tool.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): ReadProjectDocTool"
```

### Phase 2 verification

Run `cargo test -p qsf_app tools::` then
`cargo clippy --all-targets -- -D warnings` and `cargo fmt`. Expect all
clean.

**Diary discipline for this phase.** Per `Agents.md`, implementation
changes must be recorded in `docs/EngineeringDiary.md`. This plan groups
the *application* work of Phases 1-8 under a single diary entry written
in Phase 9 — which means **Phases 2-8 are not considered complete or
mergeable until that Phase 9 diary entry lands**. If Phase 2 is reviewed,
merged, or handed off as an isolated deliverable ahead of the grouped
feature, a short standalone Phase 2 diary entry must accompany that merge
(read the *Instructions how to use* at the top of the diary first). Do
not merge Phase 2 in isolation with no diary entry at all.

**Acceptance criteria for Phase 2:**

- `ToolPermission::read_only()` exists, admits a `ReadOnly`/`ReadOnly`
  request, and rejects both `WriteCapable`/`ExternalWrite` and
  `ComputeOnly`/`None` (Task 2.1 tests).
- A new defaulted `ToolContext::project_doc_service()` accessor exists
  and is **tested**: `ProjectDocToolContext` returns `Some(service)` and
  `EmptyToolContext` returns `None` (Task 2.2 tests); `EmptyToolContext`
  and `SessionToolContext` compile and behave unchanged via the `None`
  default.
- `SearchProjectDocsTool` and `ReadProjectDocTool` implement `Tool`,
  advertise `category = ReadOnly` / `side_effect_level = ReadOnly`,
  expose a `model_tool_definition` with the documented JSON schema, and
  produce a `ToolResult` whose `output_text` round-trips via serde to
  `Vec<DocHit>` / `DocRead` respectively (asserted by deserializing in
  the success tests).
- `SearchProjectDocsTool` normalizes `max_results` into
  `1..=DEFAULT_MAX_RESULTS` (a `0` argument falls back to the default;
  Task 2.3 test). `ReadProjectDocTool` clamps `max_tokens` to
  `MAX_TOKENS_HARD_CAP` (an above-cap argument behaves identically to one
  at the cap; Task 2.4 test).
- Both tools fail with a clear, `ProjectDocToolContext`-mentioning error
  when run against a context lacking the service (tested for **both**
  search and read), and forward allowlist/path enforcement to
  `ProjectDocService` rather than re-implementing it.
- The tools are **not** yet referenced by `ToolRegistry`,
  `dispatch_model_tool_calls`, or any responder role — that wiring is
  deliberately deferred to Phases 3-6. `lib.rs` is unchanged in this
  phase.
- `cargo test -p qsf_app tools::`, `cargo clippy --all-targets -- -D
  warnings`, and `cargo fmt` are all clean.

**Open question to surface if it arises:** if, while writing Task 2.2,
the borrowed-`&ProjectDocService` context proves awkward at the eventual
call site (e.g. the dispatcher only has an owned/`Arc` handle, or needs
the combined session+project-doc context described in the *Mixed-batch
dispatch* note above), raise Open Question #2 before Phase 3 rather than
silently switching to an `Arc<ProjectDocService>` or a one-off context —
the change touches both the context type and every construction site.

---

## Phase 3: `ToolRegistry` wiring

Extend the hand-coded registry to dispatch the two new tools. Per
`Agents.md`, keep shared constants DRY — the names already live in
their respective tool modules; the registry imports them.

**Before this phase hardens the wiring**, settle the combined-context
decision from Phase 2's *Mixed-batch dispatch* note (Open Question #2):
the dispatch path will need a single `ToolContext` that answers both
`session_state()` (for `recall_turn`) and `project_doc_service()` (for
the project-doc tools). The registry itself does not hold the context,
but the construction site it feeds (Phases 4 and 6) does — do not defer
the choice past this point.

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

This is the first phase that actually dispatches the project-doc tools
through a live `ToolContext` alongside `recall_turn`, so the combined
context from Open Question #2 (a single context answering both
`session_state()` and `project_doc_service()`) must already exist here.

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
`ModelRequest`, a `ToolRegistry`, and a combined `ToolContext`, then
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

This is the call site where `recall_turn`, `search_project_docs`, and
`read_project_doc` are advertised together, so the `ToolContext`
constructed here (and passed to `dispatch_model_tool_calls`) **must** be
the combined context that answers both `session_state()` and
`project_doc_service()` — see Phase 2's *Mixed-batch dispatch* note and
Open Question #2. If that combined context does not yet exist, build it
before extending `allowed_tools`.

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
constants. Keep the constants imported from `crate::tools`. Confirm the
context handed to `dispatch_model_tool_calls` at each updated call site
is the combined context (exposes both `session_state()` and
`project_doc_service()`); otherwise `recall_turn` or the project-doc
tools will fail at runtime.

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
reference planning documents. **Because Phase 1 was committed as a
standalone slice, make sure this entry (or a separate library-slice
entry) explicitly accounts for the `project_docs` library work, not
only Phases 2-8.** If any of Phases 2-8 were merged in isolation ahead
of this pass and already carry their own standalone diary entries (per
the Phase 2 diary discipline), reconcile rather than duplicate them
here.

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

## Phase 11: Associative project-doc context pointers (future planning handoff)

This is a follow-on planning phase, not part of the v1 project-doc tool
channel. It gives a project manager enough shape to create a separate
design or implementation plan after Phases 1-10 have produced trace
evidence about how project-doc lookup behaves in live dialogue.

The goal is to explore an automatic, association-driven context source
for project documents. Unlike `search_project_docs` and
`read_project_doc`, this mechanism is not activated by a model tool
call. It is driven by the same memory/context-selection path that
retrieves relevant memories for the current input. Its output should be
compact project-doc pointers, not full document bodies.

### Candidate shape

```text
current input / active focus
  -> associative retrieval cues
  -> project-doc pointer candidates
  -> context budget and authority ranking
  -> selected ProjectDocContextPointer fragments enter active context
  -> model may answer from the pointer metadata, or call read_project_doc
     when body content is needed
```

A `ProjectDocContextPointer` should include only enough material to
orient the model and preserve provenance:

```text
ProjectDocContextPointer
  path
  title
  kind
  maturity_tag
  last_reviewed
  section_hint
  reason_selected
  association_path_or_score
  header_excerpt
  authority_note
  suggested_followup_tool_call
```

`header_excerpt` should be limited to the document title and document
status/header material, such as `## Status`, `## Maturity`, and the
`## Implementation Status` summary when present. It should not include
arbitrary body sections by default. If the model needs actual body text,
the expected path is an explicit `read_project_doc` call with a focused
section or token budget.

### Boundaries for the future plan

- This phase must not make project documents always-present prompt
  material.
- This phase must not inject complete documents, long plan bodies, or
  broad search excerpts into live context.
- The context assembler, not the project-doc service alone, decides
  whether a pointer enters active context.
- Pointer fragments must carry kind/maturity metadata so brainstorm,
  plan, decision, architecture, and implementation-status material are
  not flattened into one authority level.
- Stable project anchors and accepted decisions may be protected from
  ordinary memory decay, but speculative plans and ideas should remain
  weaker and clearly labeled.
- Full-text reads remain observable tool calls; automatic context
  injection should not hide document inspection that materially affects
  a reply.

### Planning work to flesh out later

- Decide whether pointers are generated from the same allowlisted corpus
  as `search_project_docs`, from post-hoc `project_doc_*` traces, from
  curated stable project facts, or from a combination.
- Define the `ContextSource` / `ContextFragment` boundary that turns
  project-doc pointer candidates into active context.
- Define ranking signals: query similarity, association strength,
  document authority, recency, prior successful influence, and diversity.
- Set initial live budgets, for example maximum pointer count and maximum
  total pointer tokens per turn.
- Define trace records for selected and omitted project-doc pointers,
  including why each pointer was selected or rejected.
- Decide whether pointer selection can request asynchronous follow-up
  reflection when the relevant material is too large for the live turn.
- Update `Architecture.ContextManagement.md`,
  `Architecture.MemorySystem.md`, and possibly
  `Architecture.ToolSystem.md` if the mechanism becomes implemented
  architecture.
- Record a decision only if the project commits to automatic
  project-doc pointer injection as a standing context mechanism.

### Verification expectations

A later detailed plan should include tests or review checks for these
cases:

- A project-self question about a stable boundary selects a compact
  pointer to a frame or decision document.
- A question related to a brainstorm or plan selects a pointer with
  explicit low-authority voicing metadata.
- An unrelated ordinary question selects no project-doc pointers.
- Pointer context contains path/title/status/header metadata, but not
  full document bodies.
- When body content is needed, the responder still uses
  `read_project_doc` and the read remains visible in tool traces.
- Selected and omitted pointer candidates are observable enough for a
  reviewer to understand context influence.

**External testing recommended:** after implementation, run live
side-by-side sessions with associative pointers disabled and enabled.
Check whether the pointers improve grounding without causing prompt
bloat, false authority, or over-eager project self-reference.

---

## Self-Review

Run after Phases 1-10 land:

- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt`
- [ ] `cargo test -p qsf_app`
- [ ] Verify the production allowlist excludes `docs/Reviews/**` and
  `docs/EngineeringDiary.md` (Task 1.2 test should already cover this
  in CI).
- [ ] Verify the bounded read rejects `..` traversal and absolute paths
  (Phase 1 read tests should already cover this in CI).
- [ ] Verify `read_project_doc` clamps an above-cap `max_tokens` to
  `MAX_TOKENS_HARD_CAP` and `search_project_docs` normalizes
  `max_results` into `1..=DEFAULT_MAX_RESULTS` (Phase 2 tests should
  already cover both in CI).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  since Phase 1 was committed independently, a standalone library-slice
  entry plus the Phases 2-8 entry), with any isolated-merge diary
  entries reconciled rather than duplicated.
- [ ] Confirm Phase 11 remains a follow-on planning handoff unless it has
  been promoted into a separate detailed design or implementation plan.