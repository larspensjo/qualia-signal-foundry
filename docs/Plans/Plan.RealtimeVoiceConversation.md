# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Active implementation plan. Phase 0 (decisions & contracts), Phase 1 (extract
`qsf_session`), Phase 2 (thin media plane — live browser voice), and Phase 3
(authoritative sideband + memory injection) are **complete and human-tested**;
**Phase 4 (model-invoked read-only perception tools) is the active phase**, expanded
below into an actionable build and revised 2026-06-10 after external review
(`Review.RealtimeVoiceConversation.phase4.Plan.codex.json`) — all open Phase-4
decisions are now resolved (D12 stays verify-at-implementation). Phase 5 remains
intentionally high-level until reached.

> Companion to the design note
> [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md), which
> is authoritative for the rationale, trust boundary, and contracts. This document
> is the **phased build plan**: it sequences the work into independently testable
> slices and marks where external human verification is required.
>
> **Intentionally high-level (for future phases).** Each unstarted phase is a
> self-contained slice, not a task-by-task script. Expand a phase into detailed steps
> (file paths, test code, commits) immediately before executing it, surfacing that
> phase's open questions first (per `Agents.md`).

## Goal

Enable a live, browser-based, full-duplex spoken conversation where `gpt-realtime`
owns the voice (media plane) and QSF owns the mind (memory, context, tools,
observability) — built incrementally so each phase ends in a verifiable state.

This is the intended primary operating mode of the project, not merely another
experiment path. The experiment documents named below are validation scaffolds for
building and measuring slices of the mode; the end state is a normal way to run
QSF.

## Phasing Principles

- Each phase builds, passes `cargo test`, and is green under
  `cargo clippy --all-targets -- -D warnings` then `cargo fmt`. UI changes also pass
  `npm run check` then `npm run fmt` in the relevant `ui/` directory.
- Reducers stay pure and unit-tested; side effects live at the edge and feed back as
  actions (`input -> action -> reducer -> state -> render`).
- A phase that adds a flag/threshold must default to exercising the new path.
- "Human testing" marks steps that need external manual verification — automated
  tests cannot cover the live spoken experience.
- The launcher should eventually expose realtime voice conversation as a first-class
  mode. `app -Experiment ...` remains the current harness for experiments and tests,
  but it is not the intended final operator surface for live conversation.

## Phase Overview

| Phase | Slice | Code? | Human test? |
|-------|-------|-------|-------------|
| 0 | Decisions & contracts — complete (accepted 2026-06-09) | No | No |
| 1 | Extract `qsf_session` crate (pure refactor) — complete (`45ed9cd`) | Yes | No |
| 2 | Thin media plane — live browser voice — complete, human-tested 2026-06-09 | Yes | ✅ |
| 3 | Authoritative sideband + memory injection — complete, human-tested 2026-06-10 | Yes | ✅ |
| 4 | Model-invoked read-only perception tools — **active** | Yes | **Yes** |
| 5 | Live memory extraction + presence refinement | Yes | **Yes** |

---

## Phases 0–3 — complete (compacted)

Full per-step detail lives in git history, `docs/DecisionLog.md`, and
`EngineeringDiary.md`. What follows is the carry-forward that still constrains
Phases 4–5.

### Phase 0 — Decisions & contracts (accepted 2026-06-09)

- `qsf_realtime_server` owns live realtime side effects; `qsf_browser_server` stays a
  read-only inspection server depending only on `qsf_memory`.
- The browser owns the WebRTC media plane; the QSF server owns the server-side SDP
  rendezvous (it holds `OPENAI_API_KEY`, performs the SDP exchange, and captures the
  `{ qsf_session_id ↔ provider call_id }` binding). Media flows directly
  browser↔OpenAI; only signaling is proxied.
- Browser-relayed provider events are untrusted, diagnostic-only, excluded from sleep
  and continuity. The server sideband (Phase 3) is the authoritative trusted source.
- Session defaults: `gpt-realtime-2`, voice `marin`, `output_modalities = ["audio"]`,
  provider `server_vad`. `reasoning_effort` is QSF metadata only — the provider
  rejects it (`unknown_parameter`). The Phase-0 "automatic response creation" default
  was **reversed in Phase 3** to `create_response = false` (sideband owns
  `response.create`).
- Mapping contract: the exchange boundary is a paired user-utterance →
  assistant-response turn keyed by provider `item_id` / `response_id`;
  `ExchangeCompleted` carries an explicit `exchange_index`. The required reducer
  matrix (out-of-order, duplicate, interruption cases) is implemented and green.
- The `call_id` binding is active-call scoped, invalidated on stop/error/expiry.
- Realtime voice conversation is the long-term primary QSF operating mode.

### Phase 1 — Extract `qsf_session` (pure refactor, `45ed9cd`)

- **Lean dependency graph is the model for every extraction since** (and for Phase 4's
  tool-registry move): `qsf_session` depends only on `anyhow`, `qsf_memory`,
  `qsf_context` (since Phase 3), `serde`, `serde_json`, `tempfile`, `time`, `uuid` —
  no `tokio`, `reqwest`, `engine_logging`, or provider crates.
- `qsf_session` owns the pure reducer/state/event contracts (`LiveSessionEvent`,
  `LiveSessionState`, `Exchange`, `Turn`, `ProviderEventRecord`), persistence
  (`persist_session_state` / `load_session_state`), continuity manifest, sleep
  records, `ContentHash`, and the `ToolCategory` / `ToolSideEffectLevel` enums
  (`crates/qsf_session/src/tools.rs`) — **Phase 4 builds on these enums.**
- **Persistence constraint:** `persist_session_state` skips
  `LiveSessionState.completed_exchanges` (in-memory only, guarded by test). Durable
  state requires promotion to a `Turn` (`Turn::try_from(&Exchange)`). The persisted
  `session-state.json` / `continuity-manifest.json` schemas are guarded by a golden
  test — **any new persisted field (e.g. Phase-4 tool records) needs serde defaults
  and golden-test updates to stay compatible.**
- Reducer: single `active_exchange` / `active_response`; overlap policy **B**
  (finalize-prior); stale events for finalized exchanges are no-ops.

### Phase 2 — Thin media plane: live browser voice (human-tested 2026-06-09)

- Introduced `crates/qsf_realtime_server` (axum/tokio, thin `main.rs`/`lib.rs`/`cli.rs`,
  `state.rs` with `AppState`) and the browser WebRTC client in
  `crates/qsf_realtime_server/ui/`. Routes: `POST /api/realtime/session`,
  `POST /api/realtime/sdp` (server-side rendezvous, captures `call_id` from the
  `Location` header), `WS /api/realtime/events` (relay, now diagnostic-only),
  `POST /api/realtime/stop`.
- **`qsf_realtime_server` must not depend on `qsf_app`** — it uses the lean domain
  crates only. This boundary held through Phase 3 and must hold in Phase 4.
- Provider `event_id` dedupe/order is the server translator's job, not the reducer's.
- `ProviderEventRecord` carries `call_id`, `event_id`, `item_id`, `previous_item_id`,
  `response_id` — the same identity fields Phase 4 reuses to link tool calls.
- Test pattern to mirror: provider endpoints are injected via `AppState` base URLs so
  tests run against mocked HTTP/WS servers, with key-absence assertions throughout.

### Phase 3 — Authoritative sideband + memory injection (complete, human-tested 2026-06-10)

A live browser session completed a full memory-grounded voice turn: sideband attach,
input transcription, per-turn memory injection, server-issued `response.create`,
audible reply. **Carry-forward facts and constraints for Phase 4:**

- **Crate layering (D1, landed):**
  `qsf_memory ← qsf_context ← qsf_session ← {qsf_app, qsf_realtime_server}`, with
  `qsf_realtime_protocol` an independent lean leaf. Retrieval scoring
  (`retrieve_memories` et al.) lives in `qsf_memory`; the context-assembly domain
  (`ContextFragment`, `ContextBudget`, `ContextAssembly`, `assemble_context`, the
  `From<&RetrievedMemory>` adapter) lives in `qsf_context`; realtime JSON
  builders/parser live in `qsf_realtime_protocol` (`crates/qsf_realtime_protocol/src/lib.rs`).
  `qsf_app` re-exports moved items through facades. **The same extract-to-lean-crate +
  facade pattern is how Phase 4 makes the tool registry reachable from the server.**
- **Sideband is authoritative (D2/D5/D6, landed in
  `crates/qsf_realtime_server/src/realtime/sideband.rs`):** a long-lived task attaches
  to `wss://api.openai.com/v1/realtime?call_id=...` with bearer `OPENAI_API_KEY`
  (D3 **verified** against the provider's realtime server-controls docs, recorded in
  `DecisionLog.md` 2026-06-10). Provider events flow through the pure translator into
  `apply_live_session_event`; exchanges are stamped `Trusted`; the browser relay is
  UI-only diagnostics.
- **Response timing is server-owned (D5):** default `create_response = false`; on each
  committed user turn (`conversation.item.input_audio_transcription.completed` — input
  transcription had to be explicitly enabled for this) the sideband retrieves memory,
  injects a small packet (`conversation.item.create` + `session.update`, built by the
  pure builder in `realtime/injection.rs`), then sends `response.create`. Live timing
  was confirmed acceptable; latency parity is **not** an open issue. **Phase 4's tool
  loop extends this same server-owned `response.create` choreography.**
- **Trusted promotion preconditions (D2):** `Turn::try_from(&Exchange)` hard-requires
  `completed_at`, `output`, `context_assembly`, and `ExchangeModelUse`.
  `context_assembly` comes from the injection builder; model use from `response.done`
  usage. Only **complete** trusted exchanges promote into the shared continuity root
  (`promote_completed_trusted_exchanges`); incomplete/failed/degraded never do.
  **Phase-4 tool activity must not break these preconditions or finalize an exchange
  mid-tool-loop.**
- **Gap semantics (D6):** an unrecoverable/lossy sideband disconnect marks the session
  *degraded* until reconnect + `session.updated` acknowledgement; gap-window exchanges
  are permanently non-promotable. Covered by tests in `sideband.rs`. **A disconnect
  during a Phase-4 tool loop falls under the same rule.**
- The memory-store resolver (`realtime/memory_store.rs`:
  `load_session_memory_store` / `retrieve_session_memories`) handles
  existing/absent/malformed/empty stores without panicking — **Phase 4's memory-search
  tool should reuse it, not re-resolve stores.**
- Key-absence assertions extend across the sideband; ageing/consolidation was
  deliberately deferred to Phase 5. Sideband health is surfaced to the browser UI.

---

## Phase 4 — Tool plane: model-invoked read-only perception tools *(active)*

**Outcome.** During a live voice conversation the model can invoke a small allow-list
of **read-only** perception tools (search memory, retrieve associations, inspect
session state). On a provider `function_call`, the sideband records the request,
makes an explicit permission decision, executes via the tool registry, records the
execution result (not just the intent), returns a `function_call_output` item, and
re-issues `response.create` so the model speaks an answer grounded in the tool
result. Non-allow-listed or over-privileged calls are **never executed** and are
recorded as denied. No credential leaves the server; the trusted-promotion and
degraded-gap rules from Phase 3 continue to hold.

### Decisions — resolved 2026-06-10 (external review + owner confirmation)

The blocking questions (D8 scope, D9 persistence) were confirmed by the owner after
the external review (`Review.RealtimeVoiceConversation.phase4.Plan.codex.json`); the
review's technical findings are folded into D7/D10 and the steps below. D12 remains a
verify-at-implementation item. Numbering continues from Phase 3 (D1–D6).

- **D7 — resolved: extract a *generic* registry core into a lean `qsf_tools` crate.**
  A move-as-is would not compile: today's `ToolRegistry`
  (`crates/qsf_app/src/tools/tool_registry.rs`) hardcodes the four concrete `qsf_app`
  tools and imports `qsf_app::models::ModelToolDefinition`, `ProjectDocService`, and
  `SessionState`; even the `ToolContext` trait exposes app-typed accessors.
  `qsf_tools` therefore receives a **generic core** instead: the `Tool` trait,
  `ToolRequest`/`ToolPermission`, `ToolResult` (its context dependency is already
  lean `qsf_context` via facade), `ToolMetadata`, a new parameters-bearing
  **`ToolDefinition`** (name, description, JSON-schema parameters as
  `serde_json::Value`) replacing the trait's `ModelToolDefinition` hook, and a
  **dynamic registry** (registered boxed tools) replacing the hardcoded dispatch.
  The `qsf_tools` `ToolContext` must not reference app types; app-specific context
  access (session state, project docs) stays in `qsf_app` behind adapter/downcast
  helpers. Concrete `qsf_app` tools do **not** move; `qsf_app::tools` re-exports the
  moved generics (and converts `ToolDefinition` → `ModelToolDefinition`) so existing
  call sites (`multi_turn_text_loop` tool/turn runtimes,
  `tool_as_perception_calculator`, the concrete tools) compile unchanged. Crate
  dependencies: `anyhow`, `serde`, `serde_json`, `qsf_session` (category enums),
  `qsf_context` (context fragments); nothing heavier.
- **D8 — resolved: three-tool perception scope.** Phase 4 exposes exactly
  `search_memory(query)`, `get_associations(memory_id)`, and
  `inspect_session_state()` — all `ToolCategory::ReadOnly` /
  `ToolSideEffectLevel::ReadOnly`, implemented in `qsf_realtime_server` where their
  data lives:
  - `search_memory(query)` — association-weighted retrieval over the session's store
    via the existing `retrieve_session_memories`
    (`crates/qsf_realtime_server/src/realtime/memory_store.rs`), returning a small
    capped list of memory summaries (reuse the `ContextBudget` discipline from the
    injection builder — a tool result is a context payload too).
  - `get_associations(memory_id)` — a **capped, deterministic neighborhood query**
    over `qsf_memory` association records: bidirectional by default, sorted by
    descending weight, explicit not-found vs. empty-neighborhood results, compact
    summaries for dangling endpoints.
  - `inspect_session_state()` — a compact summary of the live session (exchange
    count, active exchange status, trust/degraded state) derived from
    `LiveSessionState` — no internals dump.
  No existing `qsf_app` tool is exposed live this phase: their data services
  (`ProjectDocService`, durable-session access) sit behind the no-`qsf_app`
  boundary. **Long-term intent (owner, 2026-06-10): the live model eventually gets
  the full tool set; that lands as its own later phase** (see "Deferred beyond
  Phase 5"), and the D7 generic registry is what makes it cheap.
- **D9 — resolved: tool execution records persist onto durable `Turn`s.** Keep
  `ToolRequested` (`ToolRequestRecord` in `crates/qsf_session/src/exchange.rs`) as
  the request record; `auto_executed` is **not** execution evidence. Add a
  `ToolExecutionRecord` to `qsf_session::exchange` — `exchange_index`, provider
  `call_id` (links to the matching `ToolRequestRecord`), `tool_name`, permission
  decision (allowed / denied-with-reason), status (completed / failed / aborted),
  budget-capped result summary, error, requested/completed timing, per-response
  model usage (D13), and the returning provider `event_id` — plus a
  `LiveSessionEvent::ToolResolved(ToolExecutionRecord)` variant reduced purely onto
  the active exchange. `Turn` gains the records behind `#[serde(default)]` with the
  schema golden tests (`crates/qsf_session/tests/session_state_schema.rs`) updated,
  so tool activity — including denials — is inspectable post-session, visible to the
  read-only browser server, and available to Phase-5 extraction/ageing.
- **D10 — confirmed, with revised enforcement point.** A `response.done` whose
  output is a function call must **not** finalize the exchange — it stays active
  across the tool loop and completes on the eventual audio response. Finalization
  today is *emitted by the sideband* (`response.done` → `OutputProduced`,
  `ModelRoleCompleted`, `ExchangeCompleted` in `realtime/sideband.rs`) and
  `ProviderEventKind` has no tool-call variant, so the rule is enforced where the
  completion events originate: the protocol layer classifies `response.done` output
  (function-call vs. spoken message), a new `ProviderEventKind` / translator case
  represents tool-call completion, and the sideband suppresses the finalization
  events for function-call-only completions. The reducer handles the new events
  purely (out-of-order matrix-style tests). Cap the loop (max 3 sequential tool
  calls per turn) so a pathological model cannot spin; on cap, return a denial-style
  output and force a spoken response.
- **D11 — confirmed: denied calls get structured verbal recovery.** A
  non-allow-listed or over-privileged call stays unexecuted and is recorded as
  denied (the phase gate), but still receives a `function_call_output` containing a
  brief structured denial followed by `response.create`, so the conversation
  recovers verbally instead of leaving the provider waiting on a dangling call.
- **D12 — verify at implementation time (unchanged, same caution as D3).** Working
  assumption from the realtime API: tools are declared in `session.update` (`tools`
  array with name/description/JSON-schema parameters + `tool_choice`), arguments
  stream via `response.function_call_arguments.delta/.done`, and the completed call
  appears as a `function_call` output item carrying the provider `call_id`; results
  return as a `conversation.item.create` with a `function_call_output` item
  referencing that `call_id`. **Verify against the live API before coding step 3;
  record drift in `DecisionLog.md`.**
- **D13 — resolved: model-use accounting aggregates across the tool loop.** A
  tool-loop turn produces multiple `response.done` events (function-call
  response(s) + the final spoken response), while the sideband runtime tracks a
  single `current_request_hash`/`current_message_count` slot reset after each
  `response.done`. Rule: the exchange's `ExchangeModelUse` **aggregates token counts
  and total latency across all `response.done` events of the turn**; each
  `ToolExecutionRecord` carries its own per-response usage/timing; the request hash
  and message count reflect the final spoken response's request sequence.
  Token/latency accounting across a tool call is covered by tests (step 5).

### Architecture constraints (must hold)

- `qsf_realtime_server` still must not depend on `qsf_app`; the tool registry arrives
  via the lean `qsf_tools` crate (D7). Keep `main.rs` / `lib.rs` / `mod.rs` thin.
- The reducer stays pure: `ToolRequested` / `ToolResolved` are reduced by
  `apply_live_session_event` with no I/O; tool **execution** is the effectful edge
  (sideband), feeding results back as events — `input -> action -> reducer -> state ->
  render` is unchanged.
- Permission decision logic (allow-list + category/side-effect caps via the existing
  `ToolPermission::allows` machinery) must be a pure, unit-testable function separate
  from async execution — same discipline as the Phase-3 injection builder.
- The sideband must **not** hold the session lock during tool execution
  (`handle_provider_event` takes the session mutex at entry today): snapshot the
  needed state under the lock, drop the guard for the permission decision and tool
  execution, then reacquire only to reduce `ToolResolved` and update runtime state.
  Stop/disconnect races against the unlocked execution window are covered by tests.
- Tool result payloads are budget-capped like injection packets; never dump a store.
- Trusted-promotion preconditions (D2) and degraded-gap semantics (D6) are unchanged:
  a disconnect mid-tool-loop aborts the loop, records the execution as aborted, and
  the gap-window exchange remains non-promotable.
- `OPENAI_API_KEY` stays server-side; extend the key-absence assertions to every tool
  result, `function_call_output` payload, and log line.

### Incremental, independently reviewable steps (each ends green; commit per step)

1. **Pure records first: `ToolExecutionRecord`, tool-call events, `Turn` persistence
   (D9, D10, D13).** Add the record (including per-response usage/timing fields) and
   the `LiveSessionEvent::ToolResolved` variant to `qsf_session`; add the
   tool-call-completion representation (new `ProviderEventKind` variant) so a
   function-call completion is expressible without finalization events; reduce
   `ToolResolved` onto the active exchange, linked to its `ToolRequestRecord` by
   provider `call_id`. Extend `Turn` with the persisted records behind serde
   defaults and update the schema golden tests
   (`crates/qsf_session/tests/session_state_schema.rs`). The
   exchange-stays-active-across-the-loop behavior is *enforced* at the sideband
   (step 5), where finalization events originate. *Verify (unit):* request→resolve
   linking; denied/failed/aborted statuses representable; duplicate/late
   `ToolResolved` for a finalized exchange is a no-op; legacy artifacts without tool
   records still load; schema golden tests green. *Green:* full `cargo test`.
2. **Extract the generic registry core into lean `qsf_tools` (D7).** Move the `Tool`
   trait, `ToolRequest`/`ToolPermission`, `ToolResult`, `ToolMetadata`; add the
   parameters-bearing `ToolDefinition`; replace the hardcoded four-tool dispatch
   with a dynamic registry (registered boxed tools); keep the `qsf_tools`
   `ToolContext` free of app types (app-typed context access stays in `qsf_app` as
   adapters). `qsf_app::tools` becomes a re-exporting facade with its concrete
   tools and a `ToolDefinition` → `ModelToolDefinition` conversion, so all existing
   call sites compile unchanged; move the generic registry unit tests with the
   code. No behavior change. *Green:* full `cargo test` (existing tool-loop
   experiment tests prove parity).
3. **Realtime protocol additions in `qsf_realtime_protocol` (D12, D10).** Pure
   builders and parsers: tool declarations in the `session.update` builder from a
   **protocol-native tool-definition DTO** (name, description, JSON-schema
   parameters — the leaf crate stays independent; the server maps
   `qsf_tools::ToolDefinition` into it), the `function_call_output`
   `conversation.item.create` builder, extractors for the function-call
   events/arguments and the provider tool `call_id` (extending
   `parse_realtime_server_event`'s extractor family), and the **`response.done`
   output classifier** (function-call vs. spoken output) that D10's sideband
   enforcement consumes. *Verify (unit):* fixture-based round-trips for each
   builder/parser, including malformed arguments JSON and mixed-output
   `response.done` payloads. *Green.*
4. **Implement the perception tools + allow-list wiring in the server (D8).** Three
   `Tool` impls in `crates/qsf_realtime_server/src/realtime/` (e.g. `tools.rs`):
   `search_memory` (reusing `retrieve_session_memories` + a `ContextBudget` cap),
   `get_associations` (per the D8 spec: capped, deterministic, bidirectional,
   weight-descending, explicit not-found vs. empty-neighborhood, compact summaries
   for dangling endpoints), `inspect_session_state`. Build the per-session
   allow-listed registry in `AppState`/`session_config`, and a **pure**
   permission-decision function (allow-list + ReadOnly caps →
   allowed/denied-with-reason). Defaults exercise the new path: the default session
   declares the tools in `session.update`. *Verify (unit):* each tool against
   existing/empty/malformed stores (mirror the Phase-3 resolver matrix); the
   `get_associations` case matrix; caps enforced; the permission matrix including a
   write-capable or unknown tool → denied. *Green.*
5. **Sideband tool loop (D10, D11, D13, D6).** In `handle_provider_event`
   (`realtime/sideband.rs`): classify `response.done` via the step-3 classifier; for
   a function-call completion, suppress `OutputProduced`/`ModelRoleCompleted`/
   `ExchangeCompleted`, record `ToolRequested`, then — **outside the session lock**
   (snapshot under the lock, drop the guard, reacquire to reduce) — run the
   permission decision and execute via the registry (or deny), record
   `ToolResolved`, send the `function_call_output` item, then `response.create`;
   aggregate model use across the loop per D13; enforce the per-turn loop cap;
   abort + record on disconnect/stop. *Verify (mocked WS, mirroring the Phase-3
   harness):* full chain function-call → decision → execution →
   `function_call_output` → `response.create`; a non-allow-listed call is **never
   executed** and is recorded as denied while the conversation recovers (D11); the
   exchange stays active across the loop and the eventual trusted promotion carries
   `context_assembly` + aggregated `ExchangeModelUse` (D13 token/latency accounting
   asserted); disconnect/stop racing the unlocked execution window → aborted record
   + degraded session, no promotion; key absent from all tool payloads/logs.
   *Green.*
6. **Gates + docs.** `cargo clippy --all-targets -- -D warnings`, `cargo fmt`; UI gate
   (`npm run check`, `npm test`, `npm run fmt`) only if `ui/` is touched (e.g. if tool
   activity is surfaced alongside sideband health — optional, not required this
   phase). Docs per the list below, using the exact repo paths.

### Acceptance criteria

- `cargo build`, full `cargo test`, clippy clean, `cargo fmt` applied; UI gate green
  if `ui/` changed.
- `qsf_realtime_server` depends on `qsf_tools` (new) plus the existing lean crates —
  still **no `qsf_app` dependency**; `qsf_app` behavior is unchanged through its
  re-export facade (existing tool-loop tests green untouched).
- Function-call → permission decision → registry execution → `function_call_output` →
  `response.create` verified end-to-end against a mocked WS provider.
- A non-allow-listed (or over-privileged) tool call is proven to stay **unexecuted
  and recorded as denied**; `auto_executed` is not used as execution evidence —
  execution facts live in `ToolExecutionRecord` (decision, status, result summary,
  error, timing, returning event), linked by `call_id`.
- A function-call response does not finalize the exchange; trusted promotion
  preconditions (D2) and degraded-gap semantics (D6) hold across tool loops, verified
  by tests; the per-turn tool-loop cap is enforced; promoted `ExchangeModelUse`
  aggregates across the loop (D13).
- `Turn` persists tool execution records behind serde defaults: legacy artifacts
  still load, schema golden tests are green, and persisted result summaries are
  budget-capped.
- Defaults exercise the new path: the default realtime session declares the read-only
  tools, and a model-issued call executes without extra configuration.
- `OPENAI_API_KEY` proven absent from tool results, `function_call_output` payloads,
  and logs.
- D12 (provider function-call event shapes) verified against the live API, with any
  drift recorded in `DecisionLog.md` before defaults change.

### Verification guidance (fits a live-service + tool-integration slice)

- *Automated (Rust):* reducer matrix for `ToolRequested`/`ToolResolved` linking;
  the `response.done` output-classifier fixtures and the sideband-enforced
  exchange-boundary rule (D10); permission-decision matrix (allow-listed / unknown /
  over-privileged); per-tool store matrix (existing/empty/malformed) plus the
  `get_associations` case matrix; protocol builder/parser fixtures (D12 shapes); the
  mocked-WS sideband chain including denial, loop cap, disconnect-mid-loop, and
  stop/disconnect racing the unlocked execution window; promotion preconditions and
  aggregated model-use accounting (D13) across a tool loop; schema golden tests for
  the persisted `Turn` tool records (D9); key-absence assertions extended to tool
  payloads.
- *Automated (TS):* only if `ui/` changes — `npm run check` + `npm test` (Vitest).
- **Human testing (required):** in a live browser session, ask something that
  requires memory search (e.g. reference a fact known to be in the store but not in
  the injected packet); confirm the model calls the tool and uses the result in its
  spoken reply; ask for something absent and confirm a graceful spoken outcome;
  inspect artifacts to confirm request + execution records and that tool results are
  small; note the added per-tool-call latency (feeds the Phase-5 presence work and
  the future Live Activation Dashboard "thinking" cue).
- *Provider reality check:* confirm the `session.update` tools declaration, the
  function-call event stream, and the `function_call_output` shape against the
  current API (D12); record drift in `DecisionLog.md`.

### Docs (per `ProjectWorkflow.md`)

- `Experiment.LiveToolPerception` as the validation record.
- Update `docs/Architecture/Architecture.ToolSystem.md` (the `qsf_tools` extraction,
  the realtime perception tools, the live execution/permission path) and
  `docs/Architecture/Architecture.StateAndObservability.md` (request vs. execution
  records; persisted `Turn` tool records).
- Refresh `docs/Architecture/Architecture.RealtimeSessionServer.md` (tool loop in
  the sideband; update `Last reviewed:`).
- `DecisionLog.md`: the resolved decisions (three-tool scope D8, `Turn` persistence
  D9, model-use aggregation D13) were recorded 2026-06-10 at plan-revision time;
  remaining entries land with implementation — the `qsf_tools` crate boundary (D7),
  the function-call exchange-boundary rule and loop cap (D10) with denial feedback
  (D11), and the verified provider function-call shapes (D12).
- One `EngineeringDiary.md` entry (follow the diary's "How to use" header).
- README / launcher notes as the realtime mode grows.

---

## Phase 5 — Live memory extraction + presence / interruption refinement

**Scope.** Lightweight extraction over completed **trusted** turns (reuse the
sleep/memory proposers) feeding the existing review/consolidation path. Refine
interruption representation and end-to-end / per-stage latency reporting for presence
research (including the latency-measurement gap carried forward from Phase 2 and the
ageing/consolidation work deferred from Phase 3).

**Verify (automated).** Extraction tests over trusted turns; latency measurements
recorded.

**Human testing (required).** Presence evaluation against the
`Concept.RealtimePresence` open questions; record latency observations.

**Docs.** Experiment doc + report; refresh `ResearchQuestions.Audio.md` (injection
relevance, ASR-vs-model transcript divergence); update `Concept.RealtimeAudio.md` /
cross-link `Concept.RealtimePresence`; diary entry.

### Deferred beyond Phase 5

- **Full tool set for the live model (owner intent, 2026-06-10, recorded under
  D8).** Expose the broader `qsf_app` tool set (project docs, recall-turn,
  calculator, and successors) to the live realtime model as its own phase. Requires
  moving the tools' data services (`ProjectDocService`, durable-session access)
  past the no-`qsf_app` boundary; the D7 generic `qsf_tools` registry exists to
  make that phase an additive change.

---

## Launcher / Operator Surface

Today `scripts/qsf.ps1` mainly launches `qsf_app` experiments, the memory browser,
the UI, and the workbench, plus the realtime preview path (start
`qsf_realtime_server` with the sideband, start/open the browser UI, apply non-secret
QSF defaults, and verify required secrets without printing them).

As realtime voice conversation becomes the primary mode, the launcher should grow a
first-class operator command rather than treating it as only `app -Experiment <name>`.
The exact first-class command name is decided when the server and UI entry point
stabilize; the intended shape is:

```text
qsf.ps1 <realtime-conversation-mode>
  -> start qsf_realtime_server (with sideband)
  -> start/open the browser UI
  -> apply non-secret QSF defaults through the launcher
  -> verify required secrets without printing them
```

The experiment runner should remain available for regression tests, fixture-backed
validation, and phase reports.

---

## Cross-Cutting Verification

- **Lint gates every phase:** Rust → `cargo clippy --all-targets -- -D warnings` then
  `cargo fmt`; UI → `npm run check` then `npm run fmt`.
- **Phase 1 gate (met):** schema golden/fixture parity (legacy + current
  `SessionState`) plus normalized-artifact parity (volatile fields scrubbed).
- **Phase 2 gate (met):** the reducer overlap / out-of-order test matrix is green.
- **Phase 3 gate (met):** trusted sideband exchanges land in the shared continuity
  root and are sleep-eligible while diagnostic exchanges are not; the
  injection-payload builder matrix is green; no credential leaks through the sideband;
  confirmed live 2026-06-10.
- **Phase 4 gate:** the function-call → decision → execution → output → response chain
  is green against a mocked provider; a non-allow-listed tool is proven unexecuted and
  recorded as denied; trusted promotion and degraded semantics hold across tool loops;
  no credential leaks through tool payloads.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience (done), cross-session continuity (done), model-invoked tool use, and
  presence.

## Remaining Checks Before Each Phase

- **Phases 0–3 (complete):** all prior-phase open questions are resolved and recorded
  in `DecisionLog.md` / `EngineeringDiary.md`; the D3 attach shape and D5 manual-response
  timing were both confirmed live 2026-06-10. Keep overlap policy B (D4) unless the
  authoritative sideband reveals real overlap — still a watch item, not a change.
- **Phase 4 (active):** D7–D11 and D13 are resolved/confirmed 2026-06-10 (external
  review `Review.RealtimeVoiceConversation.phase4.Plan.codex.json` + owner): generic
  `qsf_tools` core, three-tool scope, `Turn` persistence, sideband-enforced exchange
  boundary with loop cap, structured denial feedback, aggregated model-use
  accounting — recorded in `DecisionLog.md` (D8/D9/D13) at plan-revision time. Only
  D12 (provider function-call shapes) remains: verify against the live API at step 3
  and record drift.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream — Phase 4 adds the first function-call events through the
  authoritative sideband.

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (Phases 0–3 entries
have landed, including the authoritative-sideband, extraction-boundary,
trusted-promotion, manual-response-default, gap-semantics, and verified-attach entries;
Phase 4 adds the read-only-tools, `qsf_tools`-boundary, execution-recording,
exchange-boundary, and verified-function-call entries), `EngineeringDiary.md` (one
entry per logical application change), `README.md` and launcher documentation (as
phases land), refreshes to `Architecture.RealtimeSessionServer.md`,
`Architecture.AudioLoop.md`, `Architecture.ToolSystem`, `Architecture.MemorySystem`,
`Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.