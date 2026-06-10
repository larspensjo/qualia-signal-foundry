# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Active implementation plan. Phase 0 (decisions & contracts), Phase 1 (extract
`qsf_session`), and Phase 2 (thin media plane — live browser voice) are **complete
and accepted/human-tested**; **Phase 3 (authoritative sideband + memory injection)
is the active phase**, expanded below into an actionable build. Phases 4–5 remain
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
| 0 | Decisions & contracts (complete, accepted 2026-06-09) | No | No |
| 1 | Extract `qsf_session` crate (pure refactor) — complete (`45ed9cd`) | Yes | No |
| 2 | Thin media plane — live browser voice — complete, human-tested 2026-06-09 | Yes | ✅ |
| 3 | Authoritative sideband + memory injection — **active** | Yes | **Yes** |
| 4 | Model-invoked read-only perception tools | Yes | **Yes** |
| 5 | Live memory extraction + presence refinement | Yes | **Yes** |

---

## Phase 0 — Decisions & contracts — complete (accepted 2026-06-09)

Lock-in pass, no implementation. The provider-event → QSF-event mapping contract is
recorded in [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md)
and `docs/DecisionLog.md`. **Accepted decisions that still constrain Phase 3 and
later:**

- `qsf_realtime_server` owns live realtime side effects; `qsf_browser_server` stays a
  read-only inspection server depending only on `qsf_memory`.
- The browser owns the WebRTC media plane; the QSF server owns the **server-side SDP
  rendezvous** (it holds `OPENAI_API_KEY`, performs the SDP exchange, and captures the
  `{ qsf_session_id ↔ provider call_id }` binding first-hand). Media (audio RTP) flows
  directly browser↔OpenAI; only signaling is proxied. **This supersedes the original
  ephemeral-token decision** (recorded as a reversal in `DecisionLog.md`).
- Browser-relayed provider events are **untrusted, diagnostic-only**, excluded from
  sleep and continuity. **The Phase-3 server sideband is the authoritative source for
  trusted live exchanges** — this phase realizes that decision.
- Session defaults: `gpt-realtime-2`, voice `marin`, `reasoning_effort = medium`
  (QSF metadata only — *not* forwarded to the provider; see Phase 2), `output_modalities
  = ["audio"]`, provider `server_vad` with automatic response creation and interruption.
  (**Phase 3 reverses the "automatic response creation" half of this default to
  `create_response = false` so per-turn memory injection can precede the response — see D5.**)
- **Mapping contract:** the exchange boundary in speech-to-speech mode is a paired
  user-utterance → assistant-response turn keyed by provider `item_id` / `response_id`;
  `ExchangeCompleted` carries an explicit `exchange_index` so a late/duplicate
  completion cannot close the wrong exchange. The required reducer matrix (transcript
  completion after response start; duplicate events; interruption before
  `response.created`; completion after interruption; a second user turn before the prior
  response finishes; out-of-order lifecycle) is implemented and green.
- The `call_id` binding is active-call scoped, invalidated on stop/error/expiry, with a
  short cleanup grace; the Phase-3 sideband attaches to this server-captured `call_id`.
- Realtime voice conversation is the long-term primary QSF operating mode.

---

## Phase 1 — Extract `qsf_session` crate (pure refactor) — complete (`45ed9cd`)

A lean `crates/qsf_session` crate holds the pure session reducer, state, event,
persistence, and continuity contracts. **Carry-forward facts and constraints that
matter to Phase 3:**

- **Lean dependency graph (the model to mirror for any new extraction).**
  `qsf_session` depends only on `anyhow`, `qsf_memory`, `serde`, `serde_json`,
  `tempfile`, `time`, and `uuid` — none of `cpal`, `openai_provider_kit`, `reqwest`,
  `tokio`, `tokio-tungstenite`, `hound`, `base64`, or `engine_logging`. New shared
  crates should keep a comparably lean graph so `qsf_realtime_server` can depend on them
  without the heavy `qsf_app` runtime.
- **What moved:** the reducer/state/event contracts (`LiveSessionEvent`,
  `LiveSessionState`, `reduce_live_session`, the pure `reduce_session` family),
  `Exchange` plus `ProviderEventRecord` / `ProviderEventKind`, persistence
  (`persist_session_state` / `load_session_state`), the continuity manifest,
  continuation, sleep records, the `context` value types with their pure methods,
  `ContentHash`, and the `ToolCategory` / `ToolSideEffectLevel` enums. (**Phase 3 D1
  relocates the `context` value types again — out of `qsf_session` into a new lean
  `qsf_context` crate that `qsf_session` then depends on — so the assembler and the
  `RetrievedMemory → ContextFragment` adapter have a shared home below `qsf_app`.**)
- **What stayed in `qsf_app` (effectful edge):** the run-log `EventType` taxonomy and
  `EventRecord` writer; the effectful `runtime` functions, `live_memory`, `ageing`;
  `assemble_context`, the prompt algorithms, and `From<&RetrievedMemory> for
  ContextFragment`; the two `from_env` constructors (now free functions). **Memory
  retrieval scoring (`retrieve_memories`) also stayed in `qsf_app` — Phase 3 must
  resolve where it lives so the realtime server can use it (see D1 below).**
- **Persistence constraint (important for Phase 3 trusted exchanges).**
  `persist_session_state` serializes `SessionState` but **skips**
  `LiveSessionState.completed_exchanges` (in-memory only, guarded by
  `persist_keeps_completed_exchanges_in_memory_only`). To make an exchange durable it
  must be promoted to a `Turn` (`Turn::try_from(&Exchange)`) recorded into the durable
  `SessionState`, then continuity-persisted. The persisted `session-state.json` /
  `continuity-manifest.json` schemas are byte-compatible and guarded by a golden test.
- **Reducer carry-forward:** `LiveSessionState` keeps a **single** `active_exchange` /
  `active_response`; overlap policy is resolved as Phase-2 policy **B** (finalize-prior).
  `ExchangeCompleted` finalizes only the matching active exchange (index guard).

---

## Phase 2 — Thin media plane: live browser voice — complete, human-tested 2026-06-09

A live, full-duplex spoken browser conversation works end-to-end via `gpt-realtime-2`
with barge-in. The phase introduced `crates/qsf_realtime_server` (axum/tokio) and a
browser WebRTC client in `crates/qsf_realtime_server/ui/`. **What shipped, plus the
lessons and constraints Phase 3 builds on:**

- **Server shape (the surface Phase 3 extends).** Thin `main.rs`/`lib.rs`/`cli.rs`;
  `state.rs` holds `AppState` (env-sourced `OPENAI_API_KEY`, async `reqwest::Client`,
  per-session `SessionRuntime` map behind `Arc<Mutex>`, diagnostics dir). Routes in
  `realtime/routes.rs`: `POST /api/realtime/session` (allocates `qsf_session_id` +
  non-secret default config, **no credential returned**), `POST /api/realtime/sdp`
  (server-side SDP rendezvous via multipart to `/v1/realtime/calls`, captures `call_id`
  from the `Location` header, stores the `CallBinding`), `WS /api/realtime/events`
  (untrusted browser relay), `POST /api/realtime/stop`. The server depends on
  `qsf_session` (+ `qsf_memory` transitively), **not** the full `qsf_app` runtime —
  preserve this boundary in Phase 3.
- **Trust marker exists and is reserved for Phase 3.** `DiagnosticTrust` has both
  `Trusted` and `Untrusted`; `SessionRuntime.trust` is hardcoded `Untrusted` today.
  Diagnostic exchanges are written to a **run-scoped diagnostics JSONL store that is
  structurally outside the shared continuity root**, so sleep/continuity cannot consume
  them. Phase 3 introduces the first `Trusted` exchanges and the continuity-root write
  path.
- **Reducer overlap policy B (finalize-prior), dedupe split.** A new user turn
  finalizes the prior exchange first (`Interrupted` if its response was still streaming,
  else `Completed`); stale late events for a finalized exchange are no-ops. Provider
  `event_id` dedupe/order is the **server translator's** job at the WS boundary, not the
  reducer's (the reducer carries no event id). True concurrency was deferred to Phase 3
  *only if* the authoritative sideband reveals real overlap.
- **Provider identity fields landed.** `ProviderEventRecord` now carries `call_id`,
  `event_id`, `item_id`, `previous_item_id`, `response_id` (plus text/status/audio_marker)
  — reused by the sideband translation in Phase 3.
- **Provider drift recorded (2026-06-09 first live test).** Vite dev proxy needs
  `ws: true`; the SDP handler surfaces the provider error body; OpenAI `/v1/realtime/calls`
  **rejects `session.reasoning_effort`** (`unknown_parameter`), so it is kept as QSF
  session metadata but **not forwarded**. `gpt-realtime-2` / `marin` / `["audio"]` /
  `server_vad` were accepted. Per-stage latency measurement for presence research remains
  open (carried to Phase 5).
- **Required mapping-contract reducer matrix is green** (the Phase-2 gate); the SDP proxy
  is tested against a mocked OpenAI endpoint (base URL injected via `AppState`) with
  key-absence assertions; the relay rejects malformed/oversized payloads and persists
  diagnostic exchanges stamped with `source`/`trust`.

**Phase-3 entry constraint:** the browser relay path remains wired but **becomes
UI-only diagnostics** in Phase 3 — it must no longer be reduced into authoritative state
once the sideband is the trusted source.

---

## Phase 3 — Control/context plane: authoritative sideband + memory injection  *(active — the "mixture" becomes real)*

**Outcome.** A server-side **sideband** attaches to the server-captured `call_id` and
becomes the **authoritative** event source; its completed exchanges are stamped
`Trusted` and promoted into the shared continuity root so they are sleep/continuity
eligible. The browser relay reverts to **UI-only diagnostics**. Per session start and
per user turn, the server retrieves **relevant** memory (existing association-weighted
retrieval) and injects a **small** working-memory packet via `conversation.item.create`,
plus a `session.update` for identity/tone — relevance over volume, never a full dump.
No credential leaves the server; media still flows directly browser↔OpenAI.

### Open decisions — surface and confirm before coding (per `Agents.md`)

These are recommended resolutions with rationale; confirm (or override) each before the
step that depends on it. **D1, D2, and D5 were confirmed 2026-06-10** (after the
`Review.RealtimeVoiceConversation.phase3.Plan.codex` review); D6 was added by that
review. **D3 still requires a live-provider verification at implementation time.**

- **D1 — Where do memory retrieval and realtime protocol helpers live so
  `qsf_realtime_server` can use them without depending on the full `qsf_app` runtime?**
  `retrieve_memories` / `RetrievalStrategy` / `RetrievedMemory` / `RetrievalScore` and the
  co-retrieval delta logic currently live in `qsf_app::memory` (they operate on
  `qsf_memory::{MemoryRecord, Association}` and a couple of timing helpers, so they are
  nearly pure). The realtime JSON protocol builders/parsers (`session.update`,
  `conversation.item.create`, `response.create`, `parse_realtime_server_event` and its
  field extractors) live in `qsf_app::audio::{voice_session_provider, transcript_provider}`.
  **Confirmed (2026-06-10): three lean homes — `qsf_memory` + new `qsf_context` + new
  `qsf_realtime_protocol`** (not a single `qsf_realtime_core`; never a `qsf_app`
  dependency):
  - **`qsf_memory`** gains the pure retrieval scoring (`retrieve_memories` /
    `RetrievalStrategy` / `RetrievedMemory` / `RetrievalScore` / co-retrieval delta) — it
    already owns the record/association/store types these operate on.
  - **`qsf_context` (new, sits *between* `qsf_memory` and `qsf_session`)** owns the whole
    context-assembly domain: the context value types (`ContextFragment`, `ContextBudget`,
    `ContextAssembly`, `ContextSourceKind`, `ContextSelection`, `ContextOmission`) **moved
    out of `qsf_session::context`**, the pure `assemble_context` algorithm, and the
    `From<&RetrievedMemory> for ContextFragment` adapter. `qsf_session` then depends on
    `qsf_context` (its `Exchange`/`Turn` embed `ContextAssembly`).
  - **`qsf_realtime_protocol` (new, lean, independent)** owns the realtime JSON
    builders/parser/translator (see step 2).
  **Why the context assembler cannot live in `qsf_memory` (the review's
  P3-HIGH-MISSING-CONTEXT-EXTRACTION, resolved):** `qsf_session → qsf_memory` (fixed
  direction), the context value types live in `qsf_session::context`, and `assemble_context`
  consumes them — so hosting the assembler in `qsf_memory` would force a dependency cycle or
  drag context types into the lowest crate. Rust's **orphan rule** also pins
  `From<&RetrievedMemory> for ContextFragment` to the crate defining `ContextFragment` or
  `RetrievedMemory`; it physically cannot be duplicated into both `qsf_app` and
  `qsf_realtime_server`. A dedicated `qsf_context` crate is therefore the elegant resolution.
  Resulting acyclic layering:
  `qsf_memory ← qsf_context ← qsf_session ← {qsf_app, qsf_realtime_server}` (with
  `qsf_realtime_protocol` an independent lean leaf). `qsf_app` re-exports the moved retrieval
  and context items through its existing `memory`/`context` facades so current call sites
  (`retrieve_memories`, `assemble_context`, …) are unchanged. **Note this partially reverses
  the Phase-1 placement of the context value types into `qsf_session`** (record the boundary
  in `DecisionLog.md`).
- **D2 — How are trusted sideband exchanges made sleep/continuity-eligible?** Diagnostic
  exchanges are structurally outside the continuity root (Phase 2), and
  `persist_session_state` skips `completed_exchanges` (Phase 1). **Recommended:** on
  finalizing a trusted exchange, promote it to a durable `Turn` (`Turn::try_from(&Exchange)`
  → `SessionState` turns) and write the shared continuity root via
  `qsf_session::persist_session_state` + the continuity manifest. **Confirmed
  (2026-06-10): minimal continuity-persist in the server reusing only `qsf_session`; defer
  ageing/consolidation to Phase 5** (do not extract the `qsf_app` `ageing` /
  `persist_continuity_state_from_dirs` helpers this phase).
  **Ordering requirement from review (P3-HIGH-TURN-PROMOTION-SEQUENCE):**
  `Turn::try_from(&Exchange)` hard-requires `completed_at`, `output`, **`context_assembly`,
  and `model` (`ExchangeModelUse`)** (`crates/qsf_session/src/exchange.rs`). The sideband
  translation only supplies transcript/provider/output lifecycle data, so context and model
  metadata must be recorded on the trusted exchange **before** promotion: `context_assembly`
  from the injection builder (the selected fragments under budget — D5/step 6) and
  `ExchangeModelUse` from the provider `response.done` usage/latency. Promotion runs only for
  **complete** trusted exchanges that carry both; incomplete, failed, or degraded exchanges
  (see D6) are never silently promoted.
- **D3 — Sideband attach endpoint/auth shape.** Working assumption (from Phase-0/2):
  `wss://api.openai.com/v1/realtime?call_id=…` authenticated with the server-held
  `OPENAI_API_KEY`. **This must be verified against the current OpenAI realtime API at
  implementation time** (same caution as Phase-2 decision 4); record any drift in
  `DecisionLog.md` before changing accepted defaults.
- **D4 — Overlap policy.** Keep policy **B** (single active exchange, finalize-prior).
  Revisit true concurrency only if the authoritative sideband reveals real overlapping
  turns; treat it as a watch item, not a required change this phase.
- **D5 — Response timing for per-turn injection (confirmed 2026-06-10; blocker
  P3-BLOCKER-RESPONSE-CONTROL).** Today the realtime defaults set provider `server_vad`
  `create_response = true` (`crates/qsf_realtime_server/src/state.rs`), so the provider can
  start generating the moment VAD commits the user turn — *before* the sideband can retrieve
  memory from the final transcript and inject it. OpenAI's realtime docs call out disabling
  `turn_detection.create_response` for exactly this RAG/control pattern. **Decision: default
  `create_response = false` and make the sideband own response timing per turn** — on each
  committed user turn the sideband retrieves + injects memory, then sends `response.create`.
  This **supersedes the Phase-0 "automatic response creation" default** for Phase 3 (record
  the reversal in `DecisionLog.md`, mirroring the ephemeral-token reversal). Rationale and
  latency expectation: the sideband is a persistent server↔OpenAI WebSocket already attached
  to the call, so for the **fast** injection path (memory + associations assembled locally)
  `response.create` fires immediately and should be ~as snappy as auto-response; only when an
  injection requires an **LLM round-trip** does the turn deliberately take longer. **Verify
  this parity empirically** (manual-`response.create` vs. an auto-response baseline) rather
  than assuming it — if parity holds there is no reason to keep a `create_response = true`
  mode at all. The slower "thinking" path will later surface a visual cue in the Live
  Activation Dashboard (forward pointer; **out of scope this phase**). Update the Phase-3
  acceptance tests, defaults, and docs to match.
- **D6 — Authoritative-sideband gap semantics (added by review;
  P3-MED-SIDEBAND-GAP-SEMANTICS).** Once the sideband is the authoritative source, a
  disconnect can mean **missed provider events**, so reconnect/backoff alone is not enough.
  **Decision: on any unrecoverable or potentially lossy gap, mark transport trust
  *degraded* until the sideband reconnects and receives a `session.updated`
  acknowledgement; any exchange active during the gap is permanently
  *non-promotable* (no trusted continuity write), while later fresh exchanges can promote
  after recovery.** This is valid because D5 sets `create_response = false`: while the
  sideband is disconnected, no unseen assistant response can be generated by QSF, though
  user audio/provider events may be missed. The browser relay (diagnostic-only) is
  unaffected. Cover disconnect-during-active-exchange and disconnect-during-injection in
  tests.

### Architecture constraints (must hold)

- Keep `main.rs` / `lib.rs` / `mod.rs` thin (per `Agents.md`); the sideband and injection
  are the **effectful edge**. Event translation continues to flow through the **pure**
  `qsf_session` reducer (`apply_live_session_event`) — do not add provider I/O or async to
  the reducer.
- The injection-packet construction must be a **pure, unit-testable builder** (retrieved
  memory + transcript/identity → payload), separate from the async sideband I/O. The
  fragments it selects under `ContextBudget` are also the exchange's `context_assembly`
  record (so trusted promotion has the metadata it needs — D2).
- Raw-provider-event → `LiveSessionEvent`/diagnostic **translation must be a pure module**
  (extracted from `process_relay_envelope`, `crates/qsf_realtime_server/src/realtime/routes.rs`),
  shared by the authoritative sideband and reused for relay UI diagnostics — not re-embedded
  alongside async WS acking/locking/persistence (P3-MED-TRANSLATOR-EXTRACTION).
- **Per-turn response timing is server-owned (D5):** the default is `create_response = false`
  and the sideband issues `response.create` after injection. The reducer stays pure; response
  timing is an effectful-edge concern.
- `OPENAI_API_KEY` stays server-side and must be proven absent from every response and log
  (extend the Phase-2 no-secret assertions to the sideband).

### Incremental, independently reviewable steps (each ends green; commit per step)

1. **Extract retrieval into `qsf_memory` and the context-assembly domain into new
   `qsf_context` (D1).** Two related moves, each landing green; commit separately if cleaner:
   - *(1a) Retrieval → `qsf_memory`.* Move `retrieve_memories` + scoring/result types
     (`RetrievalStrategy`/`RetrievedMemory`/`RetrievalScore`/`AssociationPath`) + co-retrieval
     delta out of `qsf_app::memory` into `qsf_memory`, replacing the
     `crate::observability::trace` timing helpers with inline timing or a tiny local helper.
   - *(1b) Context domain → new `qsf_context`.* Create the crate **between `qsf_memory` and
     `qsf_session`**; move the context value types out of `qsf_session::context`, move the
     pure `assemble_context` algorithm and the `From<&RetrievedMemory> for ContextFragment`
     adapter out of `qsf_app::context`, and add `qsf_context = { path = ... }` to
     `qsf_session` (its `Exchange`/`Turn` now import `ContextAssembly` from `qsf_context`).
   Keep `qsf_app::{memory, context}` re-exporting the moved items so existing call sites (e.g.
   `text_owned_voice_loop`, `live_memory`, `assemble_context`) are unchanged. Move the
   retrieval/assembly unit tests with the code. *Green:* full `cargo test`.
2. **Extract realtime protocol helpers + the pure event translator into
   `qsf_realtime_protocol` (D1 + P3-MED-TRANSLATOR-EXTRACTION).** Move the request builders
   (`session.update`, `conversation.item.create`, `response.create`) and
   `parse_realtime_server_event` + field extractors into the lean crate. **Also extract the
   raw-provider-event → `LiveSessionEvent`/diagnostic translation** out of
   `process_relay_envelope` into a pure module (covering current event names
   `response.output_audio.delta`, `response.output_audio_transcript.*`, and nested
   `response.done` text extraction). Refactor the `qsf_app` one-shot `voice_session_provider`
   **and** the realtime-server relay to consume the moved helpers with **no behavior change**
   (their tests stay green); the relay keeps only the UI-diagnostic mapping. Do **not** move
   the one-shot runner. *Green.*
3. **Realtime continuity root + memory-store resolver (D2 prep;
   P3-MED-MEMORY-STORE-RESOLUTION).** Give `AppState` a continuity-root path and a
   `qsf_memory`-backed resolver that loads `memory-store.json` + associations for a session.
   *Verify (unit):* existing store loads; absent store → empty (no crash); malformed store →
   surfaced error, not a panic; empty store → no-injection path is taken downstream. *Green.*
4. **Sideband adapter scaffold + gap semantics** (`crates/qsf_realtime_server/src/realtime/sideband.rs`).
   A long-lived async task that, given a stored `call_id`, opens the provider WebSocket
   (`tokio-tungstenite`), supports concurrent read/write, and shuts down gracefully on
   stop / binding invalidation (reconnect/backoff policy included). Implement **D6 gap
   semantics**: on an unrecoverable/lossy disconnect, mark the session *degraded* and make
   subsequent exchanges non-promotable until a verified recovery point. Inject the WS base URL
   via `AppState` so tests run against a **mocked** WS server (mirrors the Phase-2 mocked
   OpenAI HTTP). *Verify:* attaches to the stored `call_id`; survives a server-initiated stop;
   never logs the API key; a disconnect during an active exchange / during injection marks the
   session degraded and blocks promotion. *Green.*
5. **Memory injection — pure builder (D5).** A pure function that, given association-weighted
   `retrieve_memories` output + the current transcript/identity, builds a **small**
   working-memory `conversation.item.create` packet plus a `session.update` for identity/tone
   — cap fragments/tokens (reuse the `ContextBudget` discipline); never a full dump. The
   selected fragments are returned as the exchange's `context_assembly` (consumed by step 6).
   *Verify (unit):* memory store + transcript → expected small payloads; empty store → no
   injection; oversized → capped; the returned `context_assembly` matches what was injected.
   *Green.*
6. **Authoritative live loop: translation + recording + trust promotion + manual response
   (D5, D2).** Route sideband provider events through the existing `apply_live_session_event`
   mapping (reuse the step-2 translator, now trusted), stamp `DiagnosticTrust::Trusted`, and
   **demote the browser relay to UI-only diagnostics** (no longer reduced into authoritative
   state). Set the realtime default to **`create_response = false`** and have the sideband, on
   each committed user turn, inject the step-5 packet and then send `response.create`. Record
   `context_assembly` (from the step-5 builder) and `ExchangeModelUse` (from `response.done`
   usage/latency) onto the trusted exchange **before** promotion. On finalize, promote
   **complete** trusted exchanges into the **shared continuity root** as durable turns
   (`Turn::try_from` → `persist_session_state` + manifest, D2); incomplete/failed/degraded
   exchanges (D6) are not promoted. *Verify:* a trusted exchange carries `context_assembly`
   + model before persist and lands in the continuity root sleep-eligible; a relay
   (diagnostic) exchange does not; a degraded session does not promote; the key is absent from
   sideband responses/logs. *Green.*
7. **Launcher preview path + lint/format/UI gates + docs.** Update the preview path to start
   the sideband with `create_response = false` per-turn injection **by default**; run
   `cargo clippy --all-targets -- -D warnings` then `cargo fmt`; run the UI gate
   (`npm run check`, **`npm test`/Vitest**, then `npm run fmt`) only if the relay UI demotion
   touches `ui/`. Docs below.

### Acceptance criteria

- `cargo build`, full `cargo test`, `cargo clippy --all-targets -- -D warnings` clean,
  `cargo fmt` applied; UI gate green if `ui/` changed.
- `qsf_realtime_server` **must not depend on `qsf_app`**; it uses lean QSF domain crates
  (`qsf_session`, `qsf_memory`, `qsf_context`, `qsf_realtime_protocol`) for shared
  memory/session/context/protocol logic, alongside its runtime dependencies
  (`axum`/`tokio`/`reqwest`/`engine_logging` and the newly added `tokio-tungstenite`).
  `OPENAI_API_KEY` is proven absent from every sideband response and log.
- The sideband attaches to a stored `call_id` (mocked WS), reads/writes concurrently, and
  shuts down gracefully on stop/invalidation; an unrecoverable/lossy gap marks the session
  **degraded** until reconnect + `session.updated`, and any gap-window exchange remains
  skipped for trusted promotion (D6).
- Given a memory store + transcript, the server emits the expected **small** injection
  payloads (`conversation.item.create` + `session.update`); an empty store yields no
  injection but still records an empty `ContextAssembly`; the budget cap is enforced; the
  resolver handles existing/absent/malformed stores without panicking.
- Trusted sideband exchanges carry `context_assembly` + `ExchangeModelUse` before promotion,
  are written to the shared continuity root, and are sleep/continuity-eligible; incomplete/
  failed/degraded exchanges are not promoted; Phase-2 diagnostic relay exchanges remain
  excluded — all verified by tests.
- The browser relay is demoted to UI-only diagnostics (no longer authoritative).
- **Defaults exercise the new path (D5):** the default realtime run sets
  `create_response = false`, attaches the sideband, and performs per-turn injection followed
  by a server-issued `response.create`, with a sensible small budget.
- Manual `response.create` latency for the fast (no-LLM) injection path is measured against an
  auto-response baseline and recorded; a parity result confirms `create_response = true` is
  unnecessary (D5).

### Verification guidance (fits a live-service + memory-integration slice)

- *Automated (Rust):* sideband attach + graceful-shutdown against a mocked WS; **gap
  semantics** (disconnect during active exchange / during injection → degraded until
  `session.updated` + active exchange non-promotable, D6); key-absence assertions extended to the sideband; the **store resolver**
  matrix (existing/absent/malformed/empty); the injection-payload builder matrix
  (store+transcript → small payload, empty store → no payload plus empty assembly, cap enforced); **trust-promotion
  preconditions** (trusted exchange carries `context_assembly` + `ExchangeModelUse` before
  persist; trusted → continuity root + sleep-eligible; diagnostic/incomplete/degraded →
  excluded); retrieval/context-assembly/protocol/translator extraction parity tests.
- *Automated (TS):* if the relay demotion touches `ui/`, `npm run check` + **`npm test`
  (Vitest)** stay green (P3-LOW-UI-TEST-GATE).
- **Human testing (required):** in a live browser session, reference something earlier in the
  same conversation and across a new session; confirm continuity surfaces in the spoken reply;
  confirm injected context is relevant and small (inspect artifacts); confirm no credential
  reaches the browser; **confirm the fast (no-LLM) injection path with `create_response =
  false` feels as snappy as the Phase-2 auto-response baseline (D5 parity)**, and note where a
  slow LLM-backed injection would warrant the future Live Activation Dashboard "thinking" cue;
  record end-to-end / per-stage latency for presence research (carried to Phase 5).
- *Provider reality check:* confirm the sideband attach endpoint/auth (D3) and the live event
  stream against the current API; record drift in `DecisionLog.md` before changing defaults.

### Docs (per `ProjectWorkflow.md`)

- `Experiment.LiveContextInjection` as the validation record.
- Refresh `Architecture.RealtimeSessionServer.md`: move the Phase-3 sideband from "Not yet
  implemented" to implemented; document the trusted-vs-diagnostic stores and the
  continuity-promotion path; update `Last reviewed:`.
- Update `Architecture.MemorySystem` (live injection + the retrieval extraction home) and
  note trusted live exchanges in `Architecture.StateAndObservability`.
- `DecisionLog.md` entries: authoritative sideband supersedes the browser relay as the
  trusted source; the retrieval/**context-assembly**/protocol/translator extraction crate
  boundary (D1); the trusted-exchange continuity-promotion path + promotion preconditions
  (D2); **the `create_response = false` manual-response default, reversing the Phase-0
  automatic-response default (D5)**; the authoritative-sideband gap/degraded semantics (D6);
  the confirmed sideband endpoint/auth (D3).
- One `EngineeringDiary.md` entry (follow the diary's "How to use" header).
- README / launcher notes as the realtime mode grows.

---

## Phase 4 — Tool plane: model-invoked read-only perception tools

**Scope.**
- Expose allow-listed **read-only** tools (search memory, retrieve associations,
  inspect state). On a function call, the server executes via the existing tool
  registry, adds a `function_call_output` item, and re-issues `response.create`.
- **Record execution, not just intent.** Keep `ToolRequested` as the request record;
  add result/observability types (permission decision, status, result summary, error,
  timing, the returning event) linked by `call_id`. Do not overload `auto_executed`
  as execution evidence.

**Verify (automated).** Function-call → permission decision → registry execution →
`function_call_output` returned; a non-allow-listed tool proven to stay **unexecuted
and recorded as denied**.

**Human testing (required).** Ask something requiring memory search; confirm the
model calls the tool and uses the result in its spoken reply.

**Docs.** `Experiment.LiveToolPerception`; update `Architecture.ToolSystem` and
`Architecture.StateAndObservability`; decision-log entry (read-only tools +
permission/result recording); diary entry.

---

## Phase 5 — Live memory extraction + presence / interruption refinement

**Scope.** Lightweight extraction over completed **trusted** turns (reuse the
sleep/memory proposers) feeding the existing review/consolidation path. Refine
interruption representation and end-to-end / per-stage latency reporting for presence
research (including the latency-measurement gap carried forward from Phase 2).

**Verify (automated).** Extraction tests over trusted turns; latency measurements
recorded.

**Human testing (required).** Presence evaluation against the
`Concept.RealtimePresence` open questions; record latency observations.

**Docs.** Experiment doc + report; refresh `ResearchQuestions.Audio.md` (injection
relevance, ASR-vs-model transcript divergence); update `Concept.RealtimeAudio.md` /
cross-link `Concept.RealtimePresence`; diary entry.

---

## Launcher / Operator Surface

Today `scripts/qsf.ps1` mainly launches `qsf_app` experiments, the memory browser,
the UI, and the workbench, plus the Phase-2 realtime preview path (start
`qsf_realtime_server`, start/open the browser UI, apply non-secret QSF defaults, and
verify required secrets without printing them).

As realtime voice conversation becomes the primary mode, the launcher should grow a
first-class operator command rather than treating it as only `app -Experiment <name>`.
Phase 3 extends the preview path to also start the authoritative sideband. The exact
first-class command name is decided when the server and UI entry point stabilize; the
intended shape is:

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
- **Phase 3 gate:** trusted sideband exchanges land in the shared continuity root and
  are sleep-eligible while Phase-2 diagnostic exchanges are not; the injection-payload
  builder matrix (small/none/capped) is green; no credential leaks through the sideband.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience (done), cross-session continuity, model-invoked tool use, and presence.

## Remaining Checks Before Each Phase

- **Phase 1 (complete):** the file-level move/stay split, the `resume.rs` split, the
  hybrid `qsf_app::session` facade, and the dependency-leanness check shipped (`45ed9cd`).
- **Phase 2 (complete):** the four open questions were resolved (overlap policy B,
  per-crate UI, separate diagnostic store + source/trust marker, server-side SDP exchange
  superseding the ephemeral token) and human-tested 2026-06-09; provider drift recorded.
- **Phase 3 (active):** D1 (retrieval/context-assembly/protocol/translator extraction crate
  boundary), D2 (trusted-exchange continuity-promotion path + promotion preconditions), and D5
  (`create_response = false` manual-response default) were **confirmed 2026-06-10**; D6
  (sideband gap/degraded semantics) was added by the same review. Still to do before/at the
  dependent steps: verify D3 (the sideband attach endpoint/auth) against the live provider and
  record drift; measure the D5 manual-vs-auto latency parity; keep policy B (D4) unless real
  overlap is observed.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream — Phase 3 is the first reality check through the **authoritative**
  sideband (vs. the Phase-2 browser relay).

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (the Phase-1
lean-session-crate / `EventType`-split and the Phase-2 realtime-server, rendezvous,
server-side-SDP, diagnostic-relay, and reducer-overlap entries have landed; Phase 3 adds
the authoritative-sideband, retrieval/protocol-extraction, trusted-continuity-promotion,
and confirmed-sideband-endpoint entries), `EngineeringDiary.md` (one entry per logical
application change), `README.md` and launcher documentation (as phases land), refreshes to
`Architecture.RealtimeSessionServer.md`, `Architecture.AudioLoop.md`,
`Architecture.ToolSystem`, `Architecture.MemorySystem`, `Architecture.StateAndObservability`,
`ResearchQuestions.Audio.md`, `Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence`
cross-link), and one `Experiment.*` doc per live phase.
