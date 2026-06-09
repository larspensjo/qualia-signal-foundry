# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Active implementation plan. Phase 0 (decisions & contracts) and Phase 1 (extract
`qsf_session`) are **complete and accepted**; **Phase 2 is the active phase**,
expanded below into an actionable build. Phases 3–5 remain intentionally
high-level until reached.

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
| 1 | Extract `qsf_session` crate (pure refactor) — complete | Yes | No |
| 2 | Thin media plane — live browser voice — implemented, human-tested 2026-06-09 | Yes | ✅ |
| 3 | Authoritative sideband + memory injection | Yes | **Yes** |
| 4 | Model-invoked read-only perception tools | Yes | **Yes** |
| 5 | Live memory extraction + presence refinement | Yes | **Yes** |

---

## Phase 0 — Decisions & contracts (no code) — completed, accepted 2026-06-09

Lock-in pass, no implementation. The provider-event → QSF-event mapping contract is
recorded in [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md)
and `docs/DecisionLog.md`. Accepted decisions that **constrain Phase 2 and later**:

- `qsf_realtime_server` owns live realtime side effects; `qsf_browser_server` stays a
  read-only inspection server depending only on `qsf_memory`.
- The browser owns the WebRTC media plane. The QSF server owns the **server-side SDP
  rendezvous** (it holds the `OPENAI_API_KEY` and performs the SDP exchange — **this
  supersedes the original ephemeral-token decision; see Phase 2 decision 4 and
  `DecisionLog.md`**) and the `{ qsf_session_id ↔ provider call_id }` binding.
  Media (audio RTP) flows directly browser↔OpenAI; only signaling is proxied.
- Phase-2 browser-relayed provider events are **untrusted, diagnostic-only**, and
  excluded from sleep and continuity. The Phase-3 server sideband is the
  authoritative source for trusted live exchanges.
- Phase-2 session defaults: `gpt-realtime-2`, voice `marin`, `reasoning_effort =
  medium`, `output_modalities = ["audio"]`, and provider `server_vad` with automatic
  response creation and interruption enabled.
- *(Superseded — under the server-side SDP flow no browser client secret exists; see
  Phase 2 decision 4.)* The `call_id` binding is active-call scoped, invalidated on
  stop/error/expiry, and retained only for a short cleanup grace for diagnostics.
- **Mapping contract (Phase-2 gate):** exchange boundary in speech-to-speech mode is
  a paired user-utterance → assistant-response turn keyed by provider `item_id` /
  `response_id`; `ExchangeCompleted` carries an explicit `exchange_index` so a
  late/duplicate completion cannot close the wrong exchange; the reducer must handle
  out-of-order transcript completion, duplicate events, interruption before
  `response.created`, response completion after interruption, and a second user turn
  before the prior response finishes. The **reducer test matrix is required**.
- Realtime voice conversation is the long-term primary QSF operating mode. Phase
  experiment docs validate the path; they do not define the final operator surface.

---

## Phase 1 — Extract `qsf_session` crate (pure refactor) — completed (commit `45ed9cd`)

A lean `crates/qsf_session` crate now holds the pure session reducer, state, event,
persistence, and continuity contracts. What shipped and the constraints it leaves
for later phases:

- **Lean dependency graph.** `qsf_session` depends only on `anyhow`, `qsf_memory`,
  `serde`, `serde_json`, `tempfile`, `time`, and `uuid` — none of `cpal`,
  `openai_provider_kit`, `reqwest`, `tokio`, `tokio-tungstenite`, `hound`, `base64`,
  or `engine_logging`. The future `qsf_realtime_server` can depend on it without the
  heavy graph.
- **What moved:** the reducer/state/event contracts (`LiveSessionEvent`,
  `LiveSessionState` and friends, `reduce_live_session`, the pure `reduce_session`
  family), `Exchange` plus `ProviderEventRecord` / `ProviderEventKind`, persistence
  (`persist_session_state` / `load_session_state`), the continuity manifest,
  continuation, sleep records, the `context` value types **with their pure methods**,
  `ContentHash`, and the `ToolCategory` / `ToolSideEffectLevel` enums. `resume.rs`
  was **split**: the pure loader/`classify_resume_mode`/`ResumeInputs` moved, while
  schema-upgrade logging and env access stayed in `qsf_app`.
- **What stayed in `qsf_app` (effectful edge):** the run-log `EventType` taxonomy and
  the `EventRecord` writer (only the per-`Exchange` provider records moved — this
  refined the original "owns the `EventType` contract" wording); the effectful
  `runtime` functions, `live_memory`, `ageing`; `assemble_context`, the prompt
  algorithms, and the `From<&RetrievedMemory> for ContextFragment` conversion; the
  two `from_env` constructors, converted to free functions.
- **Compatibility preserved.** `qsf_app` re-exports the moved items through a hybrid
  (non-glob) `crate::session` facade, so the ~21 dependent call sites changed
  imports only. The persisted `session-state.json` / `continuity-manifest.json`
  schemas are byte-compatible, guarded by a golden/fixture test, so the read-only
  `qsf_browser_server` `session_context.rs` parser still reads state untouched.
- **Sanctioned reducer change landed.** `LiveSessionEvent::ExchangeCompleted` gained
  `exchange_index`; the reducer finalizes only the matching active exchange and a
  mismatched index is a no-op (unit-tested in `live_state.rs`).
- **Carry-forward constraint into Phase 2 (important).** `LiveSessionState` still
  holds a **single** `active_exchange` / `active_response`. This is adequate for the
  one-shot bridge but **insufficient for overlapping full-duplex events**; Phase 2
  must resolve the overlap policy and satisfy the required mapping-contract test
  matrix (resolved — see Phase 2 decision 1).
- **Docs landed:** `docs/DecisionLog.md` records the shipped crate boundary and the
  `EventType`/provider-event-record split; an `EngineeringDiary.md` entry covers the
  refactor.

---

## Phase 2 — Thin media plane: live browser voice — implemented, human-tested 2026-06-09  *(first time you can talk)*

**Status.** Implemented and human-tested 2026-06-09: a live, full-duplex spoken
browser conversation works end-to-end via `gpt-realtime-2`, with barge-in, and
diagnostic exchanges persisted outside the shared continuity root. It introduces the
`crates/qsf_realtime_server` axum crate and a browser WebRTC client. Persisted
exchanges this phase are **untrusted, diagnostic-only**.

**First live verification (2026-06-09) — drift recorded.** Three defects surfaced on
the first human test and were fixed: the Vite dev proxy needed `ws: true` to relay the
events WebSocket; the SDP handler now surfaces the provider error body instead of
swallowing it; and OpenAI `/v1/realtime/calls` rejected `session.reasoning_effort`
(`unknown_parameter`), so `reasoning_effort` is kept as QSF session metadata but no
longer forwarded (see `DecisionLog.md`). `gpt-realtime-2` / `marin` / `["audio"]` /
`server_vad` were accepted by the provider. Remaining open: end-to-end / per-stage
latency measurement for presence research (carried to Phase 5).

**Outcome.** A human can open a browser, speak, and hear a streamed spoken reply via
`gpt-realtime-2`. A new `qsf_realtime_server` performs the **server-side SDP exchange**
using the server-held `OPENAI_API_KEY` (no credential ever leaves the server), capturing
and storing the provider `call_id` first-hand, and receives browser-relayed provider events that
it translates into `LiveSessionEvent`s, reduces via `qsf_session`, and persists as
diagnostic-only exchanges excluded from sleep and continuity. Media (audio RTP) flows
directly browser↔OpenAI; only signaling is proxied, so no media latency is added. The
server depends on `qsf_session` (not the full `qsf_app` runtime), reusing the
established async `reqwest` client pattern (already used by
`crates/qsf_app/src/models/openai_tool_client.rs`).

**Resolved design decisions (confirmed 2026-06-09).** The four questions below were
surfaced (per `Agents.md`) and decided before coding; each records the chosen path and
the rationale. The reducer test matrix remains the Phase-2 gate regardless. In brief:
(1) reducer policy **B**; (2) UI in a new per-crate `ui/`; (3) separate diagnostic store
**plus** an explicit source/trust marker; (4) **server-side SDP exchange, no ephemeral
token** (supersedes the Phase-0 token decision, recorded as a reversal in `DecisionLog.md`).

1. **Reducer overlap policy → (B) single `active_exchange`, finalize-prior.** A new user
   turn arriving before the prior response completes **finalizes the prior exchange first**
   (status `Interrupted` if its response was still streaming, else `Completed`), then opens
   the new one; late `response.done` / transcript events for an already-finalized exchange
   are no-ops (the same index guard `ExchangeCompleted` already uses). Chosen over true
   concurrency (A) because Phase-2 exchanges are diagnostic-only, the cost of an occasional
   early-finalize is negligible, and `server_vad` + interruption means a fresh user turn
   cancels the prior response in practice. True concurrency is deferred to Phase 3, where
   the authoritative sideband can revisit it if real overlap is observed.
   - **Dedupe split:** dedupe/order **by provider `event_id` is the server translator's
     job** at the `WS` boundary, not the reducer's — `LiveSessionEvent` carries no event
     id. Pure reducer tests stay focused on identity guards and no-op-on-stale behavior.

2. **Browser UI home → new `crates/qsf_realtime_server/ui/`.** A dedicated Vite + TS +
   Biome + Vitest project mirroring the browser-server setup, dev proxy pointing `/api` at
   the realtime server's port. Respects the dedicated-crate boundary (the read-only
   browser server's UI stays decoupled from live concerns), at the cost of duplicated
   tooling config. Build wiring: check in its own `package-lock.json`; use a dev port
   **distinct from the memory browser UI's `5173`** to avoid collision; the launcher
   selects this `ui/` directory and runs the `npm run check` / `npm run fmt` gate there.

3. **Diagnostic persistence → separate run-scoped store *and* an explicit source/trust
   marker.** Persist Phase-2 diagnostic exchanges to a **run/diagnostic-scoped store owned
   by the realtime server** (structural exclusion: they never enter the shared continuity
   root, so no sleep-side filter has to be honored), **and** stamp a `source`/`trust` field
   on the diagnostic records so they are self-describing and forward-compatible with Phase
   3 (where trusted + untrusted exchanges share one store). This **keeps** the accepted
   trust/source-marker decision in `DecisionLog.md` rather than deferring it. Two
   implementation prerequisites surfaced by review:
   - **Promotion-to-durable write:** `persist_session_state` serializes `SessionState` and
     **skips** `LiveSessionState.completed_exchanges` (guarded by
     `persist_keeps_completed_exchanges_in_memory_only`). Finalized diagnostic exchanges
     must be **written into the persisted artifact** (the diagnostic store's durable
     `SessionState.exchanges` via the `ExchangeRecorded` path, or an explicit diagnostic
     artifact) **before** `persist_session_state` — with a regression test asserting the
     persisted file actually contains the exchange.
   - **Identity model:** the mapping contract needs `event_id`, `item_id`,
     `previous_item_id`, `response_id`, and `call_id`, but `ProviderEventRecord` currently
     carries only `exchange_index` / `provider_id` (a name) / `response_id` / text / status
     / audio. Extend the diagnostic/provider records (or a separate relay-event artifact
     linked by `exchange_index`) to persist the missing provider identity fields.

4. **OpenAI WebRTC initialization → server-side SDP exchange with the API key (no
   ephemeral token).** This **supersedes** the Phase-0 ephemeral-token decision (recorded
   as a reversal in `DecisionLog.md`). The browser sends its SDP **offer** to the server;
   the **server** POSTs it to OpenAI's realtime calls endpoint authenticated with the
   server-held `OPENAI_API_KEY`, reads the provider `call_id` **first-hand** (authoritative
   — not laundered through the untrusted browser), stores the binding, and returns the SDP
   **answer**. Media (audio RTP) still flows directly browser↔OpenAI, so no media latency
   is added; only signaling is proxied. No credential of any kind leaves the server. This
   is the only flow consistent with the trust boundary (browser untrusted) and the Phase-3
   sideband (which attaches to the server-captured `call_id`); it also removes the
   ephemeral-secret lifecycle/store entirely.
   - **Implementation-time verification (was open question 4):** confirm the exact
     endpoint, headers, `call_id` location (working assumption: `POST /v1/realtime/calls`
     with `Content-Type: application/sdp`, `call_id` via the `Location` header;
     `wss://api.openai.com/v1/realtime?call_id=…` reserved for the Phase-3 sideband), and
     the session-config payload schema against the **current** OpenAI realtime API; record
     any drift explicitly before changing accepted defaults. If live verification shows the
     server-side path cannot supply session config or return `call_id` to the server, fall
     back to the ephemeral-token flow **and** explicitly accept a browser-reported
     (untrusted) `call_id` until the Phase-3 sideband validates it.

**Scope — Server (`qsf_realtime_server`, axum):**
- New crate mirroring the `qsf_browser_server` shape: thin `main.rs` (`#[tokio::main]`
  → `run()`), thin `lib.rs` (`run()` → `server::serve`), `cli.rs` (host/port/state-dir
  args), and `state.rs` (`AppState` holding the env-sourced API key, an async
  `reqwest::Client`, the `{ qsf_session_id ↔ call_id }` binding store behind `Arc`, and
  the diagnostic store path). Deps: `axum`, `tokio`, `reqwest` (async — `json`,
  `rustls-tls`; *not* the `blocking` feature qsf_app uses), `serde`/`serde_json`,
  `thiserror`, `time`, `uuid`, `engine_logging`, and `qsf_session`. Keep `main`/`lib`
  thin (per `Agents.md`); the server is the effectful edge, the reducer stays pure.
- Routes (a `realtime/` module):
  - `POST /api/realtime/session` — allocate a `qsf_session_id` and return the accepted
    default session config (`gpt-realtime-2`, `marin`, `medium`, `["audio"]`, `server_vad`
    auto-create + interrupt) plus the `qsf_session_id`. **No credential is minted or
    returned** (server-side SDP flow — decision 4); this route does not call OpenAI.
  - `POST /api/realtime/sdp` — accept the browser SDP **offer** + `qsf_session_id`; forward
    it to OpenAI's realtime calls endpoint authenticated with the **server-held
    `OPENAI_API_KEY`** (decision 4 — no ephemeral secret); read the provider `call_id`
    **first-hand** (`Location` header); store the validated `{ qsf_session_id ↔ call_id }`
    binding (active-call scoped, invalidated on stop/error/expiry, short cleanup grace);
    return the SDP **answer**. No credential leaves the server.
  - `WS /api/realtime/events` — receive browser-relayed provider events as a typed,
    **untrusted** relay envelope; schema-validate, enforce a max payload size, and
    dedupe/order by provider `event_id`; translate to `LiveSessionEvent`s per the
    mapping contract; reduce via `qsf_session`; persist diagnostic exchanges (decision 3:
    run-scoped diagnostic store, with the source/trust marker and provider identity
    fields). Reject malformed/oversized payloads.
  - `POST /api/realtime/stop` (or a WS close) — invalidate the binding and finalize any
    open diagnostic exchange.
- **Local exposure boundary:** default-bind `127.0.0.1`; the SDP route spends the
  server-held `OPENAI_API_KEY`, so refuse (or loudly warn) on a non-loopback bind, assume a
  same-origin/local UI, and keep CORS closed by default. Launcher/doctor checks verify
  `OPENAI_API_KEY` is present **without printing it**.
- **Server-owned diagnostic writer:** since the crate deliberately does not depend on
  `qsf_app` (where the event/trace writers live), add a small server-owned diagnostic
  artifact writer — call-binding events, redaction (no-secret) evidence, persisted
  diagnostic exchanges, and latency observations — with tests, before the relay step.

**Scope — Reducer hardening (in `qsf_session`, pure):**
- Implement overlap policy **(B)** (decision 1: finalize-prior on a new user turn),
  keeping `reduce_live_session` pure and unit-tested in `qsf_session`; add the provider
  identity fields (decision 3) to the persisted records. Event-`id` dedupe lives in the
  server translator, not the reducer.
- Add/extend unit tests covering the **required mapping-contract matrix**: transcript
  completion after response start; duplicate provider events; interruption before
  `response.created`; response completion after interruption; a second user turn before
  the prior response finishes; out-of-order lifecycle events. **This matrix is the
  Phase-2 gate.**

**Scope — Browser (TS in the chosen `ui/`):**
- New WebRTC client: fetch the `qsf_session_id` + session config from
  `POST /api/realtime/session` (no credential is returned); create `RTCPeerConnection`; add
  the mic track (`getUserMedia`); create the provider `oai-events` data channel; on
  `ontrack`, play the remote audio; create the SDP **offer**, send it via
  `POST /api/realtime/sdp`, and apply the returned **answer**. Media flows directly
  browser↔OpenAI; only signaling goes through the server.
- Relay observed data-channel provider events to `WS /api/realtime/events` as the typed
  relay envelope (diagnostic only).
- Minimal UI: start/stop, live transcript, and a listening/thinking/speaking status
  driven by provider events (mirrors `RuntimePhase`). Provider VAD + barge-in are
  server-config defaults; the UI only reflects state.
- TS unit tests (Vitest) for the provider-event → relay-envelope mapping.

**Incremental, independently reviewable steps** (each ends green; commit per step):
1. **Reducer hardening + mapping-contract matrix** in `qsf_session` (pure, no server
   yet) — reviewable in isolation, de-risks the central decision. Green: `cargo test`.
2. **Scaffold `crates/qsf_realtime_server`** (thin `main`/`lib`/`cli`/`state`, a
   `/health` route, an empty `realtime` module), wired into the workspace `crates/*`
   glob; add async `reqwest`. Green: `cargo build` / `clippy` / `fmt`, plus a `/health`
   test.
3. **`POST /api/realtime/session`** session allocation + default session config (no
   provider call); assert **no credential of any kind** is in the response and the default
   session config is correct. Green.
4. **`POST /api/realtime/sdp`** SDP proxy against a **mocked** OpenAI endpoint (inject the
   base URL via `AppState` so tests never hit the network), authenticated with the
   server-held API key; capture and store the `call_id` binding; assert the API key is
   absent from the response/log; unit-test the binding lifecycle (invalidate on
   stop/expiry). Green.
5. **`WS /api/realtime/events`** relay: typed envelope, schema/size validation +
   dedupe; event-translation → reduced → persisted diagnostic `Exchange`; reject
   malformed/oversized payloads (tested). Green.
6. **Browser WebRTC client + minimal UI** in the chosen `ui/` home; Vitest mapping
   tests; `npm run check` then `npm run fmt`. (No automated browser e2e — covered by
   human testing.)
7. **Launcher preview path + lint/format gates + docs** (below).

**Acceptance criteria:**
- `cargo build`, full `cargo test`, `cargo clippy --all-targets -- -D warnings` clean,
  `cargo fmt` applied; UI `npm run check` then `npm run fmt` green.
- `qsf_realtime_server` depends on `qsf_session`, not the full `qsf_app` runtime;
  `OPENAI_API_KEY` is read only server-side and proven absent from every response and
  log in tests; **no credential of any kind reaches the browser**.
- The session route returns a `qsf_session_id` + non-secret session config (no
  credential); the SDP proxy (server API key, mocked provider) captures and stores the
  `{ qsf_session_id ↔ call_id }` binding; the relay endpoint rejects malformed/oversized
  payloads and persists diagnostic exchanges carrying the source/trust marker.
- The reducer mapping-contract matrix is green (Phase-2 gate).
- Diagnostic exchanges are observable in artifacts but written **outside** the shared
  continuity root, so sleep/continuity cannot consume them — verified by a test
  asserting the shared root is untouched.
- **Defaults exercise the new path:** the server's default config is the accepted
  `gpt-realtime-2` / `marin` / `medium` / `["audio"]` / `server_vad` set, and the
  default run mode persists diagnostic exchanges.

**Verification guidance** (fits a new live-service + UI slice):
- *Automated (Rust):* session route (no credential leaks); SDP-proxy `call_id` capture +
  binding lifecycle (server API key, mocked OpenAI); relay validation + event-translation
  → persisted diagnostic `Exchange` (with source/trust marker); the reducer overlap /
  out-of-order matrix.
- *Automated (TS):* Vitest provider-event → relay-envelope mapping; `npm run check`.
- **Human testing (required):** open the browser, start a session, speak and hear a
  streamed reply, interrupt mid-reply (barge-in); confirm the listening/thinking/
  speaking status tracks the conversation; confirm diagnostic exchanges appear in
  artifacts; inspect browser devtools/network to confirm **no credential reaches the
  browser** — neither the `OPENAI_API_KEY` nor any ephemeral token (only SDP and the
  `qsf_session_id` cross the wire). Record end-to-end and per-stage latency
  observations for presence research (carried to Phase 5).
- *Provider reality check:* confirm the mapping contract holds against the actual live
  event stream (first real check); record any API drift per decision 4's
  implementation-time verification before changing accepted defaults.

**Docs (per `ProjectWorkflow.md`):**
- **Refresh** the already-existing `Architecture.RealtimeSessionServer.md`: move its
  Implementation Status bands from "not yet implemented" to what shipped (crate, token
  route, SDP proxy + binding store, diagnostic relay WS, reducer hardening), and update
  the `Last reviewed:` date.
- `Experiment.RealtimeBrowserVoiceMVP` as the validation record.
- Refresh `Architecture.AudioLoop.md` Implementation Status; note the realtime
  diagnostic/untrusted-exchange surface in `Architecture.StateAndObservability`.
- DecisionLog entries: realtime-server crate exists; browser-owns-media +
  server-owns-rendezvous; **the server-side SDP exchange flow** (its reversal of the
  ephemeral-token decision is already recorded; confirm on landing); diagnostic-only relay
  persisted outside the continuity root **with the source/trust marker**; and the chosen
  reducer overlap policy (B).
- One `EngineeringDiary.md` entry (follow the diary's "How to use" header).
- README "What works today" + launcher notes for the preview path and the intended
  future first-class realtime mode.

---

## Phase 3 — Control/context plane: authoritative sideband + memory injection  *(the "mixture" becomes real)*

**Scope.**
- Extract reusable **protocol helpers** (request builders, event parsing) from
  `voice_session_provider` (small extraction step), then build a **new async sideband
  adapter** (long-lived, concurrent read/write, cancellation/shutdown) that connects
  to the stored `call_id`. Reuse the helpers, **not** the one-shot runner.
- The sideband becomes the **authoritative** event source; its exchanges are trusted
  and sleep/continuity-eligible. Browser relay reverts to UI-only diagnostics.
- Per session start and per user turn, retrieve relevant memory (existing
  association-weighted retrieval) and inject a **small** working-memory packet via
  `conversation.item.create`, plus `session.update` for identity/tone. Relevance over
  volume — never a full memory dump.

**Verify (automated).** Sideband attaches to a stored `call_id` (mocked); given a
memory store + transcript, the server emits the expected (small) injection payloads;
trusted exchanges are sleep-eligible while Phase-2 diagnostic ones are not.

**Human testing (required).** Reference something across turns and across sessions;
confirm continuity surfaces in the spoken conversation.

**Docs.** `Experiment.LiveContextInjection`; update
`Architecture.RealtimeSessionServer.md` and `Architecture.MemorySystem`; decision-log
entry (authoritative sideband); diary entry.

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
research.

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
the UI, and the workbench. That is appropriate for the current state of the repo.

As realtime voice conversation becomes runnable, the launcher should grow a
first-class operator mode for it rather than treating it as only
`app -Experiment <name>`. Phase 2 should add at least a **preview path** (start
`qsf_realtime_server`, start/open the browser UI, apply non-secret QSF defaults, and
verify required secrets without printing them). The exact first-class command name is
decided when the server and UI entry point exist; the intended shape is:

```text
qsf.ps1 <realtime-conversation-mode>
  -> start qsf_realtime_server
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
  `SessionState`) plus normalized-artifact parity (volatile fields scrubbed), not
  byte-for-byte.
- **Phase 2 gate:** the reducer overlap / out-of-order test matrix.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience, cross-session continuity, model-invoked tool use, and presence.

## Remaining Checks Before Each Phase

- **Phase 1 (complete):** the file-level move/stay split, the `resume.rs` split, the
  hybrid `qsf_app::session` facade, the dependency-leanness check, and the
  `EventType` / `ContextAssembly` / `ContentHash` boundary questions were all resolved
  and shipped (commit `45ed9cd`). The key residue for Phase 2 is the single
  `active_exchange` reducer (Phase 2 decision 1).
- **Phase 2:** open questions 1–4 are **resolved** (see Phase 2 "Resolved design
  decisions": overlap policy B, per-crate UI, separate diagnostic store + source/trust
  marker, server-side SDP exchange superseding the ephemeral token). The remaining
  implementation-time check is verifying the server-side `/v1/realtime/calls` endpoint
  shape, `call_id` location, session-config schema, and the accepted model/voice/VAD
  defaults against the live provider — recording any drift explicitly before changing
  them.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream observed once live (Phase 2 is the first reality check).

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (accepted
decisions as each lands — the Phase-1 lean-session-crate and `EventType`/
provider-event-record split entries have landed; Phase 2 adds the realtime-server,
rendezvous, server-side-SDP-flow, diagnostic-relay, and reducer-overlap entries),
`EngineeringDiary.md` (one entry per logical application change), `README.md` and
launcher documentation (as phases land), a **refresh** of the existing (Phase-0
sketch) `Architecture.RealtimeSessionServer.md`, refreshes to
`Architecture.AudioLoop.md` / `Architecture.ToolSystem` / `Architecture.MemorySystem`
/ `Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.