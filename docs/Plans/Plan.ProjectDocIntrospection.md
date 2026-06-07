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

Phases 1-7 have landed and are committed. The minimum viable channel is
live and guarded by a CI regression battery: the `ConversationalResponder`
advertises both project-doc tools in the multi-turn text loop, can run a
bounded `search -> read -> answer` sequence inside one human turn under a
true per-turn budget, the dispatch layer emits success/refusal traces for
every call, and an offline self-question battery replays a fixed list of
questions through the live bounded loop as a deterministic regression gate.

- **Phase 1** — pure `ProjectDocService` library
  (`crates/qsf_app/src/project_docs/`).
- **Phase 2** — the two `Tool` implementations, `ToolPermission::read_only()`,
  the defaulted `ToolContext::project_doc_service()` accessor, and the
  standalone `ProjectDocToolContext`.
- **Phase 3** — both tools wired into `ToolRegistry` (struct, `Default`,
  and the three `match` sites).
- **Phase 4** — the combined `ResponderToolContext` plus the true
  per-human-turn `ProjectDocToolBudget`, with refusal telemetry for
  over-cap project-doc calls.
- **Phase 5** — success `TraceRecord` emission for executed
  `search_project_docs` / `read_project_doc` calls (`refused == false`).
- **Phase 6** — the responder role wired into the live multi-turn loop:
  the four-tool advertisement, the bounded two-round tool loop
  (`MAX_RESPONDER_TOOL_ROUNDS_PER_TURN = 2`) reusing one
  `ProjectDocToolBudget` per turn, the `ResponderToolContext` constructed
  over a `ProjectDocService` built from the absolute workspace root, a
  `tool_result` appended for every returned `ToolResult` (executed or
  refused), and the kind/maturity voicing block present on every
  responder provider call in a project-doc turn — including the final
  no-tools answer call.
- **Phase 7** — the offline self-question battery: a data-driven test
  that drives the real bounded responder loop with a scripted
  `ModelClient` and asserts on the tool calls made (including round),
  recorded events and traces, the voicing-block presence on every
  provider call, and the hedging language in the canned replies.

**Phase 8 (the `influenced_reply` post-hoc enrichment) is the next
implementation step.** It is the first phase that consumes a *finished*
run's artifacts: it joins each `project_doc_*` trace to the same-turn
final assistant reply and writes a follow-up `project_doc_influence`
trace marking whether the reply overlapped the returned content. Phase 9
lands the documentation updates, Phase 10 records the live external
verification, and Phase 11 is a future planning handoff.

## Background

The design at `docs/Plans/Design.ProjectDocIntrospection.md` specifies a
live-first introspection channel for project documents. This plan
implements the v1 channel in sequential implementation and documentation
phases that each produce something independently testable. Phases 1-6
(landed) are the minimum viable channel: the tools work end-to-end and
the responder can call them mid-dialogue. Phase 7 (landed) delivered the
offline self-question battery promised by the design's *Live-First
Rationale* as a deterministic, CI-runnable regression gate over the live
loop's shape and voicing rules. Phase 8 adds the `influenced_reply`
post-hoc enrichment. Phase 9 lands the documentation updates required by
`docs/ProjectFrame/ProjectWorkflow.md`. Phase 10 records the live
external verification step. Phase 11 is a future planning handoff for
associative project-doc context pointers; it is not part of the v1 tool
implementation.

## Current Anchors

Code anchors:

- `crates/qsf_app/src/project_docs/` — **landed (Phase 1).** Pure
  library: `Allowlist`, metadata extraction, lexical `search`, bounded
  `read`, and the `ProjectDocService` facade. Later phases consume it;
  they do not modify it. Phase 8 adds `influence` and `enrichment`
  sibling modules. The public `DocHit` carries textual fields `snippet`
  and `section_hint`; `DocRead` carries `content` (see
  `crates/qsf_app/src/project_docs/types.rs`) — Phase 8's overlap check
  consumes these.
- `crates/qsf_app/src/tools/mod.rs` — re-exports the tool surface,
  including `ProjectDocToolContext`, `SEARCH_PROJECT_DOCS_TOOL_NAME`,
  `SearchProjectDocsTool`, `READ_PROJECT_DOC_TOOL_NAME`,
  `ReadProjectDocTool` (Phase 2), and `ResponderToolContext` (Phase 4).
- `crates/qsf_app/src/tools/tool_registry.rs` — `ToolRegistry`, `Tool`
  trait, `ToolMetadata`, the `ToolContext` trait, and `EmptyToolContext`.
  **Landed (Phase 3):** all three `match` sites route all four tools.
- `crates/qsf_app/src/tools/tool_request.rs` — `ToolPermission` has both
  `compute_only()` and `read_only()` (Phase 2).
- `crates/qsf_app/src/tools/tool_result.rs` — `ToolResult` with fields
  `tool_name`, `category`, `side_effect_level`, `input`, `output_text`,
  `numeric_value`, `observation_summary`.
- `crates/qsf_app/src/tools/recall_turn_tool.rs` — defines
  `RecallTurnTool` **and** `SessionToolContext`. Reference for the `Tool`
  trait and the single-accessor context shape.
- `crates/qsf_app/src/tools/project_doc_tool.rs` — **landed (Phase 2).**
  `ProjectDocToolContext` (implements `project_doc_service()` only).
- `crates/qsf_app/src/tools/responder_tool_context.rs` — **landed
  (Phase 4).** `ResponderToolContext<'a> { state, project_docs }`
  implementing both accessors; constructed at the live multi-turn call
  site in Phase 6.
- `crates/qsf_app/src/tools/search_project_docs_tool.rs` and
  `crates/qsf_app/src/tools/read_project_doc_tool.rs` — **landed (Phase
  2), wired (Phase 3).** The two `Tool` impls.
- `crates/qsf_app/src/models/tool_dispatch.rs` —
  `dispatch_model_tool_calls(...)` with the `ProjectDocToolBudget`
  parameter, the per-turn cap gate, refusal traces (Phase 4), and
  success traces (Phase 5). The `project_doc_*` trace `details` carry
  `turn_index`, `refused`, sanitized arguments, and (on success) the
  returned content — the search trace stores `details.hits` (array of
  serialized `DocHit`, each with `snippet`/`section_hint`) plus
  `details.hit_count`; the read trace stores `details.read` (serialized
  `DocRead`, with `content`). Not modified by Phase 8.
- `crates/qsf_app/src/models/model_client.rs` — the `ModelClient` trait,
  `ModelResponse`, and `ModelToolCall`.
- `crates/qsf_app/src/models/mock_model.rs` — `MockModelClient`, the
  fixture-driven mock.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — **landed
  (Phase 6).** `run_one_turn` holds the bounded loop. **Relevant to
  Phase 8:** `turn_index = completed_turn_count(state)` is computed
  *before* the turn is pushed (line ~355) and is used both for the
  `ProjectDocToolBudget` (and therefore the `project_doc_*` trace
  `details.turn_index`) and as the completed turn's index, so the trace
  turn index and the `TurnCompleted` event's `turn.index` align.
- `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs` —
  **the in-crate harness Phase 7 reused** (`SequencedResponderClient`,
  `PlannedResponderResponse`, `run_with_io_and_components`,
  `test_context`, `TestMemorySource`, `responder_tool_names`,
  `parse_event_records`, `parse_trace_records`).
- `crates/qsf_app/src/observability/trace.rs` — `TraceRecord::new(...)`
  with `.with_details(Value)`, `.with_latency_ms(u64)`, and the public
  fields `trace_id`, `operation`, `details`. `TraceRecord` derives both
  `Serialize` and `Deserialize`, so Phase 8 can parse `traces.jsonl`
  lines straight back into `TraceRecord`. Note `TraceLogWriter::create`
  opens with `truncate(true)`; Phase 8 must therefore append with its own
  open-options writer, not via that constructor (see Task 8.2).
- `crates/qsf_app/src/observability/event_log.rs` —
  `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed`, and
  `EventType::TurnCompleted`. The `TurnCompleted` event payload is
  `{ session_id, turn, full_request_hash }`; the assistant reply is
  `payload.turn.assistant_response` and the turn index is
  `payload.turn.index` (`Turn` defined in
  `crates/qsf_app/src/session/mod.rs` ~120; serialized in
  `crates/qsf_app/src/session/runtime.rs` ~289).
- `crates/qsf_app/src/runtime/run_context.rs` — `RunContext` exposes
  `experiment_id()`, `run_id()`, `run_dir()`, `record_event(...)`, and
  `record_trace(...)`; the workspace root supplied via `--workspace-root`
  is canonicalized here and consumed by the live `ProjectDocService`
  construction. Its constructors create a fresh run directory with a
  `truncate`-mode `TraceLogWriter`, so there is **no** safe reopened-context
  append path today — Phase 8 appends directly with open-options.

Documentation anchors:

- `docs/Plans/Design.ProjectDocIntrospection.md` — the spec this plan
  implements.
- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md` — broader
  brainstorm (updated in Phase 9).
- `docs/ProjectFrame/DocumentStatus.md` — defines `kind` and
  `maturity_tag` taxonomies; updated in Phase 9 to reference the
  allowlist file.
- `docs/Architecture/Architecture.ToolSystem.md` — its *Implementation
  Status* section is refreshed in Phase 9.

## Open Questions To Surface During Implementation

Per `Agents.md`, ambiguities should be surfaced rather than silently
resolved. The plan picks a default for each; if any plays out
differently, raise it before changing direction.

1. **Config file path.** This plan uses
   `config/project-doc-introspection.toml` at the repo root (settled in
   Phase 1). *Path-resolution note (still binding):* `cargo test` runs
   with the working directory at the package root (`crates/qsf_app`),
   **not** the workspace root, so tests and production code must never
   load the config via a bare relative path. Tests resolve it from
   `CARGO_MANIFEST_DIR`; production wiring constructs `ProjectDocService`
   with an explicit absolute repo root and an explicit absolute allowlist
   path. **Resolved in Phase 6:** the experiment runner accepts
   `--workspace-root`, canonicalizes it into `RunContext`, and the
   launcher passes the script-derived repo root.
2. **Combined `ToolContext` shape — DECIDED AND LANDED IN PHASE 4.** Live
   dispatch needs one context answering *both* `session_state()` and
   `project_doc_service()`. Phase 4 shipped a dedicated combined context
   (`ResponderToolContext`). The single-accessor contexts are unchanged.
3. **`influenced_reply` storage — ACTIVE IN PHASE 8.** Phase 8 writes the
   marker as a follow-up `TraceRecord` (operation = `project_doc_influence`)
   referencing the original `project_doc_*` record by `trace_id`. The pass
   is **idempotent by `source_trace_id`**: re-running `enrich` over a
   directory that already holds influence records appends nothing for
   already-enriched sources (see Task 8.2). If an annotation on the
   original record is preferred instead, raise it before Task 8.2. A
   **second, distinct** Phase 8 decision — *where the final reply text is
   read from* — is surfaced inside Phase 8 itself because it was
   discovered to differ from the original sketch (the reply is in the
   `TurnCompleted` **event**, not in any trace).
4. **Module naming.** Per `Agents.md`, name modules after stable
   behavior. This plan uses `project_docs` / `ProjectDocService`; the
   combined context is named for the role it serves
   (`ResponderToolContext`); the Phase 8 modules are named for stable
   behavior (`influence`, `enrichment`), not the plan phase. Keep this
   discipline.
5. **Hard latency cap.** Decision 4 of the spec sets a 1500 ms hard cap.
   With lexical search over a small markdown corpus the cap is not
   expected to fire, so this plan **deliberately defers** cap-enforcement
   (synchronous `search`/`read`, no deadline parameter). Recorded as a
   conscious scope decision in `Design.ProjectDocIntrospection.md`
   Decision 4 (Phase 9). If real-run traces ever show `latency_ms` over
   1000, add enforcement **at the `ProjectDocService` boundary**.
6. **Integration-test setup in the dispatch tests.** Mirror the existing
   `tool_dispatch.rs` tests: `RunContext::create_in`,
   `ToolRegistry::default()`, parsed `EventRecord`s from `events.jsonl`,
   and parsed `TraceRecord`s from `traces.jsonl`.
7. **Per-turn budget scope — DECIDED AND LANDED IN PHASE 4.** The caps
   apply across the whole human/responder turn, not merely one provider
   tool-call batch. Phase 6 reuses the same budget across the bounded
   two-round responder tool loop.
8. **Voicing-prompt scope across the turn — DECIDED, LANDED IN PHASE 6,
   ASSERTED IN PHASE 7.** The kind/maturity voicing block is present on
   **every** responder provider call in a project-doc turn, *including the
   final no-tools answer call*. It is gated on channel/turn availability,
   not on whether the current request advertises the two tool names. This
   is distinct from *tool advertisement*: the voicing block is present on
   every call, whereas the four tool definitions are advertised only while
   `tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN`.
9. **Phase 7 battery placement and live-read paths — RESOLVED AND LANDED
   IN PHASE 7.** The battery lives inside the `multi_turn_text_loop` test
   module, reusing the private harness in place; the fixture JSON lives
   under `crates/qsf_app/tests/fixtures/` and is loaded via
   `CARGO_MANIFEST_DIR`; `read_project_doc` fixtures point at
   allowlisted, present documents. The Phase 7 outcome carries one
   binding constraint forward to Phase 8: **the final assistant reply is
   read from the `TurnCompleted` event payload, not from a trace** —
   `traces.jsonl` has no assistant-reply record.

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
     kind/maturity hedging; the reply is recorded in the TurnCompleted
     event (NOT in traces.jsonl)
  -> post-hoc enrichment pass (Phase 8) joins each project_doc_* trace to
     the same-turn TurnCompleted reply and writes a project_doc_influence
     trace marking whether the reply overlapped the returned content
```

---

## Phase 1: `ProjectDocService` library — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 1").
Source of truth: `crates/qsf_app/src/project_docs/`.

A pure, side-effect-free `project_docs` module (`pub mod project_docs;`
in `lib.rs`): `types` (`DocKind`, `MaturityTag`, `MatchStrength`,
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
  production wiring uses **absolute** repo-root and allowlist paths.
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
- **Live-read consequence (Phase 7, held).** Because `read` is
  path-confined against the allowlist+corpus, any test that drives a
  *real* `read_project_doc` must use paths that are allowlisted and
  present, or the read returns a refusal/error rather than a
  `refused == false` success trace.

**Acceptance outcome (met):** `cargo test -p qsf_app project_docs`
passes; clippy and fmt clean.

**Diary follow-up constraint:** Phase 1 was committed as a standalone
slice; the Phase 9 diary pass must explicitly account for the
`project_docs` library work, not silently skip it.

---

## Phase 2: Tool implementations — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 2").
Source of truth: `crates/qsf_app/src/tools/`.

`ToolPermission::read_only()`; a defaulted
`ToolContext::project_doc_service() -> Option<&ProjectDocService>`
(returns `None`); `ProjectDocToolContext<'a> { service }`;
`SearchProjectDocsTool` (reads `query`/`max_results`, **normalizes
`max_results` into `1..=DEFAULT_MAX_RESULTS`**); `ReadProjectDocTool`
(reads `path`/`focus`/`max_tokens`, **clamps `max_tokens` to
`MAX_TOKENS_HARD_CAP` (4000)**, forwards the raw `path`).
`tools/mod.rs` re-exports the names and types.

**Binding constraints on later phases:**

- **Combined context required for live dispatch (OQ #2).** Built in
  Phase 4 as `ResponderToolContext`.
- **Upper-bound discipline is the tool's job.** `max_results` and
  `max_tokens` are clamped/normalized inside the tools; later phases must
  not assume the model honors the advertised schema.
- **Diary discipline.** Application work of Phases 1-8 groups under a
  single Phase 9 diary entry; isolated merges carry a short standalone
  entry, reconciled (not duplicated) in Phase 9.

**Acceptance outcome (met):** `cargo test -p qsf_app tools::` passes;
clippy and fmt clean.

---

## Phase 3: `ToolRegistry` wiring — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 3").
Source of truth: `crates/qsf_app/src/tools/tool_registry.rs`.

The registry knows all four tools. The struct and `Default` carry
`search_project_docs` and `read_project_doc`, and all three `match` sites
(`metadata_for`, `dispatch`, `model_tool_definitions_for`) route both new
tool names via the shared constants (no duplicated literals). Because
`tool_request_from_model_tool_call` routes unrecognized tools through its
catch-all arm, the dispatch request-builder needed no change.

**Binding constraints on later phases:**

- The registry holds no `ToolContext`; it receives one per call, so the
  combined-context question (OQ #2) was settled in Phase 4.
- No runtime call site changed in Phase 3.

**Acceptance outcome (met):** registry tests assert metadata, definitions,
and dispatch for both tools; build, focused tests, clippy, fmt clean.

---

## Phase 4: Combined context + true per-turn budget state — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 4").
Source of truth: `crates/qsf_app/src/tools/responder_tool_context.rs` and
`crates/qsf_app/src/models/tool_dispatch.rs`.

- **Combined context (OQ #2).** `ResponderToolContext<'a> { state,
  project_docs }` so one `&dyn ToolContext` answers *both* accessors with
  `Some(_)`. Dedicated, purely additive type; single-accessor contexts
  unchanged.
- **True per-turn budget.** `PROJECT_DOC_SEARCH_CAP_PER_TURN = 2` and
  `PROJECT_DOC_READ_CAP_PER_TURN = 1`, plus `ProjectDocToolBudget {
  turn_index, search_calls, read_calls }`. The dispatcher takes
  `project_doc_budget: &mut ProjectDocToolBudget`; callers create one
  budget per human turn and reuse it across batches.
- **Refusal path.** An over-cap call is refused *before* reaching the
  registry, emitting a `ToolFailed` event (no preceding `ToolRequested`,
  preserving symmetry for executed calls) with `refusal_reason ==
  "per_turn_cap"`, `cap`, `attempted_count`, `turn_index`, and sanitized
  arguments; a refusal `TraceRecord` (`details.refused == true`); and a
  `ToolResult` whose `observation_summary` names `per_turn_cap`.
- **`sanitized_project_doc_arguments` helper.** Preserves only stable,
  non-sensitive replay inputs.

**Binding constraints on later phases:**

- **Reuse, don't reinvent.** Success-path traces (Phase 5) reuse this
  helper, the budget's `turn_index`, and the same correlation fields, so
  refused and executed calls join cleanly.
- **Budget invariant (held in Phase 6).** Fresh `ProjectDocToolBudget`
  per human turn, shared across all provider tool batches inside that
  turn; calculator and `recall_turn` never consume the budget.
- **Refused calls still return a `ToolResult`.** Phase 6 appends a
  provider-native `tool_result` for it so the provider gets a response
  for every `tool_call_id` it emitted. **Phase 8 note:** refused traces
  carry `turn_index` but no returned content, so the enrichment pass
  skips them (nothing to overlap).

**Acceptance outcome (met):** both accessors return `Some(_)`; caps
enforced within one batch and across two batches sharing one budget;
fresh budget resets next turn; refusal telemetry carries the full
correlation fields. Build, focused tests, clippy, fmt clean.

**Diary discipline (still binding):** isolated-merge entries reconciled,
not duplicated, in Phase 9.

---

## Phase 5: Success TraceRecord emission for project-doc calls — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 5").
Source of truth: `crates/qsf_app/src/models/tool_dispatch.rs`.

Symmetric *success* traces (`details.refused == false`) on the executed
`search_project_docs` / `read_project_doc` paths, so a researcher can
replay **every** project-doc call from a run's `traces.jsonl`:

- `project_doc_search` stores the parsed `hits` array **and** an explicit
  `details.hit_count`. Each hit is a serialized `DocHit` whose textual
  fields are `snippet` and (optional) `section_hint`.
- `project_doc_read` stores the **parsed read output** (`details.read`) —
  a serialized `DocRead` whose body text is `content` — alongside
  `is_full` / `omitted_sections`.

**Binding constraints on later phases (Phase 8 in particular):**

- **Replayability is the success criterion.** Phase 8's `influenced_reply`
  enrichment computes overlap directly from `details.hits[].snippet`
  (search) and `details.read.content` (read). If either is missing or
  shaped differently than recorded here, surface it before Task 8.2.
- **Reuse discipline (held).** Traces reuse
  `sanitized_project_doc_arguments`, the budget's `turn_index`, the single
  `ToolCompleted` latency value, and the Phase 4 correlation fields.
- **Failure semantics.** A project-doc call that reaches execution and
  fails writes `ToolFailed` and emits **no** success trace; the over-cap
  refusal path never reaches execution.

**Acceptance outcome (met):** successful search/read dispatch each emit
exactly one `project_doc_*` trace with `refused == false`, the explicit
`hit_count` / bounded `read` content, sanitized arguments, `turn_index`,
the full correlation fields, and a recorded latency; failed-execution and
non-project-doc regression tests confirm no spurious success traces.

**Diary discipline (still binding):** reconcile any isolated-merge entry
in Phase 9.

---

## Phase 6: Responder role wired + bounded two-round tool loop — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 6: wire
responder introspection loop"). Source of truth:
`crates/qsf_app/src/experiments/multi_turn_text_loop.rs` (and its
`/tests.rs`).

This is the first **live** call site for the channel. `run_one_turn`:

- **Advertises all four tools** via
  `conversational_responder_role_with_session_and_project_doc_tools()`
  and `responder_request_for_messages(..., advertise_tools)`.
- **Builds a live `ResponderToolContext` over absolute paths (OQ #1)**
  using `project_doc_service_for_multi_turn_text_loop(context)`, which
  constructs the `ProjectDocService` once from `RunContext`'s
  canonicalized `--workspace-root`.
- **Runs a bounded two-round tool loop** capped by
  `MAX_RESPONDER_TOOL_ROUNDS_PER_TURN = 2`, reusing a single
  `ProjectDocToolBudget::new(turn_index)` across batches, appending one
  `assistant_tool_calls` plus one `tool_result` per returned `ToolResult`
  (executed OR refused), with
  `advertise_tools = tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN`
  governing the next request's tools (dropped only on the call following
  the second tool round).
- **Guards against an unbounded loop:** a third tool batch records an
  `ErrorOccurred` event (stage `bounded-tool-loop`) and bails without
  appending the turn.
- **Keeps the voicing block present across the whole turn (OQ #8)** via
  `assemble_prompt_with_summaries_and_project_doc_channel(..., project_doc_channel_enabled)`,
  including the final no-tools answer call.
- **Preserves accounting and recalls** across every provider call.

**Binding constraints on later phases:**

- **`turn_index` alignment (Phase 8).** `turn_index =
  completed_turn_count(state)` is computed before the turn is pushed and
  drives both the budget/`project_doc_*` trace `details.turn_index` and
  the completed turn's `index`, so a `project_doc_*` trace and its
  same-turn `TurnCompleted` event share one turn index.
- **Live reads execute** against the real allowlisted corpus.

**Acceptance outcome (met):** the live request advertises all four tool
definitions; the responder completes a bounded `search -> read -> answer`
sequence across two batches in one turn with both tool results appended
before the final answer and the two success traces sharing one
`turn_index`; a second read inside the turn is refused
(`per_turn_cap`); a third batch records `ErrorOccurred` and does not
append the turn; the voicing block is present on every call including the
final no-tools answer; an ordinary no-tool answer completes exactly one
turn with no project-doc traces. Build,
`cargo test -p qsf_app multi_turn_text_loop`, clippy, fmt clean.

**Diary discipline (still binding):** reconcile any isolated-merge entry
in Phase 9.

---

## Phase 7: Self-question battery fixture test — completed

**Status: landed and committed** ("ProjectDocIntrospection Phase 7:
Self-question battery fixture test"). Source of truth:
`crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs` and
`crates/qsf_app/tests/fixtures/self_question_battery.json`.

A data-driven offline battery drives a fixed list of self-questions
through the **real** bounded responder loop (Phase 6) via
`run_with_io_and_components` and a scripted `SequencedResponderClient`.
Because the responder is driven by a scripted `ModelClient`, the battery
proves the **plumbing and voicing contract**, not the model's
natural-language choices; reply-text assertions guard the canned fixtures
against authoring drift.

What landed:

- **Fixture `self_question_battery.json`** (loaded via
  `CARGO_MANIFEST_DIR`) encodes each question's emitted tool calls (with
  `round` + concrete `arguments`) and its `expected_reply_*` assertions.
  The battery validates each question's rounds are unique, 1-based,
  contiguous, and ≤ `MAX_RESPONDER_TOOL_ROUNDS_PER_TURN` (2) before
  driving (P7-002), failing with the offending `question.id`.
- **In-crate harness reused in place (OQ #9).** The battery reuses the
  private `multi_turn_text_loop` test scaffolding
  (`SequencedResponderClient`, `PlannedResponderResponse`,
  `run_with_io_and_components`, `test_context`, `TestMemorySource`,
  `responder_tool_names`, `parse_event_records`, `parse_trace_records`)
  rather than widening the public surface.
- **Assertions per question.** Provider-call shape (advertised tools per
  the `advertise_tools = tool_rounds < 2` gate — empty advertised tools
  only on the two-round search-then-read question's final answer call;
  one-tool and off-topic questions' final calls still advertise the four
  tools, so their final *response* carrying no tool calls is asserted
  instead); the kind/maturity voicing block on **every** provider call
  including the final answer call (OQ #8); one `project_doc_*` success
  trace (`refused == false`) per emitted tool call with matching
  sanitized arguments and a shared `turn_index` for the two-round
  question; exactly one `TurnCompleted` per question; the canned reply's
  `contains` / `must_not_contain` hedging assertions.
- **Off-topic control** routes through the loop with zero project-doc
  traces and no project-doc `ToolCompleted`, completing exactly one turn
  while still carrying the voicing block.
- **Reply extraction (P7-003).** The reply is read from the single
  `TurnCompleted` event payload (`payload.turn.assistant_response`), not
  from `ExperimentOutcome` or console output.

**Binding constraints on later phases (Phase 8 in particular):**

- **The reply source is the event stream, not a trace.** The final
  assistant reply lives in the `TurnCompleted` **event** in
  `events.jsonl` (`payload.turn.assistant_response`, turn index
  `payload.turn.index`), *not* in `traces.jsonl`. Phase 8's enrichment
  join must read the reply from `events.jsonl` and the `project_doc_*`
  records from `traces.jsonl`, matching on the (aligned) turn index.
- **Live reads execute against the real corpus.** Fixture `read` paths
  must remain allowlisted and present
  (e.g. `docs/ProjectFrame/ProjectVision.md`).
- **Scripted client proves plumbing, not reply quality.** True
  reply-quality verification is the live Phase 10 gate.

**Acceptance outcome (met):** the battery loads and validates the
fixture, drives each question through the real bounded loop, exercises
the two-round search-then-read path with a shared `turn_index` and an
empty advertised-tools list only on that question's final answer call,
asserts the voicing block on every provider call, and confirms the
off-topic control makes zero project-doc calls — all under
`cargo test -p qsf_app`. Build, `cargo test -p qsf_app multi_turn_text_loop`,
`cargo test -p qsf_app project_doc`, clippy, fmt clean. No registry,
library, dispatch-layer, or loop change was needed.

**Diary discipline (still binding):** Phase 7's coverage is grouped into
the single Phase 9 diary entry; if Phase 7 merged in isolation it carries
a short standalone entry (per P7-004), reconciled — not duplicated — in
Phase 9.

---

## Phase 8: `influenced_reply` post-hoc enrichment

A small, deterministic, **read-mostly** post-run pass that joins each
executed `project_doc_*` trace in a run's `traces.jsonl` to the same-turn
final assistant reply and appends a follow-up `TraceRecord`
(operation = `project_doc_influence`) marking whether the reply
substantively overlapped the returned document content. The original
record is referenced by `trace_id` (OQ #3); no existing record is mutated.

This phase consumes the Phase 5 success-trace content: the search trace's
`details.hits` (each hit's `snippet` / `section_hint`) and the read
trace's `details.read.content` bounded excerpt are the source material the
overlap check runs against. Refused traces (`details.refused == true`)
carry no content and are skipped.

This is the only phase whose deliverable is a post-hoc analysis function
plus its tests; it adds no runtime wiring to the live loop and changes no
reducer, keeping entry points thin (per `Agents.md`). Follow
`superpowers:test-driven-development` (or plain TDD): write the failing
test first for each module, then implement.

### Phase 8 open question to confirm before Task 8.2 (reply source)

The original Phase 8 sketch assumed the final reply was stored as an
`assistant_reply` **trace** in `traces.jsonl`. **That trace does not
exist.** As confirmed in Phase 7 and against
`crates/qsf_app/src/session/runtime.rs` (~289), the final assistant reply
is recorded only in the `TurnCompleted` **event** in `events.jsonl`:

```text
events.jsonl line (TurnCompleted): payload.turn.assistant_response  (reply text)
                                   payload.turn.index               (turn index)
traces.jsonl line (project_doc_*): details.turn_index               (turn index)
```

Both turn-index keys derive from `completed_turn_count(state)` taken
*before* the turn is pushed (`multi_turn_text_loop.rs` ~355), so a
`project_doc_*` trace's `details.turn_index` equals its same-turn
`TurnCompleted` event's `payload.turn.index`. The two are joinable.

**Decision needed before implementing the join — DEFAULT chosen, raise if
you disagree:**

- **Default (recommended): join across files, no production change.**
  `enrich` reads the reply from `events.jsonl` (`TurnCompleted` →
  `payload.turn.assistant_response`, keyed by `payload.turn.index`) and
  the `project_doc_*` records from `traces.jsonl` (keyed by
  `details.turn_index`), and appends `project_doc_influence` records to
  `traces.jsonl`. This keeps the live loop untouched and keeps the
  enrichment a pure post-run consumer.
- **Alternative (only if a single-file pass is required): emit an
  assistant-reply trace at turn completion.** This would add a production
  `record_trace` in the session runtime so `traces.jsonl` is
  self-contained. It changes runtime behavior and touches an entry path,
  so it is **out of scope for this phase** unless explicitly chosen —
  raise it before Task 8.2 rather than adding it silently.

The `Turn` field names (`index`, `assistant_response`) are confirmed in
`crates/qsf_app/src/session/mod.rs` (~120); the hit/read JSON shapes
(`DocHit.snippet`, `DocRead.content`) are confirmed in
`crates/qsf_app/src/project_docs/types.rs`. Re-confirm against the actual
artifacts while implementing (per the Phase 5 replayability constraint).

### No-reply behavior (resolved — was contradictory in the original sketch)

A `project_doc_*` trace whose turn never emitted a `TurnCompleted` reply
(e.g. an aborted turn that executed a tool then hit the unbounded-loop
guard in Phase 6) has **no reply to overlap**. **Decision: skip it** — do
not append an influence record, and do not count it. `enrich` therefore
appends and returns *one influence record per executed `project_doc_*`
trace **that has a same-turn reply***, not per executed trace
unconditionally. The acceptance criteria below reflect this. (The
alternative — appending `influenced_reply == false` with a
`details.reason = "no_reply"` note — is rejected for v1 to keep the
appended set meaning "a reply existed and was checked"; raise it before
Task 8.2 if a complete row-per-execution audit is preferred instead.)

### Idempotency (per review P8-003)

`enrich` must be **idempotent by `source_trace_id`**. Appending to the
same `traces.jsonl` means a second naive run would duplicate every
influence record and corrupt post-hoc counts. Before appending, `enrich`
scans existing `project_doc_influence` records and collects their
`details.source_trace_id` set; any source already enriched is skipped.
Re-running `enrich` over an already-enriched directory appends `0` records.

### Task 8.1: Overlap check

**Files:**
- Create: `crates/qsf_app/src/project_docs/influence.rs`
- Modify: `crates/qsf_app/src/project_docs/mod.rs`

A pure, dependency-free word-overlap predicate. False negatives are
acceptable; false positives are guarded against by requiring a contiguous
multi-word run. Comparison is at the **word** level (not raw substring),
case-insensitive, and ignores surrounding punctuation, so ordinary
markdown prose punctuation in either side does not defeat a real match.

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/qsf_app/src/project_docs/influence.rs (test block)
#[cfg(test)]
mod tests {
    use super::reply_overlaps_excerpt;

    #[test]
    fn overlapping_reply_is_marked_influenced() {
        // The reply quotes a contiguous run of >= MIN_NGRAM_SIZE qualifying
        // (length >= 3) words from the excerpt: "project's accepted framing says".
        let excerpt = "The project's accepted framing says autonomy is deferred.";
        let reply = "As the project's accepted framing says, that part is deferred.";
        assert!(reply_overlaps_excerpt(reply, excerpt));
    }

    #[test]
    fn unrelated_reply_is_not_influenced() {
        let excerpt = "The project's accepted framing says autonomy is deferred.";
        let reply = "The capital of France is Paris.";
        assert!(!reply_overlaps_excerpt(reply, excerpt));
    }

    #[test]
    fn short_excerpt_below_ngram_size_is_not_influenced() {
        // Fewer than MIN_NGRAM_SIZE qualifying words => no false positive.
        assert!(!reply_overlaps_excerpt("anything at all here", "tiny note"));
    }
}
```

- [ ] **Step 2: Implement the check.**

```rust
// crates/qsf_app/src/project_docs/influence.rs
//! Best-effort overlap check used to mark whether a tool-returned excerpt
//! influenced the final assistant reply. False negatives are acceptable;
//! false positives are guarded against by requiring a contiguous
//! multi-word run. Comparison is word-level, case-insensitive, and
//! punctuation-insensitive.

use std::collections::HashSet;

const MIN_NGRAM_SIZE: usize = 4;
const MIN_WORD_LEN: usize = 3;

/// Lowercased, punctuation-stripped words of length >= MIN_WORD_LEN.
/// Apostrophes are preserved so possessives like "project's" stay one token.
fn qualifying_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_ascii_lowercase())
        .filter(|word| word.len() >= MIN_WORD_LEN)
        .collect()
}

/// Returns true when `reply` and `excerpt` share a contiguous run of at
/// least `MIN_NGRAM_SIZE` qualifying words. Word n-grams (not raw
/// substrings) so punctuation in either side does not defeat the match.
pub fn reply_overlaps_excerpt(reply: &str, excerpt: &str) -> bool {
    let excerpt_words = qualifying_words(excerpt);
    let reply_words = qualifying_words(reply);
    if excerpt_words.len() < MIN_NGRAM_SIZE || reply_words.len() < MIN_NGRAM_SIZE {
        return false;
    }
    let reply_ngrams: HashSet<String> = reply_words
        .windows(MIN_NGRAM_SIZE)
        .map(|window| window.join(" "))
        .collect();
    excerpt_words
        .windows(MIN_NGRAM_SIZE)
        .any(|window| reply_ngrams.contains(&window.join(" ")))
}
```

- [ ] **Step 3: Re-export and run tests.**

```rust
// crates/qsf_app/src/project_docs/mod.rs  (add alongside the existing mods)
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

A function that, given a finished run's directory, reads the
`TurnCompleted` replies from `events.jsonl` and the executed
`project_doc_*` records from `traces.jsonl`, joins them on turn index,
computes the overlap signal against the recorded content, and appends one
`project_doc_influence` record per executed project-doc call that has a
same-turn reply and is not already enriched.

**Shape (default join, per the reply-source open question above):**

```text
pub fn enrich(run_dir: &Path) -> anyhow::Result<usize>
  // 1. Parse events.jsonl line by line. For each EventType::TurnCompleted,
  //    extract (turn_index = payload.turn.index, reply = payload.turn.assistant_response)
  //    into a turn_index -> reply map. (Field names confirmed in session/mod.rs.)
  // 2. Parse traces.jsonl line by line into TraceRecord (it derives Deserialize).
  //    a. First collect already_enriched: the set of details.source_trace_id from
  //       every existing operation == "project_doc_influence" record (idempotency).
  //    b. Keep source records whose operation is "project_doc_search" or
  //       "project_doc_read" AND details.refused == false (skip refused: no
  //       content) AND whose trace_id is not in already_enriched.
  // 3. For each kept source record:
  //    - Look up the same-turn reply by details.turn_index. If none (no
  //      TurnCompleted for that turn), SKIP (no append, not counted) -- per the
  //      no-reply decision above.
  //    - Extract the source content:
  //        - project_doc_search: concatenate each hit's "snippet" (and
  //          "section_hint" when present) from details.hits.
  //        - project_doc_read:   details.read.content.
  //    - Compute influenced = reply_overlaps_excerpt(reply, content).
  // 4. Append one TraceRecord per kept+matched source (operation
  //    "project_doc_influence") to run_dir/traces.jsonl, details:
  //      { source_trace_id, source_operation, turn_index, influenced_reply }
  //    referencing the original by trace_id (OQ #3). Return the count appended.
```

**Append safely (per review P8-005).** `TraceLogWriter::create` opens with
`truncate(true)`, and the current `RunContext` constructors create a fresh
run directory with that same truncate-mode writer — there is **no** safe
"reopen an existing run's context and append" API today. So `enrich` must
append directly:
`OpenOptions::new().create(true).append(true).open(run_dir.join("traces.jsonl"))`,
writing each serialized `TraceRecord` followed by `\n`. Do **not** route
this through `TraceLogWriter::create` or a reopened `RunContext` (either
would truncate the run's existing records). If a reusable append-mode
`TraceLogWriter` constructor is later wanted, that is a separate change;
do not add it silently here.

Reuse `TraceRecord` (`Deserialize`) for the trace lines and the existing
`EventType` / event-record parsing for the event lines rather than
hand-rolling new structs where existing ones fit. Surface any remaining
naming/shape uncertainty (hit-field names beyond `snippet`/`section_hint`,
the exact `TurnCompleted` payload nesting) as a question rather than
guessing.

- [ ] **Step 1: Write the failing tests.** Cover **both** the search and
  read content paths (per review P8-002), the refused skip, and
  idempotency.

```rust
// crates/qsf_app/src/project_docs/enrichment.rs (test block)
#[cfg(test)]
mod tests {
    use super::enrich;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn enrichment_appends_influence_record_for_search() {
        let dir = tempdir().unwrap();
        let run_dir = dir.path();

        // events.jsonl: one TurnCompleted whose turn.index = 0 and whose
        // turn.assistant_response quotes a >=4-qualifying-word run from the
        // search snippet below. Match the real event/Turn JSON shape
        // (session/runtime.rs TurnCompleted payload).
        fs::write(run_dir.join("events.jsonl"), /* one TurnCompleted line */)
            .unwrap();

        // traces.jsonl: one project_doc_search trace, turn_index = 0,
        // refused = false, details.hits[0].snippet carrying the excerpt text.
        fs::write(run_dir.join("traces.jsonl"), /* one project_doc_search line */)
            .unwrap();

        let appended = enrich(run_dir).unwrap();
        assert_eq!(appended, 1);

        // Re-read traces.jsonl; assert a project_doc_influence record was
        // appended with details.influenced_reply == true and
        // details.source_trace_id == the original search trace_id, and that
        // the original search line is still present (not truncated).
    }

    #[test]
    fn enrichment_appends_influence_record_for_read() {
        // Same as above but the source is a project_doc_read trace whose
        // details.read.content is quoted by the reply; assert one appended
        // project_doc_influence with influenced_reply == true.
    }

    #[test]
    fn non_overlapping_reply_marks_not_influenced() {
        // A read/search trace whose content is NOT quoted by the reply yields
        // one influence record with influenced_reply == false.
    }

    #[test]
    fn refused_traces_are_skipped() {
        // A refused project_doc_* trace yields no influence record.
    }

    #[test]
    fn enrich_is_idempotent() {
        // First enrich appends N>0; a second enrich over the same dir appends 0
        // (sources already enriched are skipped by source_trace_id).
    }
}
```

- [ ] **Step 2: Implement `enrich`.** Follow the pattern of any existing
  post-hoc artifact reader in `crates/qsf_app/src/` (e.g. the report
  module that names `events.jsonl` / `traces.jsonl`) for path handling and
  JSON-lines parsing, and the open-options append described above.

- [ ] **Step 3: Re-export and run tests.**

```rust
// crates/qsf_app/src/project_docs/mod.rs
pub mod enrichment;
pub use enrichment::enrich;
```

Run: `cargo test -p qsf_app project_docs::enrichment`
Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/qsf_app/src/project_docs/enrichment.rs \
        crates/qsf_app/src/project_docs/mod.rs
git commit -m "feat(project_docs): traces.jsonl post-hoc influenced_reply enrichment"
```

### Task 8.3: Invocation point (decision + minimal wiring)

`enrich` is a library function with deterministic tests; **whether and
where it is invoked on real runs is a separate decision** that this phase
must make explicitly rather than leave dangling.

- **DEFAULT (recommended): ship as a library + tests only, no automatic
  invocation in v1.** The live loop stays untouched and entry points stay
  thin; there is **no operator-facing entry point in v1** — running it on
  real artifacts requires calling the Rust function (e.g. from a test or a
  follow-up binary). Phase 10's "optionally run the enrich pass" step is
  therefore explicitly *not* operator-runnable yet (see review P8-006 and
  the amended Phase 10 note).
- **Follow-up (deferred, not in this phase):** if a researcher needs to
  run `enrich` against a run directory without writing Rust, add a small
  standalone analysis surface — a `cargo` subcommand / `xtask` / tiny
  binary taking `run_dir`. Capture this as a backlog item; do **not**
  build it here unless the project asks for it now.
- If the project instead decides the enrichment must run automatically at
  run completion, that adds a call from the run/experiment teardown path
  and is a small, separable change; raise it before wiring, since it
  touches a runtime entry path.

- [ ] Record the chosen invocation decision (a one-line note in this task
  and, if it is a standing behavior, a `docs/DecisionLog.md` entry folded
  into Phase 9 Task 9.2 rather than a separate commit).

### Phase 8 verification

Per `Agents.md`, build first, then focused tests, then the lint/format
gates:

```bash
cargo build
cargo test -p qsf_app project_docs::influence
cargo test -p qsf_app project_docs::enrichment
cargo test -p qsf_app project_docs
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expect all clean. Phase 8 is pure Rust and deterministic — no live model
provider is required; the overlap and enrichment behavior is fully
exercised by unit tests over in-test `events.jsonl` / `traces.jsonl`
fixtures.

**Acceptance criteria for Phase 8:**

- `reply_overlaps_excerpt` returns true on a reply that quotes a
  contiguous ≥4-qualifying-word run from the excerpt and false on an
  unrelated reply or an excerpt with too few qualifying words; matching is
  word-level, case-insensitive, and punctuation-insensitive; it is
  re-exported from `project_docs`.
- `enrich` reads the final reply from the `TurnCompleted` event in
  `events.jsonl` (keyed by `payload.turn.index`) and the executed
  `project_doc_*` records from `traces.jsonl` (keyed by
  `details.turn_index`), joins them on the aligned turn index, and
  appends exactly one `project_doc_influence` record per executed
  project-doc call **that has a same-turn `TurnCompleted` reply and is not
  already enriched** — each referencing its source by `trace_id` and
  carrying `details.influenced_reply`.
- The overlap signal is computed from `details.hits[].snippet`
  (`project_doc_search`) and `details.read.content` (`project_doc_read`);
  **both** content paths are covered by tests.
- Refused project-doc traces (`details.refused == true`) produce no
  influence record.
- A `project_doc_*` trace whose turn emitted no `TurnCompleted` reply is
  skipped (no influence record, not counted).
- `enrich` is **idempotent**: a second run over an already-enriched
  directory appends `0` records (sources skipped by `source_trace_id`).
- `enrich` appends to `traces.jsonl` via open-options append mode without
  truncating the existing records (it does not use `TraceLogWriter::create`
  or a reopened `RunContext`).
- The reply-source, no-reply, and invocation-point decisions are taken
  explicitly per the in-phase open questions; if any alternative
  (production assistant-reply trace, append-false-on-no-reply, or
  automatic/operator invocation) is chosen, it is surfaced before
  implementation rather than added silently.
- `cargo build`, the focused tests above,
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` are clean.

**Diary discipline (still binding):** Phase 8 application work is grouped
under the single Phase 9 diary entry; reconcile, don't duplicate, any
isolated-merge entry there.

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

If Phase 8 Task 8.3 chose automatic enrichment invocation as a standing
behavior, fold that decision in here as a second entry rather than a
separate commit.

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
discipline noted in those phases — including a possible standalone Phase 7
entry per P7-004), reconcile rather than duplicate them here.

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
  post-hoc reply-overlap check and traces.jsonl influence enrichment.
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
- Post-hoc `influenced_reply` enrichment joins each project-doc trace to
  the same-turn TurnCompleted reply (idempotent, skips refused and
  no-reply turns).

Refs: crates/qsf_app/src/project_docs, crates/qsf_app/src/tools,
crates/qsf_app/src/models/tool_dispatch.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
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
- [ ] The Phase 8 `enrich` pass is **library-only in v1** (no
  operator-facing entry point — see Task 8.3). Running it over a real run
  directory requires invoking the Rust function (e.g. an ad-hoc test or
  the deferred analysis subcommand), so it is **optional and not part of
  the standard operator flow** for this phase. If you do invoke it,
  spot-check that the appended `project_doc_influence` records agree with
  your reading of which replies were grounded in fetched content.
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
  as `search_project_docs`, from post-hoc `project_doc_*` traces
  (including the Phase 8 `project_doc_influence` signal), from curated
  stable project facts, or from a combination.
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
  under the correct names (Phase 3 tests should already cover this in
  CI — search *and* read dispatch tests).
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
- [ ] Verify the Phase 7 self-question battery loads its fixture,
  validates fixture rounds, drives each question through the **real**
  bounded responder loop (not a direct dispatch call), asserts the
  search-then-read two-round path with shared `turn_index` and an empty
  advertised-tools list only on that question's final answer call,
  asserts the voicing block on every provider call including each
  question's final answer call, and asserts the off-topic control makes
  zero project-doc calls — all under `cargo test -p qsf_app`.
- [ ] Verify the Phase 8 enrichment joins each executed `project_doc_*`
  trace to its same-turn `TurnCompleted` reply (reply read from
  `events.jsonl`, traces from `traces.jsonl`, matched on the aligned turn
  index), appends exactly one `project_doc_influence` record per executed
  call that has a same-turn reply (referencing the source by `trace_id`),
  computes overlap from both `details.hits[].snippet` and
  `details.read.content`, skips refused traces and no-reply turns, is
  idempotent (a second `enrich` appends 0), and does not truncate
  `traces.jsonl`; `reply_overlaps_excerpt` requires a contiguous
  multi-word run (Phase 8 tests should already cover this in CI).
- [ ] Verify `Architecture.ToolSystem.md`'s *Implementation Status*
  section lists the two new tools under "Implemented today" with code
  refs and a refreshed `Last reviewed:` date.
- [ ] Confirm there is exactly one diary entry covering Phases 1-8 (or,
  since Phase 1 was committed independently, a standalone library-slice
  entry plus the Phases 2-8 entry), with any isolated-merge diary entries
  (including a possible standalone Phase 7 entry) reconciled rather than
  duplicated.
- [ ] Confirm Phase 11 remains a follow-on planning handoff unless it has
  been promoted into a separate detailed design or implementation plan.