# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Active implementation plan. Phase 0 (decisions & contracts), Phase 1 (extract
`qsf_session`), Phase 2 (thin media plane — live browser voice), Phase 3
(authoritative sideband + memory injection), and Phase 4 (model-invoked read-only
perception tools) are **complete and human-tested**, with follow-on noise /
exchange-integrity hardening landed 2026-06-12/13; **Phase 5 (live memory extraction +
presence / interruption refinement) is the active phase**, expanded below into an
actionable build and revised 2026-06-13. The last open Phase-5 product/research
decision (D18, interruption-representation depth) is now resolved **diagnostics-only**
(`DecisionLog.md`, 2026-06-13); everything needed to start is resolved.

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
| 4 | Model-invoked read-only perception tools — complete, human-tested 2026-06-11 | Yes | ✅ |
| 5 | Live memory extraction + presence / interruption refinement — **active** | Yes | **Yes** |

---

## Phases 0–4 — complete (compacted)

Full per-step detail lives in git history, `docs/DecisionLog.md`. What follows is the carry-forward that still constrains Phase 5.

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

- **Lean dependency graph is the model for every extraction since** (it is how
  Phase 4 made the tool registry reachable from the server, and how Phase 5 keeps
  extraction out of the realtime server): `qsf_session` depends only on `anyhow`,
  `qsf_memory`, `qsf_context` (since Phase 3), `serde`, `serde_json`, `tempfile`,
  `time`, `uuid` — no `tokio`, `reqwest`, `engine_logging`, or provider crates.
- `qsf_session` owns the pure reducer/state/event contracts (`LiveSessionEvent`,
  `LiveSessionState`, `Exchange`, `Turn`, `ProviderEventRecord`), persistence
  (`persist_session_state` / `load_session_state`), continuity manifest, sleep
  records, `ContentHash`, the `ToolCategory` / `ToolSideEffectLevel` enums, and the
  interruption contracts (`InterruptionRecord`, `InterruptionAction`,
  `InterruptionStopOutcome`, `ExchangeStatus::Interrupted`,
  `LiveSessionEvent::UserInterrupted`) — **Phase 5's interruption refinement builds on
  these existing contracts, not new ones.**
- **Persistence constraint:** `persist_session_state` skips
  `LiveSessionState.completed_exchanges` (in-memory only, guarded by test). Durable
  state requires promotion to a `Turn` (`Turn::try_from(&Exchange)`). The persisted
  `session-state.json` / `continuity-manifest.json` schemas are guarded by a golden
  test (`crates/qsf_session/tests/session_state_schema.rs`) — **any new persisted
  field needs serde defaults and golden-test updates to stay compatible.**
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
  crates only. This boundary held through Phase 4 and **constrains where Phase 5
  extraction can run.**
- Provider `event_id` dedupe/order is the server translator's job, not the reducer's.
- `ProviderEventRecord` carries `call_id`, `event_id`, `item_id`, `previous_item_id`,
  `response_id` — the same identity fields later phases reuse.
- Test pattern to mirror: provider endpoints are injected via `AppState` base URLs so
  tests run against mocked HTTP/WS servers, with key-absence assertions throughout.
- **Carried-forward gap:** end-to-end / per-stage live-loop latency measurement was
  deferred — **Phase 5 closes it.**

### Phase 3 — Authoritative sideband + memory injection (human-tested 2026-06-10)

- **Crate layering (D1, landed):**
  `qsf_memory ← qsf_context ← qsf_session ← {qsf_app, qsf_realtime_server}`, with
  `qsf_realtime_protocol` an independent lean leaf. Retrieval scoring lives in
  `qsf_memory`; the context-assembly domain (`ContextFragment`, `ContextBudget`,
  `assemble_context`) lives in `qsf_context`; realtime JSON builders/parser live in
  `qsf_realtime_protocol`. `qsf_app` re-exports moved items through facades. **This
  extract-to-lean-crate + facade pattern is reused throughout.**
- **Sideband is authoritative (D2/D5/D6):** a long-lived task attaches to
  `wss://api.openai.com/v1/realtime?call_id=...` with bearer `OPENAI_API_KEY` (D3
  verified live). Provider events flow through the pure translator into
  `apply_live_session_event`; exchanges are stamped `Trusted`; the browser relay is
  UI-only diagnostics.
- **Response timing is server-owned (D5):** default `create_response = false`; on each
  committed user turn the sideband retrieves memory, injects a small packet
  (`conversation.item.create` + `session.update`, built by the pure builder in
  `realtime/injection.rs`), then sends `response.create`. Live timing confirmed
  acceptable.
- **Trusted promotion preconditions (D2):** `Turn::try_from(&Exchange)` hard-requires
  `completed_at`, `output`, `context_assembly`, and `ExchangeModelUse`. Only
  **complete** trusted exchanges promote into the shared continuity root
  (`promote_completed_trusted_exchanges`); incomplete/failed/degraded/interrupted
  never do. **Phase-5 extraction reuses this trust gate as its eligibility filter.**
- **Gap semantics (D6):** an unrecoverable/lossy sideband disconnect marks the session
  *degraded* until reconnect + `session.updated` acknowledgement; gap-window exchanges
  are permanently non-promotable.
- The memory-store resolver (`realtime/memory_store.rs`:
  `load_session_memory_store` / `retrieve_session_memories`) handles
  existing/absent/malformed/empty stores without panicking — **reuse it, do not
  re-resolve stores.**
- Key-absence assertions extend across the sideband; sideband health is surfaced to
  the browser UI. **Ageing/consolidation of realtime memory was deliberately deferred
  to Phase 5.**

### Phase 4 — Model-invoked read-only perception tools (human-tested 2026-06-11)

A live voice session had the model invoke read-only perception tools and speak a
tool-grounded answer; non-allow-listed calls were proven unexecuted and recorded as
denied. **Carry-forward facts and constraints for Phase 5:**

- **`qsf_tools` lean generic registry crate landed (D7):** the `Tool` trait,
  `ToolRequest`/`ToolPermission`, `ToolResult`, `ToolMetadata`, a parameters-bearing
  `ToolDefinition`, and a dynamic (registered boxed tools) registry; `qsf_app::tools`
  is a re-export facade (with `ToolDefinition` → `ModelToolDefinition` conversion).
  `qsf_realtime_server` depends on `qsf_tools` — **still no `qsf_app` dependency.**
  The generic registry is what makes the deferred "full tool set for the live model"
  an additive change.
- **Three read-only perception tools (D8)** in
  `crates/qsf_realtime_server/src/realtime/tools.rs`: `search_memory`,
  `get_associations`, `inspect_session_state`. `inspect_session_state` reports trusted
  durable completion as `completed_exchange_count` and active-exchange presence
  separately as `active_exchange_present` (the count-fix decision). No `qsf_app` tool
  is exposed live yet (see "Deferred beyond Phase 5").
- **Tool records persist on durable `Turn`s (D9):** `ToolExecutionRecord` +
  `LiveSessionEvent::ToolResolved` (reduced purely, linked to `ToolRequestRecord` by
  provider `call_id`) are persisted behind `#[serde(default)]` with golden tests
  updated. Denials are durable records, not execution evidence; `auto_executed` is not
  execution evidence. **Tool activity is therefore inspectable post-session and is
  part of Phase-5 extraction input.**
- **Tool loop choreography (D10/D11/D13):** a `response.done` whose output is a
  function call does **not** finalize the exchange (the sideband suppresses
  `OutputProduced`/`ModelRoleCompleted`/`ExchangeCompleted` for function-call
  completions); the loop is capped (max 3 sequential tool calls/turn); denied calls
  still receive a structured `function_call_output` + `response.create` for verbal
  recovery; `ExchangeModelUse` aggregates token/latency across the loop. The permission
  decision is a pure function; the sideband does **not** hold the session lock during
  tool execution.
- **Provider function-call wire shape verified (D12):** function tools declared on
  `session.tools` with `tool_choice`; results returned as a `function_call_output`
  `conversation.item.create`, then `response.create` — recorded in `DecisionLog.md`.
- **Stable default session id (2026-06-11):** browser sessions use the stable QSF
  session id `default` unless `--random-session-id` is passed; realtime continuity and
  memory live at `state/realtime/continuity/default`. **Phase-5 extraction defaults to
  this root.**

### Follow-on hardening — noise & exchange integrity (2026-06-12/13)

Landed after Phase 4 and directly feeding Phase 5's presence/interruption scope:

- **Sideband-owned interruption (2026-06-13):** provider `server_vad` stays enabled but
  `interrupt_response = false`; QSF decides from final transcripts whether to start,
  ignore, or interrupt a turn. Genuine interruptions send `response.cancel`; empty
  final transcripts are diagnostic-only. The sideband already emits
  `LiveSessionEvent::UserInterrupted(InterruptionRecord)` into the **in-memory** exchange
  model (`crates/qsf_realtime_server/src/realtime/sideband.rs:378`); persisting it is
  Phase-5 step 4 (diagnostics-only, D18).
- **Turn integrity (2026-06-12):** `realtime/turn_integrity.rs` guards the active
  exchange across interruptions and cancelled continuations; in-flight continuation
  courtesy transcripts and stale/superseded provider events are audited as
  diagnostic-only (`diagnostics.rs`), never mutating the live exchange. Expected
  recovery paths log at info, not warning.
- **Latency observability scaffold:** `DiagnosticRecord::LatencyObservation` exists in
  `crates/qsf_realtime_server/src/diagnostics.rs` and is already emitted (currently
  around the SDP rendezvous in `realtime/routes.rs`). **Phase 5 extends it to live-loop
  stages.**

---

## Phase 5 — Live memory extraction + presence / interruption refinement *(active)*

**Outcome.** After a live trusted realtime conversation, a lightweight extraction pass
runs over the promoted **trusted** `Turn`s (and their tool records) in the realtime
continuity root, reusing the existing sleep summarizer + association proposers to feed
the existing review/consolidation/commit path — producing memory/association/decision
candidates without changing the live loop. Realtime memory becomes subject to the
existing ageing/consolidation discipline (the Phase-3 deferral). Separately, presence
observability improves: interruptions are durably represented (building on the
sideband's existing `UserInterrupted` emission) and end-to-end + per-stage live-loop
latency is measured and surfaced (closing the Phase-2 latency-measurement gap), giving
the presence research concrete signals. The trust boundary, trusted-promotion
preconditions (D2), and degraded-gap semantics (D6) are unchanged.

### Starting state (already landed)

- Trusted `Turn`s promote to the shared continuity root
  `state/realtime/continuity/<session>` (default `default`); only complete trusted
  exchanges promote (D2); degraded/interrupted/incomplete exchanges do not.
- Tool execution records (D9) persist on `Turn`s and are part of the extractable
  record.
- The sideband already emits `LiveSessionEvent::UserInterrupted(InterruptionRecord)`
  into the **in-memory** exchange model (`qsf_session::Exchange.interruptions:
  Vec<InterruptionRecord>`, `ExchangeStatus::Interrupted`, reduced purely in
  `qsf_session::live_state`). **This is not durable today:** interrupted exchanges land
  in `completed_exchanges` (which is `#[serde(skip)]`, in-memory only, guarded by
  `persist_keeps_completed_exchanges_in_memory_only`) and are non-promotable (D2/D6), so
  the `InterruptionRecord` reaches neither the continuity root nor the diagnostics log.
  Step 4 therefore *adds* the diagnostic persistence path (D18); it is not a
  confirmation pass.
- `DiagnosticRecord::LatencyObservation` exists and is already emitted (SDP
  rendezvous); the emission/record pattern is the template for live-loop latency.
- The sleep machinery in `qsf_app` (`summarize_session`, the `AssociationProposer`
  impls `llm_candidate` and `safety_net_co_retrieval`, `merge_and_dedupe`, the
  `sleep/commit.rs` + `sleep/auto_promote.rs` review path, the text-based
  `SleepInputBundle`) consumes **text input**, and `SleepPhaseSessionSummary` /
  `RealtimeVoiceSession` experiments already exist as harness templates.

### Decisions (numbering continues from Phase 4; D1–D13 are prior)

- **D14 — decided: extraction runs in `qsf_app` over the realtime continuity root,
  not in `qsf_realtime_server`.** The proposer/commit machinery lives in `qsf_app` and
  consumes `qsf_app` types; `qsf_realtime_server` must not depend on `qsf_app` (the
  boundary held through Phases 2–4). `qsf_app` already depends on `qsf_session` and can
  read the continuity root, and `SleepInputBundle` is text-based, so building the
  extraction input from promoted trusted `Turn`s is a read-only adaptation. This also
  matches the presence concept's "keep the live loop cheap; defer consolidation to a
  sleep-like phase between sessions." Concretely: a new `Experiment.LiveMemoryExtraction`
  (or an extension of `SleepPhaseSessionSummary` to accept a realtime continuity
  source) reads `state/realtime/continuity/<session>`, builds a `SleepInputBundle` from
  the trusted `Turn`s (+ tool records), then runs `summarize_session` + the existing
  proposers + `merge_and_dedupe` + the commit/auto-promote review path.
- **D15 — decided: only trusted, promoted `Turn`s are extraction-eligible.**
  Diagnostic-only exchanges, degraded/gap-window exchanges, and
  interrupted/incomplete exchanges (which never promote, D2/D6) are excluded from the
  extraction input. This keeps untrusted browser-relayed material out of long-term
  memory and reuses the existing trust gate instead of inventing a new one.
- **D16 — decided: extraction is explicitly invoked for Phase 5; auto-trigger at
  session stop is deferred.** Auto-trigger would require the realtime server to invoke
  `qsf_app` (boundary violation) or an out-of-process post-session hook — a larger
  orchestration change. The incremental, testable slice is an explicit pass over a
  named continuity root. Auto-trigger/orchestration is recorded under "Deferred beyond
  Phase 5."
- **D17 — decided: realtime memory ageing/consolidation reuses the existing ageing
  path** (`qsf_memory` retrieval recency weighting + the `qsf_app` session ageing in
  `crates/qsf_app/src/session/`), applied to the realtime session's memory store during
  the extraction pass. No separate ageing model for realtime.
- **D18 — decided (product/research): interruptions are diagnostics-only**
  (`DecisionLog.md`, 2026-06-13). The durable `InterruptionRecord` already captures
  `action`, `stop_outcome`, and `response_id`, and the sideband emits it — but only into
  the in-memory exchange model, which is never persisted (see "Starting state"). Rather
  than promote interruptions into the trusted continuity/memory schema, Phase 5 persists
  the interrupted exchange + raw timing/silence signals to the per-session **diagnostics
  log** and leaves the durable continuity schema and its golden tests unchanged. This
  keeps interrupted/incomplete material out of trusted long-term memory (D2/D6), avoids
  golden-test churn for an unvalidated research feature, stays durable-on-disk for
  after-the-fact presence analysis, and matches `Concept.RealtimePresence` ("log
  interruptions without over-interpreting them"). Durable enrichment (pause/silence
  durations, barge-in classification, topic-shift flags) stays deferred beyond Phase 5,
  to be promoted from diagnostics only if presence evaluation shows a concrete need.
- **D19 — decided (with research note): extraction provenance uses both the
  input-transcription text and the assistant-output text of each trusted `Turn`,
  labeling each candidate's source.** ASR-vs-model transcript divergence (an open
  `ResearchQuestions.Audio` item) is recorded as provenance, not reconciled in Phase 5;
  reconciliation stays a research question.

### Architecture constraints (must hold)

- `qsf_realtime_server` still must not depend on `qsf_app`. Extraction lives in
  `qsf_app`; the realtime server only writes the continuity/diagnostic artifacts it
  already writes. Keep `main.rs` / `lib.rs` / `mod.rs` thin.
- Reducers stay pure: any new presence/latency events (if added) reduce with no I/O;
  latency/interruption capture is the effectful edge (sideband/diagnostics) feeding
  results back as events/records — `input -> action -> reducer -> state -> render` is
  unchanged.
- The "build `SleepInputBundle` from trusted `Turn`s" transform is a pure,
  unit-testable function (same discipline as the Phase-3 injection builder and the
  Phase-4 permission decision), separate from the async model/commit calls.
- Persistence/schema: D18 resolved diagnostics-only, so Phase 5 adds **no** durable
  continuity field and the `session_state_schema.rs` golden tests stay unchanged;
  interruption/presence durability lives in the diagnostics log. (Any future durable
  field would still go behind `#[serde(default)]` with golden-test updates, legacy
  artifacts loading — that is deferred beyond Phase 5.)
- Extraction must not mutate the live session or block the live loop; it runs over
  persisted artifacts, independent of an active call.
- `OPENAI_API_KEY` / key-absence assertions extend to every new extraction, latency,
  and interruption artifact and log line.

### Incremental, independently reviewable steps (each ends green; commit per step)

1. **Pure extraction-input builder (`qsf_app`).** Add a pure function that reads a
   realtime continuity root's promoted trusted `Turn`s (+ tool records) and builds a
   `SleepInputBundle`. Use `SessionState.turns` as the **single canonical transcript
   source** (`session_text` with labeled provenance per D19) and use the matching
   persisted `exchanges` only as metadata for already-promoted turn indices — do **not**
   feed both through `sleep_records()`, which would double-count each promoted voice turn
   (HIGH-001). Populate `review_notes` from trust context; keep any extraction provenance
   the summarizer must act on in `session_text` / `review_notes`, not artifact-only
   `diagnostic_notes` (MEDIUM-002, see "Remaining Checks"). Exclude
   non-trusted/degraded/interrupted/incomplete material per D15 — that material is already
   absent from the continuity root, so these tests prove only *trusted promoted turns are
   included*; interruption/degraded observability is tested against **diagnostics**, not
   the continuity root (HIGH-002). *Verify (unit):* bundle content from a fixture
   continuity root (trusted-only inclusion; **no duplicate transcript text**; tool records
   reflected; empty/absent root handled; malformed artifact tolerated). *Green:* full
   `cargo test`.
2. **Wire the extraction pass (`qsf_app` experiment/entry; D14/D16/D17).** Add
   `Experiment.LiveMemoryExtraction` (or extend `SleepPhaseSessionSummary` to accept a
   realtime continuity source) that resolves a realtime continuity root, runs the
   step-1 builder, calls `summarize_session`, runs the proposers (`llm_candidate`,
   `safety_net_co_retrieval`) + `merge_and_dedupe`, applies ageing/consolidation (D17)
   to the realtime memory store, and routes candidates through the existing
   review/commit/auto-promote path. Defaults exercise the path (defaults to the
   `default` continuity root). *Verify (experiment harness with a mocked
   `ModelClient`):* end-to-end extraction over a fixture trusted session produces a
   `SleepReport` and routes candidates to the existing review path; ageing applied; no
   candidates from non-trusted material. *Green.*
3. **Presence: live-loop latency observability (`qsf_realtime_server`).** Extend
   `DiagnosticRecord::LatencyObservation` (reuse the existing record + emission pattern
   in `realtime/routes.rs` / `realtime/sideband.rs`) to capture per-stage live-loop
   latencies: final-input-transcript-received → memory-injected →
   `response.create`-sent → `response.created`/first-audio, plus an end-to-end
   speech-end → first-audio measure. Emit from the sideband at the existing turn
   lifecycle points. Closes the Phase-2 latency-measurement gap. *Verify (unit /
   mocked WS):* stage timestamps recorded in order; `latency_ms` computed; key absent
   from all latency records. *Green.*
4. **Presence: interruption observability (`qsf_realtime_server`; D18 diagnostics-only).**
   At the point where an interrupted trusted exchange is currently dropped into in-memory
   `completed_exchanges`, emit it to the per-session **diagnostics log** (reuse
   `DiagnosticRecord::DiagnosticExchangeRecorded` with a trusted/sideband source, or add
   a dedicated interruption diagnostic) carrying the `InterruptionRecord` + timing — this
   is the new durable path, since interruptions are not persisted today. Confirm and test
   that the interrupted exchange still follows the D2/D6 promotion rules (non-promotable,
   absent from the continuity root). **No durable continuity-schema change** (per resolved
   D18). *Verify (mocked WS, mirroring the Phase-3/4 harness):* an interruption
   mid-response writes a durable **diagnostic** record carrying `action`/`stop_outcome`;
   the interrupted exchange is non-promotable and absent from the continuity root;
   presence signals are captured; key absent from all interruption/latency payloads.
   *Green.*
5. **Surface presence signals (optional UI).** Only if `ui/` is touched: surface
   end-to-end / per-stage latency and recent interruptions alongside the existing
   sideband health in the browser UI (informational only). Gate with `npm run check`,
   `npm test`, `npm run fmt`. Optional, not required this phase.
6. **Gates + docs.** `cargo clippy --all-targets -- -D warnings`, `cargo fmt`; UI gate
   only if `ui/` changed. Docs per the list below, using the exact repo paths.

### Acceptance criteria

- `cargo build`, full `cargo test`, clippy clean, `cargo fmt` applied; UI gate green
  if `ui/` changed.
- `qsf_realtime_server` still has **no `qsf_app` dependency**; extraction runs in
  `qsf_app` over persisted continuity artifacts.
- An extraction pass over a fixture trusted realtime continuity root produces a
  `SleepReport` and routes memory/association/decision candidates through the existing
  review/commit path; non-trusted/degraded/interrupted material is proven excluded
  (D15).
- Realtime memory ageing/consolidation is applied during the extraction pass (D17).
- Per-stage and end-to-end live-loop latency is recorded via `LatencyObservation` and
  is inspectable; the Phase-2 latency-measurement gap is closed.
- Sideband-owned interruptions produce a durable *diagnostic* interruption record;
  interrupted exchanges remain non-promotable (D2/D6) and absent from the continuity
  root; presence signals are captured per the resolved D18 (diagnostics-only).
- Any new durable field is behind `#[serde(default)]` with golden tests green; legacy
  artifacts still load.
- Defaults exercise the new path: extraction defaults to the `default` continuity root;
  latency/interruption capture is on by default.
- `OPENAI_API_KEY` proven absent from extraction inputs/outputs, latency/interruption
  records, and logs.
- D18 is resolved diagnostics-only and recorded in `DecisionLog.md`; step 4 adds a
  durable *diagnostic* interruption record and changes no durable continuity schema.

### Verification guidance (fits an extraction + live-service observability slice)

- *Automated (Rust):* pure extraction-input-builder fixtures (trusted-only inclusion,
  provenance labeling, notes from trust/degraded/interruption, empty/absent/malformed
  roots); the extraction pass with a mocked `ModelClient` (`SleepReport` produced,
  candidates routed, ageing applied, nothing from untrusted material);
  latency-observation ordering/computation; the mocked-WS interruption chain (durable
  diagnostic record, non-promotable interrupted exchange absent from the continuity
  root); key-absence assertions on all new artifacts; schema golden tests if any durable
  field is added.
- *Automated (TS):* only if `ui/` changes — `npm run check` + `npm test` (Vitest).
- **Human testing (required):** in a live browser session, hold a multi-turn
  conversation including at least one interruption; after stopping, run the extraction
  pass over the `default` continuity root and confirm sensible memory/association
  candidates flow into the review path (and that nothing leaks from untrusted
  exchanges); inspect latency observations and interruption records; evaluate presence
  against the `Concept.RealtimePresence` open questions (`PresenceEvaluation`,
  `InterruptionSemantics`, `LatencyBudget`) and record latency observations.

### Docs (per `ProjectWorkflow.md`)

- `Experiment.LiveMemoryExtraction` (or extend the sleep experiment doc) as the
  validation record, plus a presence report.
- Refresh `docs/Research/ResearchQuestions.Audio.md` (injection relevance; ASR-vs-model
  transcript divergence per D19).
- Update `docs/Concepts/Concept.RealtimeAudio.md` and cross-link
  `docs/Concepts/Concept.RealtimePresence.md` (presence / latency / interruption
  findings).
- Refresh `docs/Architecture/Architecture.RealtimeSessionServer.md` (latency /
  interruption observability), `docs/Architecture/Architecture.MemorySystem.md` and
  `docs/Architecture/Architecture.StateAndObservability.md` (realtime extraction +
  ageing; latency/interruption records); update each `Last reviewed:`.
- `DecisionLog.md`: extraction location (D14), trust gate (D15), explicit invocation
  (D16), realtime ageing (D17), the interruption-representation resolution (D18), and
  extraction provenance (D19).
- Clear commit history for implementation chronology; update active project
  documents when current behavior or design changes.
- README / launcher notes as the realtime mode grows.

### Deferred beyond Phase 5

- **Full tool set for the live model (owner intent, 2026-06-10, recorded under D8).**
  Expose the broader `qsf_app` tool set (project docs, recall-turn, calculator, and
  successors) to the live realtime model as its own phase. Requires moving the tools'
  data services (`ProjectDocService`, durable-session access) past the no-`qsf_app`
  boundary; the D7 generic `qsf_tools` registry exists to make that phase an additive
  change.
- **Automatic extraction trigger / orchestration (D16).** Auto-run extraction at
  realtime session stop (or on a scheduled sleep cadence) via an out-of-process hook or
  a launcher-owned post-session step, so consolidation does not require an explicit
  manual pass — without giving `qsf_realtime_server` a `qsf_app` dependency.
- **Durable interruption enrichment** (D18 resolved diagnostics-only): richer
  interruption signals (pause/silence durations, barge-in classification, topic-shift
  flags) promoted from diagnostics into the durable model once presence evaluation shows
  a concrete need.

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
validation, and phase reports — including the Phase-5 `Experiment.LiveMemoryExtraction`
pass over a realtime continuity root.

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
- **Phase 4 gate (met):** the function-call → decision → execution → output → response
  chain is green against a mocked provider; a non-allow-listed tool is proven
  unexecuted and recorded as denied; trusted promotion and degraded semantics hold
  across tool loops; no credential leaks through tool payloads; confirmed live
  2026-06-11.
- **Phase 5 gate:** an extraction pass over a fixture trusted continuity root produces
  a `SleepReport` and routes candidates through the existing review/commit path with
  non-trusted material proven excluded; per-stage + end-to-end latency is recorded;
  interruptions produce durable records and interrupted exchanges stay non-promotable;
  no credential leaks through extraction/latency/interruption artifacts.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience (done), cross-session continuity (done), model-invoked tool use (done),
  and presence + extraction quality.

## Remaining Checks Before Each Phase

- **Phases 0–4 (complete):** all prior-phase open questions are resolved and recorded
  in `DecisionLog.md` / archived historical notes; the D3 attach shape, D5 manual-response
  timing, and D12 provider function-call shapes were all confirmed live. Keep overlap
  policy B (D4) unless the authoritative sideband reveals real overlap — still a watch
  item, not a change.
- **Phase 5 (active):** D14–D19 are decided (D18 resolved diagnostics-only,
  `DecisionLog.md` 2026-06-13). Confirm the realtime continuity-root layout
  (`state/realtime/continuity/<session>`) and the `SleepInputBundle` adaptation against
  the actual promoted-`Turn` artifacts before step 1. Two implementation calls remain
  before the steps they affect:
  - *Before steps 1–2 (MEDIUM-002):* `build_sleep_user_prompt` includes `review_notes`
    but **not** `diagnostic_notes` today, so any tool/trust/provenance context the
    summarizer must act on belongs in `session_text` or `review_notes`; keep
    `diagnostic_notes` artifact-only unless that contract is deliberately changed.
  - *Before step 3 (MEDIUM-001):* no `input_audio_buffer.speech_stopped` handler exists
    yet, so an "end-to-end speech-end → first-audio" metric would be a proxy off the
    final-transcript timestamp. Either add a speech-stopped handler or name the metric
    `final-transcript-received → first-audio`; tests assert the exact event source per
    stage.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream.

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (Phases 0–4 entries
have landed, including the authoritative-sideband, extraction-boundary,
trusted-promotion, manual-response-default, gap-semantics, verified-attach,
read-only-tools, `qsf_tools`-boundary, execution-recording, verified-function-call,
stable-default-session, and sideband-owned-interruption entries; Phase 5 adds the
extraction-location, trust-gate, explicit-invocation, realtime-ageing,
interruption-representation, and extraction-provenance entries), `README.md` and
launcher documentation (as phases land), refreshes to `Architecture.RealtimeSessionServer.md`,
`Architecture.AudioLoop.md`, `Architecture.ToolSystem`, `Architecture.MemorySystem`,
`Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.
