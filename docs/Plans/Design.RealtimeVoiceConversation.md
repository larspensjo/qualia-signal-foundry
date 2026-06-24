# Design: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Candidate; Phase 0 decisions accepted on 2026-06-09.

> This design narrows the broader vision in `docs/Concepts/Concept.RealtimeAudio.md`
> (the "three planes" idea) into a committed, phased implementation direction:
> full voice input and voice output enabling live, interruptible spoken
> conversation, where `gpt-realtime` owns the *voice* and Qualia Signal Foundry
> owns the *mind* (memory, continuity, context, tools, observability).
>
> Revised 2026-06-09 after `docs/Reviews/Review.RealtimeVoiceConversationDesign.2026-06-08.md`.
> Key changes from that review: the QSF server owns the WebRTC SDP rendezvous and
> stores the provider `call_id` from Phase 2; browser-relayed events are
> diagnostic-only until the server sideband is authoritative; a dedicated
> `qsf_realtime_server` crate owns live side effects; the sideband is a new async
> adapter built from reusable protocol helpers (not the one-shot client); a
> provider-event mapping contract precedes Phase 2; and tool execution records
> permission/result, not just `auto_executed`.
>
> Further revised 2026-06-09 (Phase-2 plan review): WebRTC initialization uses a
> **server-side SDP exchange with the `OPENAI_API_KEY`**; the ephemeral-token flow is
> dropped (see the `DecisionLog.md` reversal).

## Summary

Today QSF can transcribe microphone/WAV input into the runtime, and a one-shot
`realtime-voice-session` experiment maps a single provider turn into QSF events
and a persisted `Exchange`. But the system **cannot actually hold a live spoken
conversation**: speech output is metadata-only (no audible audio), the realtime
session is single-turn, nothing is injected back into the session, and
provider tool-call requests are recorded but never executed.

This design adds a **live, browser-based, full-duplex voice conversation** built
on the **three-plane** separation:

```text
gpt-realtime = realtime voice / persona surface (media plane)
QSF backend  = memory, associations, tools, world model, continuity, logging
```

This is the long-term primary operating mode the project is aiming at. The phased
experiments are the way QSF validates and grows that mode, not the final category
for the user-facing runtime.

The realtime model is treated as the **live conversational surface** of a larger
system, never as the whole simulated mind. Continuity comes from QSF's memory
system; the model provides low-latency voice, turn-taking, and interruption.

The live realtime session is an **effectful adapter at the edge**. It never
mutates state directly. Every fact it produces (transcripts, response lifecycle,
interruptions, tool requests) feeds back as a `LiveSessionEvent` *action* into the
existing **pure session reducer**, preserving the project's
`input -> action -> reducer -> state -> render` rule.

## Goals

- A human can open a browser, speak, and hear a spoken reply in real time.
- Provider-driven VAD turn-taking and barge-in (interrupt the reply by speaking).
- The live conversation stays inside the QSF platform: each turn is recorded as an
  observable `Exchange`, persisted to the shared session-continuity root, and is
  eligible for sleep consolidation — once it comes from a **trusted** source.
- QSF steers the session by injecting only-what-is-relevant working-memory packets
  and behavioral instructions (control plane).
- The model can call **read-only** perception tools that QSF executes and returns,
  with the permission decision and result observable.
- No invisible magic: the SDP rendezvous, the provider `call_id` binding, injected
  context, tool calls, and interruptions are all observable as events/traces.

## Non-Goals

- Not a polished voice product; this is a research instrument.
- No write-capable or outbound-action tools exposed to the model (read-only only).
- No multi-user / multi-tenant / auth concerns — single-user local research tool.
- The realtime model is **not** the long-term memory or identity store.
- Native (non-browser) realtime audio playback in Rust is explicitly out of scope
  (the browser owns the media plane).

## Current State (what this builds on)

| Plane | Vision | Current state |
|-------|--------|---------------|
| Media (live voice) | `gpt-realtime` duplex, VAD, barge-in, audible | Single-turn; **no live audio playback**; no continuous loop |
| Control/context | `session.update` + `conversation.item.create` mid-session | **Missing entirely** |
| Memory | event stream -> extract -> consolidate | Partial: exchanges persist, sleep consolidates; no live extraction |
| Tool | model requests, app executes, feeds back | Partial: requests recorded with `auto_executed: false`, never executed |

Reusable assets (and their real shape, per the review):

- An OpenAI realtime **WebSocket protocol implementation** exists
  ([voice_session_provider.rs](../../crates/qsf_app/src/audio/voice_session_provider.rs)):
  request builders, `session.update`, the `session.created` handshake, and event
  parsing. **However it is a synchronous one-shot runner** — it `block_on`s a
  single turn and explicitly refuses to run inside an existing Tokio runtime
  ([voice_session_provider.rs:399-413](../../crates/qsf_app/src/audio/voice_session_provider.rs#L399-L413)).
  It is reusable as **protocol helpers**, not as a long-lived sideband adapter.
- The **session reducer models voice turns**
  (`LiveSessionEvent`, `Exchange`, `apply_live_session_event`), but
  `LiveSessionState` holds only a **single** `active_exchange` / `active_response`
  ([live_state.rs:81-99](../../crates/qsf_app/src/session/live_state.rs#L81-L99)).
  Adequate for the one-shot bridge; insufficient for overlapping, multi-ID
  full-duplex events without hardening (see *Provider-Event Mapping*).
- One-shot bridging logic in
  [realtime_voice_session.rs](../../crates/qsf_app/src/experiments/realtime_voice_session.rs)
  (`bridge_realtime_session_into_shared_state`) is the template for streaming, but
  uses `SystemTime::now()` for several timestamps — so artifacts are not
  byte-stable.
- The browser server is **axum + tokio**
  ([qsf_browser_server](../../crates/qsf_browser_server)) but is explicitly
  **read-only** and depends only on `qsf_memory`, never `qsf_app`
  ([lib.rs:1-3](../../crates/qsf_browser_server/src/lib.rs#L1-L3)). The UI is
  **vanilla TS + Vite** ([ui/src](../../crates/qsf_browser_server/ui/src)).

## Architecture & Data Flow

```text
┌───────────────────────── BROWSER (Vite UI, new TS) ──────────────────────────┐
│  mic  ──getUserMedia──►  RTCPeerConnection ══ WebRTC media (audio) ══► gpt-realtime
│  speaker ◄══ remote audio track ◄═══════════════════════════════════  (VAD,   │
│                                                                  barge-in)     │
│      ▲ SDP offer/answer relayed THROUGH the QSF server (signaling only)        │
└──────┼─────────────────────────────────────────────────────────────────────────┘
       │ (1) POST /api/realtime/session   → allocate qsf_session_id + session config (no secret)
       │ (2) POST /api/realtime/sdp       → server proxies SDP to OpenAI,
       │                                     captures + stores provider call_id
       │ (3) WS  /api/realtime/events     → browser relays observed events (DIAGNOSTIC)
       ▼
┌──────────────────── RUST SERVER (qsf_realtime_server, axum) ──────────────────┐
│  • holds OPENAI_API_KEY — NO credential ever reaches the browser               │
│  • proxies SDP, stores {qsf_session_id ↔ provider call_id} binding             │
│         ▼ translate                                                            │
│   LiveSessionEvent ──► pure session reducer (qsf_session) ──► SessionState     │
│         ▼                                                                      │
│   persist Exchange + continuity + event/trace logs (existing machinery)        │
│   Phase 2: source = browser relay  → marked UNTRUSTED, excluded from sleep     │
│                                                                                │
│  [Phase 3+] server-side SIDEBAND WS ══► same call (wss://…/realtime?call_id=…) │
│     new async adapter (protocol helpers from voice_session_provider)           │
│     → AUTHORITATIVE event source; session.update + conversation.item.create     │
│  [Phase 4+] tool-call → QSF tool registry executes (read-only) → result back   │
└────────────────────────────────────────────────────────────────────────────────┘
```

Plane → component homes:

| Plane | Component | Home | First active |
|-------|-----------|------|--------------|
| Media | WebRTC client, mic/speaker, SDP offer + relay | `ui/src/` (new TS) + `qsf_realtime_server` routes | Phase 2 |
| Memory (record) | event relay → reducer → persist `Exchange` | `qsf_realtime_server` + `qsf_session` | Phase 2 (diagnostic) / Phase 3 (trusted) |
| Control/context | sideband WS → `session.update` + memory packets | new async sideband adapter | Phase 3 |
| Tool | model tool-call → QSF registry → result back | existing tool registry + new result records | Phase 4 |
| Memory (live extract) | extract candidates from live turns | reuse `qsf_app::sleep`/memory proposers | Phase 5 |

### Mode vs. Experiment Harness

The current repository is experiment-runner centric: `scripts/qsf.ps1 app
-Experiment ...` starts named validation paths in `qsf_app`. That remains useful
for tests, reports, and controlled comparisons.

Realtime voice conversation should eventually be launched as a first-class QSF
mode, not hidden as one more experiment id. The launcher should grow a dedicated
operator path once `qsf_realtime_server` and the browser UI entry point exist. The
experiment docs in this plan are evidence and verification artifacts for building
that mode.

### Phase 0 Accepted Defaults

These decisions are accepted for the first implementation pass and should be
changed only through a new decision-log entry if live provider behavior forces a
revision.

- **Realtime model:** `gpt-realtime-2`.
- **Voice:** `marin`.
- **Reasoning effort:** `medium`.
- **Output modality:** `["audio"]`; the spoken transcript is recorded as
  observability, not as proof that the model heard exactly that text.
- **Turn detection:** provider `server_vad` with automatic response creation and
  automatic response interruption enabled. `semantic_vad` remains an experiment
  candidate after baseline latency and interruption behavior are measured.
- **WebRTC initialization:** *(superseded 2026-06-09 — see the `DecisionLog.md`
  reversal)* the server performs the SDP exchange with the `OPENAI_API_KEY` and
  returns no credential to the browser; there is no browser client secret. The
  `OPENAI_API_KEY` is never sent to the browser.
- **`call_id` binding:** active-call scoped, invalidated on stop/error/expiry, and
  retained only for a short cleanup grace for diagnostics.
- **Lean `qsf_session`:** extract reducer/state/event contracts, `Exchange`,
  persistence DTOs, continuity manifest, and the event-record/`EventType` contract;
  keep `RunContext`, provider clients, memory retrieval, tools, and OpenAI/CPAL
  dependencies outside.
- **Operator surface:** phase experiments remain validation harnesses; the final
  realtime voice conversation path should become a first-class launcher mode.

### Provider Session Identity & Sideband Rendezvous

The browser and the QSF server must attach to the **same** realtime call so the
server can become the authoritative event source and inject context.

- The browser asks the server for a session (`POST /api/realtime/session`), which
  allocates a `qsf_session_id` and returns the non-secret session config. **No
  credential is returned** — the server holds the `OPENAI_API_KEY` and is the only
  party that talks to OpenAI's REST surface (revised 2026-06-09; see the
  `DecisionLog.md` reversal).
- The browser creates its `RTCPeerConnection` and produces an **SDP offer**. It
  sends the offer to the QSF server (`POST /api/realtime/sdp`), which forwards it to
  OpenAI's realtime calls endpoint **authenticated with the server-held
  `OPENAI_API_KEY`** and reads the provider **`call_id`** first-hand from the
  response (`Location` header), returning the SDP answer to the browser. Media
  (audio RTP) still flows **directly** browser↔OpenAI — only signaling is proxied,
  so no media latency is added.
- The server persists a validated `{ qsf_session_id ↔ provider call_id }` binding
  with the accepted active-call lifetime. Phase 3's sideband connects with
  `wss://api.openai.com/v1/realtime?call_id=<stored>` — no second browser protocol
  is needed to discover the call.

### Trust Boundary for Browser-Relayed Events

In Phase 2 the only event source is the browser relay. The browser is **not** an
authoritative source for provider facts, so:

- Relayed events are tagged with an explicit **untrusted-source** marker, schema-
  validated, size-limited, and de-duplicated/ordered by provider event IDs.
- Phase-2 persisted exchanges are **diagnostic-only**: visible in run artifacts and
  the session log, but **excluded from sleep consolidation and continuity
  promotion**. They prove media + UI + reducer wiring, not durable memory.
- Durable, sleep-eligible exchanges begin in Phase 3, sourced from the server-side
  **sideband** (authoritative), not the browser.

### Provider-Event → QSF-Event Mapping (contract, precedes Phase 2)

Full-duplex provider events are multi-ID and can overlap; the current single-
`active_exchange` reducer is too narrow. Before Phase 2 we pin a mapping contract:

- **Exchange boundary** in speech-to-speech mode = a paired user-utterance →
  assistant-response turn, keyed by provider `item_id` / `response_id`.
- Define how `item_id`, `previous_item_id`, `response_id`, `call_id`, and
  `event_id` map onto QSF exchange identity; add an `exchange_index` (or id) to
  `ExchangeCompleted` so completion is unambiguous.
- Define behavior for: input transcription completing **after** a response starts;
  `response.done` arriving **after** an interruption; a new user turn beginning
  before the prior response is done (overlap); duplicate/out-of-order events.
- **Reducer test matrix (required):** out-of-order transcript completion, duplicate
  provider events, interruption before `response.created`, response completion
  after interruption, two user turns before the prior response finishes.

### Crate / Component Ownership

`qsf_browser_server` is, by its own contract, a read-only inspection server. Rather
than overload it with live side effects (SDP proxy, credential handling, reducer
access, sideband control, tool execution), this design introduces a dedicated
**`qsf_realtime_server`** crate for the live runtime. `qsf_browser_server` stays
the read-only memory browser. The realtime server depends on `qsf_session` (Phase
1) plus the specific `qsf_app` capabilities it needs (memory retrieval, sleep
proposers, tool registry, realtime protocol helpers) — surfaced explicitly as that
dependency graph grows across phases.

**Security:** `OPENAI_API_KEY` stays server-side and is the only credential used to
talk to OpenAI; **no credential of any kind is sent to the browser** (it receives
only the SDP answer and its `qsf_session_id`). Raw audio is never logged (existing
`AudioSafetyMarkers` invariant). Listening is explicit (start/stop).

## Phased Plan

Each phase is independently testable and ends in a verifiable state. "Human
testing" marks steps that need external manual verification.

### Phase 0 — Decisions & contracts (no code)

Lock the decisions the review surfaced, so later phases do not churn:

- `qsf_realtime_server` as the live-runtime crate (vs. converting
  `qsf_browser_server`).
- The provider-event → QSF-event mapping contract above.
- The trust boundary: Phase-2 relay is diagnostic-only.

Recorded in the decision log; no implementation.

Phase 0 is accepted as of 2026-06-09. Later phases may still discover provider API
drift during live verification, but should treat that as a scoped revision rather
than silently reopening the architecture.

### Phase 1 — Extract `qsf_session` crate (pure refactor, no behavior change)

Move the reducer, `LiveSessionEvent`/`SessionEvent`, `Exchange`, `SessionState`,
persistence, and continuity manifest into a lean `qsf_session` crate so both the
experiment runner and the realtime server can depend on it without pulling in
`cpal`/OpenAI deps.

- The session module currently reaches into `observability::event_log::EventType`,
  `runtime::run_context`, and `memory`. The accepted lean surface moves only the
  reducer/state/event contracts, `Exchange`, persistence DTOs, continuity manifest,
  and event-record/`EventType` contract. `RunContext`, provider clients, memory
  retrieval, tools, and OpenAI/CPAL dependencies stay outside.
- Apply the `ExchangeCompleted` identity change from the mapping contract here
  (reducer-local, no provider integration yet) with its unit tests.
- `qsf_app` re-exports from `qsf_session` so existing call sites barely change.
- **Verify:** `cargo build` + full `cargo test` green; `cargo clippy --all-targets
  -- -D warnings`. Behavior parity is checked by **diffing normalized artifacts
  with volatile fields (timestamps, UUIDs) scrubbed**, or by deterministic
  reducer/persistence fixtures — *not* byte-for-byte run-dir comparison (the bridge
  uses `SystemTime::now()`). No human testing needed.

### Phase 2 — Thin media plane: live browser voice *(first time you can talk)*

- **Server (`qsf_realtime_server`, axum):** `POST /api/realtime/session` allocates a
  `qsf_session_id` + session config (no credential returned; it does not call
  OpenAI). `POST /api/realtime/sdp` proxies the SDP exchange **authenticated with the
  server-held `OPENAI_API_KEY`** and stores the `{ session ↔ call_id }` binding (see
  *Rendezvous*). `WS /api/realtime/events` receives browser-relayed events.
- **Browser (new TS in `ui/src/`):** fetch session config → `RTCPeerConnection`, send
  SDP offer via the server, attach mic, play remote audio, provider VAD + barge-in.
  Minimal UI: start/stop, live transcript, listening/thinking/speaking status.
  The initial session config uses `gpt-realtime-2`, voice `marin`, medium
  reasoning effort, audio output, and provider `server_vad` with automatic response
  creation and interruption enabled.
- **Server:** translate relayed events → `LiveSessionEvent` (per the mapping
  contract) → reducer (`qsf_session`) → persist exchanges + event/trace logs,
  **marked untrusted / diagnostic-only and excluded from sleep + continuity**.
- **Verify:**
  - *Automated:* session route returns no credential; SDP-proxy (server API key,
    mocked OpenAI) stores call_id; event-translation → persisted-`Exchange` tests
    including the reducer overlap/out-of-order matrix; relayed-event validation
    rejects malformed/oversized payloads; TS event-mapping unit tests.
  - *Human testing (required):* open the browser, speak, hear a reply, interrupt
    mid-reply; confirm diagnostic exchanges appear; inspect network to confirm **no
    credential (API key or token) is ever sent to the browser**.
- **Safety:** explicit start/stop; no raw audio logged; no credential sent to the
  browser; untrusted relay cannot reach durable memory.

### Phase 3 — Control/context plane: authoritative sideband + memory injection *(the "mixture" becomes real)*

- Build a **new async sideband adapter** (long-lived connection, concurrent
  read/write, cancellation/shutdown) that connects to the stored `call_id` and
  **reuses protocol helpers** (request builders, event parsing) extracted from
  `voice_session_provider` — not the one-shot runner. A small extraction step
  factors those helpers out first.
- The sideband becomes the **authoritative** event source; its exchanges are
  trusted and **sleep/continuity-eligible**. Browser relay reverts to UI-only
  diagnostics.
- Per session start and per user turn, QSF retrieves relevant memory (existing
  association-weighted retrieval) and injects a **small** working-memory packet via
  `conversation.item.create`, plus `session.update` for identity/tone. Inject only
  what is currently relevant — never a full memory dump.
- **Verify:** *Automated* — sideband attaches to a stored call_id (mocked);
  given a memory store + transcript, the server emits the expected (small)
  injection payloads; trusted exchanges are sleep-eligible while Phase-2 diagnostic
  ones are not. *Human* — reference something across turns and across sessions;
  confirm continuity surfaces.

### Phase 4 — Tool plane: model-invoked read-only perception tools

- Expose allow-listed **read-only** tools (search memory, retrieve associations,
  inspect state) to the session. On a function call, the server executes via the
  existing tool registry, adds a `function_call_output` item, and re-issues
  `response.create` so the model continues.
- **Record execution, not just intent.** Keep `ToolRequested` as the request
  record; add result/observability types (e.g. `ToolCallPermissionDecided`,
  `ToolCallExecuted`, `ToolResultReturnedToProvider`, or a `ToolResultRecord`
  linked by `call_id`) capturing the permission decision, status, result summary,
  error, timing, and the event that returned the result. Do not overload
  `auto_executed` as execution evidence.
- **Verify:** *Automated* — function-call → permission decision → registry
  execution → `function_call_output` returned, with a non-allow-listed tool proven
  to stay unexecuted and recorded as denied. *Human* — ask something requiring
  memory search; confirm it calls the tool and uses the result.

### Phase 5 — Live memory extraction + presence/interruption refinement

- Lightweight extraction over completed **trusted** turns (reuse sleep/memory
  proposers) feeding the existing review/consolidation path; refine interruption
  representation and latency reporting for presence research.
- **Verify:** *Automated* extraction tests; *human* presence evaluation against the
  `Concept.RealtimePresence` open questions; latency measurements recorded.

## Resolved Decisions

Accepted on 2026-06-09:

1. **`qsf_session` lean shared surface.** Move reducer/state/event contracts,
   `Exchange`, persistence DTOs, continuity manifest, and event-record/`EventType`
   contract; keep `RunContext`, provider clients, memory retrieval, tools, and
   OpenAI/CPAL dependencies outside.
2. **Realtime model + voice + turn detection.** Phase 2 starts with
   `gpt-realtime-2`, voice `marin`, medium reasoning effort, audio output, and
   provider `server_vad` with automatic response creation and interruption enabled.
3. **WebRTC initialization + `call_id` lifetimes.** *(Revised 2026-06-09 — supersedes
   the original ephemeral-token decision; see the `DecisionLog.md` reversal.)* The
   server performs the SDP exchange with the `OPENAI_API_KEY` and mints no browser
   credential. Keep the `{ session ↔ call_id }` binding active-call scoped,
   invalidated on stop/error/expiry, with only a short cleanup grace for diagnostics.
4. **Trust boundary.** Browser-relayed events are untrusted diagnostic facts until
   the Phase-3 server sideband becomes authoritative.

## Remaining Validation Questions

- Does the actual provider event stream preserve the mapping assumptions under
  overlap, interruption, duplicate delivery, and out-of-order transcript completion?
- Is `server_vad` acceptable for presence once human latency/interruption testing is
  available, or should a later experiment compare `semantic_vad`?
- How small can working-memory injection packets be while still improving
  cross-turn and cross-session continuity?

## Decision-Log Status

- Accepted Phase-0 entries live in `docs/DecisionLog.md` under 2026-06-09.
- The 2026-06-09 reversal "Realtime WebRTC uses a server-side SDP exchange, not
  ephemeral tokens" supersedes the ephemeral-token portion of the Phase-0 entries.
- Future entries are still expected when Phase 3 makes the sideband trusted in code,
  when Phase 4 adds live realtime tool execution, and when later experiments promote
  results into architecture.
- Launcher command naming for the first-class realtime mode should be decided when
  the server/UI entry point is implemented.

## Documentation Updates (per `ProjectWorkflow.md`)

- **Concepts:** `Concept.RealtimeAudio.md` has been normalized into the concept
  format; `Concept.RealtimePresence.md` cross-links the realtime browser voice
  direction.
- **Research:** `ResearchQuestions.Audio.md` has been refreshed to reconcile the
  earlier transcript-first direction with this design and to add questions for
  injection relevance and ASR-vs-model transcript divergence.
- **Experiments:** one experiment doc per live phase (e.g.
  `Experiment.RealtimeBrowserVoiceMVP`, `Experiment.LiveContextInjection`,
  `Experiment.LiveToolPerception`) as validation artifacts, not as the final
  runtime category.
- **Architecture:** `Architecture.AudioLoop.md` Implementation Status has been
  refreshed; `Architecture.RealtimeSessionServer.md` has been added; and
  `Architecture.ToolSystem`, `Architecture.MemorySystem`, and
  `Architecture.StateAndObservability` now note the accepted future realtime
  surfaces.
- **DecisionLog.md:** accepted Phase-0 decisions plus later entries as code lands.
- **Commit history:** clear commits carry implementation chronology; active project
  documents are updated when current behavior or design changes.
- **README.md / launcher docs:** update "What works today" and the PowerShell
  launcher section as phases land, making the eventual first-class realtime mode
  clear.
- **Lint gates:** Rust → `cargo clippy --all-targets -- -D warnings` then
  `cargo fmt`; `ui/` → `npm run check` then `npm run fmt`.

## Risks & Failure Modes

- **Audio becomes the product.** Mitigate by keeping the control/memory planes
  central and the model as a surface, not the mind.
- **Provider lock-in.** The media plane and sideband are OpenAI-specific; isolate
  them behind the browser client + sideband adapter so the QSF brain stays
  provider-agnostic.
- **Untrusted input into memory.** Browser-relayed facts must never reach durable
  memory; the trust boundary + diagnostic-only Phase 2 enforce this.
- **Reducer identity errors under overlap.** Out-of-order/overlapping provider
  events could mutate the wrong exchange; the mapping contract + test matrix guard
  against this before integration.
- **Transcript divergence.** The realtime model hears audio natively; the ASR
  transcript is a rough guide. Store both the raw event stream and a normalized
  transcript; treat transcripts as approximate.
- **Latency breaks presence.** Measure end-to-end and per-stage latency from
  Phase 2; presence is a research question, not a "fast enough" claim.
- **Over-injection.** Keep working-memory packets small; relevance over volume.
- **Crate-extraction churn (Phase 1).** Pure refactor; gate on normalized-artifact
  parity and green tests before any feature work.

## Verification Summary

- Every phase has automated coverage via `cargo test` (+ `npm run check` for UI)
  behind the clippy/fmt gates. Phase 1 gates on **normalized-artifact** parity
  (volatile fields scrubbed), not byte-for-byte.
- The reducer overlap/out-of-order test matrix is a Phase 2 gate.
- Human testing is required at Phases 2, 3, 4, and 5 — for the live spoken
  experience, cross-session continuity, model-invoked tool use, and presence
  evaluation respectively.
