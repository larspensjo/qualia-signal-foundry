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

Phases 1-3 have landed and are committed:

- **Phase 1** — the pure `ProjectDocService` library
  (`crates/qsf_app/src/project_docs/`).
- **Phase 2** — the two `Tool` implementations, `ToolPermission::read_only()`,
  the defaulted `ToolContext::project_doc_service()` accessor, and the
  standalone `ProjectDocToolContext`.
- **Phase 3** — both tools wired into `ToolRegistry` (struct, `Default`,
  and the three `match` sites in `metadata_for`, `dispatch`, and
  `model_tool_definitions_for`).

**Phase 4 (true per-turn project-doc budget state, plus the combined
`ToolContext` needed for live dispatch) is the next implementation
step.** Phase 6 then uses that budget state in a bounded two-round
responder tool loop so the model can search first, read after seeing
search results, and still stay inside one human turn's caps.

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

Code anchors:

- `crates/qsf_app/src/project_docs/` — **landed (Phase 1).** Pure
  library: `Allowlist`, metadata extraction, lexical `search`, bounded
  `read`, and the `ProjectDocService` facade. Later phases consume it;
  they do not modify it.
- `crates/qsf_app/src/tools/mod.rs` — re-exports the tool surface,
  including `ProjectDocToolContext`, `SEARCH_PROJECT_DOCS_TOOL_NAME`,
  `SearchProjectDocsTool`, `READ_PROJECT_DOC_TOOL_NAME`, and
  `ReadProjectDocTool` (Phase 2). **Phase 4** adds a re-export for the
  new combined `ToolContext`.
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`, `Tool`
  trait, `ToolMetadata`, the `ToolContext` trait (both
  `session_state()` and `project_doc_service()` defaulted to `None`),
  and `EmptyToolContext`. **Landed (Phase 3):** the registry struct,
  `Default`, and all three `match` sites route all four tools; tests
  assert metadata/definition identity and dispatch for both project-doc
  tools. No change expected here in Phase 4.
- `crates/qsf_app/src/tools/tool_request.rs` — `ToolPermission` has both
  `compute_only()` and `read_only()` (Phase 2), plus `ToolRequest`,
  `ToolCategory`, `ToolSideEffectLevel`.
- `crates/qsf_app/src/tools/tool_result.rs` — `ToolResult` with fields
  `tool_name`, `category`, `side_effect_level`, `input`, `output_text`,
  `numeric_value`, `observation_summary`.
- `crates/qsf_app/src/tools/recall_turn_tool.rs` — defines
  `RecallTurnTool` **and** `SessionToolContext<'a> { state: &'a
  SessionState }` (implements `session_state()` only). Reference for the
  `Tool` trait and for the single-accessor context shape.
- `crates/qsf_app/src/tools/project_doc_tool.rs` — **landed (Phase 2).**
  `ProjectDocToolContext<'a> { service: &'a ProjectDocService }`
  (implements `project_doc_service()` only).
- `crates/qsf_app/src/tools/search_project_docs_tool.rs` and
  `crates/qsf_app/src/tools/read_project_doc_tool.rs` — **landed (Phase
  2), wired (Phase 3).** The two `Tool` impls, now routed by the
  registry.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls(context, request, registry, state_ctx,
  project_doc_budget, tool_calls)`. Its loop checks `allowed_tools`,
  builds a `ToolRequest`
  via `tool_request_from_model_tool_call` (whose catch-all `_ =>` arm
  already builds correct requests for the two project-doc tools now that
  the registry knows them), records `ToolRequested`, then
  `validate_and_execute` + `ToolCompleted`/`ToolFailed`. **Phase 4**
  adds a `ProjectDocToolBudget` that tracks true per-human-turn counts
  across dispatch batches; **Phase 5** adds success traces.
- `crates/qsf_app/src/models/model_role.rs` — `ModelRole::predefined`
  for `ConversationalResponder`; `allowed_tools` is overridden by call
  sites (see `multi_turn_text_loop.rs`).
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — reference
  for how a `ToolResult` becomes a `ModelMessage::tool_result` and is
  appended before the next provider call; **Phase 6** call site that
  constructs the `ResponderToolContext`, creates one
  `ProjectDocToolBudget` per human turn, and permits a bounded
  two-round tool loop (`search` then optional `read`) before the final
  responder reply.
- `crates/qsf_app/src/observability/trace.rs` — `TraceRecord::new(
  experiment_id, operation, input_summary, output_summary)` with
  builder methods `.with_details(Value)` and `.with_latency_ms(u64)`;
  new operations (`project_doc_search`, `project_doc_read`) ride in
  `operation` + `details`, no schema change required.
- `crates/qsf_app/src/observability/event_log.rs` —
  `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed`. No new
  event type is added.
- `crates/qsf_app/src/runtime/run_context.rs` — `RunContext` exposes
  `experiment_id()`, `run_id()`, `record_event(EventType, Value, None)`,
  and `record_trace(TraceRecord) -> Result<TraceRecord>`.

Documentation anchors:

- `docs/Plans/Design.ProjectDocIntrospection.md` — the spec this plan
  implements.
- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md` — broader
  brainstorm (updated in Phase 9).
- `docs/ProjectFrame/DocumentStatus.md` — defines `kind` and
  `maturity_tag` taxonomies; updated in Phase 9 to reference the
  allowlist file.
- `docs/Architecture/Architecture.ToolSystem.md` — its *Implementation
  Status* section is refreshed in Phase 9 to move the two new tools to
  "Implemented today".

## Open Questions To Surface During Implementation

Per `Agents.md`, ambiguities should be surfaced rather than silently
resolved. The plan picks a default for each; if any plays out
differently, raise it before changing direction.

1. **Config file path.** This plan uses
   `config/project-doc-introspection.toml` at the repo root (settled in
   Phase 1). *Path-resolution note (still binding):* `cargo test` runs
   with the working directory at the package root
   (`crates/qsf_app`), **not** the workspace root, so tests and
   production code must never load the config via a bare relative path.
   Tests resolve it from `CARGO_MANIFEST_DIR`; production wiring (Phase 6
   onward) must construct `ProjectDocService` with an explicit absolute
   repo root and an explicit absolute allowlist path, never relying on
   the process working directory.
2. **Combined `ToolContext` shape — DECIDED IN PHASE 4.** Live dispatch
   needs one context answering *both* `session_state()` (for
   `recall_turn`) and `project_doc_service()` (for the project-doc
   tools), because the `ConversationalResponder` advertises them
   together and `dispatch_model_tool_calls` threads a single
   `&dyn ToolContext` per batch. The existing single-accessor contexts
   (`SessionToolContext`, `ProjectDocToolContext`) each answer only one.
   The two candidate shapes are (a) extend `SessionToolContext` with an
   optional `&ProjectDocService`, or (b) add a dedicated combined
   context implementing both accessors. **Phase 4 Task 4.1 picks (b)**
   (a dedicated combined context) because it is purely additive: option
   (a) would force every existing literal `SessionToolContext { state }`
   construction (dispatch tests, future call sites) to gain a new field.
   The decision callout in Task 4.1 records this; if a reviewer prefers
   (a), raise it before Task 4.1 wiring.
3. **`influenced_reply` storage.** Phase 8 writes the marker as a
   follow-up `TraceRecord` referencing the original by `trace_id`. If an
   annotation on the original record is preferred, raise before Phase 8.
4. **Module naming.** Per `Agents.md`, name modules after stable
   behavior. This plan uses `project_docs` / `ProjectDocService`, and
   the Phase 4 combined context is named for the role it serves
   (`ResponderToolContext`), not a plan phase. Keep this discipline.
5. **Hard latency cap.** Decision 4 of the spec sets a 1500 ms hard
   cap. With lexical search over a small markdown corpus the cap is not
   expected to fire, so this plan **deliberately defers**
   cap-enforcement: `ProjectDocService` exposes synchronous
   `search`/`read` with no deadline parameter. Record this as a
   conscious scope decision in `Design.ProjectDocIntrospection.md`
   Decision 4 (one line in Phase 9). If real-run traces ever show
   `latency_ms` over 1000, add enforcement **at the `ProjectDocService`
   boundary**: thread a deadline / max-elapsed budget through
   `search`/`read`, return partial results, and surface an
   `omitted_due_to_budget` signal Phase 5's trace emission can record.
   Note the change in the diary when it happens.
6. **Integration-test setup in Tasks 4.x and 5.1.** The plan still uses
   compact test sketches rather than fully inlined harness code, but the
   assertions are binding. Mirror the existing `tool_dispatch.rs` tests:
   `RunContext::create_in`, `ToolRegistry::default()`,
   `ModelRequest::new(...).with_session_id(...).with_tools(...)`, parsed
   `EventRecord`s from `events.jsonl`, and parsed `TraceRecord`s from
   `traces.jsonl`. Phase 4 tests must prove both returned
   `ToolResult`s and persisted telemetry, including the two-batch
   shared-budget case. If the inline test module grows unwieldy, write a
   focused integration test under
   `crates/qsf_app/tests/project_doc_dispatch.rs` and treat the
   skeletons as the assertion contract.
7. **Per-turn budget scope — DECIDED AFTER PHASE 4 PLAN REVIEW.** The
   caps apply across the whole human/responder turn, not merely one
   provider tool-call batch. Phase 4 therefore introduces explicit
   `ProjectDocToolBudget` state, keyed by the current `turn_index`, and
   threads a mutable budget through each dispatch call. Phase 6 then
   reuses the same budget across a bounded two-round responder tool
   loop. If a future generic tool runtime changes the loop structure, it
   must preserve this invariant: fresh budget per human turn, shared
   budget across all provider calls inside that turn.

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
  -> next provider call with tools still available; model may then call
     read_project_doc
  -> dispatch checks per-turn cap, runs ReadProjectDocTool
  -> ProjectDocService returns focused DocRead under budget
  -> ToolResult, ToolCompleted, TraceRecord
     (operation = "project_doc_read") emitted
  -> final provider call is made without tools; provider produces the
     human-facing reply with kind/maturity hedging
  -> post-hoc enrichment pass marks influenced_reply on traces whose
     content overlapped the final reply
```

---

## Phase 1: `ProjectDocService` library — completed

**Status: landed and committed** (git: "ProjectDocIntrospection phase 1").
Source of truth is the code under `crates/qsf_app/src/project_docs/`.

**What shipped:** a pure, side-effect-free `project_docs` module
(declared by `pub mod project_docs;` in `lib.rs`) with the public surface
later phases depend on — `types` (`DocKind`, `MaturityTag`,
`MatchStrength`, `DocHit`, `DocRead`), `allowlist`
(`Allowlist::from_file`/`from_str`, exclude-then-include globs),
`metadata` (`kind_for_path`, `maturity_for`, `last_reviewed_for` scoped
to `## Implementation Status`, ISO-date enforced), `search`, `read`, and
the `ProjectDocService` facade (`new(repo_root, allowlist_path)`,
`.search`, `.read`, `.allowlist()` re-read per call for hot-reload,
`.repo_root()`). Deps added: `globset`, `toml`, `regex`, `once_cell`,
`walkdir`, `tempfile` (dev). Production allowlist:
`config/project-doc-introspection.toml`.

**Lessons / constraints still binding on later phases:**

- **Path resolution (OQ #1).** Tests resolve paths from
  `CARGO_MANIFEST_DIR`; production wiring (Phase 6 on) must use
  **absolute** repo-root and allowlist paths.
- **Path-safety lives in the library, not the tool.** Bounded `read`
  normalizes and confines caller-supplied paths *before* touching the
  allowlist or filesystem (rejects absolute paths and any `..`); the
  `read_project_doc` tool forwards the raw `path` and re-implements no
  guards.
- **Allowlist hot-reload + production defaults.** Excludes
  `docs/EngineeringDiary.md` and `docs/Reviews/**`; admits
  `docs/ProjectFrame/**` and `docs/DecisionLog.md`; picks up edits
  without a rebuild.
- **Latency cap deferred (OQ #5).** Synchronous API, no deadline
  parameter; if traces show `latency_ms` over 1000, enforce at the
  service boundary.

**Acceptance outcome (met):** `cargo test -p qsf_app project_docs`
passes (allowlist precedence; kind/maturity/last-reviewed extraction with
Implementation-Status scoping and malformed-date rejection;
heading-first lexical search; bounded read with focus/truncation;
traversal/absolute-path refusals; service-level hot-reload). Clippy and
fmt clean.

**Diary follow-up constraint:** Phase 1 was committed as a standalone
slice. The Phase 9 diary pass must explicitly account for the
`project_docs` library work (fold into the Phases 1-8 entry or add a
separate library-slice entry); do not silently skip it.

---

## Phase 2: Tool implementations — completed

**Status: landed and committed** (git: "ProjectDocIntrospection phase 2").
Source of truth is the code under `crates/qsf_app/src/tools/`.

**What shipped:**

- `ToolPermission::read_only()` (grants `ReadOnly` category + `ReadOnly`
  max side-effect level), with tests asserting it admits
  `ReadOnly`/`ReadOnly` and rejects `WriteCapable`/`ExternalWrite` and
  `ComputeOnly`/`None`.
- A defaulted `ToolContext::project_doc_service() -> Option<&ProjectDocService>`
  (returns `None`) on the trait; `EmptyToolContext` and
  `SessionToolContext` unchanged via the default.
- `ProjectDocToolContext<'a> { service: &'a ProjectDocService }`
  returning `Some(service)`.
- `SearchProjectDocsTool` — reads `query`/`max_results` from
  `ToolRequest::structured`, calls `service.search`, serializes
  `Vec<DocHit>`; **normalizes `max_results` into `1..=DEFAULT_MAX_RESULTS`**.
  Advertises `ReadOnly`/`ReadOnly` with a documented JSON schema.
- `ReadProjectDocTool` — reads `path`/`focus`/`max_tokens`, calls
  `service.read`, serializes `DocRead`; **clamps `max_tokens` to
  `MAX_TOKENS_HARD_CAP` (4000)** in sync with the schema `maximum`;
  forwards the raw `path` (path safety enforced by the Phase 1 library).
- `tools/mod.rs` re-exports `ProjectDocToolContext`,
  `SEARCH_PROJECT_DOCS_TOOL_NAME`, `SearchProjectDocsTool`,
  `READ_PROJECT_DOC_TOOL_NAME`, `ReadProjectDocTool`.

**Lessons / constraints still binding on later phases:**

- **Combined context required for live dispatch (OQ #2).** The
  standalone `ProjectDocToolContext` is enough for unit tests and for
  Phase 3's registry wiring, but live dispatch (responder advertising
  `recall_turn` *and* the project-doc tools) needs one context answering
  both accessors. **Built in Phase 4.**
- **Upper-bound discipline is the tool's job.** `max_results` and
  `max_tokens` are clamped/normalized inside the tools; later phases must
  not assume the model honors the advertised schema.
- **Diary discipline.** The *application* work of Phases 1-8 is grouped
  under a single Phase 9 diary entry; if any phase merges in isolation it
  carries a short standalone entry, reconciled (not duplicated) in
  Phase 9.

**Acceptance outcome (met):** `cargo test -p qsf_app tools::` passes
(read-only permission, both context accessors, search hit/metadata +
`max_results` normalization + missing-context failure, read content +
`max_tokens` clamp + out-of-allowlist refusal + missing-context
failure). Clippy and fmt clean.

---

## Phase 3: `ToolRegistry` wiring — completed

**Status: landed and committed** (git: "ProjectDocIntrospection phase 3:
wire project-doc tools into ToolRegistry"). Source of truth is
`crates/qsf_app/src/tools/tool_registry.rs`.

**What shipped:** the registry now knows all four tools. The
`ToolRegistry` struct and its `Default` carry `search_project_docs:
SearchProjectDocsTool` and `read_project_doc: ReadProjectDocTool`
fields, and all three `match` sites — `metadata_for`, `dispatch`, and
`model_tool_definitions_for` — route both new tool names (imported via
`super::SEARCH_PROJECT_DOCS_TOOL_NAME` / `super::READ_PROJECT_DOC_TOOL_NAME`,
no duplicated string literals). Because `tool_request_from_model_tool_call`
already routes unrecognized tools through its catch-all `_ =>` arm, the
dispatch request-builder needed no change for this phase.

**Lessons / constraints still binding on later phases:**

- The registry holds no `ToolContext`; it receives one per call, so the
  combined-context question (OQ #2) was deliberately out of scope here
  and is settled in **Phase 4**, the first phase to dispatch these tools
  through a live context alongside `recall_turn`.
- No runtime call site changed in Phase 3.

**Acceptance outcome (met):** the registry tests assert that
`metadata_for` returns `Some` advertising `ReadOnly`/`ReadOnly` for both
tools (by name, not just presence); that
`model_tool_definitions_for(&[search, read])` returns exactly the two
definitions under the correct names; and that **both** tools route
through `dispatch` (a `read_only()` search request and a `read_only()`
read request executed via `registry.execute(...)` against a
`ProjectDocToolContext` each succeed and return a `ReadOnly` result,
confirming `validate_request` admits them under `read_only()`). The
calculator, recall_turn, and permission-rejection tests pass unchanged.
`cargo build`, `cargo test -p qsf_app tools::`, `cargo clippy
--all-targets -- -D warnings`, and `cargo fmt` are clean.

---

## Phase 4: Combined context + true per-turn budget state

This phase does two things, in order:

1. **Task 4.1** introduces the combined `ToolContext` that live dispatch
   needs (OQ #2) — a single context answering both `session_state()` and
   `project_doc_service()`. It is a prerequisite for any batch that
   advertises `recall_turn` alongside the project-doc tools, and it is
   exercised by the cap tests in Task 4.2.
2. **Task 4.2** introduces explicit per-turn project-doc budget state and
   extends `dispatch_model_tool_calls` to consume that state. The budget
   is owned by the caller for one human/responder turn and reused across
   all provider tool-call batches inside that turn. Excess
   `search_project_docs` / `read_project_doc` calls fail fast — with a
   `ToolFailed` event, a refusal `TraceRecord`, and a structured
   `ToolResult` — instead of reaching the registry.

Follow `superpowers:test-driven-development` (or plain TDD if that skill
is unavailable): the failing test precedes the implementation. Keep the
context changes additive; do not touch `SessionToolContext`,
`ProjectDocToolContext`, or the registry. The dispatcher signature does
change in Task 4.2 so callers can pass explicit budget state.

### Task 4.1: Combined `ToolContext` for live dispatch

**Decision callout (OQ #2 — confirm before wiring).** This task adds a
**dedicated** combined context rather than extending `SessionToolContext`.
Rationale grounded in the current code: `SessionToolContext { state }` is
a plain pub-field struct constructed by literal everywhere it is used
(e.g. the `tool_dispatch.rs` tests, and the Phase 6 responder call
site); adding a field would force every such literal to change, whereas a
new struct is purely additive and leaves the existing single-accessor
contexts intact for their isolated unit tests. If a reviewer prefers
extending `SessionToolContext` (carrying an `Option<&ProjectDocService>`)
instead, raise it before implementing this task. Per `Agents.md`, the new
type is named for the role it serves, not the plan phase.

**Files:**
- Create: `crates/qsf_app/src/tools/responder_tool_context.rs`
- Modify: `crates/qsf_app/src/tools/mod.rs` (declare the module and
  re-export the type)

- [ ] **Step 1: Write the failing test.**

In the new file's `#[cfg(test)] mod tests` block, assert the combined
context answers both accessors. Mirror the accessor-test style in
`project_doc_tool.rs`.

```rust
// crates/qsf_app/src/tools/responder_tool_context.rs (tests)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_docs::ProjectDocService;
    use crate::session::{SessionConfig, SessionState};
    use crate::tools::tool_registry::ToolContext;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project_docs/fixtures")
    }

    #[test]
    fn responder_context_answers_both_accessors() {
        let service =
            ProjectDocService::new(fixtures_root(), fixtures_root().join("allowlist_basic.toml"));
        let state = SessionState::new(/* a minimal test SessionConfig */);
        let ctx = ResponderToolContext { state: &state, project_docs: &service };

        assert!(ctx.session_state().is_some());
        assert!(ctx.project_doc_service().is_some());
    }
}
```

- [ ] **Step 2: Run the test; verify it fails to compile** (the type
  does not yet exist).

Run: `cargo test -p qsf_app tools::responder_tool_context`

- [ ] **Step 3: Implement the combined context.**

```rust
// crates/qsf_app/src/tools/responder_tool_context.rs
use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use super::tool_registry::ToolContext;

/// Tool context for the ConversationalResponder, which advertises
/// `recall_turn` (needs session state) alongside the project-doc tools
/// (need the project-doc service). Holds borrows of both so a single
/// `&dyn ToolContext` can serve a mixed batch.
pub struct ResponderToolContext<'a> {
    pub state: &'a SessionState,
    pub project_docs: &'a ProjectDocService,
}

impl ToolContext for ResponderToolContext<'_> {
    fn session_state(&self) -> Option<&SessionState> {
        Some(self.state)
    }

    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        Some(self.project_docs)
    }
}
```

In `crates/qsf_app/src/tools/mod.rs`, declare the module alongside the
other tool modules and re-export the type:

```rust
mod responder_tool_context;
pub use responder_tool_context::ResponderToolContext;
```

(If `project_docs` is `pub(crate)` rather than `pub` for both
accessors, match the visibility the existing accessors expect — the
trait already exposes both, so no trait change is needed.)

- [ ] **Step 4: Run the test.** Expected: PASS.

> **Note on scope.** The combined context is *constructed* at the Phase 6
> call site; Task 4.1 only defines and exports the type and proves it
> satisfies both accessors. Task 4.2 below uses it in the cap tests to
> prove a mixed `recall_turn` + project-doc batch dispatches through one
> context, de-risking Phase 6.

- [ ] **Step 5: Commit** (on the unmerged feature branch — see the diary
  discipline note at the end of this phase).

```bash
git add crates/qsf_app/src/tools/responder_tool_context.rs \
        crates/qsf_app/src/tools/mod.rs
git commit -m "feat(tools): add ResponderToolContext combining session + project-doc accessors"
```

### Task 4.2: Explicit per-turn budget enforcement

**Files:**
- Modify: `crates/qsf_app/src/models/tool_dispatch.rs`

Caps per human/responder turn:
- `search_project_docs`: 2
- `read_project_doc`: 1

Both defaults exercise the new code path on the third search / second
read call (`Agents.md`: defaults must exercise the new path).

**Implementation decision (review blocker P4-001).** Do **not** use local
per-batch counters. Add named module-level constants and a small budget
state type:

```rust
pub const PROJECT_DOC_SEARCH_CAP_PER_TURN: usize = 2;
pub const PROJECT_DOC_READ_CAP_PER_TURN: usize = 1;

#[derive(Clone, Debug)]
pub struct ProjectDocToolBudget {
    pub turn_index: usize,
    search_calls: usize,
    read_calls: usize,
}

impl ProjectDocToolBudget {
    pub fn new(turn_index: usize) -> Self {
        Self {
            turn_index,
            search_calls: 0,
            read_calls: 0,
        }
    }
}
```

`dispatch_model_tool_calls` receives `&mut ProjectDocToolBudget` and
increments it for project-doc tools only. Callers create one fresh budget
per human/responder turn. Tests that call dispatch directly should create
`ProjectDocToolBudget::new(test_turn_index)`.

- [ ] **Step 1: Write the failing tests.**

Add to the `#[cfg(test)] mod tests` block in `tool_dispatch.rs`, mirroring
the existing harness there (`RunContext::create_in`, `ToolRegistry::default()`,
a `ModelRole::predefined(ModelRoleId::ConversationalResponder)` with
`allowed_tools` set, `model_tool_definitions_for`, and
`ModelRequest::new(...).with_session_id(...).with_tools(...)`). Use the
`ResponderToolContext` from Task 4.1 as the dispatch context, with the
Phase 1/2 fixture service (`src/project_docs/fixtures`,
`allowlist_basic.toml`) and a fixture `SessionState`. The search calls use
the `"Maturity"` query Phase 2 proved returns hits; the read call uses the
`"sample_concept.md"` fixture path Phase 3's read test used.

Make the harness concrete enough that it proves telemetry, not just return
values: parse `events.jsonl` into existing `EventRecord` values and parse
`traces.jsonl` into `TraceRecord` values. Assertions should check both the
returned `ToolResult`s and the persisted event/trace details.

```rust
#[test]
fn third_search_call_in_one_turn_is_refused() {
    // allowed_tools = [SEARCH_PROJECT_DOCS_TOOL_NAME, READ_PROJECT_DOC_TOOL_NAME]
    // Emit three search_project_docs calls in one batch with one
    // ProjectDocToolBudget.
    // Assert:
    //   - results.len() == 3
    //   - results[0] and results[1] are real ReadOnly results
    //   - results[2].tool_name == SEARCH_PROJECT_DOCS_TOOL_NAME
    //   - results[2].observation_summary contains "per_turn_cap"
    //   - events.jsonl has a ToolFailed event for the third call whose
    //     payload.refusal_reason == "per_turn_cap",
    //     payload.cap == PROJECT_DOC_SEARCH_CAP_PER_TURN, and
    //     payload.attempted_count == 3
    //   - traces.jsonl has a "project_doc_search" record with
    //     details.refused == true, details.call_id, details.session_id,
    //     details.tool_name, details.turn_index, details.cap,
    //     details.attempted_count, and sanitized details.arguments.query
}

#[test]
fn second_read_call_in_one_turn_is_refused() {
    // Same shape with two read_project_doc calls; second is refused with
    // a "project_doc_read" refusal trace containing sanitized
    // details.arguments.path.
}

#[test]
fn budget_persists_across_dispatch_batches_in_same_turn() {
    // Create one ProjectDocToolBudget::new(7).
    // First dispatch batch: one read_project_doc call succeeds.
    // Second dispatch batch, using the same budget: another read_project_doc
    // call is refused with attempted_count == 2 and turn_index == 7.
    // This guards the review blocker: caps are per human turn, not per
    // provider batch.
}

#[test]
fn fresh_budget_allows_next_turn_to_use_caps_again() {
    // Exhaust a read budget for turn 7, then create
    // ProjectDocToolBudget::new(8) and verify a read succeeds.
}

#[test]
fn mixed_batch_runs_recall_turn_and_project_doc_through_one_context() {
    // allowed_tools includes recall_turn + both project-doc tools.
    // Batch: one recall_turn (on a summarized turn, as in the existing
    // dispatcher_executes_allowed_recall_turn_tool test) + one search.
    // Assert both succeed through a single ResponderToolContext and one
    // ProjectDocToolBudget, proving the combined context serves a mixed
    // batch (OQ #2) while non-project-doc tools do not consume the budget.
}
```

The traces are read from `context.run_dir().join("traces.jsonl")` (the
events tests already read `events.jsonl` the same way). If wiring grows
unwieldy, lift these into
`crates/qsf_app/tests/project_doc_dispatch.rs` per OQ #6.

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app tool_dispatch`
Expected: FAIL (the dispatcher does not yet accept explicit budget state;
the third search / second read currently run instead of being refused).

- [ ] **Step 3: Implement the cap.**

Add imports to `tool_dispatch.rs`:

```rust
use crate::observability::trace::TraceRecord;
use crate::tools::{
    READ_PROJECT_DOC_TOOL_NAME, SEARCH_PROJECT_DOCS_TOOL_NAME, ToolCategory, ToolSideEffectLevel,
};
```

Declare the named cap constants and `ProjectDocToolBudget` near
`dispatch_model_tool_calls`, not as hidden locals inside the loop. This
keeps the policy discoverable for future tuning.

Change the dispatcher signature to accept the budget:

```rust
pub fn dispatch_model_tool_calls(
    context: &mut RunContext,
    request: &ModelRequest,
    registry: &ToolRegistry,
    state_ctx: &dyn ToolContext,
    project_doc_budget: &mut ProjectDocToolBudget,
    tool_calls: &[ModelToolCall],
) -> Result<Vec<ToolResult>> {
    // ...
}
```

Update existing tests and call sites to pass
`&mut ProjectDocToolBudget::new(turn_index)`. Direct tests can use a
fixed test turn index; the live loop in Phase 6 uses
`completed_turn_count(state)` / the `turn_index` already computed at the
start of `run_one_turn`.

Inside the loop, **after** the existing `allowed_tools` membership check
and **before** `tool_request_from_model_tool_call`, add a helper-backed
cap gate. Match the exact `ToolResult` field shape used by
`recall_turn_tool.rs` when building the refusal result:

```rust
let cap_check = match tool_call.name.as_str() {
    SEARCH_PROJECT_DOCS_TOOL_NAME => {
        project_doc_budget.record_search_attempt()
    }
    READ_PROJECT_DOC_TOOL_NAME => {
        project_doc_budget.record_read_attempt()
    }
    _ => ProjectDocCapCheck::not_applicable(),
};

if cap_check.over_cap {
    let is_search = tool_call.name == SEARCH_PROJECT_DOCS_TOOL_NAME;
    let operation = if is_search { "project_doc_search" } else { "project_doc_read" };
    let arguments = sanitized_project_doc_arguments(&tool_call.name, &tool_call.arguments);

    context.record_event(
        EventType::ToolFailed,
        json!({
            "session_id": &request.session_id,
            "role_id": request.role.role_id,
            "tool_name": &tool_call.name,
            "call_id": &tool_call.call_id,
            "turn_index": project_doc_budget.turn_index,
            "error": "per-turn budget exhausted",
            "refusal_reason": "per_turn_cap",
            "cap": cap_check.cap,
            "attempted_count": cap_check.attempted_count,
            "arguments": &arguments,
            "scope": "model_tool_dispatch",
        }),
        None,
    )?;
    context.record_trace(
        TraceRecord::new(
            context.experiment_id(),
            operation,
            "(refused)",
            "per_turn_cap",
        )
        .with_details(json!({
            "session_id": &request.session_id,
            "role_id": request.role.role_id,
            "call_id": &tool_call.call_id,
            "tool_name": &tool_call.name,
            "turn_index": project_doc_budget.turn_index,
            "refused": true,
            "refusal_reason": "per_turn_cap",
            "cap": cap_check.cap,
            "attempted_count": cap_check.attempted_count,
            "arguments": arguments,
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
            "{} refused: per_turn_cap (max {} call(s) per turn).",
            tool_call.name,
            cap_check.cap
        ),
    });
    continue;
}
```

Notes:
- `record_search_attempt` and `record_read_attempt` should increment
  before checking the cap so `attempted_count` is diagnostically useful
  (3 for the third search, 2 for the second read).
- `sanitized_project_doc_arguments` should preserve only stable,
  non-sensitive model inputs needed for replay: `query` / `max_results`
  for search and `path` / `focus` / `max_tokens` for read. Do not dump
  arbitrary JSON wholesale.
- The refusal path deliberately emits only `ToolFailed` (plus a refusal
  trace) and does **not** emit a preceding `ToolRequested`, because the
  call is rejected before a `ToolRequest` is built — it never reaches the
  registry. This keeps the existing `ToolRequested → ToolCompleted/Failed`
  symmetry intact for *executed* calls.
- Calculator and `recall_turn` fall through the `_ => false` arm and are
  unaffected; they do not consume `ProjectDocToolBudget`.

- [ ] **Step 4: Run tests.**

Run: `cargo test -p qsf_app tool_dispatch`
Expected: PASS (the five new tests, plus the existing dispatcher tests
unchanged).

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/models/tool_dispatch.rs
git commit -m "feat(dispatch): enforce per-turn caps for project-doc tools"
```

### Phase 4 verification

Per `Agents.md`, run the build first, then focused tests, then the
lint/format gates:

```bash
cargo build
cargo test -p qsf_app tools::responder_tool_context
cargo test -p qsf_app tool_dispatch
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expect all clean. No external/human testing is required for this phase —
it is pure Rust with deterministic unit/integration coverage. (Live
end-to-end behaviour is verified in Phase 7's battery and Phase 10's
manual session.)

**Diary discipline for this phase.** As with Phases 2-3, the *application*
work of Phases 1-8 is grouped under a single Phase 9 diary entry, so
Phase 4 is not considered complete or mergeable until that entry lands.
The commits above are intended for an unmerged feature branch. If Phase 4
is merged in isolation ahead of the grouped feature, a short standalone
Phase 4 diary entry **must** accompany that merge (read the *Instructions
how to use* at the top of `docs/EngineeringDiary.md` first); reconcile,
don't duplicate, in Phase 9.

**Acceptance criteria for Phase 4:**

- A combined `ResponderToolContext` exists and returns `Some(_)` from
  **both** `session_state()` and `project_doc_service()`; it is
  re-exported from `crate::tools`. `SessionToolContext` and
  `ProjectDocToolContext` are unchanged (Task 4.1).
- `ProjectDocToolBudget` exists with named module-level cap constants
  (`2` searches, `1` read) and is passed mutably into
  `dispatch_model_tool_calls` so one budget can be reused across
  dispatch batches in the same human/responder turn (Task 4.2).
- The 3rd `search_project_docs` call and the 2nd `read_project_doc`
  call are refused whether they occur in one batch or across two batches
  that share one `ProjectDocToolBudget`: each produces a `ToolFailed`
  event with `refusal_reason == "per_turn_cap"`, `turn_index`, `cap`,
  `attempted_count`, and sanitized arguments; a refusal `TraceRecord`
  (`operation` = `project_doc_search`/`project_doc_read`,
  `details.refused == true` with the same correlation fields); and a
  `ToolResult` whose `observation_summary` names `per_turn_cap`. The
  first 2 searches and the first read still execute normally, and a
  fresh budget for the next turn resets the counts (Task 4.2).
- A mixed batch (`recall_turn` + `search_project_docs`) dispatches
  successfully through a single `ResponderToolContext` and a
  `ProjectDocToolBudget`, proving the combined context serves both
  accessors and non-project-doc tools do not consume the budget
  (Task 4.2).
- Calculator, recall_turn, and existing dispatcher tests pass unchanged;
  no registry or library change was needed.
- `cargo build`, `cargo test -p qsf_app` (the relevant modules),
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` are
  clean.

---

## Phase 5: TraceRecord emission for successful project-doc calls

Phase 4 added refusal traces. This phase adds traces for the *successful*
search and read paths, so a researcher can replay every call.

### Task 5.1: Emit success traces

**Files:**
- Modify: `crates/qsf_app/src/models/tool_dispatch.rs`

In the success path of the dispatch loop, after the `ToolCompleted`
event is written, emit a `TraceRecord` for the two project-doc
operations. Calculator and recall_turn continue to behave as today.

- [ ] **Step 1: Write the failing test.**

```rust
// crates/qsf_app/src/models/tool_dispatch.rs (tests)
#[test]
fn successful_search_emits_project_doc_search_trace() {
    // Run one search_project_docs call through dispatch_model_tool_calls
    // with ProjectDocToolBudget::new(3).
    // Read traces.jsonl from context.run_dir().
    // Assert a TraceRecord with operation == "project_doc_search" and
    // details containing session_id, call_id, tool_name, turn_index == 3,
    // sanitized arguments.query, hits count, and refused == false.
}

#[test]
fn successful_read_emits_project_doc_read_trace() {
    // Same shape for read_project_doc, including sanitized
    // arguments.path/focus/max_tokens.
}
```

- [ ] **Step 2: Implement the emission.**

After the existing `ToolCompleted` event write in
`dispatch_model_tool_calls` (capture the elapsed latency the same way
the event does, via `elapsed_ms(started_at)`), branch on tool name:

```rust
let tool_latency_ms = elapsed_ms(started_at);
match tool_request.tool_name.as_str() {
    SEARCH_PROJECT_DOCS_TOOL_NAME => {
        let parsed_hits: serde_json::Value =
            serde_json::from_str(&result.output_text).unwrap_or_else(|_| json!([]));
        let hit_count = parsed_hits.as_array().map(|a| a.len()).unwrap_or(0);
        context.record_trace(
            TraceRecord::new(
                context.experiment_id(),
                "project_doc_search",
                tool_call.arguments.get("query").and_then(|v| v.as_str()).unwrap_or(""),
                format!("{hit_count} hit(s)"),
            )
            .with_details(json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "call_id": &tool_call.call_id,
                "tool_name": &tool_request.tool_name,
                "turn_index": project_doc_budget.turn_index,
                "arguments": sanitized_project_doc_arguments(
                    &tool_request.tool_name,
                    &tool_call.arguments
                ),
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
                tool_call.arguments.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                parsed.get("is_full").map(|v| format!("is_full={v}")).unwrap_or_else(|| "?".to_string()),
            )
            .with_details(json!({
                "session_id": &request.session_id,
                "role_id": request.role.role_id,
                "call_id": &tool_call.call_id,
                "tool_name": &tool_request.tool_name,
                "turn_index": project_doc_budget.turn_index,
                "arguments": sanitized_project_doc_arguments(
                    &tool_request.tool_name,
                    &tool_call.arguments
                ),
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

(Note: `started_at` is currently consumed inline by the `ToolCompleted`
event's `elapsed_ms(started_at)`. Bind `tool_latency_ms` once before the
event write and reuse it in both places so the event and the trace report
the same latency.) The success traces complement the `ToolCompleted`
event; they do not replace it.

- [ ] **Step 3: Run tests.** Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/models/tool_dispatch.rs
git commit -m "feat(dispatch): emit project_doc_search/read trace records on success"
```

---

## Phase 6: Wire the responder role + bounded two-round tool loop

Adds the two tools to the `ConversationalResponder` allowed-tools list
used by the multi-turn text loop (and, by extension, the unified
text/voice path once that lands), adds the bounded multi-round tool
loop needed for `search_project_docs` followed by `read_project_doc` in
the same human turn, and adds the always-on prompt block that teaches
the model when and how to use the tools.

This is the call site where `recall_turn`, `search_project_docs`, and
`read_project_doc` are advertised together, so the context constructed
here and passed to `dispatch_model_tool_calls` **must** be the
`ResponderToolContext` introduced in Phase 4 (answers both
`session_state()` and `project_doc_service()`). The `ProjectDocService`
must be built with absolute repo-root and allowlist paths (OQ #1), and
one `ProjectDocToolBudget::new(turn_index)` must be created per
human/responder turn and reused across every tool batch inside that
turn.

**Loop decision (review blocker P4-001, option 3).** The responder may
make at most **two provider tool-call batches** in one human turn:

1. initial provider call with tools advertised;
2. if it calls tools, dispatch the batch, append provider-native tool
   messages, and make one follow-up provider call with tools still
   advertised;
3. if that follow-up calls tools, dispatch the second batch with the
   same `ProjectDocToolBudget`, append tool messages, and make the
   final provider call **without tools**;
4. if any provider response after the two permitted tool batches still
   contains tool calls, record `ErrorOccurred` and fail the turn without
   appending it, preserving the existing "no unbounded tool loop"
   safety behavior.

This is deliberately narrower than a generic autonomous tool loop. It is
just enough for `search -> read -> answer`, while the Phase 4 budget
state enforces the 2-search / 1-read per-turn caps across both batches.

### Task 6.1: Extend `allowed_tools` and build responder tool context

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
  (and any other call site that constructs a `ConversationalResponder`
  request with explicit `allowed_tools` — grep for `allowed_tools`).

- [ ] **Step 1: Grep for current advertising patterns.**

```bash
grep -rn "allowed_tools" crates/qsf_app/src
```

Identify every call site that builds a request for the responder. The
multi-turn loop currently advertises `calculator` and `recall_turn`;
extend each such list to include `SEARCH_PROJECT_DOCS_TOOL_NAME` and
`READ_PROJECT_DOC_TOOL_NAME`.

- [ ] **Step 2: Write a test confirming the responder advertises the
  tools.**

```rust
#[test]
fn responder_advertises_project_doc_tools() {
    let role = build_conversational_responder_with_tools();
    assert!(role.allowed_tools.iter().any(|n| n == crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME));
    assert!(role.allowed_tools.iter().any(|n| n == crate::tools::READ_PROJECT_DOC_TOOL_NAME));
}
```

- [ ] **Step 3: Update the call site(s).**

Extend each existing `vec![...]` of tool names to include the two new
constants (imported from `crate::tools`). At each updated call site,
replace the `SessionToolContext` handed to `dispatch_model_tool_calls`
with a `ResponderToolContext { state: &state, project_docs: &service }`,
constructing the `ProjectDocService` once with absolute paths; otherwise
`recall_turn` or the project-doc tools will fail at runtime. Also update
the dispatch call to pass a `ProjectDocToolBudget`, using a fresh
budget for the existing single-batch path until Task 6.2 introduces the
two-round loop.

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/experiments
git commit -m "feat(responder): advertise project-doc tools in multi-turn loop"
```

### Task 6.2: Bounded two-round responder tool loop

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] **Step 1: Write the failing tests.**

Add tests near the existing multi-turn tool-call tests, using the same
mock-client style as `RepeatingToolCallClient` and the same assertions
over `events.jsonl`, `traces.jsonl`, and completed turn counts.

```rust
#[test]
fn responder_can_search_then_read_across_two_tool_batches() {
    // Mock client sequence for one user input:
    //   1. initial responder call returns search_project_docs
    //   2. follow-up responder call, after search tool result, returns read_project_doc
    //   3. final responder call, after read tool result, returns natural text
    // Assert one turn completed, both tool results were appended before the
    // final answer, ToolCompleted events exist for search and read, and the
    // project_doc_* traces share the same turn_index.
}

#[test]
fn responder_reuses_project_doc_budget_across_tool_batches() {
    // Mock client returns read_project_doc in batch 1 and read_project_doc
    // again in batch 2. The second read should be refused by the shared
    // ProjectDocToolBudget with attempted_count == 2, then the final
    // no-tools response should still complete the turn with the refusal
    // tool message in context.
}

#[test]
fn third_tool_batch_is_rejected_without_appending_turn() {
    // Mock client returns tool calls in the initial call, in the first
    // follow-up, and again in the final no-tools response. Assert an
    // ErrorOccurred event records that the bounded tool loop was exceeded
    // and no TurnCompleted event is appended for that input.
}

#[test]
fn ordinary_no_tool_response_still_completes_one_turn() {
    // Regression check: a normal answer with no tool calls still takes the
    // same path as before and does not create project-doc traces.
}
```

- [ ] **Step 2: Run tests; verify they fail.**

Run: `cargo test -p qsf_app multi_turn_text_loop`
Expected: FAIL (the current loop makes only one tool-follow-up request,
without tools, and rejects further tool calls).

- [ ] **Step 3: Implement the bounded loop.**

Add a named constant near the other loop policy constants:

```rust
const MAX_RESPONDER_TOOL_ROUNDS_PER_TURN: usize = 2;
```

Refactor the current one-shot `if !response.tool_calls.is_empty()` block
inside `run_one_turn` into an explicit loop:

```text
create ProjectDocToolBudget::new(turn_index)
while response has tool_calls and tool_round < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN:
  dispatch tool_calls with the same ResponderToolContext and budget
  append assistant tool-call message and provider-native tool-result messages
  record PromptAssembled for the augmented prompt
  increment tool_round
  if tool_round < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN:
    invoke responder again with tools still advertised
  else:
    invoke responder again with no tools advertised

if the response after the loop still has tool_calls:
  record ErrorOccurred with stage = "bounded-tool-loop"
  bail without appending the turn
```

Important details:
- Reuse the same `ProjectDocToolBudget` for every dispatch inside the
  human turn; this is what makes Phase 4's true per-turn cap meaningful.
- Preserve the existing usage accounting: total latency, input tokens,
  cached input tokens, and output tokens should include every provider
  call in the turn.
- Preserve the existing provider-native message shape:
  `ModelMessage::assistant_tool_calls(...)` followed by one
  `ModelMessage::tool_result(...)` per execution.
- Continue to collect `recalled_turns` from `recall_turn` executions in
  both tool rounds.
- The final no-tools request must still use
  `ModelRole::predefined(ModelRoleId::ConversationalResponder)` and the
  same model settings, but no `with_tools(...)` call. This keeps the
  final answer step from starting an unbounded tool loop.

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat(responder): allow bounded search-read tool loop"
```

### Task 6.3: Always-on prompt block

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

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/conversation
git commit -m "feat(prompt): append project-doc voicing block when tools advertised"
```

### Phase 6 verification

Run the focused tests first, then the repo gates:

```bash
cargo test -p qsf_app multi_turn_text_loop
cargo clippy --all-targets -- -D warnings
cargo fmt
```

At this point the responder can call the tools end-to-end against the
real `docs/` tree, including `search -> read -> answer` in one human
turn. A short manual smoke test (run the multi-turn text loop, ask "what
are you?") is optional here; the full battery arrives in Phase 7.

---

## Phase 7: Self-question battery fixture test

A small structured offline test that exercises the responder with a
fixed list of self-questions and asserts on the calls made and the
hedging language used. Runs as a normal `cargo test` so it is part of
CI.

### Task 7.1: Battery fixture and harness

**Files:**
- Create: `crates/qsf_app/tests/project_doc_self_question_battery.rs`
- Create: `crates/qsf_app/tests/fixtures/self_question_battery.json`

The harness uses a mock provider (mirror the existing `MockResponder`
test pattern) to produce predetermined tool calls and replies, then
asserts on the recorded events and traces. The intent is to verify
plumbing and voicing rules, not to test the model's natural-language
choices. For questions that expect both `search_project_docs` and
`read_project_doc`, the mock provider should emit them in separate
provider tool-call batches so the battery exercises Phase 6's bounded
`search -> read -> answer` loop and Phase 4's shared per-turn budget.

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
        { "tool": "search_project_docs", "round": 1, "query_contains": "sleep" },
        { "tool": "read_project_doc", "round": 2, "path_contains": "Architecture.SleepPhase.md" }
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
    round: Option<usize>,
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
            if let Some(round) = expected.round {
                assert_eq!(actual.round, round, "question {}", question.id);
            }
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
// using a small stub model client. The stub should drive the actual bounded
// responder tool loop from Phase 6, not call dispatch directly, so expected
// round numbers prove that search and read can span two provider batches.
// Implementation mirrors the test patterns in
// crates/qsf_app/src/models/openai_tool_client.rs which already use
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

A small, deterministic pass that joins each `project_doc_*` trace record
in a run's `traces.jsonl` to the same-turn final assistant reply and
writes a follow-up `TraceRecord` (operation = `project_doc_influence`)
marking whether the reply substantively overlapped the returned content.

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
`TraceRecord`, groups them by turn (carried in `details.role_id` or an
equivalent turn marker — confirm against the actual trace shape during
implementation), pairs each `project_doc_*` record with the final
`assistant_reply` trace in the same turn, computes
`reply_overlaps_excerpt`, and appends one `project_doc_influence` record
per pair.

This is plumbing work whose precise shape depends on existing trace
conventions; follow the pattern of any other post-hoc analysis tool
already in `crates/qsf_app/src/`. Surface naming choices as open
questions if existing conventions are unclear.

- [ ] **Step 3: Run tests.** Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/project_docs/enrichment.rs \
        crates/qsf_app/src/project_docs/mod.rs
git commit -m "feat(project_docs): traces.jsonl post-hoc influenced_reply enrichment"
```

---

## Phase 9: Documentation updates

Per `Agents.md` and `docs/ProjectFrame/ProjectWorkflow.md`. These are
documentation changes only; no application code changes. Per the diary
discipline, a diary entry covers the *application* work from Phases 1-8;
this phase does not need its own diary entry beyond that.

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
design and the implementation in agreement per OQ #5.

- [ ] Commit.

```bash
git add docs/ProjectFrame/DocumentStatus.md docs/Plans/Design.ProjectDocIntrospection.md
git commit -m "docs(frame): pointer to allowlist; record deferred latency cap"
```

### Task 9.5: Engineering diary entry

**Files:**
- Modify: `docs/EngineeringDiary.md`

Per the *Instructions how to use* at the top of the diary, add one entry
at the end of the file covering the application work landed in Phases
1-8. Keep it short, reference concrete artifacts, do not reference
planning documents. **Because Phase 1 was committed as a standalone
slice, make sure this entry (or a separate library-slice entry)
explicitly accounts for the `project_docs` library work, not only Phases
2-8.** If any of Phases 2-8 were merged in isolation ahead of this pass
and already carry their own standalone diary entries (per the diary
discipline noted in those phases), reconcile rather than duplicate them
here.

Template:

```markdown
## YYYY-MM-DD - Project-doc introspection channel

The `ConversationalResponder` can now call `search_project_docs` and
`read_project_doc` mid-dialogue to ground self-questions in actual
project material, with per-turn budget enforcement, kind/maturity
hedging, bounded search-then-read tool rounds, and trace records.

What changed:
- New `project_docs` module: allowlist loader, metadata extraction,
  lexical search, bounded read (path-confined against traversal),
  post-hoc reply-overlap check.
- New tools `search_project_docs` and `read_project_doc` wired into
  `ToolRegistry`.
- New `ResponderToolContext` combining session + project-doc accessors
  for live dispatch.
- New `ProjectDocToolBudget` enforces per-turn caps (2 search, 1 read)
  across all tool batches in a human turn.
- The responder tool loop allows a bounded `search -> read -> answer`
  sequence while preserving a no-unbounded-tool-loop guard.
- `dispatch_model_tool_calls` emits `project_doc_search` /
  `project_doc_read` trace records for successful and refused calls,
  including call correlation fields and sanitized arguments.
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
provider and judgement about reply quality. Treat the fixture battery in
Phase 7 as the regression gate and this phase as the qualitative
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
  - At least one project-self prompt exercises the bounded
    `search -> read -> answer` path across two provider tool-call
    batches inside one human turn.
  - `kind` and `maturity_tag` in trace details match the documents
    fetched.
  - Hedging in the reply text matches the maturity tag (e.g.
    "brainstorm idea" language only for Idea/Brainstorm material).
  - No claim of current behavior is made from a Plan, Idea, or Concept.
  - The control question made no introspection calls.
  - Recorded `latency_ms` values stay well under 1000 ms; if any exceed
    it, follow OQ #5 and add a cap-enforcement task at the
    `ProjectDocService` boundary.
- [ ] If anything fails, do **not** patch the prompt to mask it — open a
  new diary entry describing the failure and add a follow-on ticket in
  the experiment backlog.

---

## Phase 11: Associative project-doc context pointers (future planning handoff)

This is a follow-on planning phase, not part of the v1 project-doc tool
channel. It gives a project manager enough shape to create a separate
design or implementation plan after Phases 1-10 have produced trace
evidence about how project-doc lookup behaves in live dialogue.

The goal is to explore an automatic, association-driven context source
for project documents. Unlike `search_project_docs` and
`read_project_doc`, this mechanism is not activated by a model tool call.
It is driven by the same memory/context-selection path that retrieves
relevant memories for the current input. Its output should be compact
project-doc pointers, not full document bodies.

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
  `docs/EngineeringDiary.md` (Phase 1 tests should already cover this in
  CI).
- [ ] Verify the bounded read rejects `..` traversal and absolute paths
  (Phase 1 read tests should already cover this in CI).
- [ ] Verify `read_project_doc` clamps an above-cap `max_tokens` to
  `MAX_TOKENS_HARD_CAP` and `search_project_docs` normalizes
  `max_results` into `1..=DEFAULT_MAX_RESULTS` (Phase 2 tests should
  already cover both in CI).
- [ ] Verify the `ToolRegistry` routes **both** project-doc tools through
  `metadata_for`, `dispatch`, and `model_tool_definitions_for`, with
  metadata advertising `ReadOnly`/`ReadOnly` and definitions returned
  under the correct names (Phase 3 tests should already cover this in CI
  — search *and* read dispatch tests).
- [ ] Verify a combined `ResponderToolContext` answers both
  `session_state()` and `project_doc_service()`, and that one
  `ProjectDocToolBudget` is reused across dispatch batches inside the
  same human/responder turn (Phase 4 tests should already cover this in
  CI).
- [ ] Verify `dispatch_model_tool_calls` refuses the 3rd search / 2nd
  read with `per_turn_cap` events and refusal traces whether the calls
  occur in one batch or across two batches sharing the same
  `ProjectDocToolBudget`; refusal telemetry must include session ID,
  call ID, tool name, turn index, cap, attempted count, and sanitized
  arguments.
- [ ] Verify successful `search_project_docs` / `read_project_doc`
  dispatch emits `project_doc_search` / `project_doc_read` traces with
  `refused == false` plus the same call-correlation fields (Phase 5
  tests should already cover this in CI).
- [ ] Verify the responder can complete a bounded
  `search -> read -> answer` sequence in one human turn and rejects a
  third tool-call batch without appending the turn (Phase 6 tests should
  already cover this in CI).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  since Phase 1 was committed independently, a standalone library-slice
  entry plus the Phases 2-8 entry), with any isolated-merge diary entries
  reconciled rather than duplicated.
- [ ] Confirm Phase 11 remains a follow-on planning handoff unless it has
  been promoted into a separate detailed design or implementation plan.
