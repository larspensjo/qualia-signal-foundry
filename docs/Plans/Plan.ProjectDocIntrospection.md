# Plan: Project-Doc Introspection

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task.
> *(If those skills are unavailable in your environment, treat them as
> optional guidance and fall back to plain test-driven development for
> this repo — the per-task "failing test first" steps below stand on
> their own.)* Steps use checkbox (`- [ ]`) syntax for tracking.

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

Phase 1 (the `ProjectDocService` library) and Phase 2 (the two `Tool`
implementations, plus `ToolPermission::read_only()` and the defaulted
`ToolContext::project_doc_service()` accessor) have landed and are
committed. **Phase 3 (wiring the two tools into `ToolRegistry`) is the
next implementation step.**

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
  `read`, and the `ProjectDocService` facade. Later phases consume it;
  they do not modify it.
- `crates/qsf_app/src/tools/mod.rs` — re-exports the tool surface.
  **Phase 2 added** the `project_doc_tool`, `search_project_docs_tool`,
  and `read_project_doc_tool` submodules and re-exported
  `ProjectDocToolContext`, `SEARCH_PROJECT_DOCS_TOOL_NAME`,
  `SearchProjectDocsTool`, `READ_PROJECT_DOC_TOOL_NAME`, and
  `ReadProjectDocTool` from `crate::tools`. Phase 3 needs no new
  re-exports here — only a verification that they are present.
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`,
  `Tool` trait, `ToolMetadata`, `ToolContext`, `EmptyToolContext`.
  **Phase 2 added** the defaulted `ToolContext::project_doc_service()`
  accessor (returns `None`). **Phase 3** extends the `ToolRegistry`
  struct, its `Default`, and the `match` sites in `metadata_for`,
  `dispatch`, and `model_tool_definitions_for` to route the two new
  tools.
- `crates/qsf_app/src/tools/tool_request.rs` — `ToolPermission` now has
  both `compute_only()` and `read_only()` (the latter landed in Phase
  2), plus `ToolRequest`, `ToolCategory`, `ToolSideEffectLevel`.
- `crates/qsf_app/src/tools/tool_result.rs` — `ToolResult` with fields
  `tool_name`, `category`, `side_effect_level`, `input`, `output_text`,
  `numeric_value`, `observation_summary`.
- `crates/qsf_app/src/tools/calculator_tool.rs` and
  `crates/qsf_app/src/tools/recall_turn_tool.rs` — reference
  implementations of the `Tool` trait and custom `ToolContext`
  (`SessionToolContext`). The Phase 2 project-doc tools mirror these.
- `crates/qsf_app/src/tools/project_doc_tool.rs` — **landed in Phase
  2.** Holds `ProjectDocToolContext<'a> { service: &'a
  ProjectDocService }` implementing `project_doc_service()`.
- `crates/qsf_app/src/tools/search_project_docs_tool.rs` and
  `crates/qsf_app/src/tools/read_project_doc_tool.rs` — **landed in
  Phase 2.** The two `Tool` impls; not yet referenced by the registry.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls`; per-turn caps are enforced here (Phase 4)
  and tool-result trace records are emitted here (Phase 5). Its
  `tool_request_from_model_tool_call` already routes structured/unknown
  tools through a catch-all arm, so once the registry knows the two
  tools (Phase 3), correct `ToolRequest`s are built for them with no
  change to the request-builder.
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
2. **`ProjectDocService` injection shape (combined context).** Phase 2
   landed the chosen shape: a dedicated `ProjectDocToolContext<'a>`
   holding a borrowed `&'a ProjectDocService`, parallel to the existing
   `SessionToolContext`, surfaced through the defaulted
   `ToolContext::project_doc_service()` accessor. That standalone context
   is sufficient for isolated unit tests and for Phase 3's registry
   wiring (the registry holds no context). It is **not** sufficient for
   live dispatch: the `ConversationalResponder` advertises `recall_turn`
   (needs `session_state()`) *alongside* the project-doc tools (need
   `project_doc_service()`), and `dispatch_model_tool_calls` threads a
   single `ToolContext` per batch. So before **Phase 4** (the first live
   dispatch of these tools), a **combined** context answering *both*
   accessors must exist — otherwise `recall_turn` fails under a
   project-doc-only context, or the project-doc tools fail under the
   session-only context. The recommended default: extend
   `SessionToolContext` to also carry an optional `&ProjectDocService`
   and return it from `project_doc_service()`, or add a dedicated
   combined context implementing both accessors (the defaulted accessors
   keep this composable). **Decision to confirm before Phase 4:** which
   of the two shapes. If a different pattern is preferred, raise it
   before Phase 4 wiring begins.
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
   wiring a `RunContext`, mock model client, combined `ToolContext`,
   and `ModelRequest` for a unit test of `dispatch_model_tool_calls`
   is a lot of code that already has working examples in the file (see
   the `tests` module in `tool_dispatch.rs`, which builds a
   `RunContext`, a `ToolRegistry`, a `SessionToolContext`, and a
   `ModelRequest`). The assertions in those skeletons are concrete; the
   harness wiring is not. If existing patterns are unclear, write the
   integration test under `crates/qsf_app/tests/project_doc_dispatch.rs`
   and treat the skeletons as the assertion contract.

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
feat(project_docs): add read-only project document service"). The source
of truth is the code under `crates/qsf_app/src/project_docs/`.

### What shipped

A pure, side-effect-free module declared by `pub mod project_docs;` in
`lib.rs`, with the public surface later phases depend on:

- `types` — `DocKind`, `MaturityTag`, `MatchStrength`, `DocHit`,
  `DocRead` (all serde, re-exported from `crate::project_docs`).
- `allowlist` — `Allowlist::from_file` / `from_str`, with
  `allows(repo_relative_path)` evaluating exclude-then-include globs.
- `metadata` — `kind_for_path`, `maturity_for`, `last_reviewed_for`
  (the last scoped to `## Implementation Status` and enforcing ISO
  `YYYY-MM-DD`).
- `search` — `search(repo_root, allowlist, query, max_results)`.
- `read` — `read(repo_root, allowlist, relative_path, focus, max_tokens)`.
- `service` — the facade: `ProjectDocService::new(repo_root,
  allowlist_path)`, `.search(query, max_results)`, `.read(path, focus,
  max_tokens)`, `.allowlist()` (re-read per call, hot-reload), and
  `.repo_root()`.

Dependencies added: `globset`, `toml`, `regex`, `once_cell`, `walkdir`,
and `tempfile` (dev). The production allowlist lives at
`config/project-doc-introspection.toml`.

### Lessons and constraints binding later phases

- **Path resolution (Open Question #1).** Tests resolve paths from
  `CARGO_MANIFEST_DIR`; production wiring (Phase 6 onward) must construct
  `ProjectDocService` with **absolute** repo root and allowlist paths —
  never a bare relative path, because the test/runtime working directory
  is the package root, not the workspace root.
- **Path-safety lives in the library, not the tool.** The bounded `read`
  normalizes and confines any caller-supplied path *before* the
  allowlist or filesystem is touched: absolute paths and any `..`
  component are rejected, `.` is dropped, result is a clean
  forward-slash repo-relative string. The Phase 2 `read_project_doc`
  tool therefore forwards the raw `path` straight to `service.read(...)`
  and does **not** re-implement traversal guards.
- **Allowlist hot-reload + production defaults.** The production
  allowlist excludes `docs/EngineeringDiary.md` and `docs/Reviews/**`
  while admitting `docs/ProjectFrame/**` and `docs/DecisionLog.md`, and
  picks up edits without a rebuild.
- **Latency cap deferred (Open Question #5).** The service API is
  synchronous with no deadline parameter; if real traces show
  `latency_ms` over 1000, enforcement is added at the service boundary,
  not in the tools or dispatch.
- **Purity.** The module is side-effect-free apart from reading files
  under the repo root. No tool/registry/dispatch/responder wiring was
  introduced here.

### Acceptance outcome (met)

`cargo test -p qsf_app project_docs` passes (allowlist precedence,
kind/maturity/last-reviewed extraction with Implementation-Status
scoping and malformed-date rejection, heading-first lexical search,
bounded read with focus/truncation, traversal/absolute-path refusals,
service-level hot-reload). `cargo clippy --all-targets -- -D warnings`
and `cargo fmt` are clean.

### Diary follow-up constraint

Phase 1 was committed as a standalone deliverable. The Phase 9 diary
pass must account for it explicitly — fold it into the Phases 1-8 entry
or add a separate library-slice entry. Do not silently skip it.

---

## Phase 2: Tool implementations — completed

**Status: landed and committed** (git: "ProjectDocIntrospection phase 2:
add project-doc tool surface"). The source of truth is the code under
`crates/qsf_app/src/tools/`.

### What shipped

- `ToolPermission::read_only()` in
  `crates/qsf_app/src/tools/tool_request.rs` — grants the `ReadOnly`
  category and a `ReadOnly` max side-effect level, matching the metadata
  the two tools advertise so `ToolRegistry::validate_request` admits them
  once Phase 3 wires them in. Covered by tests asserting it admits
  `ReadOnly`/`ReadOnly` and rejects both `WriteCapable`/`ExternalWrite`
  and `ComputeOnly`/`None`.
- A second defaulted `ToolContext` accessor,
  `project_doc_service(&self) -> Option<&ProjectDocService>` (returns
  `None`), in `tool_registry.rs`. `EmptyToolContext` and
  `SessionToolContext` compile and behave unchanged via the default.
- `ProjectDocToolContext<'a> { service: &'a ProjectDocService }` in
  `crates/qsf_app/src/tools/project_doc_tool.rs`, returning
  `Some(service)` from the accessor. Tested against the Phase 1 fixture
  corpus (`src/project_docs/fixtures`, `allowlist_basic.toml`).
- `SearchProjectDocsTool` (`search_project_docs_tool.rs`): reads
  `query`/`max_results` from `ToolRequest::structured`, calls
  `service.search`, serializes `Vec<DocHit>` into `output_text`.
  **Normalizes `max_results` into `1..=DEFAULT_MAX_RESULTS`** (clamps the
  upper bound; treats an out-of-schema `0` as the default). Advertises
  `ReadOnly`/`ReadOnly` and a `model_tool_definition` with the documented
  JSON schema.
- `ReadProjectDocTool` (`read_project_doc_tool.rs`): reads
  `path`/`focus`/`max_tokens`, calls `service.read`, serializes `DocRead`.
  **Clamps `max_tokens` to `MAX_TOKENS_HARD_CAP` (4000)** — kept in sync
  with the schema `maximum` — so a request ignoring the schema cannot
  produce an unbounded read. Default budget is smaller for a focused read
  than a no-focus read. Forwards the raw `path` to `service.read` (path
  safety enforced by the Phase 1 library, not re-implemented here).
- `tools/mod.rs` re-exports `ProjectDocToolContext`,
  `SEARCH_PROJECT_DOCS_TOOL_NAME`, `SearchProjectDocsTool`,
  `READ_PROJECT_DOC_TOOL_NAME`, and `ReadProjectDocTool`.

Both tools fail with a clear, `ProjectDocToolContext`-mentioning error
when run against a context lacking the service, and round-trip their
`output_text` back to `Vec<DocHit>` / `DocRead` via serde.

### Lessons and constraints binding later phases

- **Not yet wired.** The tools are **not** referenced by `ToolRegistry`,
  `dispatch_model_tool_calls`, or any responder role — that is the work
  of Phases 3-6. `lib.rs` was unchanged in this phase.
- **Combined context required before Phase 4 (Open Question #2).** The
  standalone `ProjectDocToolContext` is enough for unit tests and Phase
  3's registry wiring, but live dispatch needs one context answering both
  `session_state()` (for `recall_turn`) and `project_doc_service()` (for
  the project-doc tools). Build that combined context before Phase 4 —
  see Open Question #2 for the recommended shapes and the decision to
  confirm.
- **Upper-bound discipline is the tool's job.** `max_results` and
  `max_tokens` are clamped/normalized inside the tools; later phases must
  not assume the model honors the advertised schema.
- **Diary discipline.** This plan groups the *application* work of Phases
  1-8 under a single Phase 9 diary entry — so Phases 2-8 are not
  considered complete or mergeable until that entry lands. If Phase 2 was
  merged in isolation, a short standalone Phase 2 diary entry must
  accompany that merge (read the *Instructions how to use* at the top of
  the diary first). Reconcile, don't duplicate, in Phase 9.

### Acceptance outcome (met)

`cargo test -p qsf_app tools::` passes (the `read_only` permission tests,
both context-accessor tests, search hit/metadata + `max_results`
normalization + missing-context failure, read content + `max_tokens`
clamp + out-of-allowlist refusal + missing-context failure).
`cargo clippy --all-targets -- -D warnings` and `cargo fmt` are clean.

---

## Phase 3: `ToolRegistry` wiring

Extend the hand-coded `ToolRegistry` so it knows about the two
project-doc tools that landed in Phase 2. Today the registry's struct,
its `Default`, and its three `match` sites (`metadata_for`, `dispatch`,
`model_tool_definitions_for`) route only `calculator` and `recall_turn`;
this phase adds `search_project_docs` and `read_project_doc` to all
three. Per `Agents.md`, keep shared constants DRY — the tool-name
constants already live in their modules and are re-exported from
`crate::tools`, so the registry imports them rather than re-declaring
strings.

This is a small, self-contained, independently reviewable slice: it
touches only `tool_registry.rs` and changes no runtime call site.

**Already in place from Phase 2 — do not redo:**

- The tool-name constants and tool structs are re-exported from
  `crates/qsf_app/src/tools/mod.rs`
  (`SEARCH_PROJECT_DOCS_TOOL_NAME`, `SearchProjectDocsTool`,
  `READ_PROJECT_DOC_TOOL_NAME`, `ReadProjectDocTool`). The "re-export the
  constants" step from the original draft is now a *verification*, not
  new work.
- `ToolContext::project_doc_service()` exists on the trait (defaulted to
  `None`).
- `dispatch_model_tool_calls`'s `tool_request_from_model_tool_call`
  already routes unrecognized tools through its catch-all `_ =>` arm,
  copying `tool_call.arguments` into `ToolRequest.structured` and
  deriving permission from the registry metadata. Once the registry
  knows the two tools (this phase), that catch-all builds correct
  `ToolRequest`s for them — **no change to the dispatch request-builder
  is needed in this phase.**

**Scope boundary — combined context is settled before Phase 4, not
here.** The registry holds no `ToolContext`; it receives one per call.
So the mixed-batch combined-context question (Open Question #2 — a single
context answering both `session_state()` and `project_doc_service()`)
does **not** need to be built in this phase. It must be settled before
Phase 4, the first phase that dispatches these tools through a live
context alongside `recall_turn`. **Decision to confirm before Phase 4**
(stated for the implementer's awareness, but not blocking Phase 3):
whether to extend `SessionToolContext` with an optional
`&ProjectDocService` or to introduce a dedicated combined context. If
neither default is acceptable, raise it before Phase 4 wiring begins.

Follow `superpowers:test-driven-development` (or plain TDD if that skill
is unavailable): the failing test precedes the implementation.

### Task 3.1: Extend the registry

**Files:**
- Modify: `crates/qsf_app/src/tools/tool_registry.rs`
- Verify (no change expected): `crates/qsf_app/src/tools/mod.rs`
  already re-exports the two constants and structs.

- [ ] **Step 1: Write the failing tests.**

Add to the existing `#[cfg(test)] mod tests` block in
`tool_registry.rs`. The first two prove the `metadata_for` and
`model_tool_definitions_for` arms *for both tools and with their
identity, not just their presence*; the last two drive real calls
through `execute` → `dispatch`, exercising the new `dispatch` arms *and*
confirming `read_only()` admits the tools. The dispatch tests reuse the
Phase 1/2 fixture corpus: the search test uses the `"Maturity"` query
that Phase 2 already proved returns non-empty hits, and the read test
uses the same allowlisted fixture path that Phase 2's read test proved
returns content.

The metadata/definition assertions deliberately go beyond `is_some()` /
`len() == 2` so a wrong-tool-under-the-right-name regression cannot
slip through (review finding L1). Mirror the exact field/accessor shape
the existing calculator and recall_turn tests use for `ToolMetadata`
and the model tool definition — the snippets below assume a `name`
field on the definition and `category` / `side_effect_level` fields on
`ToolMetadata`; adjust to whatever the current types expose.

```rust
#[test]
fn registry_exposes_project_doc_tools() {
    let registry = ToolRegistry::default();
    let defs = registry.model_tool_definitions_for(&[
        crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
        crate::tools::READ_PROJECT_DOC_TOOL_NAME,
    ]);
    assert_eq!(defs.len(), 2);
    // Not just a count: assert the right definitions came back under the
    // right names (a bare count would pass if one tool were returned twice).
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME));
    assert!(names.contains(&crate::tools::READ_PROJECT_DOC_TOOL_NAME));
}

#[test]
fn registry_metadata_for_project_doc_tools() {
    let registry = ToolRegistry::default();
    for name in [
        crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
        crate::tools::READ_PROJECT_DOC_TOOL_NAME,
    ] {
        let meta = registry.metadata_for(name).expect("metadata present");
        // Not just is_some(): Phase 3's dispatch path (and read_only()
        // admission) depends on these advertising ReadOnly/ReadOnly.
        assert_eq!(meta.category, crate::tools::ToolCategory::ReadOnly);
        assert_eq!(
            meta.side_effect_level,
            crate::tools::ToolSideEffectLevel::ReadOnly
        );
    }
}

#[test]
fn registry_dispatches_search_project_docs() {
    use crate::project_docs::ProjectDocService;
    use crate::tools::{ProjectDocToolContext, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures");
    let service =
        ProjectDocService::new(fixtures.clone(), fixtures.join("allowlist_basic.toml"));
    let ctx = ProjectDocToolContext { service: &service };
    let registry = ToolRegistry::default();

    let request = ToolRequest {
        tool_name: crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME.to_string(),
        input: "Maturity".to_string(),
        structured: Some(serde_json::json!({ "query": "Maturity" })),
        permission: ToolPermission::read_only(),
        requested_by: "test".to_string(),
    };

    let result = registry.execute(&request, &ctx).unwrap();
    assert_eq!(result.tool_name, crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME);
    assert_eq!(result.category, crate::tools::ToolCategory::ReadOnly);
}

#[test]
fn registry_dispatches_read_project_doc() {
    use crate::project_docs::ProjectDocService;
    use crate::tools::{ProjectDocToolContext, ToolPermission, ToolRequest};
    use std::path::PathBuf;

    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures");
    let service =
        ProjectDocService::new(fixtures.clone(), fixtures.join("allowlist_basic.toml"));
    let ctx = ProjectDocToolContext { service: &service };
    let registry = ToolRegistry::default();

    // Use the same allowlisted fixture path Phase 2's read test proved
    // returns content. Replace FIXTURE_DOC_PATH with that repo-relative path.
    const FIXTURE_DOC_PATH: &str = /* the path Phase 2's read test used */;

    let request = ToolRequest {
        tool_name: crate::tools::READ_PROJECT_DOC_TOOL_NAME.to_string(),
        input: FIXTURE_DOC_PATH.to_string(),
        structured: Some(serde_json::json!({ "path": FIXTURE_DOC_PATH })),
        permission: ToolPermission::read_only(),
        requested_by: "test".to_string(),
    };

    let result = registry.execute(&request, &ctx).unwrap();
    assert_eq!(result.tool_name, crate::tools::READ_PROJECT_DOC_TOOL_NAME);
    assert_eq!(result.category, crate::tools::ToolCategory::ReadOnly);
}
```

The read dispatch test exists because Phase 3's acceptance says
*both* tools must route through `dispatch` — testing only the search
arm would let a missing or wrong `read_project_doc` dispatch arm slip
through even when the metadata and definition arms were added correctly
(review finding M1).

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tools::tool_registry`
Expected: all four new tests FAIL (registry does not yet know the two
tools — `metadata_for` returns `None`, so `validate_request`/`execute`
bail with "unknown tool", and `model_tool_definitions_for` yields 0
definitions).

- [ ] **Step 3: Implement the extension.**

Add imports near the existing tool imports at the top of
`tool_registry.rs`:

```rust
use super::read_project_doc_tool::ReadProjectDocTool;
use super::search_project_docs_tool::SearchProjectDocsTool;
```

(The name constants are reached via `super::SEARCH_PROJECT_DOCS_TOOL_NAME`
and `super::READ_PROJECT_DOC_TOOL_NAME`, mirroring how the existing arms
use `super::CALCULATOR_TOOL_NAME` and `super::RECALL_TURN_TOOL_NAME`.)

Extend the struct and `Default`:

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

Add one arm to each of the three `match` sites, mirroring the existing
calculator/recall_turn arms. **Match the exact shape of the existing
arm at each site** — in particular, if `model_tool_definitions_for`
wraps each definition in `Some(...)` (or pushes into a `Vec`, or
filters `Option`s), copy that wrapping rather than the bare-call
snippets shown below; a literal paste of an unwrapped call will not
compile against a `Some`-wrapping arm (review finding N1). The snippets
below show the *call*, not necessarily the surrounding wrapper:

- in `metadata_for` (existing arms return `Some(...)`, so mirror that):
  ```rust
  super::SEARCH_PROJECT_DOCS_TOOL_NAME => Some(self.search_project_docs.metadata()),
  super::READ_PROJECT_DOC_TOOL_NAME => Some(self.read_project_doc.metadata()),
  ```
- in `dispatch`:
  ```rust
  super::SEARCH_PROJECT_DOCS_TOOL_NAME => self.search_project_docs.execute(request, ctx),
  super::READ_PROJECT_DOC_TOOL_NAME => self.read_project_doc.execute(request, ctx),
  ```
- in `model_tool_definitions_for` (wrap to match the existing arm's
  shape — e.g. `Some(self.search_project_docs.model_tool_definition())`
  if the existing arms yield `Option`):
  ```rust
  super::SEARCH_PROJECT_DOCS_TOOL_NAME => self.search_project_docs.model_tool_definition(),
  super::READ_PROJECT_DOC_TOOL_NAME => self.read_project_doc.model_tool_definition(),
  ```

- [ ] **Step 4: Verify the re-exports already exist.**

Confirm `crates/qsf_app/src/tools/mod.rs` already exposes
`SEARCH_PROJECT_DOCS_TOOL_NAME`, `SearchProjectDocsTool`,
`READ_PROJECT_DOC_TOOL_NAME`, and `ReadProjectDocTool` (it does, from
Phase 2). No edit expected here; if a re-export is missing, add it.

- [ ] **Step 5: Run tests.**

Run: `cargo test -p qsf_app tools::tool_registry`
Expected: PASS (all four new tests, plus the existing calculator /
recall_turn / permission tests unchanged).

- [ ] **Step 6: Commit.**

This commit is the code-only registry slice. Per the diary-discipline
note below, only commit it on an **unmerged feature branch** whose
Phase 9 diary entry will land before review/merge; if Phase 3 is to be
merged independently ahead of that, add a short standalone Phase 3
diary entry (see the diary note) *before* opening the merge.

```bash
git add crates/qsf_app/src/tools/tool_registry.rs
git commit -m "feat(tools): wire project-doc tools into ToolRegistry"
```

### Phase 3 verification

Per `Agents.md` the build command is `cargo build`, so run it first,
then the focused tests, then the lint/format gates:

```bash
cargo build
cargo test -p qsf_app tools::
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expect all clean. (`cargo test`/`clippy` compile the crate too, but
running `cargo build` explicitly keeps this phase aligned with the
documented repo workflow — review finding L2.)

**Diary discipline for this phase.** As with Phase 2, the *application*
work of Phases 1-8 is grouped under a single Phase 9 diary entry, so
Phase 3 is not considered complete or mergeable until that entry lands.
The Step 6 commit is therefore intended for an unmerged feature branch.
If Phase 3 is instead merged in isolation ahead of the grouped feature,
a short standalone Phase 3 diary entry **must** accompany that merge
(read the *Instructions how to use* at the top of the diary first), to
satisfy the repo requirement that implementation changes are documented
in `EngineeringDiary.md`. Do not merge Phase 3 in isolation with no
diary entry at all (review finding M2).

**Acceptance criteria for Phase 3:**

- `ToolRegistry::default()` constructs with `search_project_docs` and
  `read_project_doc` fields.
- `metadata_for` returns `Some(_)` for both new tool names, and the
  returned metadata advertises `ReadOnly`/`ReadOnly` for both (Task 3.1
  test).
- `model_tool_definitions_for(&[search, read])` returns exactly 2
  definitions, named `search_project_docs` and `read_project_doc`
  (Task 3.1 test).
- `dispatch` routes **both** new names to their tools: a `read_only()`
  search request and a `read_only()` read request executed through
  `registry.execute(...)` against a `ProjectDocToolContext` each
  succeed and return a `ReadOnly` result (Task 3.1 tests). This also
  confirms `validate_request` admits the tools under `read_only()`
  permission.
- The calculator, recall_turn, and permission-rejection tests still pass
  unchanged.
- No runtime call site changed; the combined-context build is
  deliberately deferred to Phase 4 (per the scope boundary above).
- `cargo build`, `cargo test -p qsf_app tools::`, `cargo clippy
  --all-targets -- -D warnings`, and `cargo fmt` are all clean.

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
**Build it first** if it does not — see Open Question #2 for the
recommended shapes and the decision to confirm.

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
    // mirror the pattern used by the existing tool_dispatch tests.

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
setup mirrors the existing test patterns in the same file — see the
`tests` module that already builds a `RunContext`, `ToolRegistry`,
`SessionToolContext`, and `ModelRequest`. The live calls here need the
combined context, not the session-only one. If the harness gets large,
write a focused integration test under
`crates/qsf_app/tests/project_doc_dispatch.rs` that builds a
`RunContext`, `ModelRequest`, `ToolRegistry`, and combined `ToolContext`,
then calls `dispatch_model_tool_calls` directly.)

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
`project_doc_service()` — see Open Question #2. If that combined context
does not yet exist, build it before extending `allowed_tools`.

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
the Phase 2 / Phase 3 diary discipline), reconcile rather than
duplicate them here.

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

- [ ] `cargo build`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt`
- [ ] `cargo test -p qsf_app`
- [ ] Verify the production allowlist excludes `docs/Reviews/**` and
  `docs/EngineeringDiary.md` (Phase 1 tests should already cover this
  in CI).
- [ ] Verify the bounded read rejects `..` traversal and absolute paths
  (Phase 1 read tests should already cover this in CI).
- [ ] Verify `read_project_doc` clamps an above-cap `max_tokens` to
  `MAX_TOKENS_HARD_CAP` and `search_project_docs` normalizes
  `max_results` into `1..=DEFAULT_MAX_RESULTS` (Phase 2 tests should
  already cover both in CI).
- [ ] Verify the `ToolRegistry` routes **both** project-doc tools
  through `metadata_for`, `dispatch`, and `model_tool_definitions_for`,
  with metadata advertising `ReadOnly`/`ReadOnly` and definitions
  returned under the correct names (Phase 3 tests should already cover
  this in CI — search *and* read dispatch tests).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  since Phase 1 was committed independently, a standalone library-slice
  entry plus the Phases 2-8 entry), with any isolated-merge diary
  entries reconciled rather than duplicated.
- [ ] Confirm Phase 11 remains a follow-on planning handoff unless it has
  been promoted into a separate detailed design or implementation plan.