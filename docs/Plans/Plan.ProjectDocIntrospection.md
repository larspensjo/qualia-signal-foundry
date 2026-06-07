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

Phases 1-5 have landed and are committed:

- **Phase 1** — the pure `ProjectDocService` library
  (`crates/qsf_app/src/project_docs/`).
- **Phase 2** — the two `Tool` implementations, `ToolPermission::read_only()`,
  the defaulted `ToolContext::project_doc_service()` accessor, and the
  standalone `ProjectDocToolContext`.
- **Phase 3** — both tools wired into `ToolRegistry` (struct, `Default`,
  and the three `match` sites in `metadata_for`, `dispatch`, and
  `model_tool_definitions_for`).
- **Phase 4** — the combined `ResponderToolContext` (answers both
  `session_state()` and `project_doc_service()`) plus the true
  per-human-turn `ProjectDocToolBudget`, with refusal telemetry
  (`ToolFailed` event + refusal `TraceRecord`) for over-cap project-doc
  calls.
- **Phase 5** — success `TraceRecord` emission for executed
  `search_project_docs` / `read_project_doc` calls (`refused == false`),
  symmetric with the Phase 4 refusal traces and carrying the returned
  content needed for Phase 8 overlap.

**Phase 6 (wiring the responder role + a bounded two-round tool loop) is
the next implementation step.** It is the first phase to construct the
`ResponderToolContext` and a per-turn `ProjectDocToolBudget` at a live
call site, advertise the two tools to the `ConversationalResponder`, and
permit a bounded `search -> read -> answer` sequence inside one human
turn. Until Phase 6 lands, the dispatch-level machinery (Phases 3-5) is
proven only in tests; no live call site advertises the tools.

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
  `ReadProjectDocTool` (Phase 2), and `ResponderToolContext`
  (**landed, Phase 4**).
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`, `Tool`
  trait, `ToolMetadata`, the `ToolContext` trait (both
  `session_state()` and `project_doc_service()` defaulted to `None`),
  and `EmptyToolContext`. **Landed (Phase 3):** the registry struct,
  `Default`, and all three `match` sites route all four tools; tests
  assert metadata/definition identity and dispatch for both project-doc
  tools. Unchanged after Phase 3.
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
- `crates/qsf_app/src/tools/responder_tool_context.rs` — **landed
  (Phase 4).** `ResponderToolContext<'a> { state, project_docs }`
  implementing both accessors for live dispatch. **Phase 6** constructs
  this type at the multi-turn call site.
- `crates/qsf_app/src/tools/search_project_docs_tool.rs` and
  `crates/qsf_app/src/tools/read_project_doc_tool.rs` — **landed (Phase
  2), wired (Phase 3).** The two `Tool` impls, routed by the registry.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls(context, request, registry, state_ctx,
  project_doc_budget, tool_calls)`. Its loop checks `allowed_tools`,
  builds a `ToolRequest` via `tool_request_from_model_tool_call`,
  records `ToolRequested`, then `validate_and_execute` +
  `ToolCompleted`/`ToolFailed`. **Phase 4 (landed)** added the
  `ProjectDocToolBudget` parameter, the per-turn cap gate, the refusal
  `ToolFailed`/`TraceRecord` path, and the
  `sanitized_project_doc_arguments` helper. **Phase 5 (landed)** added
  success traces (`project_doc_search` / `project_doc_read`,
  `refused == false`) on the executed path, reusing that helper and the
  budget's `turn_index`. No change in Phase 6.
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
  operations (`project_doc_search`, `project_doc_read`) ride in
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
   the process working directory. **Resolved in Phase 6:** the experiment
   runner accepts `--workspace-root`, canonicalizes it into `RunContext`,
   and the launcher passes the script-derived repo root.
2. **Combined `ToolContext` shape — DECIDED AND LANDED IN PHASE 4.** Live
   dispatch needs one context answering *both* `session_state()` and
   `project_doc_service()`. Phase 4 shipped a dedicated combined context
   (`ResponderToolContext`) rather than extending `SessionToolContext`,
   because the dedicated type is purely additive. The existing
   single-accessor contexts are unchanged.
3. **`influenced_reply` storage.** Phase 8 writes the marker as a
   follow-up `TraceRecord` referencing the original by `trace_id`. If an
   annotation on the original record is preferred, raise before Phase 8.
4. **Module naming.** Per `Agents.md`, name modules after stable
   behavior. This plan uses `project_docs` / `ProjectDocService`, and
   the combined context is named for the role it serves
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
   `omitted_due_to_budget` signal the success traces (Phase 5) can
   record. Note the change in the diary when it happens.
6. **Integration-test setup in the dispatch tests.** The plan uses
   compact test sketches rather than fully inlined harness code, but the
   assertions are binding. Mirror the existing `tool_dispatch.rs` tests:
   `RunContext::create_in`, `ToolRegistry::default()`,
   `ModelRequest::new(...).with_session_id(...).with_tools(...)`, parsed
   `EventRecord`s from `events.jsonl`, and parsed `TraceRecord`s from
   `traces.jsonl`. If an inline test module grows unwieldy, write a
   focused integration test under
   `crates/qsf_app/tests/project_doc_dispatch.rs` and treat the
   skeletons as the assertion contract.
7. **Per-turn budget scope — DECIDED AND LANDED IN PHASE 4.** The caps
   apply across the whole human/responder turn, not merely one provider
   tool-call batch. Phase 4 introduced explicit `ProjectDocToolBudget`
   state, keyed by the current `turn_index`, threaded mutably through
   `dispatch_model_tool_calls`. Phase 6 reuses the same budget across a
   bounded two-round responder tool loop. If a future generic tool
   runtime changes the loop structure, it must preserve this invariant:
   fresh budget per human turn, shared budget across all provider calls
   inside that turn.
8. **Voicing-prompt scope across the turn — DECIDED HERE (review
   P6-001).** The kind/maturity voicing block (Task 6.3) must be present
   on **every** responder provider call in a turn where the project-doc
   channel is enabled, *including the final no-tools answer call*. It is
   therefore gated on channel/turn availability, not on whether the
   current request's advertised `allowed_tools` happens to contain the
   two tool names. Keying off the current request's tools would silently
   drop the hedging instructions from the final answer step — exactly
   when the model must apply kind/maturity hedging to the tool results
   already in context.

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
  -> final provider call is made without tools but WITH the voicing block
     still present; provider produces the human-facing reply with
     kind/maturity hedging
  -> post-hoc enrichment pass marks influenced_reply on traces whose
     content overlapped the final reply
```

---

## Phase 1: `ProjectDocService` library — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 1").
Source of truth: `crates/qsf_app/src/project_docs/`.

**What shipped:** a pure, side-effect-free `project_docs` module
(`pub mod project_docs;` in `lib.rs`) with the surface later phases
depend on — `types` (`DocKind`, `MaturityTag`, `MatchStrength`,
`DocHit`, `DocRead`), `allowlist` (`Allowlist::from_file`/`from_str`,
exclude-then-include globs), `metadata` (`kind_for_path`, `maturity_for`,
`last_reviewed_for` scoped to `## Implementation Status`, ISO-date
enforced), `search`, `read`, and the `ProjectDocService` facade
(`new(repo_root, allowlist_path)`, `.search`, `.read`, `.allowlist()`
re-read per call for hot-reload, `.repo_root()`). Deps added: `globset`,
`toml`, `regex`, `once_cell`, `walkdir`, `tempfile` (dev). Production
allowlist: `config/project-doc-introspection.toml`.

**Binding constraints on later phases:**

- **Path resolution (OQ #1).** Tests resolve from `CARGO_MANIFEST_DIR`;
  production wiring (Phase 6 on) must use **absolute** repo-root and
  allowlist paths.
- **Path-safety lives in the library, not the tool.** Bounded `read`
  normalizes and confines caller-supplied paths *before* touching the
  allowlist or filesystem (rejects absolute paths and any `..`); the
  `read_project_doc` tool forwards the raw `path` and re-implements no
  guards.
- **Allowlist hot-reload + production defaults.** Excludes
  `docs/EngineeringDiary.md` and `docs/Reviews/**`; admits
  `docs/ProjectFrame/**` and `docs/DecisionLog.md`; picks up edits
  without a rebuild.
- **Latency cap deferred (OQ #5).** Synchronous API, no deadline param.

**Acceptance outcome (met):** `cargo test -p qsf_app project_docs`
passes (allowlist precedence; kind/maturity/last-reviewed extraction with
Implementation-Status scoping and malformed-date rejection; heading-first
lexical search; bounded read with focus/truncation; traversal/absolute-
path refusals; service-level hot-reload). Clippy and fmt clean.

**Diary follow-up constraint:** Phase 1 was committed as a standalone
slice; the Phase 9 diary pass must explicitly account for the
`project_docs` library work (fold into the Phases 1-8 entry or add a
separate library-slice entry); do not silently skip it.

---

## Phase 2: Tool implementations — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 2").
Source of truth: `crates/qsf_app/src/tools/`.

**What shipped:** `ToolPermission::read_only()` (grants
`ReadOnly`/`ReadOnly`); a defaulted
`ToolContext::project_doc_service() -> Option<&ProjectDocService>`
(returns `None`), leaving `EmptyToolContext`/`SessionToolContext`
unchanged; `ProjectDocToolContext<'a> { service }` returning
`Some(service)`; `SearchProjectDocsTool` (reads `query`/`max_results`,
calls `service.search`, serializes `Vec<DocHit>`, **normalizes
`max_results` into `1..=DEFAULT_MAX_RESULTS`**); `ReadProjectDocTool`
(reads `path`/`focus`/`max_tokens`, calls `service.read`, serializes
`DocRead`, **clamps `max_tokens` to `MAX_TOKENS_HARD_CAP` (4000)**,
forwards the raw `path`). `tools/mod.rs` re-exports the names and types.

**Binding constraints on later phases:**

- **Combined context required for live dispatch (OQ #2).** Built in
  Phase 4 as `ResponderToolContext`.
- **Upper-bound discipline is the tool's job.** `max_results` and
  `max_tokens` are clamped/normalized inside the tools; later phases must
  not assume the model honors the advertised schema.
- **Diary discipline.** Application work of Phases 1-8 groups under a
  single Phase 9 diary entry; isolated merges carry a short standalone
  entry, reconciled (not duplicated) in Phase 9.

**Acceptance outcome (met):** `cargo test -p qsf_app tools::` passes
(read-only permission; both context accessors; search hit/metadata +
`max_results` normalization + missing-context failure; read content +
`max_tokens` clamp + out-of-allowlist refusal + missing-context
failure). Clippy and fmt clean.

---

## Phase 3: `ToolRegistry` wiring — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 3: wire
project-doc tools into ToolRegistry"). Source of truth:
`crates/qsf_app/src/tools/tool_registry.rs`.

**What shipped:** the registry knows all four tools. The `ToolRegistry`
struct and `Default` carry `search_project_docs: SearchProjectDocsTool`
and `read_project_doc: ReadProjectDocTool`, and all three `match` sites
(`metadata_for`, `dispatch`, `model_tool_definitions_for`) route both new
tool names (via `super::SEARCH_PROJECT_DOCS_TOOL_NAME` /
`super::READ_PROJECT_DOC_TOOL_NAME`, no duplicated literals). Because
`tool_request_from_model_tool_call` routes unrecognized tools through its
catch-all `_ =>` arm, the dispatch request-builder needed no change.

**Binding constraints on later phases:**

- The registry holds no `ToolContext`; it receives one per call, so the
  combined-context question (OQ #2) was settled in **Phase 4**, the first
  phase to dispatch these tools through a live context alongside
  `recall_turn`.
- No runtime call site changed in Phase 3.

**Acceptance outcome (met):** registry tests assert `metadata_for`
returns `Some` advertising `ReadOnly`/`ReadOnly` for both tools (by
name); `model_tool_definitions_for(&[search, read])` returns exactly the
two definitions under the correct names; **both** tools route through
`dispatch`. Calculator/recall_turn/permission-rejection tests pass
unchanged. Build, focused tests, clippy, fmt clean.

---

## Phase 4: Combined context + true per-turn budget state — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 4:
Combined context + true per-turn budget state"). Source of truth:
`crates/qsf_app/src/tools/responder_tool_context.rs` and
`crates/qsf_app/src/models/tool_dispatch.rs`.

**What shipped:**

- **Combined context (OQ #2).** `ResponderToolContext<'a> { state:
  &'a SessionState, project_docs: &'a ProjectDocService }`, implementing
  `ToolContext` so one `&dyn ToolContext` answers *both* accessors with
  `Some(_)`. Re-exported from `crate::tools`. Dedicated type (purely
  additive); single-accessor contexts unchanged.
- **True per-turn budget.** Module-level caps
  `PROJECT_DOC_SEARCH_CAP_PER_TURN = 2` and
  `PROJECT_DOC_READ_CAP_PER_TURN = 1`, plus `ProjectDocToolBudget {
  turn_index, search_calls, read_calls }` (`new(turn_index)`,
  attempt-recording methods). Dispatcher signature became
  `dispatch_model_tool_calls(context, request, registry, state_ctx,
  project_doc_budget: &mut ProjectDocToolBudget, tool_calls)`. Callers
  create one budget per human turn and reuse it across dispatch batches.
- **Refusal path.** After the `allowed_tools` membership check and before
  building a `ToolRequest`, the loop gates project-doc tools on the
  budget. An over-cap call is refused *before* reaching the registry,
  emitting only a `ToolFailed` event (no preceding `ToolRequested`, to
  keep the `ToolRequested → ToolCompleted/Failed` symmetry intact for
  executed calls) with `refusal_reason == "per_turn_cap"`, `cap`,
  `attempted_count`, `turn_index`, and sanitized arguments; a refusal
  `TraceRecord` (`operation` = `project_doc_search`/`project_doc_read`,
  `details.refused == true`, same correlation fields); and a `ToolResult`
  whose `observation_summary` names `per_turn_cap`.
- **`sanitized_project_doc_arguments` helper.** Preserves only stable,
  non-sensitive replay inputs (`query`/`max_results` for search;
  `path`/`focus`/`max_tokens` for read), never dumping arbitrary JSON.

**Binding constraints on later phases:**

- **Reuse, don't reinvent.** Success-path traces (Phase 5) reuse
  `sanitized_project_doc_arguments` and the budget's `turn_index`, and
  carry the same correlation fields (`session_id`, `role_id`, `call_id`,
  `tool_name`, `turn_index`) as the refusal traces so refused and
  executed calls join cleanly.
- **Budget invariant (Phase 6).** Fresh `ProjectDocToolBudget` per human
  turn, shared across all provider tool batches inside that turn;
  calculator and `recall_turn` never consume the budget.
- **Refused calls still return a `ToolResult` (Phase 6).** The over-cap
  refusal path returns a `ToolResult` *without* executing the registry.
  Phase 6's loop must still append a provider-native `tool_result`
  message for it, so the provider receives a response for every
  `tool_call_id` it emitted (review P6-002).
- **Live wiring still pending.** Phase 4 proved the combined context and
  budget in dispatch-level tests only; the `ResponderToolContext` is
  *constructed* and the budget *created per turn* at the **Phase 6** call
  site, where `ProjectDocService` must be built with absolute paths
  (OQ #1).

**Acceptance outcome (met):** both accessors return `Some(_)`; the budget
enforces the 2-search / 1-read caps whether the over-cap call occurs in
one batch or across two batches sharing one budget; a fresh budget resets
counts next turn; a mixed `recall_turn` + `search_project_docs` batch
dispatches through one `ResponderToolContext` with non-project-doc tools
not consuming the budget; refusal telemetry includes session ID, call ID,
tool name, turn index, cap, attempted count, sanitized arguments;
existing dispatcher tests pass unchanged. Build, focused tests, clippy,
fmt clean.

**Diary discipline (still binding):** as with Phases 1-3, an isolated
Phase 4 merge ahead of the grouped feature carries a short standalone
diary entry, reconciled (not duplicated) in Phase 9.

---

## Phase 5: Success TraceRecord emission for project-doc calls — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 5:
feat(dispatch): emit project_doc_search/read trace records on success").
Source of truth: `crates/qsf_app/src/models/tool_dispatch.rs`.

**What shipped:** symmetric *success* traces (`details.refused == false`)
on the executed `search_project_docs` and `read_project_doc` paths, so a
researcher can replay **every** project-doc call in a run's
`traces.jsonl`, not just refusals. After the existing `ToolCompleted`
event, the dispatch loop emits one `TraceRecord` per project-doc op
(calculator and `recall_turn` untouched via the `_ => {}` arm):

- `project_doc_search` stores the parsed `hits` array **and** an explicit
  `details.hit_count` (= `hits.len()`).
- `project_doc_read` stores the **parsed read output** (`details.read`) —
  the bounded content/excerpt the tool returned (already `max_tokens`-
  capped in Phase 2, no second cap) plus its metadata — alongside the
  `is_full` / `omitted_sections` signals.

**Binding constraints on later phases:**

- **Replayability is the success criterion.** Phase 8's
  `influenced_reply` enrichment computes reply-overlap directly from
  these records: the search trace's `details.hits` and the read trace's
  `details.read` are the source material. If either is missing or shaped
  differently than recorded here, surface it before Phase 8's join rather
  than re-deriving content elsewhere.
- **Reuse discipline (held).** Traces reuse
  `sanitized_project_doc_arguments`, the budget's `turn_index`, the
  single `ToolCompleted` latency value (one clock read, shared via
  `with_latency_ms`), and the Phase 4 correlation fields — so refused and
  executed records union on `(session_id, turn_index, tool_name,
  call_id)`. No new sanitizer or latency helper was added.
- **Failure semantics.** A project-doc call that reaches execution and
  **fails** writes `ToolFailed` and emits **no** `refused == false`
  success trace; the success branch fires only on the executed success
  path. The Phase 4 over-cap refusal path never reaches execution.

**Acceptance outcome (met):** successful search/read dispatch each emit
exactly one `project_doc_*` trace with `refused == false`, the explicit
`hit_count` / bounded `read` content, sanitized arguments, `turn_index`,
the full correlation fields, and a recorded latency — in addition to (not
replacing) the `ToolCompleted` event; failed-execution and
non-project-doc regression tests confirm no spurious success traces;
existing dispatcher and Phase 4 refusal tests pass unchanged. Build,
`cargo test -p qsf_app tool_dispatch`, clippy, fmt clean. Pure-Rust
change, no external/human testing required (live behaviour is verified in
Phase 7's battery and Phase 10's manual session).

**Diary discipline (still binding):** Phase 5 commit is on the unmerged
feature branch; if merged in isolation it carries a short standalone
diary entry, reconciled (not duplicated) in Phase 9.

---

## Phase 6: Wire the responder role + bounded two-round tool loop

Adds the two tools to the `ConversationalResponder` allowed-tools list
used by the multi-turn text loop (and, by extension, the unified
text/voice path once that lands), adds the bounded multi-round tool loop
needed for `search_project_docs` followed by `read_project_doc` in the
same human turn, and adds the voicing prompt block that teaches the model
when and how to use the tools. That block is present on **every**
responder provider call in a project-doc turn — including the final
no-tools answer call, so the kind/maturity hedging instructions stay in
context when the model writes the human-facing reply (Task 6.3, OQ #8,
review finding P6-001).

This is the first **live** call site, so the context constructed here and
passed to `dispatch_model_tool_calls` **must** be the
`ResponderToolContext` from Phase 4 (answers both `session_state()` and
`project_doc_service()`). The `ProjectDocService` must be built once with
**absolute** repo-root and allowlist paths (OQ #1), and one
`ProjectDocToolBudget::new(turn_index)` must be created per human/
responder turn and reused across every tool batch inside that turn
(Phase 4 budget invariant; Phase 5 traces inherit `turn_index` from it).

**Loop decision (review blocker P4-001, option 3 — DECIDED).** The
responder may make at most **two provider tool-call batches** in one
human turn:

1. initial provider call with tools advertised;
2. if it calls tools, dispatch the batch, append provider-native tool
   messages (one per `ToolResult` returned, executed or refused), and
   make one follow-up provider call with tools still advertised;
3. if that follow-up calls tools, dispatch the second batch with the
   **same** `ProjectDocToolBudget`, append tool messages, and make the
   final provider call **without tools** (but with the voicing block
   still present — see Task 6.3);
4. if any provider response after the two permitted tool batches still
   contains tool calls, record `ErrorOccurred` and fail the turn without
   appending it, preserving the existing "no unbounded tool loop" safety
   behavior.

This is deliberately narrower than a generic autonomous tool loop. It is
just enough for `search -> read -> answer`, while the Phase 4 budget
state enforces the 2-search / 1-read per-turn caps across both batches.

This phase is pure Rust with deterministic unit/integration coverage.
Follow `superpowers:test-driven-development` (or plain TDD if that skill
is unavailable): the failing test precedes the implementation in each
task. A short manual smoke test is optional here; the full battery
arrives in Phase 7 and the qualitative live gate in Phase 10.

### Task 6.1: Extend `allowed_tools` and build responder tool context

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
  (and any other call site that constructs a `ConversationalResponder`
  request with explicit `allowed_tools` — grep for `allowed_tools`).

- [ ] **Step 1: Enumerate the advertising call sites.**

```bash
grep -rn "allowed_tools" crates/qsf_app/src
```

Identify every call site that builds a request for the responder. The
multi-turn loop currently advertises `calculator` and `recall_turn`;
each such list must also include `SEARCH_PROJECT_DOCS_TOOL_NAME` and
`READ_PROJECT_DOC_TOOL_NAME`. If the grep reveals a second live
responder call site outside `multi_turn_text_loop.rs` that also dispatches
tools, apply the same context/budget changes there; if it only builds a
role without dispatching, advertising the tools is sufficient.

- [ ] **Step 2: Write the failing tests — responder advertises the tools
  *in the live request*.**

A role-level helper assertion alone can pass while the live `ModelRequest`
still omits the project-doc tool *definitions* (review P6-003). Cover
**both** levels: the role/allowed-tools list, and the actual request the
loop hands to the provider.

```rust
#[test]
fn responder_advertises_project_doc_tools() {
    let role = build_conversational_responder_with_tools();
    assert!(role.allowed_tools.iter().any(|n| n == crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME));
    assert!(role.allowed_tools.iter().any(|n| n == crate::tools::READ_PROJECT_DOC_TOOL_NAME));
}

#[test]
fn live_responder_request_advertises_all_four_tool_definitions() {
    // Drive the actual multi-turn request-assembly path (or a small pure
    // helper it uses) and inspect the ModelRequest / model tool definitions
    // it builds for the initial responder call.
    let request = build_initial_responder_request_for_test(/* … */);
    let names: Vec<&str> = request.tool_definitions().iter().map(|d| d.name()).collect();
    for expected in [
        "calculator",
        "recall_turn",
        crate::tools::SEARCH_PROJECT_DOCS_TOOL_NAME,
        crate::tools::READ_PROJECT_DOC_TOOL_NAME,
    ] {
        assert!(names.contains(&expected), "missing tool definition: {expected}");
    }
}
```

If exposing the assembled request directly is awkward, extract a small
pure helper (e.g. `responder_tool_names()` /
`model_tool_definitions_for(...)` call site) that the live code actually
uses, and assert against that — the point is to prove the *live path*, not
a parallel fixture.

- [ ] **Step 3: Run the tests; verify they fail.** Expected: FAIL (the
  current list omits both names, so the live request omits both
  definitions).

- [ ] **Step 4: Update the call site(s).**

Extend each existing `vec![...]` of tool names to include the two new
constants (imported from `crate::tools`). At each updated call site,
replace the `SessionToolContext` handed to `dispatch_model_tool_calls`
with a `ResponderToolContext { state: &state, project_docs: &service }`,
constructing the `ProjectDocService` **once** with absolute repo-root and
allowlist paths (OQ #1) — otherwise `recall_turn` or the project-doc
tools fail at runtime. Update the dispatch call to pass a
`ProjectDocToolBudget`, using a fresh budget for the existing
single-batch path until Task 6.2 introduces the two-round loop.

> **Resolved after review P6-004:** the live loop receives the absolute
> repo root through the experiment runner's `--workspace-root` option,
> which is canonicalized into `RunContext`; `scripts/qsf.ps1` passes the
> launcher-derived project root. The live service construction must use
> that runtime accessor and must not derive production paths from
> `CARGO_MANIFEST_DIR` or the process working directory.

- [ ] **Step 5: Run tests.** Expected: PASS.

- [ ] **Step 6: Commit.**

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
    //   2. follow-up responder call, after the search tool result, returns read_project_doc
    //   3. final responder call, after the read tool result, returns natural text
    // Assert one turn completed, both tool results were appended before the
    // final answer, ToolCompleted events exist for search and read, and the
    // project_doc_* success traces share the same turn_index.
}

#[test]
fn responder_reuses_project_doc_budget_across_tool_batches() {
    // Mock client returns read_project_doc in batch 1 and read_project_doc
    // again in batch 2. The second read is refused by the shared
    // ProjectDocToolBudget with attempted_count == 2 (per_turn_cap), then the
    // final no-tools response still completes the turn with the refusal tool
    // message appended in context (a tool_result for the refused call_id).
}

#[test]
fn third_tool_batch_is_rejected_without_appending_turn() {
    // Mock client returns tool calls in the initial call, in the first
    // follow-up, and again in the final no-tools response. Assert an
    // ErrorOccurred event records that the bounded tool loop was exceeded
    // (stage = "bounded-tool-loop") and no TurnCompleted event is appended
    // for that input.
}

#[test]
fn ordinary_no_tool_response_still_completes_one_turn() {
    // Regression: a normal answer with no tool calls takes the same path as
    // before and creates no project-doc traces.
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
tool_round = 0
while response has tool_calls and tool_round < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN:
  dispatch tool_calls with the same ResponderToolContext and the same budget
  append assistant tool-call message and one provider-native tool-result
    message per ToolResult returned by dispatch (executed OR refused)
  record PromptAssembled for the augmented prompt
  tool_round += 1
  if tool_round < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN:
    invoke responder again with tools still advertised
  else:
    invoke responder again with no tools advertised (voicing block still present)

if the response after the loop still has tool_calls:
  record ErrorOccurred with stage = "bounded-tool-loop"
  bail without appending the turn
```

Important details:
- Reuse the same `ProjectDocToolBudget` for every dispatch inside the
  human turn; this is what makes Phase 4's true per-turn cap meaningful
  and what lets Phase 5's traces share `turn_index` across batches.
- **Append a `ModelMessage::tool_result` for *every* `ToolResult`
  returned by `dispatch_model_tool_calls`, including per-turn-cap
  refusals and validation failures** (which return a `ToolResult` without
  executing the registry), preserving `call_id` correlation for each
  model tool call dispatch handled. The provider must receive a response
  for every `tool_call_id` it emitted, or the follow-up call errors
  (review P6-002). Do **not** append only for executed calls.
- Preserve the existing usage accounting: total latency, input tokens,
  cached input tokens, and output tokens must include every provider call
  in the turn.
- Preserve the existing provider-native message shape:
  `ModelMessage::assistant_tool_calls(...)` followed by one
  `ModelMessage::tool_result(...)` per `ToolResult`.
- Continue to collect `recalled_turns` from `recall_turn` executions in
  both tool rounds.
- The final no-tools request must still use
  `ModelRole::predefined(ModelRoleId::ConversationalResponder)` and the
  same model settings, but no `with_tools(...)` call — this keeps the
  final answer step from starting an unbounded tool loop. **The voicing
  prompt block (Task 6.3) must still be present on this final call**,
  because the project-doc channel is enabled for the turn; otherwise the
  model loses the kind/maturity hedging instructions exactly when it
  writes the human-facing reply (review P6-001, OQ #8). Drive this by
  gating the block on channel/turn state, not on the call's advertised
  tools (see Task 6.3).

- [ ] **Step 4: Run tests.** Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat(responder): allow bounded search-read tool loop"
```

### Task 6.3: Voicing prompt block, present across the whole turn

**Files:**
- Modify: `crates/qsf_app/src/conversation/prompt.rs` (or whichever
  module assembles the system prompt for the responder; grep for
  `ConversationalResponder` and `system` to find it).

The block is the verbatim text from `Design.ProjectDocIntrospection.md`
Decision 3, *Voicing prompt*. It is appended to the responder's system
prompt **on every responder provider call in a turn where the
project-doc channel is enabled** — including the final no-tools answer
call — and omitted entirely in responder contexts where the channel is
unavailable. Gate it on channel/turn state, **not** on whether the
current request's advertised `allowed_tools` happens to contain the two
tool names (review P6-001, OQ #8): the final answer call advertises no
tools yet must still carry the hedging instructions for the tool results
already in context.

- [ ] **Step 1: Add a constant.**

```rust
// in crates/qsf_app/src/conversation/prompt.rs (or a new sibling module)
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

- [ ] **Step 2: Write the failing tests.**

Cover all three states: channel enabled (tool-advertising call), the
final no-tools answer call in a channel-enabled turn, and channel
disabled.

```rust
#[test]
fn responder_system_prompt_includes_introspection_block_when_channel_enabled() {
    let prompt = build_system_prompt(/* project_doc_channel_enabled = */ true /*, … */);
    assert!(prompt.contains("search_project_docs"));
    assert!(prompt.contains("kind and maturity"));
}

#[test]
fn final_no_tools_answer_call_still_includes_introspection_block() {
    // The final provider call in a project-doc turn advertises no tools but
    // the channel is still enabled, so the block must remain present so the
    // model applies kind/maturity hedging to the tool results already in
    // context (review P6-001).
    let prompt = build_system_prompt_for_final_answer(/* channel_enabled = */ true /*, … */);
    assert!(prompt.contains("kind and maturity"));
}

#[test]
fn responder_system_prompt_omits_block_when_channel_disabled() {
    let prompt = build_system_prompt(/* project_doc_channel_enabled = */ false /*, … */);
    assert!(!prompt.contains("search_project_docs"));
}
```

- [ ] **Step 3: Run tests; verify they fail.** Expected: FAIL (the block
  is not yet appended, and the final-answer case is not yet wired to the
  channel flag).

- [ ] **Step 4: Append the block whenever the project-doc channel is
  enabled for the turn.**

In the prompt-assembly path that builds the responder's system message,
append `PROJECT_DOC_INTROSPECTION_PROMPT` whenever the project-doc
channel is enabled for the responder turn. Derive that from turn/channel
state — e.g. a `project_doc_channel_enabled: bool` threaded from the
Task 6.1/6.2 call site that constructed the `ResponderToolContext` —
**not** from the current request's advertised `allowed_tools`. Keying off
the current request's tools would drop the block on the final no-tools
answer call (review P6-001), exactly when the model must apply
kind/maturity hedging to the tool results already in context. Still omit
the block entirely in responder contexts where the project-doc channel is
unavailable. Keep entry-point files (`main.rs`/`mod.rs`/`lib.rs`) thin and
any prompt-assembly helper pure/unit-testable per `Agents.md`.

- [ ] **Step 5: Run tests.** Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add crates/qsf_app/src/conversation
git commit -m "feat(prompt): append project-doc voicing block across responder turn"
```

### Phase 6 verification

Per `Agents.md`, build first, then focused tests, then the lint/format
gates:

```bash
cargo build
cargo test -p qsf_app multi_turn_text_loop
cargo test -p qsf_app project_doc       # context/prompt/dispatch coverage touched here
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expect all clean. At this point the responder can call the tools
end-to-end against the real `docs/` tree, including `search -> read ->
answer` in one human turn. A short manual smoke test (run the multi-turn
text loop, ask "what are you?") is optional here; the full fixture
battery arrives in Phase 7 and the qualitative live gate in Phase 10.

**Acceptance criteria for Phase 6:**

- The `ConversationalResponder` request built by the multi-turn loop
  advertises `calculator`, `recall_turn`, `search_project_docs`, and
  `read_project_doc`; every other live responder call site found by the
  `allowed_tools` grep is updated consistently. A test exercises the
  **live** request-assembly path (not just a role helper) and asserts the
  assembled `ModelRequest` / tool definitions include all four names
  (review P6-003).
- Each live dispatch passes a `ResponderToolContext` (both accessors
  return `Some(_)`) built over a `ProjectDocService` constructed with
  **absolute** paths from `RunContext::workspace_root()` (the runtime
  root supplied by `--workspace-root`; review P6-004), and a single
  `ProjectDocToolBudget::new(turn_index)` reused across every tool batch
  inside the human turn.
- The responder completes a bounded `search -> read -> answer` sequence
  across two provider tool-call batches in one human turn, appending both
  tool results before the final answer; the two project-doc success
  traces share the same `turn_index`.
- Every `ToolResult` returned by dispatch — executed, validation-failed,
  or per-turn-cap refused — is appended as a `ModelMessage::tool_result`
  with its `call_id`, so the provider has a response for every tool call
  it emitted (review P6-002).
- A second `read` (or a third project-doc call) inside the same turn is
  refused by the shared budget (`per_turn_cap`, `attempted_count`), the
  refusal tool message is appended to context, and the turn still
  completes from the final no-tools response.
- A provider response that still contains tool calls after the two
  permitted batches records `ErrorOccurred` (stage `bounded-tool-loop`)
  and does **not** append the turn; an ordinary no-tool answer completes
  exactly one turn with no project-doc traces.
- The responder system prompt includes `PROJECT_DOC_INTROSPECTION_PROMPT`
  on **every** responder provider call in a project-doc-enabled turn,
  **including the final no-tools answer call**, and omits it entirely
  when the channel is unavailable — gated on channel/turn state, not the
  current request's advertised tools (review P6-001, OQ #8).
- Usage accounting (latency, input/cached/output tokens) includes every
  provider call in the turn; provider-native message shape
  (`assistant_tool_calls` + one `tool_result` per `ToolResult`) is
  preserved; `recall_turn` recalls are collected across both rounds.
- `cargo build`, the focused tests above,
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` are clean.
  No registry, library, or dispatch-layer change is expected — if one
  proves necessary, surface it before proceeding.

**Diary discipline for this phase.** As with Phases 1-5, the application
work of Phases 1-8 is grouped under the single Phase 9 diary entry, so
Phase 6 is not considered complete or mergeable until that entry lands.
If Phase 6 is merged in isolation ahead of the grouped feature, a short
standalone Phase 6 diary entry must accompany that merge (read the
*Instructions how to use* at the top of `docs/EngineeringDiary.md`
first); reconcile, don't duplicate, in Phase 9.

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

This phase relies on the Phase 5 success traces carrying the returned
content: the search trace's `details.hits` and the read trace's
`details.read` bounded excerpt are the source material the overlap check
runs against. If either is missing or shaped differently than Phase 5
recorded, surface it before implementing the join rather than re-deriving
content elsewhere.

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
`TraceRecord`, groups them by turn (carried in `details.turn_index`,
present on both refused and executed project-doc traces after Phase 5;
confirm against the actual trace shape during implementation), pairs each
`project_doc_*` record with the final `assistant_reply` trace in the same
turn, computes `reply_overlaps_excerpt` against the recorded content
(search `details.hits`; read `details.read`), and appends one
`project_doc_influence` record per pair.

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
  including call correlation fields, sanitized arguments, and the
  returned content (search hits / bounded read excerpt) for replay.
- `ToolPermission::read_only()` constructor.
- Responder system prompt appends a kind/maturity voicing block whenever
  the project-doc channel is enabled for the turn, including the final
  no-tools answer call.
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
    "brainstorm idea" language only for Idea/Brainstorm material) — note
    that the final answer call retains the voicing block (Phase 6), so
    hedging should appear even though the final call advertises no tools.
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
  `refused == false` plus the same call-correlation fields and a
  recorded latency; the search trace carries an explicit `hit_count` and
  the parsed `hits`, and the read trace carries the bounded returned
  content (`details.read`) so the trace alone can replay the read; a
  project-doc call that reaches execution and fails writes `ToolFailed`
  and **no** success trace; calculator and recall_turn emit no
  `project_doc_*` trace (Phase 5 tests should already cover this in CI).
- [ ] Verify the responder can complete a bounded
  `search -> read -> answer` sequence in one human turn and rejects a
  third tool-call batch without appending the turn; verify the live
  request advertises all four tool definitions, the live
  `ProjectDocService` is constructed with absolute paths, every
  `ToolResult` (executed or refused) is appended as a `tool_result`
  message, and the voicing block is present on every responder call in a
  project-doc turn (including the final no-tools answer call) and omitted
  when the channel is unavailable (Phase 6 tests should already cover
  this in CI).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  since Phase 1 was committed independently, a standalone library-slice
  entry plus the Phases 2-8 entry), with any isolated-merge diary entries
  reconciled rather than duplicated.
- [ ] Confirm Phase 11 remains a follow-on planning handoff unless it has
  been promoted into a separate detailed design or implementation plan.
