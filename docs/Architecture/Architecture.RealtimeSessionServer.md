# Architecture: Realtime Session Server

## Status

Maturity: Sketch

This document captures the accepted architecture for the browser-based realtime
voice conversation server and the implemented server-side sideband behavior.
This server is the planned home for QSF's eventual primary realtime conversation
mode, not a one-off experiment server.

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
- The server-side sideband now attaches to the server-captured
  `call_id`, injects memory before `response.create`, promotes trusted
  completed exchanges into the shared continuity root, and treats the browser
  relay as diagnostic-only.
- The realtime sideband declares a read-only realtime tool allow-list
  (`search_memory`, `get_associations`, `inspect_session_state`), records
  `ToolRequested` and `ToolResolved` events, executes tools outside the session
  mutex, returns `function_call_output`, and re-issues `response.create`.
- Function-call-only and mixed `response.done` events do not finalize the
  exchange; finalization waits for the eventual spoken response. Tool-loop model
  usage is aggregated onto the trusted exchange, while each tool execution keeps
  per-response usage.
- Live-loop latency observations cover final transcript, memory
  injection, response creation, and first audio, plus durable diagnostics for
  interrupted trusted exchanges.
- `WS /api/realtime/events` accepts typed browser relay envelopes, validates
  them, deduplicates provider `event_id`, maps them into `qsf_session` relay
  diagnostics only, and persists diagnostic-only exchanges with explicit
  trust/source markers.
- `POST /api/realtime/stop` invalidates the binding and finalizes any open
  diagnostic exchange.
- `crates/qsf_realtime_server/ui/` is a dedicated Vite + TypeScript + Biome +
  Vitest browser preview surface with a minimal WebRTC client and relay-envelope
  mapping tests.
- The shared reducer overlap matrix now handles the browser-relay out-of-order and
  interruption cases inside `qsf_session`.

**Partial:**

- The transport slice exists, but the live browser audio experience still needs
  human verification for spoken output, interruption, and barge-in.
- `qsf_browser_server` remains a read-only inspection server and is not the live
  realtime server.

**Not yet implemented:**

- Full `qsf_app` tool exposure to the live realtime model.

Last reviewed: 2026-06-28 against the implemented volition state seeding and
lifecycle protection.

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

## Accepted Browser Realtime Defaults

- Crate: `qsf_realtime_server`.
- Browser media: WebRTC, owned by the browser.
- Model: `gpt-realtime-2`.
- Voice: `marin`.
- Reasoning effort: `medium`.
- Output modality: `["audio"]`.
- Turn detection: provider `server_vad`, with `create_response = false` and
  `interrupt_response = false` so the sideband owns context injection and
  interruption decisions before issuing `response.create`.
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
  -> Authoritative sideband WebSocket with call_id
     -> authoritative provider events, context injection, tool results
```

Raw audio is not logged. `OPENAI_API_KEY` never reaches the browser.

## Tool Loop

The realtime sideband owns the live tool loop. The default session advertises only
the server-owned read-only perception tools. When the provider completes a
function call response, the sideband records the provider event and tool request,
drops the session lock before execution, applies the pure allow-list/read-only
permission decision, executes or denies the tool, then reacquires the lock to
record the execution result. The sideband returns one `function_call_output` per
call and then sends `response.create`.

The loop is capped at three sequential tool calls per turn. When the cap is hit,
the sideband returns a structured denial and creates the next response with tools
disabled so the model must speak. Denied, failed, malformed-argument, and aborted
calls are durable execution records; `auto_executed` on the request record is not
treated as execution evidence.

## Launcher Surface

The PowerShell launcher currently starts `qsf_app` experiments, the memory browser,
the UI, and the workbench. When this server exists, the launcher should add a
first-class realtime conversation command that starts the realtime server and the
browser UI together, applies non-secret QSF defaults, and checks required secrets
without printing them.

The exact command name is intentionally left open until implementation, but the
mode should not be exposed only as `app -Experiment <name>`.

## Trust Model

The browser relay is not an
authoritative source for provider facts, so relayed events are:

- schema-validated,
- size-limited,
- deduplicated by provider event id where possible,
- marked untrusted,
- persisted only as diagnostic artifacts with explicit source/trust markers,
- excluded from sleep consolidation and continuity promotion.

Events observed through the server-side sideband are authoritative and may
produce trusted, sleep-eligible exchanges.

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

It should not require the full `qsf_app` runtime for the browser media slice.
Later capabilities may add explicit dependencies or adapter crates for:

- memory retrieval and working-memory packet construction,
- sleep/live-memory eligibility policy,
- read-only tool registry execution,
- OpenAI realtime protocol helpers extracted from the one-shot provider.

## Verification

Browser media automated verification:

- session route returns non-secret defaults,
- SDP proxy stores `call_id`,
- event relay rejects malformed or oversized payloads,
- event mapping persists diagnostic exchanges,
- reducer overlap/out-of-order matrix is green,
- browser event-mapping TypeScript tests pass.

Browser media human verification:

- open browser,
- start session,
- speak and hear a reply,
- interrupt mid-reply,
- confirm diagnostic exchanges appear,
- inspect network traffic and confirm `OPENAI_API_KEY` never reaches the browser.

## Volition State

Each live session owns an independent `VolitionRuntimeState` (defined in
`crates/qsf_realtime_server/src/realtime/volition.rs`). It is:

- **Seeded** at session creation via `realtime_seed_fixture()`, which produces the
  full static fixture plus the protected tier-2 (`explicit-user-intent`) and tier-3
  (`current-task-completion`) tensions and goals.
- **Per-session**: `VolitionRuntimeState` is a field on `SessionRuntime` and never
  shared across sessions. Two concurrent sessions have independent lifecycle.
- **Mutated on trusted transcripts only**: `apply_trusted_transcript_to_volition` in
  `sideband.rs` maps each trusted turn (both `StartTurn` and `Interrupt` dispositions)
  to volition events and applies them in-memory. No other code path mutates volition
  state.
- **Not persisted**: volition state is in-memory only for the duration of a live session.
  It is not written to the continuity manifest or the diagnostic log. When the session
  ends, the state is dropped.

### Lifecycle protection for protected goals

`tick_events` accepts the `VolitionFixture` and skips retirement events for any goal
whose effective arbitration tier (minimum tier across its parent tensions) is at or
below `PROTECTED_TIER_FLOOR = 3`. This means tier-2 and tier-3 goals — the
`honor-explicit-user-request` and `complete-current-task` goals from the realtime seed
fixture — are permanently exempt from idle-lifecycle retirement. They remain `Accepted`
or `Active` regardless of session length, ensuring the safety guarantee that explicit
user intent and current-task-completion dominate arbitration is preserved even in very
long sessions.

## Related Documents

- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.StateAndObservability.md`
- `docs/Architecture/Architecture.ToolSystem.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Research/ResearchQuestions.Audio.md`
