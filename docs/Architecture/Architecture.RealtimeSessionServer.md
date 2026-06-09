# Architecture: Realtime Session Server

## Status

Maturity: Sketch

This document captures the accepted Phase-0 architecture for the browser-based
realtime voice conversation server. It describes intended structure, not current
runtime behavior. This server is the planned home for QSF's eventual primary
realtime conversation mode, not a one-off experiment server.

## Implementation Status

**Implemented today:**

- `qsf_realtime_server` exists as a thin axum/tokio crate with `main.rs`,
  `lib.rs`, `cli.rs`, `state.rs`, a `/health` route, the realtime routes, and a
  server-owned diagnostic JSONL writer.
- `POST /api/realtime/session` allocates a `qsf_session_id` and returns the
  accepted non-secret session config.
- `POST /api/realtime/sdp` performs the server-side SDP rendezvous against the
  OpenAI realtime calls endpoint with the server-held API key, captures
  `call_id`, stores the `{ qsf_session_id <-> call_id }` binding, and returns
  the SDP answer.
- `WS /api/realtime/events` accepts typed browser relay envelopes, validates
  them, deduplicates provider `event_id`, maps them into `qsf_session` live
  events, and persists diagnostic-only exchanges with explicit trust/source
  markers.
- `POST /api/realtime/stop` invalidates the binding and finalizes any open
  diagnostic exchange.
- `crates/qsf_realtime_server/ui/` is a dedicated Vite + TypeScript + Biome +
  Vitest browser preview surface with a minimal WebRTC client and relay-envelope
  mapping tests.
- The shared reducer overlap matrix now handles the phase-2 out-of-order and
  interruption cases inside `qsf_session`.

**Partial:**

- The transport slice exists, but the live browser audio experience still needs
  human verification for spoken output, interruption, and barge-in.
- `qsf_browser_server` remains a read-only inspection server and is not the live
  realtime server.

**Not yet implemented:**

- Phase 3 authoritative sideband attachment.
- Live context injection and live realtime tool execution.
- Phase 5 memory extraction / presence refinement for the browser live mode.

Last reviewed: 2026-06-09 against the implemented Phase-2 transport slice.

## Purpose

The realtime session server is the live, effectful server for browser-based
speech-to-speech conversation. It keeps OpenAI credentials server-side, coordinates
the WebRTC rendezvous, records observable session facts, and later attaches a
server-side sideband to inject context and return tool results.

It exists separately from `qsf_browser_server` because the browser server is for
read-only post-hoc inspection. Live realtime voice needs provider rendezvous,
reducer access, sideband control, and tool execution boundaries.

The implementation may be validated through experiment docs and fixture-backed
tests, but the intended operator experience is a normal realtime conversation mode.
The current `qsf.ps1 app -Experiment ...` path remains a harness for today's
experiments, not the final shape for this server.

## Accepted Phase-0 Defaults

- Crate: `qsf_realtime_server`.
- Browser media: WebRTC, owned by the browser.
- Model: `gpt-realtime-2`.
- Voice: `marin`.
- Reasoning effort: `medium`.
- Output modality: `["audio"]`.
- Turn detection: provider `server_vad`, with automatic response creation and
  interruption enabled.
- Browser session config: `POST /api/realtime/session` returns non-secret
  accepted defaults; the browser receives no client secret.
- `call_id` binding: active-call scoped, invalidated on stop/error/expiry, with
  only a short cleanup grace for diagnostics.

## Boundary

```text
Browser
  -> POST /api/realtime/session
     <- qsf_session_id + non-secret session config

Browser
  -> POST /api/realtime/sdp
     -> QSF server proxies SDP to OpenAI realtime calls endpoint with the
        server-held API key
     <- SDP answer + server-captured call_id binding

Browser <-> OpenAI
  WebRTC media flows directly

Browser
  -> WS /api/realtime/events
     -> diagnostic provider events only (untrusted)

QSF server
  -> Phase 3 sideband WebSocket with call_id
     -> authoritative provider events, context injection, tool results
```

Raw audio is not logged. `OPENAI_API_KEY` never reaches the browser.

## Launcher Surface

The PowerShell launcher currently starts `qsf_app` experiments, the memory browser,
the UI, and the workbench. When this server exists, the launcher should add a
first-class realtime conversation command that starts the realtime server and the
browser UI together, applies non-secret QSF defaults, and checks required secrets
without printing them.

The exact command name is intentionally left open until implementation, but the
mode should not be exposed only as `app -Experiment <name>`.

## Trust Model

Phase 2 has only browser-relayed provider events. The browser is not an
authoritative source for provider facts, so relayed events are:

- schema-validated,
- size-limited,
- deduplicated by provider event id where possible,
- marked untrusted,
- persisted only as diagnostic artifacts with explicit source/trust markers,
- excluded from sleep consolidation and continuity promotion.

Phase 3 introduces the server-side sideband. Events observed through that sideband
are authoritative and may produce trusted, sleep-eligible exchanges.

## Provider Event Mapping Contract

The reducer keeps a single active exchange in full-duplex mode and finalizes the
prior exchange when a new user turn starts.

Speech-to-speech exchange boundary:

```text
user audio item / transcript item
  + assistant response
  = one QSF exchange
```

Provider identity fields map as follows:

- `call_id`: provider call/session binding, not an exchange id.
- `event_id`: provider event id used for deduplication and trace correlation.
- `item_id`: conversation item id, especially user audio/transcript items and
  assistant message items.
- `previous_item_id`: ordering hint for conversation reconstruction.
- `response_id`: assistant response lifecycle id.
- `exchange_index` or equivalent QSF exchange id: required on exchange completion
  so completion cannot target the wrong active exchange.

Reducer tests for the first implementation must cover:

- transcript completion after response start,
- duplicate provider events,
- interruption before `response.created`,
- response completion after interruption,
- a second user turn before the prior response finishes,
- out-of-order lifecycle events.

## Dependency Shape

`qsf_realtime_server` should depend on a lean `qsf_session` crate for session
state, events, exchanges, persistence DTOs, continuity manifest, and event-record
contracts.

It should not require the full `qsf_app` runtime for the Phase-2 media slice. Later
phases may add explicit dependencies or adapter crates for:

- memory retrieval and working-memory packet construction,
- sleep/live-memory eligibility policy,
- read-only tool registry execution,
- OpenAI realtime protocol helpers extracted from the one-shot provider.

## Verification

Phase 2 automated verification:

- session route returns non-secret defaults,
- SDP proxy stores `call_id`,
- event relay rejects malformed or oversized payloads,
- event mapping persists diagnostic exchanges,
- reducer overlap/out-of-order matrix is green,
- browser event-mapping TypeScript tests pass.

Phase 2 human verification:

- open browser,
- start session,
- speak and hear a reply,
- interrupt mid-reply,
- confirm diagnostic exchanges appear,
- inspect network traffic and confirm `OPENAI_API_KEY` never reaches the browser.

## Related Documents

- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.StateAndObservability.md`
- `docs/Architecture/Architecture.ToolSystem.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Research/ResearchQuestions.Audio.md`
