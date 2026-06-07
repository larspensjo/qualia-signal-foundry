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

Phases 1-6 have landed and are committed. The minimum viable channel is
live: the `ConversationalResponder` advertises both project-doc tools in
the multi-turn text loop, can run a bounded `search -> read -> answer`
sequence inside one human turn under a true per-turn budget, and the
dispatch layer emits success/refusal traces for every call.

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

**Phase 7 (the offline self-question battery fixture test) is the next
implementation step.** It is the first phase whose deliverable is a CI
regression test rather than runtime wiring: it replays a fixed list of
self-questions through the now-live bounded responder loop using a
scripted `ModelClient` and asserts on the tool calls made (including
round), the recorded events and traces, the voicing-block presence on
every provider call, and the hedging language in the canned replies.
Phase 8 adds the `influenced_reply` post-hoc enrichment, Phase 9 lands the
documentation updates, Phase 10 records the live external verification,
and Phase 11 is a future planning handoff.

## Background

The design at `docs/Plans/Design.ProjectDocIntrospection.md` specifies a
live-first introspection channel for project documents. This plan
implements the v1 channel in sequential implementation and documentation
phases that each produce something independently testable. Phases 1-6
(landed) are the minimum viable channel: the tools work end-to-end and
the responder can call them mid-dialogue. Phase 7 delivers the offline
self-question battery promised by the design's *Live-First Rationale* as
a deterministic, CI-runnable regression gate over the live loop's shape
and voicing rules. Phase 8 adds the `influenced_reply` post-hoc
enrichment. Phase 9 lands the documentation updates required by
`docs/ProjectFrame/ProjectWorkflow.md`. Phase 10 records the live
external verification step. Phase 11 is a future planning handoff for
associative project-doc context pointers; it is not part of the v1 tool
implementation.

## Current Anchors

Code anchors:

- `crates/qsf_app/src/project_docs/` — **landed (Phase 1).** Pure
  library: `Allowlist`, metadata extraction, lexical `search`, bounded
  `read`, and the `ProjectDocService` facade. Later phases consume it;
  they do not modify it. Phase 8 adds an `influence` sibling module.
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
  success traces (Phase 5). No change expected in Phase 7.
- `crates/qsf_app/src/models/model_client.rs` — the `ModelClient` trait
  (`complete(&self, request: &ModelRequest) -> Result<ModelResponse>`),
  `ModelResponse` (`output_text`, `tool_calls`, `usage`, …), and
  `ModelToolCall`. Phase 7's scripted stub implements this trait.
- `crates/qsf_app/src/models/mock_model.rs` — `MockModelClient`, the
  fixture-driven mock and the existing reference for deterministic
  responder outputs.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — **landed
  (Phase 6).** `run_one_turn` holds the bounded loop:
  `MAX_RESPONDER_TOOL_ROUNDS_PER_TURN = 2`, one
  `ProjectDocToolBudget::new(turn_index)` reused across batches,
  `project_doc_service_for_multi_turn_text_loop(context)` (absolute
  workspace root), `conversational_responder_role_with_session_and_project_doc_tools()`,
  `responder_request_for_messages(..., advertise_tools)` and the
  **`advertise_tools = tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN`**
  gate at the bottom of the loop (so tools are dropped from the request
  only on the call that follows the second tool round), and
  `execute_model_tool_calls(...)` which returns one execution per tool
  call carrying the `call_id` for the appended `tool_result`. An
  `ErrorOccurred` event (stage `bounded-tool-loop`) is recorded and the
  turn bails if tool calls persist after the two rounds.
- `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs` —
  **the proven in-crate harness Phase 7 reuses.** Contains
  `SequencedResponderClient` + `PlannedResponderResponse` (a scripted
  `ModelClient` that replays a fixed list of responses, with `.calls()`
  capturing each request's `role_id`/`tools`/`messages`),
  `run_with_io_and_components(...)`, `test_context(...)`,
  `TestMemorySource`, `test_config_with_warm_threshold(...)`,
  `responder_tool_names()`, and `parse_event_records` /
  `parse_trace_records`. It already has
  `responder_can_search_then_read_across_two_tool_batches`,
  `responder_reuses_project_doc_budget_across_tool_batches`,
  `follow_up_tool_calls_fail_without_appending_turn`, and the
  voicing-block prompt tests driven by
  `assemble_prompt_with_summaries_and_project_doc_channel(..., project_doc_channel_enabled)`.
- `crates/qsf_app/src/observability/trace.rs` — `TraceRecord::new(...)`
  with `.with_details(Value)` and `.with_latency_ms(u64)`.
- `crates/qsf_app/src/observability/event_log.rs` —
  `EventType::ToolRequested` / `ToolCompleted` / `ToolFailed`.
- `crates/qsf_app/src/runtime/run_context.rs` — `RunContext` exposes
  `experiment_id()`, `run_id()`, `run_dir()`, `record_event(...)`, and
  `record_trace(...)`; the workspace root supplied via `--workspace-root`
  is canonicalized here and consumed by the live `ProjectDocService`
  construction.

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
3. **`influenced_reply` storage.** Phase 8 writes the marker as a
   follow-up `TraceRecord` referencing the original by `trace_id`. If an
   annotation on the original record is preferred, raise before Phase 8.
4. **Module naming.** Per `Agents.md`, name modules after stable
   behavior. This plan uses `project_docs` / `ProjectDocService`, and the
   combined context is named for the role it serves
   (`ResponderToolContext`). Keep this discipline.
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
8. **Voicing-prompt scope across the turn — DECIDED, LANDED IN PHASE 6.**
   The kind/maturity voicing block is present on **every** responder
   provider call in a project-doc turn, *including the final no-tools
   answer call*. It is gated on channel/turn availability, not on whether
   the current request advertises the two tool names. Phase 7 asserts
   this invariant explicitly across the battery. Note this is distinct
   from *tool advertisement*: the voicing block is present on every call,
   whereas the four tool definitions are advertised only while
   `tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN` (see the Phase 7
   loop-behavior note).
9. **Phase 7 battery placement and live-read paths — SURFACE BEFORE
   IMPLEMENTING (see Phase 7 intro).** The harness that drives the *real*
   bounded loop (`SequencedResponderClient`, `run_with_io_and_components`,
   `test_context`, …) is private to the `multi_turn_text_loop` test
   module, and `read_project_doc` calls execute against the real
   allowlisted corpus. Both points are decided with defaults in Phase 7;
   confirm them before writing the test.

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
- **Live-read consequence (Phase 7).** Because `read` is path-confined
  against the allowlist+corpus, any test that drives a *real*
  `read_project_doc` (Phase 7's battery) must use paths that are
  allowlisted and present, or the read returns a refusal/error rather
  than a `refused == false` success trace.

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
  for every `tool_call_id` it emitted.

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
  `details.hit_count`.
- `project_doc_read` stores the **parsed read output** (`details.read`) —
  the bounded content/excerpt plus metadata — alongside `is_full` /
  `omitted_sections`.

**Binding constraints on later phases:**

- **Replayability is the success criterion.** Phase 8's `influenced_reply`
  enrichment computes overlap directly from `details.hits` and
  `details.read`. If either is missing or shaped differently, surface it
  before Phase 8.
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
non-project-doc regression tests confirm no spurious success traces. Pure
Rust; live behaviour is verified in Phase 7's battery and Phase 10's
manual session.

**Diary discipline (still binding):** reconcile any isolated-merge entry
in Phase 9.

---

## Phase 6: Responder role wired + bounded two-round tool loop — completed

**Status: landed and committed** ("ProjectDocIntrospection phase 6: wire
responder introspection loop"). Source of truth:
`crates/qsf_app/src/experiments/multi_turn_text_loop.rs` (and its
`/tests.rs`).

This is the first **live** call site for the channel. `run_one_turn` now:

- **Advertises all four tools.**
  `conversational_responder_role_with_session_and_project_doc_tools()`
  lists `calculator`, `recall_turn`, `search_project_docs`, and
  `read_project_doc`; `responder_request_for_messages(role, messages,
  context, state, registry, max_output_tokens, advertise_tools)` builds
  each provider request, and the live request-assembly path is covered by
  a test asserting the assembled tool definitions include all four names.
- **Builds a live `ResponderToolContext` over absolute paths (OQ #1).**
  `project_doc_service_for_multi_turn_text_loop(context)` constructs the
  `ProjectDocService` once from `RunContext`'s canonicalized
  `--workspace-root`, never from `CARGO_MANIFEST_DIR` or the process
  working directory.
- **Runs a bounded two-round tool loop.** A `loop` makes the initial
  provider call (tools advertised), and while the response carries tool
  calls and fewer than `MAX_RESPONDER_TOOL_ROUNDS_PER_TURN = 2` rounds
  have run, dispatches the batch through `execute_model_tool_calls(...)`,
  appends one `ModelMessage::assistant_tool_calls(...)` plus one
  `ModelMessage::tool_result(call_id, …)` **per returned `ToolResult`
  (executed OR refused)**, records `PromptAssembled`, and re-invokes the
  responder. The next request's tools are governed by
  `advertise_tools = tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN`:
  tools are still advertised on the second round and dropped only on the
  call that follows the second tool round (the final answer call). A
  single `ProjectDocToolBudget::new(turn_index)` is reused across both
  batches.
- **Guards against an unbounded loop.** If a response still carries tool
  calls after the two permitted rounds, an `ErrorOccurred` event (stage
  `bounded-tool-loop`) is recorded and the turn bails without being
  appended.
- **Keeps the voicing block present across the whole turn (OQ #8).**
  `assemble_prompt_with_summaries_and_project_doc_channel(..., project_doc_channel_enabled)`
  appends the kind/maturity voicing block to the responder system prompt
  on **every** provider call in a project-doc turn, including the final
  no-tools answer call; it is gated on channel/turn state, not on the
  request's advertised tools.
- **Preserves accounting and recalls.** Latency and
  input/cached/output tokens accumulate across every provider call in the
  turn; `recalled_turns` are collected from `recall_turn` executions in
  both rounds.

**Binding constraints on later phases (Phase 7 especially):**

- **Reuse the in-crate harness.** The deterministic way to drive the real
  bounded loop is the test scaffolding already in
  `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs`:
  `SequencedResponderClient::new(Vec<PlannedResponderResponse>)` (a
  scripted `ModelClient` whose `.calls()` capture each request's
  `role_id`/`tools`/`messages`), `PlannedResponderResponse::tool_call(...)`
  / `::text(...)`, `run_with_io_and_components(...)`,
  `test_context(...)`, `TestMemorySource`,
  `test_config_with_warm_threshold(...)`, `responder_tool_names()`, and
  `parse_event_records` / `parse_trace_records`. This scaffolding is
  **private to that module** — reuse it in place rather than widening the
  public surface (see Phase 7 OQ #9).
- **Round semantics.** Planned-response index `0` (first provider call)
  is "round 1", index `1` is "round 2", index `2` is the final no-tools
  answer. A search at index 0 and a read at index 1 exercises the
  two-batch `search -> read -> answer` path; the final response must
  carry no tool calls or the turn bails.
- **Tool advertisement vs. response tool calls.** Because
  `advertise_tools = tool_rounds < 2`, the request that follows a *single*
  tool round still advertises the four tools; the request tools are
  emptied only on the call following the *second* tool round. So a final
  answer call's *advertised tools* is empty only for a two-round
  (search-then-read) question; for one-tool and no-tool questions the
  final/only request still advertises the tools, and what distinguishes
  the final call is that its *response* carries no tool calls. Phase 7
  asserts accordingly (see its loop-behavior note).
- **Live reads execute.** `read_project_doc` runs against the real
  allowlisted corpus; fixtures that expect a `refused == false` read
  trace must point at an allowlisted, present document (e.g.
  `docs/ProjectFrame/ProjectVision.md`, proven in
  `responder_can_search_then_read_across_two_tool_batches`).

**Acceptance outcome (met):** the live request advertises all four tool
definitions; the responder completes a bounded `search -> read -> answer`
sequence across two batches in one turn with both tool results appended
before the final answer and the two success traces sharing one
`turn_index`; a second read inside the turn is refused by the shared
budget (`per_turn_cap`) with its refusal tool message still appended; a
third tool batch records `ErrorOccurred` (stage `bounded-tool-loop`) and
does not append the turn; the voicing block is present on every call
including the final no-tools answer; an ordinary no-tool answer completes
exactly one turn with no project-doc traces. Build,
`cargo test -p qsf_app multi_turn_text_loop`, clippy, fmt clean.

**Diary discipline (still binding):** reconcile any isolated-merge entry
in Phase 9.

---

## Phase 7: Self-question battery fixture test

A small structured offline test that exercises the now-live bounded
responder loop with a fixed list of self-questions and asserts on the
calls made (including round), the recorded events and traces, the
voicing-block presence, and the hedging language. It runs as a normal
`cargo test` so it is part of CI, complementing the single inline
two-batch test from Phase 6 with a data-driven battery (multiple
questions, the search-then-read path, and an off-topic control).

**What this phase verifies — and what it deliberately does not.** The
responder is driven by a *scripted* `ModelClient`, so the battery proves
the **plumbing and voicing rules**, not the model's natural-language
choices: the bounded loop wires `search -> read -> answer` across two
provider batches; the per-turn budget is shared; the voicing block is
present on every provider call (including the final no-tools answer); the
off-topic control routes through the loop while making zero project-doc
calls; and events/traces are emitted with the right shape. Reply-text
`contains` / `must_not_contain` assertions run against the *canned*
fixture replies — they pin the intended voicing contract and guard
against fixture-authoring drift, but the genuine behavioral signal lives
in the captured `client.calls()` (round/tool shape, advertised tools),
the system-prompt voicing-block checks, and the `project_doc_*` traces.
True behavioral verification of reply quality is the **live Phase 10
gate**; this battery is the deterministic regression gate.

**Loop-behavior note the assertions must respect (verified against
`multi_turn_text_loop.rs:527`).** The bounded loop advertises tools on
the *next* provider call whenever
`tool_rounds < MAX_RESPONDER_TOOL_ROUNDS_PER_TURN` (= 2). Consequences
the battery must encode rather than fight:

- The request `tools` list is emptied **only** on the call that follows
  the *second* tool round. So the final answer call has empty advertised
  tools **only for the two-round search-then-read question**.
- After a *single* tool round, the next (final) call still advertises the
  four responder tools. The off-topic control's single, no-tool call also
  advertises tools.
- Therefore: assert empty advertised `tools` **only** on the final call
  of the two-round question; for one-tool and off-topic questions assert
  that the final *response* carried no `tool_calls` instead of asserting
  the *request* advertised none.
- The kind/maturity *voicing block* is orthogonal to tool advertisement:
  it is present on **every** provider call (OQ #8), including calls that
  no longer advertise tools.

**Open questions to confirm before implementing (OQ #9):**

1. **Test placement — DEFAULT: in-crate, reusing the private harness.**
   The harness that drives the real bounded loop
   (`SequencedResponderClient`, `run_with_io_and_components`,
   `test_context`, `TestMemorySource`, `responder_tool_names`,
   `parse_*`) is private to the `multi_turn_text_loop` test module. An
   external integration test under `crates/qsf_app/tests/` cannot reach
   it without promoting a public, test-only loop/stub entry point, which
   would widen the public surface purely for testing and cut against
   "keep entry points thin." **This plan therefore adds the battery
   inside `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs`
   (or a sibling `#[cfg(test)]` module that shares its helpers), not as a
   new file under `crates/qsf_app/tests/`.** This is a deliberate change
   from the original sketch's external-`tests/`-file location. *If a
   genuinely external, public-API-only battery is required (e.g. for
   cross-crate reuse), raise it before writing the harness — that path
   needs a separate task to expose a minimal public entry and is out of
   scope here.* The JSON fixture data file still lives under
   `crates/qsf_app/tests/fixtures/` and is loaded via
   `CARGO_MANIFEST_DIR`.
2. **Live-read paths must be real.** Each `read_project_doc` call in the
   battery executes against the production allowlist + `docs/` corpus, so
   every fixture `read` path must resolve to an allowlisted, present
   document or its success-trace assertion will fail. Default: reuse
   known-good paths (e.g. `docs/ProjectFrame/ProjectVision.md`) and
   verify any new path against `config/project-doc-introspection.toml`
   and the actual tree during implementation. If a question's theme has
   no suitable allowlisted doc, either pick a different doc or assert a
   refusal trace for that case instead of a success trace.

This phase is pure Rust with deterministic coverage. Follow
`superpowers:test-driven-development` (or plain TDD): add the harness and
one question first, watch it drive the real loop and pass, then extend
the fixture and assertions incrementally. The battery test *is* the
deliverable and its own verification artifact.

### Task 7.1: Encode the battery fixture

**Files:**
- Create: `crates/qsf_app/tests/fixtures/self_question_battery.json`

Encode each question as a self-driving record: the `tool_calls` array
carries both the `round` and the concrete `arguments` the scripted stub
should emit (so the fixture drives the loop), and the `expected_reply_*`
fields plus the `arguments` carry the assertions. This refines the
original sketch, which conflated "what to emit" with "what to assert".

- [ ] **Step 1: Write the fixture.**

```json
{
  "questions": [
    {
      "id": "what_are_you",
      "prompt": "What are you?",
      "reply": "The project's accepted framing describes a runtime voice loop grounded in its own docs.",
      "tool_calls": [
        { "round": 1, "tool": "search_project_docs", "arguments": { "query": "vision" } }
      ],
      "expected_reply_contains": ["accepted framing"],
      "expected_reply_must_not_contain": []
    },
    {
      "id": "framing_search_then_read",
      "prompt": "What does the project say it is, in its own words?",
      "reply": "The project's accepted framing says to keep the responder grounded in project docs.",
      "tool_calls": [
        { "round": 1, "tool": "search_project_docs", "arguments": { "query": "vision" } },
        { "round": 2, "tool": "read_project_doc",
          "arguments": { "path": "docs/ProjectFrame/ProjectVision.md", "focus": "vision", "max_tokens": 400 } }
      ],
      "expected_reply_contains": ["the project"],
      "expected_reply_must_not_contain": ["I do", "I have"]
    },
    {
      "id": "off_topic_control",
      "prompt": "What's the capital of France?",
      "reply": "The capital of France is Paris.",
      "tool_calls": [],
      "expected_reply_contains": [],
      "expected_reply_must_not_contain": ["search_project_docs"]
    }
  ]
}
```

Notes:
- Every `read_project_doc` `path` must be allowlisted and present (OQ #9
  point 2). Verify against `config/project-doc-introspection.toml` and
  `docs/` while implementing; swap any path that does not resolve.
- The `framing_search_then_read` question intentionally splits search
  (round 1) and read (round 2) so the battery exercises the bounded
  two-round loop and the shared per-turn budget. It is the **only**
  fixture whose final answer call advertises an empty `tools` list (it
  follows the second tool round); the one-tool `what_are_you` and the
  no-tool `off_topic_control` final calls still advertise the four tools
  per the loop-behavior note above.
- Keep the fixture small; add more questions only once the harness and
  assertions are proven.

### Task 7.2: Fixture-driven harness over the real bounded loop

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs`
  (add the battery alongside the existing tests, reusing its helpers —
  see OQ #9). If the module grows unwieldy, extract the new code into a
  sibling `#[cfg(test)]` module declared from `multi_turn_text_loop.rs`
  and make the shared helpers (`SequencedResponderClient`,
  `PlannedResponderResponse`, `CapturedRequest`, `test_context`,
  `TestMemorySource`, `responder_tool_names`, `parse_event_records`,
  `parse_trace_records`) `pub(super)` rather than duplicating them.

- [ ] **Step 1: Define the fixture types and loader.**

Add `serde::Deserialize` structs mirroring the JSON (`Battery`,
`Question`, `ToolCallFixture { round, tool, arguments: serde_json::Value }`)
and load via
`PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/self_question_battery.json")`.

- [ ] **Step 2: Validate fixture rounds before driving (P7-002).**

The single-call harness emits exactly one
`PlannedResponderResponse::tool_call` per fixture entry, in `round`
order, so the `round` values are load-bearing and must be checked rather
than merely sorted. When loading each `Question`, assert its
`tool_calls` rounds are:

- unique (no two calls share a round),
- 1-based and contiguous (1, then 2, …), and
- never greater than `MAX_RESPONDER_TOOL_ROUNDS_PER_TURN` (2).

Fail the test with the offending `question.id` if any rule is violated.
This prevents a fixture from silently mislabeling a round (which the bare
"sort, then emit one response each" approach would not catch) and makes
the round assertions in Task 7.3 meaningful. *(If a future fixture truly
needs more than one tool call inside one provider round, extend
`PlannedResponderResponse` to carry multiple `ModelToolCall`s grouped by
round instead of emitting one response per call — out of scope here.)*

- [ ] **Step 3: Build the per-question driver.**

For each `Question`, build the scripted responses in (already-validated)
round order, then a final text reply, and drive the existing loop entry
point:

```text
fn run_question(question) -> Outcome:
    // rounds already validated as unique, 1-based, contiguous, <= 2
    sort question.tool_calls by round
    let mut responses = vec![]
    for call in sorted tool_calls:
        responses.push(PlannedResponderResponse::tool_call(
            "scripted tool round",
            format!("{}-{}", question.id, call.round),   // stable call_id
            call.tool,
            call.arguments.clone(),
        ))
    responses.push(PlannedResponderResponse::text(question.reply))

    let client = SequencedResponderClient::new(responses)
    let base_dir = temp_dir().join(format!("qsf-battery-{}-{}", question.id, Uuid::new_v4()))
    let mut context = test_context(&base_dir, "multi-turn-text-loop")
    let input = Cursor::new(format!("{}\n:quit\n", question.prompt))
    run_with_io_and_components(&mut context, input, &mut Vec::new(), &client,
                               &TestMemorySource, test_config_with_warm_threshold(10, 10))?
    let events = parse_event_records(&read(context.run_dir().join("events.jsonl")))
    return Outcome {
        calls: client.calls(),
        events,
        traces: parse_trace_records(&read(context.run_dir().join("traces.jsonl"))),
        // Read the reply from the single TurnCompleted event payload — NOT
        // from ExperimentOutcome or formatted console output, which carry
        // loop framing. Confirm the exact field name against the event
        // shape during implementation (e.g. turn.assistant_response).
        reply: <assistant response text from the TurnCompleted event>,
        base_dir,
    }
```

Use one fresh `test_context` per question so each question's
`events.jsonl` / `traces.jsonl` are isolated. Clean up `base_dir` after
asserting, matching the existing tests' `fs::remove_dir_all` pattern.

**Reply extraction (P7-003).** `run_with_io_and_components` returns an
`ExperimentOutcome`, not the last responder text, and the captured
console output includes loop framing/formatting. The deterministic source
for `Outcome.reply` is the single `TurnCompleted` event's assistant
response payload; read it from the parsed events (confirm the exact field
during implementation) rather than from `ExperimentOutcome` or console
output.

### Task 7.3: Battery assertions and run

**Files:**
- Modify: same module as Task 7.2.

- [ ] **Step 1: Write the battery test.**

Iterate the loaded `Battery` and, per question, assert:

- **Provider-call shape.** `outcome.calls.len() == tool_calls.len() + 1`
  for tool-emitting questions, and `== 1` for the off-topic control.
  Each tool-emitting round's call advertises the four tools:
  `calls[round - 1].tools == responder_tool_names()`. For the *final*
  call:
  - the two-round `framing_search_then_read` question's final answer call
    advertises an **empty** `tools` list (it follows the second tool
    round, so `advertise_tools` is false);
  - the one-tool `what_are_you` and no-tool `off_topic_control`
    final/only calls still advertise `responder_tool_names()` (because
    `tool_rounds < 2`) — for these, assert that the final *response*
    carried no `tool_calls` rather than asserting the request advertised
    none.
- **Voicing block on every call (OQ #8).** Every captured call's first
  message is the system prompt and contains both `"search_project_docs"`
  and `"kind and maturity"` — including the final answer call of every
  question, whether or not that call still advertises tools.
- **Executed tool calls via traces.** For each expected
  `search_project_docs` / `read_project_doc` call there is a matching
  `project_doc_*` trace with `details.refused == false`; the sanitized
  `query` / `path` in the trace matches the fixture `arguments`
  (substring match on `query` / equality or substring on `path`). For
  the `framing_search_then_read` question, the search and read traces
  share one `turn_index`.
- **Events.** Exactly one `TurnCompleted` per question; a `ToolCompleted`
  event exists for each emitted tool name.
- **Off-topic control.** Zero `project_doc_*` traces, no `ToolCompleted`
  for either project-doc tool, and exactly one provider call — while the
  voicing block is still present (channel enabled for the turn) and that
  single call still advertises the four tools (no tool round occurred, so
  `advertise_tools` stays true).
- **Reply text.** `outcome.reply` contains every
  `expected_reply_contains` string (case-insensitive) and none of the
  `expected_reply_must_not_contain` strings. (As noted in the phase
  intro, these guard the canned fixtures, not model behavior.)

Include the failing `question.id` in every assertion message so a battery
failure pinpoints the offending question.

- [ ] **Step 2: Run the battery.**

```bash
cargo test -p qsf_app multi_turn_text_loop
```

Run the focused battery test name as well if you gave it one, e.g.
`cargo test -p qsf_app self_question_battery`. Expected: PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/qsf_app/tests/fixtures/self_question_battery.json \
        crates/qsf_app/src/experiments/multi_turn_text_loop/tests.rs
git commit -m "test(project_docs): self-question battery over the live responder loop"
```

**Do not merge this commit in isolation without satisfying the repo diary
rule (P7-004).** The repo instructions require implementation changes to
be documented in `docs/EngineeringDiary.md`. This phase's grouped diary
coverage lives in Phase 9, so either (a) keep the Phase 7 commit unmerged
until the Phase 9 diary commit is on the same branch, or (b) add a short
standalone Phase 7 diary entry to this commit/branch (read the
*Instructions how to use* at the top of `docs/EngineeringDiary.md`
first). See *Diary discipline for this phase* below; reconcile, don't
duplicate, in Phase 9.

### Phase 7 verification

Per `Agents.md`, build first, then focused tests, then the lint/format
gates:

```bash
cargo build
cargo test -p qsf_app multi_turn_text_loop
cargo test -p qsf_app project_doc
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expect all clean. No registry, library, dispatch-layer, or loop change is
expected — Phase 7 only adds a test and a fixture. If the battery cannot
be expressed without a production-code change, **surface it before
proceeding** rather than quietly editing runtime code from a test phase.

**Acceptance criteria for Phase 7:**

- A data-driven battery loads `self_question_battery.json`, validates each
  question's `round` values (unique, 1-based, contiguous, ≤ 2), and drives
  each question through the **real** bounded responder loop via
  `run_with_io_and_components` and a `SequencedResponderClient`, not by
  calling `dispatch_model_tool_calls` directly.
- A search-then-read question exercises the two-round path: search in
  round 1, read in round 2, a final no-tools answer (the only question
  whose final call advertises an empty `tools` list), with both
  `project_doc_*` success traces sharing one `turn_index`.
- Every provider call's system prompt carries the voicing block,
  including each question's final answer call — regardless of whether that
  call still advertises tools.
- The off-topic control makes zero project-doc calls (no `project_doc_*`
  traces, no project-doc `ToolCompleted`) and completes exactly one turn
  while still carrying the voicing block; its single call advertises the
  four tools and its response carries no tool calls.
- Each emitted tool call is observable in the traces with
  `refused == false` and arguments matching the fixture; each question
  completes exactly one `TurnCompleted`, and the reply is read from that
  event payload.
- Every `read_project_doc` fixture path resolves to an allowlisted,
  present document (or the question asserts a refusal trace by design).
- The battery runs under `cargo test -p qsf_app` as a CI regression gate;
  `cargo build`, the focused tests above,
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` are clean.

**Diary discipline for this phase.** As with Phases 1-6, the application
work of Phases 1-8 is grouped under the single Phase 9 diary entry, so
Phase 7 is not considered complete or mergeable until that entry lands.
If Phase 7 is merged in isolation ahead of the grouped feature, a short
standalone Phase 7 diary entry must accompany that merge (read the
*Instructions how to use* at the top of `docs/EngineeringDiary.md`
first); reconcile, don't duplicate, in Phase 9.

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
discipline noted in those phases — including a possible standalone Phase 7
entry per Task 7.3), reconcile rather than duplicate them here.

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