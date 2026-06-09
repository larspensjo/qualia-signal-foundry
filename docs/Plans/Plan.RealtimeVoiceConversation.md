# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Candidate

> Companion to the design note
> [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md), which
> is authoritative for the rationale, trust boundary, and contracts. This document
> is the **phased build plan**: it sequences the work into independently testable
> slices and marks where external human verification is required.
>
> **Intentionally high-level.** Each phase is a self-contained slice, not a
> task-by-task script. Expand a phase into detailed steps (file paths, test code,
> commits) immediately before executing it, surfacing that phase's open questions
> first (per `Agents.md`).

## Goal

Enable a live, browser-based, full-duplex spoken conversation where `gpt-realtime`
owns the voice (media plane) and QSF owns the mind (memory, context, tools,
observability) — built incrementally so each phase ends in a verifiable state.

## Phasing Principles

- Each phase builds, passes `cargo test`, and is green under
  `cargo clippy --all-targets -- -D warnings` then `cargo fmt`. UI changes also pass
  `npm run check` then `npm run fmt` in `crates/qsf_browser_server/ui/`.
- Reducers stay pure and unit-tested; side effects live at the edge and feed back as
  actions (`input -> action -> reducer -> state -> render`).
- A phase that adds a flag/threshold must default to exercising the new path.
- "Human testing" marks steps that need external manual verification — automated
  tests cannot cover the live spoken experience.

## Phase Overview

| Phase | Slice | Code? | Human test? |
|-------|-------|-------|-------------|
| 0 | Decisions & contracts | No | No |
| 1 | Extract `qsf_session` crate (pure refactor) | Yes | No |
| 2 | Thin media plane — live browser voice | Yes | **Yes** |
| 3 | Authoritative sideband + memory injection | Yes | **Yes** |
| 4 | Model-invoked read-only perception tools | Yes | **Yes** |
| 5 | Live memory extraction + presence refinement | Yes | **Yes** |

---

## Phase 0 — Decisions & contracts (no code)

**Scope.** Lock the choices the design review surfaced so later phases don't churn:
the `qsf_realtime_server` crate ownership, the provider-event → QSF-event mapping
contract, and the Phase-2 "diagnostic-only" trust boundary. Resolve the three items
in the design's *Open Decisions* (lean `qsf_session` surface, model/voice/VAD
settings, token + `call_id` lifetimes).

**Deliverable.** Decision-log candidate entries; the mapping contract written down
(exchange boundary, id mapping, overlap/out-of-order behavior). No implementation.

**Verify.** Review-only: the mapping contract and trust boundary are recorded and
agreed. No automated tests. **No human testing.**

**Docs.** `docs/DecisionLog.md` (candidates), diary entry.

---

## Phase 1 — Extract `qsf_session` crate (pure refactor, no behavior change)

**Scope.** Move the reducer, `LiveSessionEvent`/`SessionEvent`, `Exchange`,
`SessionState`, persistence, and continuity manifest into a lean `qsf_session` crate
that neither `cpal` nor OpenAI deps reach. Decide the lean shared surface (event
record + `EventType`, manifest, persistence) and decouple from `RunContext`. Apply
the `ExchangeCompleted` identity change from the mapping contract here (reducer-local,
unit-tested). `qsf_app` re-exports so existing call sites barely change.

**Verify (automated).** `cargo build` + full `cargo test` green; clippy/fmt gates.
Behavior parity checked by **diffing normalized artifacts** (timestamps, UUIDs
scrubbed) or deterministic reducer/persistence fixtures — *not* byte-for-byte run-dir
comparison (the bridge uses `SystemTime::now()`).

**Human testing.** None.

**Docs.** Decision-log entry (lean `qsf_session` crate); diary entry; touch
`Architecture.StateAndObservability` if the module home moves.

---

## Phase 2 — Thin media plane: live browser voice  *(first time you can talk)*

**Scope.**
- **Server (`qsf_realtime_server`, axum):** `POST /api/realtime/session` mints a
  short-lived ephemeral client secret (holds `OPENAI_API_KEY` server-side).
  `POST /api/realtime/sdp` proxies the SDP exchange and stores the
  `{ qsf_session_id ↔ provider call_id }` binding. `WS /api/realtime/events` receives
  browser-relayed events.
- **Browser (new TS in `ui/src/`):** fetch token → `RTCPeerConnection`, send SDP
  offer via the server, attach mic, play remote audio, provider VAD + barge-in.
  Minimal UI: start/stop, live transcript, listening/thinking/speaking status.
- **Server translate:** relayed events → `LiveSessionEvent` (per the Phase-0 mapping
  contract) → reducer (`qsf_session`) → persist exchanges + event/trace logs,
  **marked untrusted / diagnostic-only and excluded from sleep + continuity**.

**Verify (automated).** Token route (mocked OpenAI); SDP-proxy stores `call_id`;
event-translation → persisted-`Exchange` tests **including the reducer overlap /
out-of-order matrix** (a gate this phase); relayed-event validation rejects
malformed/oversized payloads; TS event-mapping unit tests (`npm run check`).

**Human testing (required).** Open the browser, speak, hear a reply, interrupt
mid-reply; confirm diagnostic exchanges appear in artifacts; inspect network traffic
to confirm the API key never reaches the browser.

**Docs.** `Experiment.RealtimeBrowserVoiceMVP`; new
`Architecture.RealtimeSessionServer.md` (three-plane server, rendezvous, trust
boundary); refresh `Architecture.AudioLoop.md` Implementation Status; decision-log
entries (realtime-server crate, browser-owns-media, ephemeral tokens,
diagnostic-only relay); diary entry; README "What works today".

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

## Cross-Cutting Verification

- **Lint gates every phase:** Rust → `cargo clippy --all-targets -- -D warnings` then
  `cargo fmt`; UI → `npm run check` then `npm run fmt`.
- **Phase 1 gate:** normalized-artifact parity (volatile fields scrubbed), not
  byte-for-byte.
- **Phase 2 gate:** the reducer overlap / out-of-order test matrix.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience, cross-session continuity, model-invoked tool use, and presence.

## Open Questions to Resolve Before Each Phase

- **Phase 1:** exact lean shared surface of `qsf_session` and how to decouple from
  `RunContext`.
- **Phase 2:** pinned `gpt-realtime` model name, default voice, VAD/turn-detection
  settings; ephemeral-token + `call_id` lifetimes and rotation policy.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream observed once live (Phase 2 is the first reality check).

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (candidates as
each lands), `EngineeringDiary.md` (one entry per logical change), `README.md` (as
phases land), new `Architecture.RealtimeSessionServer.md`, refreshes to
`Architecture.AudioLoop.md` / `Architecture.ToolSystem` / `Architecture.MemorySystem`
/ `Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.
