# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Candidate implementation plan; Phase 0 decisions accepted on 2026-06-09.

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

This is the intended primary operating mode of the project, not merely another
experiment path. The experiment documents named below are validation scaffolds for
building and measuring slices of the mode; the end state is a normal way to run
QSF.

## Phasing Principles

- Each phase builds, passes `cargo test`, and is green under
  `cargo clippy --all-targets -- -D warnings` then `cargo fmt`. UI changes also pass
  `npm run check` then `npm run fmt` in `crates/qsf_browser_server/ui/`.
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
contract, and the Phase-2 "diagnostic-only" trust boundary. Resolve the model,
voice, turn-detection, token lifetime, `call_id` lifetime, and lean
`qsf_session` surface questions before implementation starts.

**Deliverable.** Accepted decision-log entries; the mapping contract written down
(exchange boundary, id mapping, overlap/out-of-order behavior). No implementation.

**Verify.** Review-only: the mapping contract and trust boundary are recorded and
agreed. No automated tests. **No human testing.**

**Docs.** `docs/DecisionLog.md`; `docs/Architecture/Architecture.RealtimeSessionServer.md`;
refresh audio research/concept notes that still describe speech-to-speech as deferred.
No diary entry is required for this docs-only decision pass.

**Accepted 2026-06-09.**

- `qsf_realtime_server` owns live realtime side effects; `qsf_browser_server` stays
  a read-only inspection server.
- The browser owns the WebRTC media plane. The QSF server owns ephemeral-token
  minting, SDP rendezvous, and the `{ qsf_session_id <-> provider call_id }`
  binding.
- Phase-2 browser-relayed provider events are untrusted, diagnostic-only, and
  excluded from sleep and continuity. The Phase-3 server sideband is the
  authoritative source for trusted live exchanges.
- Phase-2 defaults are `gpt-realtime-2`, voice `marin`, `reasoning_effort =
  medium`, `output_modalities = ["audio"]`, and provider `server_vad` with
  automatic response creation and interruption enabled.
- The browser client secret lifetime is controlled by the provider-returned
  `expires_at`. The `call_id` binding is active-call scoped, invalidated on
  stop/error/expiry, and retained only for a short cleanup grace for diagnostics.
- `qsf_session` should be lean: reducer/state/event contracts, `Exchange`,
  persistence DTOs, continuity manifest, and the event-record/`EventType` contract
  may move with it; `RunContext`, provider clients, memory retrieval, tools, and
  OpenAI/CPAL dependencies stay outside.
- Realtime voice conversation is the long-term primary QSF operating mode. Phase
  experiment docs and reports are how the project validates the path, not a signal
  that the final mode belongs under the experiment runner.

---

## Phase 1 — Extract `qsf_session` crate (pure refactor, no behavior change)

**Scope.** Move the reducer, `LiveSessionEvent`/`SessionEvent`, `Exchange`,
`SessionState`, persistence DTOs, continuity manifest, and event-record/`EventType`
contract into a lean `qsf_session` crate that neither `cpal` nor OpenAI deps reach.
Keep `RunContext`, provider clients, memory retrieval, tools, and OpenAI/CPAL
dependencies outside the crate. Apply the `ExchangeCompleted` identity change from
the mapping contract here (reducer-local, unit-tested). `qsf_app` re-exports so
existing call sites barely change.

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
  browser-relayed events. Default session config: `gpt-realtime-2`, voice `marin`,
  `reasoning_effort = medium`, `output_modalities = ["audio"]`, and `server_vad`
  with automatic response creation and interruption enabled.
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

**Docs.** `Experiment.RealtimeBrowserVoiceMVP` as the validation record; new
`Architecture.RealtimeSessionServer.md` (three-plane server, rendezvous, trust
boundary); refresh `Architecture.AudioLoop.md` Implementation Status; decision-log
entries (realtime-server crate, browser-owns-media, ephemeral tokens,
diagnostic-only relay); diary entry; README "What works today". Add launcher notes
for the preview path and the intended future first-class realtime mode.

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
`app -Experiment <name>`. The exact command name should be decided when the server
and UI entry point exist, but the intended shape is:

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
- **Phase 1 gate:** normalized-artifact parity (volatile fields scrubbed), not
  byte-for-byte.
- **Phase 2 gate:** the reducer overlap / out-of-order test matrix.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience, cross-session continuity, model-invoked tool use, and presence.

## Remaining Checks Before Each Phase

- **Phase 1:** expand the extraction into a file-level refactor checklist and verify
  the chosen `qsf_session` surface stays free of `RunContext`, provider, memory, and
  tool dependencies.
- **Phase 2:** verify the accepted model/voice/VAD defaults against the live provider
  at implementation time, then record any API drift explicitly before changing them.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream observed once live (Phase 2 is the first reality check).

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (accepted
decisions as each lands), `EngineeringDiary.md` (one entry per logical application
change), `README.md` and launcher documentation (as phases land), new
`Architecture.RealtimeSessionServer.md`,
refreshes to
`Architecture.AudioLoop.md` / `Architecture.ToolSystem` / `Architecture.MemorySystem`
/ `Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.
