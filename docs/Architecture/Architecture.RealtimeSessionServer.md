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
- The same sideband now carries a stable volition baseline in the shared
  session instructions, phrased as Ari's first-person volition stance, and injects a bounded per-turn volition context packet
  before the initial `response.create`, while recording a
  `VolitionContextInjected` diagnostic trace for the trusted turn.
- The same sideband now derives bounded internal initiative from the arbitration
  winner, appends any surfaced initiative line to the existing volition packet,
  stashes `ContextRetrievalRequested` hints for the next turn, and records a
  `RealtimeBoundedInitiative` diagnostic trace alongside the context-injection
  trace.
- The realtime sideband declares a read-only realtime tool allow-list
  (`search_memory`, `get_associations`, `inspect_session_state`,
  `inspect_volition_state`, `select_volition_goals`), records `ToolRequested`
  and `ToolResolved` events, executes tools outside the session mutex, returns
  `function_call_output`, and re-issues `response.create`.
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
- `POST /api/realtime/text` accepts typed user turns for noisy environments once
  the sideband is attached. The browser can submit text during an existing live
  session, or start a receive-only WebRTC session without microphone capture and
  then submit text; the sideband records the typed turn as trusted input, adds a
  user conversation item, injects memory, and owns `response.create` as in the
  voice path.
- `POST /api/realtime/stop` invalidates the binding and finalizes any open
  diagnostic exchange.
- `crates/qsf_realtime_server/ui/` is a dedicated Vite + TypeScript + Biome +
  Vitest browser preview surface with a minimal WebRTC client and relay-envelope
  mapping tests. The UI streams assistant transcript deltas
  (`response.output_audio_transcript.delta`, legacy
  `response.audio_transcript.delta`) into the live response draft, and treats
  `response.done` (nested `response.output`) as the single authoritative source
  of the final assistant transcript entry. Per-part completion events
  (`response.output_audio_transcript.done` and legacy/text variants) are not
  relayed: a multi-part answer would otherwise append one transcript entry per
  content part plus the concatenated whole, both in the browser transcript and
  as duplicate `OutputProduced` records on the server.
- The shared reducer overlap matrix now handles the browser-relay out-of-order and
  interruption cases inside `qsf_session`.
- `SessionRuntime` now has a **second per-session `watch` channel**
  (`turn_context_tx: watch::Sender<Option<TurnContextCapture>>`) alongside the
  existing sideband-status channel. Each completed trusted turn publishes a
  `TurnContextCapture` (verbatim `turn_request_values` + `request_hash`) to all
  connected browser sockets as a `kind: "turn_context"` events-socket message.
  Late-joining sockets receive the stored latest value immediately (the same
  `send_replace` / subscribe pattern used by the sideband-status channel). Access
  via `subscribe_turn_context()` and `turn_context_sender()`.
- Every trusted turn also persists a `turn_context_captured` diagnostic record
  with the same `request_hash` and verbatim `turn_request_values`; the browser
  panel remains a view over that JSONL stream.
- `SessionRuntime` now also owns a per-session `volition_inspection_tx:
  watch::Sender<Option<VolitionInspectionCapture>>`. Each trusted turn publishes
  a compact volition inspection capture to connected browser sockets as a
  `kind: "volition_state"` events-socket message. The capture mirrors the live
  volition snapshot plus an optional decision summary, and late-joining sockets
  receive the stored latest value immediately via `subscribe_volition_inspection()`
  and `volition_inspection_sender()`.
- The browser UI renders a per-turn "What volition did this turn" panel derived
  entirely in the view-model layer from the `turn_context` and `volition_state`
  captures correlated by their shared per-attempt request hash: a plain-English verdict, the verbatim
  injected volition packet located by its prose prefix, and the previous detailed
  rows collapsed into a "Scoring detail" section.

**Partial:**

- The transport slice exists, but the live browser audio experience still needs
  human verification for spoken output, interruption, and barge-in.
- `qsf_browser_server` remains a read-only inspection server and is not the live
  realtime server.

**Not yet implemented:**

- Full `qsf_app` tool exposure to the live realtime model.

Last reviewed: 2026-07-07 against the Ari identity reframe, the
compatible-snapshot restore / incompatible-snapshot discard resume path, the
live volition inspection capture surface, the persisted turn-context capture,
and the browser "What volition did this turn" panel reframe.

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
- Model: `gpt-realtime-2.1`, defined once as
  `qsf_realtime_protocol::OPENAI_REALTIME_VOICE_MODEL` and consumed by both
  `qsf_realtime_server` and `qsf_app`.
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
- Persona: the session identifies as **Ari**, a first-person self-model that
  owns the volition state as its inner life ("as far as you can tell, you are a
  conscious being"); model-visible text carries no "simulated / not a claim of
  consciousness" disclaimers. Operator-facing traces and panels keep the
  simulated-volition framing. See the decision log entry "Realtime persona is
  Ari with a first-person self-model".

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

Browser
  -> POST /api/realtime/text
     -> trusted typed turn queued into the authoritative sideband

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

- **Seeded** at session creation via `realtime_seed_fixture()`, the standalone
  outward-facing curiosity-observer persona roster (it does *not* include
  `static_fixture()` content). Seven tensions back seven Accepted goals: three protected
  (tier ≤ `PROTECTED_TIER_FLOOR`) — `person-respect` (tier 1), `epistemic-integrity`
  (tier 2), `present-person-priority` (tier 3) — and four malleable —
  `knowledge-stewardship` (tier 4), `person-curiosity` and `ai-trajectory-concern`
  (tier 5), `world-curiosity` (tier 6). Mode bias is data: each tension carries its own
  `focused_bias` / `exploratory_bias`.
- **Per-session**: `VolitionRuntimeState` is a field on `SessionRuntime` and never
  shared across sessions. Two concurrent sessions have independent lifecycle.
- **Mutated on trusted transcripts only**: `apply_trusted_transcript_to_volition` in
  `sideband.rs` maps each trusted turn (spoken `StartTurn` / `Interrupt` dispositions
  and accepted typed turns) to volition events and applies them in-memory. No other
  code path mutates volition state.
- **Persisted for continuity, restored when compatible**: continuity promotion writes a
  versioned `volition-state.json` snapshot plus a manifest pointer under the session
  continuity root. On `create_session`, if a snapshot exists it is loaded and checked
  against the current seed fixture via `snapshot_is_fixture_compatible` (every Accepted
  seed goal must be present in the snapshot). A compatible snapshot is restored verbatim
  (preserving tick and goal lifecycle across sessions); a fixture-incompatible snapshot —
  e.g. one written under a prior persona whose goal ids no longer exist — is discarded and
  the session starts from the freshly seeded `realtime_seed_fixture()`. Either outcome
  emits a `VolitionContinuityNote` diagnostic. After the snapshot step, the explicit
  reviewed seed artifact (`volition-seed.reviewed.json`) is applied on top if one exists.
- **Human-gated reviewed seed**: reviewed volition changes are accepted explicitly and
  recorded with a human-promotion marker. Missing or corrupt reviewed seeds fall back to
  the plain fixture seed and emit a `VolitionContinuityNote` diagnostic instead of
  panicking.

The per-session continuity root now holds:

```text
<state_dir>/continuity/<qsf_session_id>/
  session-state.json
  continuity-manifest.json
  memory-store.json
  volition-state.json
  volition-seed.reviewed.json
```

and the diagnostics stream remains in:

```text
<state_dir>/diagnostics/<qsf_session_id>.jsonl
```

### Lifecycle protection for protected goals

`tick_events` accepts the `VolitionFixture` and skips retirement events for any goal
whose effective arbitration tier (minimum tier across its parent tensions) is at or
below `PROTECTED_TIER_FLOOR = 3`. In the curiosity-observer seed this covers the tier-1
`respect-persons-boundaries` (`person-respect`), tier-2 `keep-theses-distinct-from-fact`
(`epistemic-integrity`), and tier-3 `serve-the-present-person` (`present-person-priority`)
goals. They are permanently exempt from idle-lifecycle retirement and remain `Accepted`
or `Active` regardless of session length, preserving the guarantee that person respect,
epistemic integrity, and the present person's explicit request dominate arbitration even
in very long sessions.

### Read-Only Volition Tools

Two read-only tools expose per-session volition state to the live model without mutating
it, implemented in `crates/qsf_realtime_server/src/realtime/volition_tools.rs` and
registered in `default_tool_definitions()` and `built_in_tools()` in `tools.rs`:

- **`inspect_volition_state`**: Returns a compact JSON summary of the current mode, tick,
  goals grouped by status (active, accepted, blocked, cooldown, retired), candidate
  counts, and last initiative output summaries. Returns `{"status":"unavailable"}` when
  no volition snapshot is present.
- **`select_volition_goals`**: Given a `query` string, calls `select_goals_ranked` and
  `arbitrate_with_mode` from `qsf_volition` and returns ranked selected goals (capped at
  6), omitted goals (capped at 8), suppressed-cooldown count, arbitration result, and a
  SHA-256 hash of the volition snapshot. Returns `{"status":"no_match"}` when no goals
  match the query terms.

**Tool context extension**: When the sideband builds a `RealtimeToolContext` for a tool
dispatch batch, it clones the current `VolitionRuntimeState` into a
`VolitionStateSnapshot { state: VolitionState, fixture: VolitionFixture }` before any
`await` point. The updated `RealtimeToolContext` carries three new fields:

- `volition: Option<VolitionStateSnapshot>` — per-session snapshot cloned at dispatch time
- `exchange_index: usize` — used in the `artifact_or_record_reference` field of persisted traces
- `call_id: String` — the provider tool-call id for the specific tool being executed

**Trace contract**: Both tools persist a JSON trace in `ToolExecutionRecord.result_summary`.
The persisted tool record also carries model-visible `output_text`, so the
browser and experiment docs can reconstruct the full payload without re-scraping
transient provider events. Neither output ever contains `OPENAI_API_KEY` or raw
fixture dumps. See `Architecture.StateAndObservability.md` for the full field list.

## Related Documents

- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.StateAndObservability.md`
- `docs/Architecture/Architecture.ToolSystem.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Research/ResearchQuestions.Audio.md`

## Browser "What volition did this turn" panel

The realtime browser UI derives a per-turn explanation of the latest reply purely
in the TypeScript view-model layer, from two server-pushed captures it already
receives: the `volition_state` inspection capture (mode, tick, decision, scoring)
and the `turn_context` capture (the verbatim messages sent to the provider). The
panel correlates them by matching their shared request hash (unique per
`response.create` attempt, so a retry within one exchange cannot cross-match),
renders a plain-English
verdict plus the injected volition packet located from the turn-context messages,
and demotes the detailed scoring rows into a collapsed section. The volition
capture deliberately excludes the injected instruction text (privacy guardrail);
the panel reads that text from the turn-context capture instead, without exposing
the raw turn-context payload as its own browser panel. This keeps the observation
plane read-only and non-blocking: the selectors are total and a malformed capture
cannot break the transcript render.
