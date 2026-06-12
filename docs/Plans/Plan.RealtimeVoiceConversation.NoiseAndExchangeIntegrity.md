# Plan: Realtime Voice Noise Filtering and Exchange Integrity

## Status

In progress. Phase 1 is complete (commit `0badf97`). Phase 2 is expanded below and
ready for implementation. Phases 3–4 remain outlined and will be expanded when their
turn comes.

## Context

A live `qsf_realtime_server` run using the stable `default` session id and a seeded
memory store showed that Phase 4 tool calls can work in practice:

- `get_associations` resolved `memory.voice-loop-ownership` and returned the strongest
  associated neighbor.
- `inspect_session_state` executed.
- `search_memory` correctly returned no results for an absent "Pineapple Radar"
  query.

The same run exposed three issues:

1. Browser microphone input accepted likely noise or speaker bleed as short user turns
   (`Cheers.`, `Thank you.`).
2. A cancelled response/tool continuation was able to pair its eventual answer with a
   later short transcript, so a durable trusted turn had `user_input = "Thank you."`
   but an answer for the earlier memory-search request.
3. `inspect_session_state()` overcounted exchanges because it combined promoted durable
   turns with retained live completed exchanges.

This plan fixes those issues without changing the Phase 4 tool scope.

## Goals

- Reduce accidental turn creation from speaker bleed and short noise-like transcripts.
- Preserve trusted exchange integrity when provider VAD detects new speech during an
  assistant response or tool loop.
- Make `inspect_session_state()` report an auditable, non-duplicated exchange count.
- Keep defaults exercising the new protections; random or disabled behavior should be
  opt-in only if added later.

## Non-Goals

- Replacing provider `server_vad` with a custom VAD implementation.
- Adding push-to-talk as the only interaction mode.
- Changing the read-only tool allow-list.
- Solving all acoustic echo cases across all hardware. Human testing is still required.

## Open Questions

Resolved into Phase 2 defaults (see "Decisions adopted" in Phase 2; a `DecisionLog`
entry, if warranted, is a Phase 4 item):

- Continuation-scoped filtering only: short phrases are filtered solely while an
  assistant response/tool continuation is in flight, so genuine brief user turns still
  work while idle.
- Filtered noise transcripts are persisted as diagnostic-only records: observable
  without becoming trusted turns.
- Stale and superseded provider events are audited through diagnostic-only records, not
  through `ProviderEventRecord` (the live reducer would drop or misattribute them — see
  Phase 2 "Why the failure happens today").

Still open (decided in Phase 3):

- Should `inspect_session_state.exchange_count` include the active exchange? Prefer yes,
  explicitly as `completed_exchange_count` plus optional `active_exchange_index`, instead
  of a single ambiguous count only.

## Phase Completion and Gates

Each phase is implemented and verified on its own. To keep the repository compliant if a
phase is committed independently of the others:

- Run that phase's listed automated checks (UI and/or Rust) as completion gates.
- Add a concise `docs/EngineeringDiary.md` entry for that phase's implementation, after
  reading the "How to use" instructions at the top of that file.

If multiple phases land together in a single submission, the gates and diary entries can
be consolidated and run once with the Phase 4 final gates. The substantive Rust gate run
is in Phase 4 because that is where the Rust code changes (Phases 2–3) land; phases that
touch no Rust code still run the cargo gates as a workspace-sanity check when submitted
on their own.

## Phase 1: Browser Audio Capture Constraints (Completed)

Shipped in commit `0badf97` ("Realtime browser microphone capture constraints"), with
gates run and a `docs/EngineeringDiary.md` entry dated 2026-06-12:

- `MICROPHONE_AUDIO_CONSTRAINTS` is exported from
  `crates/qsf_realtime_server/ui/src/realtime.ts` (echo cancellation, noise
  suppression, and auto gain control all enabled), is the only capture path used by
  `startConversation()` in `crates/qsf_realtime_server/ui/src/main.ts`, and is guarded
  by a contract test in `realtime.test.ts`.
- The applied (not just requested) track settings are logged to the browser console
  after acquisition, because `getUserMedia` treats these as ideal constraints and
  browsers/hardware can silently apply different values.

Carried-forward notes for later phases:

- Capture constraints reduce speaker bleed at the source but do not eliminate it. The
  authoritative protection is the Phase 2 sideband guard; residual ghost turns observed
  in manual speaker testing (short courtesy phrases such as `Cheers.`, `Thank you.`)
  are the Phase 2 test fixtures.
- Surfacing the applied capture settings in the Diagnostics panel remains an optional
  follow-up if manual testing shows the need.
- Manual-test observations about provider/VAD or hardware constraint behavior belong in
  the Phase 4 diary observation entry and `docs/Research/ResearchQuestions.Audio.md`,
  not in the Phase 1 diary entry.

## Phase 2: Sideband Turn Integrity Guard

Add an authoritative sideband guard in
`crates/qsf_realtime_server/src/realtime/sideband.rs` so a new final transcript that
arrives while an assistant response or tool continuation is in flight cannot steal or
inherit that response's state. This phase is Rust-only; no UI changes.

### Why the failure happens today (code-level)

- The `conversation.item.input_audio_transcription.completed` arm
  (`sideband.rs:295`) is in-flight-blind. `ensure_authoritative_exchange`
  (`sideband.rs:557`) reuses the live active exchange, so a transcript arriving during
  a tool continuation lands `AudioFinalTranscriptCommitted` on the same exchange, and
  the live reducer (`live_state.rs:273-317`, the `AudioFinalTranscriptCommitted` arm)
  overwrites `final_transcript` and appends the utterance. The handler also
  unconditionally clears `runtime_state.response_id` / `response_started_at`
  (`sideband.rs:320-322`) and sends a second `response.create`, clobbering
  `current_request_hash` / `current_message_count`. Whichever `response.done`
  eventually completes the exchange pairs the new short transcript with the earlier
  request's answer — the exact `user_input = "Thank you."` mispairing observed live.
- The `FunctionCallOnly` / `Mixed` branch of `handle_response_done_event`
  (`sideband.rs:821-1016`) never checks `response.status`; a cancelled function-call
  response still executes tools and sends a continuation, and returns at
  `sideband.rs:1015` **without ever finalizing the active exchange** (no
  `ExchangeCompleted`/`ModelRoleFailed`). `live.active_exchange` therefore stays
  present, so the next idle transcript reuses that same (now cancelled) exchange via
  `ensure_authoritative_exchange` and corrupts a fresh turn. Only the `Spoken` /
  `Empty` path (`sideband.rs:1084-1096`) currently applies `ExchangeCompleted` and then
  marks non-`completed` exchanges non-promotable.
- The `response.created` arm (`sideband.rs:402-413`) unconditionally installs the
  event's response id into `runtime_state.response_id` and records `ResponseStarted`
  against whatever `ensure_authoritative_exchange` returns. After an interruption swaps
  in a new active exchange, a late `response.created` belonging to the superseded
  response would stamp the old response id onto the fresh exchange.
- There is no explicit in-flight phase. "Response in flight" cannot be derived from
  `response_id` alone because it is `None` between sending `response.create` and
  receiving `response.created`.

Reusable machinery — do not duplicate:

- The live reducer's `ExchangeStarted` arm (`live_state.rs:203-267`) already finalizes
  a previous active exchange as `Interrupted` when a response was in flight and
  suppresses the previous response id (`suppressed_response_ids`, honored by the
  `OutputProduced` arm). Starting a genuinely new exchange, instead of reusing the
  active one, rides this machinery.
- The `ExchangeCompleted` arm (`live_state.rs:548-583`) takes `active_exchange` and
  pushes it into `completed_exchanges`, clearing `live.active_exchange`. This is the
  finalize step the Spoken/Empty non-`completed` path already relies on, and the same
  one the cancelled function-call path must adopt (Step 4).
- `SessionRuntime::non_promotable_exchange_indices` plus
  `promote_completed_trusted_exchanges` (`sideband.rs:571`) already skip non-promotable
  exchanges (see test
  `gap_window_exchange_is_consumed_but_next_exchange_promotes_after_recovery`).
- `LiveSessionEvent::UserInterrupted(InterruptionRecord)` (`live_state.rs:498`,
  `crates/qsf_session/src/exchange.rs:155`) exists for recording interruptions.
- `DiagnosticRecord` in `crates/qsf_realtime_server/src/diagnostics.rs` is the
  persistence surface for diagnostic-only observations. The existing
  `DiagnosticExchangeRecorded` variant carries a full `Exchange` and would imply
  exchange semantics; add lean new variants instead (see Steps 3–4).
- The live reducer's `ProviderEventRecorded` arm (`live_state.rs:394-483`) only appends
  an event when `provider_event.exchange_index` equals the *current* active exchange
  index, and returns early otherwise. A stale `response.done` for a superseded exchange
  therefore cannot be recorded as a `ProviderEventRecord` without either being dropped
  (old index, no longer active) or polluting the new active exchange (current index).
  Stale provider events must be audited through a diagnostic-only record instead — this
  is why Step 4 does not reuse `ProviderEventRecord` for the audit trail.

### Decisions adopted from the plan's open questions

- Continuation-scoped filtering only: the guard is active solely while a response or
  tool continuation is in flight. Idle short turns are unaffected.
- Filtered transcripts are persisted as diagnostic-only records via a new
  `DiagnosticRecord` variant and never touch `session_state`.
- Stale / superseded provider events are audited through a separate diagnostic-only
  `DiagnosticRecord` variant, never through `ProviderEventRecord`, because the reducer
  would drop or misattribute them (see "Why the failure happens today"). This resolves
  the review's open question about how to keep a stale `response.done` auditable.
- Noise classification defaults to a narrow normalized allow-list (`cheers`, `thanks`,
  `thank you`) — the bleed phrases observed live. Anything else arriving in flight,
  including short commands such as "stop" or "wait", is treated as a genuine
  interruption, because length-only filtering would swallow real barge-in commands.
- All of this is default-on with no config flag, per the repo rule that defaults must
  exercise new code paths.

### Open question inside this phase

- Should non-allow-listed one-word fillers (for example "uh", "hmm", "okay") also be
  filtered while in flight, or remain interruptions? Default here: remain
  interruptions (safe for "stop" / "wait"). Revisit with Phase 4 live-test evidence
  before widening the allow-list; record any widening in `docs/DecisionLog.md`.

### Step 1: Pure turn-phase and classification module

Add `crates/qsf_realtime_server/src/realtime/turn_integrity.rs`, registered with a
single `pub(crate) mod turn_integrity;` line in `realtime/mod.rs` (entry points stay
thin; this classifier is an internal sideband detail, so it stays crate-scoped to match
the sibling realtime modules). Pure data and functions only — no I/O, no locks — so it
is unit-testable like a reducer:

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TurnPhase {
    #[default]
    Idle,
    AwaitingResponse,
    ToolLoop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptDisposition {
    StartTurn,
    IgnoreAsNoise,
    Interrupt,
}

pub(crate) fn classify_final_transcript(
    phase: TurnPhase,
    transcript: &str,
) -> TranscriptDisposition
```

`TurnPhase` derives `Serialize` with `#[serde(rename_all = "snake_case")]` because it is
embedded directly in the diagnostic record written in Step 3 (`DiagnosticRecord` derives
`Serialize` and is serialized to JSON in `diagnostics.rs:94`); without this the
diagnostic record will not compile. If a later change makes the diagnostic store a plain
string instead, drop the derive then — do not add it speculatively beyond this use.

Behavior:

- `Idle` → `StartTurn`, always (preserves current idle behavior, including short
  transcripts like "thanks").
- In flight (`AwaitingResponse` or `ToolLoop`): normalize the transcript (trim,
  lowercase, strip trailing punctuation); if it matches the allow-list constant
  (`cheers`, `thanks`, `thank you`) or is empty/whitespace-only → `IgnoreAsNoise`;
  otherwise → `Interrupt`.
- The allow-list is a module constant: one source of truth.

A `Speaking` phase after the final `response.done` is intentionally out of scope: the
sideband has no reliable playback-finished signal, and the post-done bleed window is
covered by the Phase 1 capture constraints plus this classifier on the next turn.

Unit tests in the same file: normalization (`"Thank you."` → `thank you`), allow-list
hit and miss per phase, idle short transcript yields `StartTurn`, `"stop"` / `"wait"`
in flight yield `Interrupt`, empty transcript in flight yields `IgnoreAsNoise`, and a
`TurnPhase` serde round-trip asserting the snake_case wire form (`awaiting_response`,
`tool_loop`).

Verify: `cargo test -p qsf_realtime_server turn_integrity`.

### Step 2: Track turn phase and response ownership in `SidebandRuntimeState`

Extend `SidebandRuntimeState` (`sideband.rs:75`) with:

- `turn_phase: TurnPhase` (defaults to `Idle`),
- `pending_response_exchange: Option<usize>` — the exchange index the most recent
  `response.create` was issued for,
- `stale_response_ids: HashSet<String>` — provider response ids cancelled by an
  interruption; their eventual `response.created` / `response.done` must be inert.

Add a single shared reset helper so every terminal path clears the same fields (the
current code clears overlapping-but-inconsistent subsets across the transcript handler,
the spoken-completion block, and `session.closed`):

```rust
impl SidebandRuntimeState {
    /// Clears all in-flight response / tool-loop accounting. Does NOT touch
    /// `active_exchange_index`, `turn_phase`, `pending_response_exchange`, or
    /// `stale_response_ids`; those are managed explicitly by each caller.
    fn clear_in_flight_response_state(&mut self) {
        self.response_id = None;
        self.response_started_at = None;
        self.current_request_hash = None;
        self.current_message_count = 0;
        self.accumulated_latency_ms = 0;
        self.accumulated_input_tokens = 0;
        self.accumulated_cached_input_tokens = 0;
        self.accumulated_output_tokens = 0;
        self.tool_calls_in_turn = 0;
    }
}
```

Every terminal path that owns the current in-flight response must call this helper
instead of resetting fields ad hoc: interruption (Step 3), the non-`completed`
function-call response (Step 4), spoken/empty completion (the existing reset block at
`sideband.rs:1098-1107`), a stale terminal event that owns the *current* response, and
`session.closed` cleanup. Stale events that do **not** own the current response must not
call it — that is the whole point of leaving the fresh turn untouched.

Transitions:

- Transcript handler sends the initial `response.create` → `AwaitingResponse`,
  `pending_response_exchange = Some(exchange_index)`.
- Function-call branch sends a continuation `response.create` → `ToolLoop`
  (`pending_response_exchange` unchanged).
- Spoken/empty `response.done` that completes the exchange → `Idle`,
  `pending_response_exchange = None`.
- `session.closed` and reconnects reset naturally (`SidebandRuntimeState` is built
  fresh per connection in `connect_and_run_once`).

This is per-connection sideband state; no reducer or persistence-schema change.

### Step 3: Gate the transcript handler on disposition

In the `conversation.item.input_audio_transcription.completed` arm, call
`classify_final_transcript(runtime_state.turn_phase, &transcript)` before touching any
state:

- `StartTurn`: existing path, unchanged.
- `IgnoreAsNoise`: write a new
  `DiagnosticRecord::IgnoredContinuationTranscript { qsf_session_id, transcript, turn_phase, response_id, at }`
  via `guard.diagnostics.write(...)`, and log it with `engine_logging` including the
  session id and transcript. Follow the existing variant shape in `diagnostics.rs`:
  `at: OffsetDateTime` set with `OffsetDateTime::now_utc()`, and the enum's
  `#[serde(tag = "kind", rename_all = "snake_case")]` applies automatically. `turn_phase`
  serializes via the `Serialize` derive added in Step 1. Do not touch `session_state`,
  do not clear in-flight runtime fields, do not send any provider message. Return.
- `Interrupt`:
  1. Mark the current active exchange non-promotable
     (`guard.non_promotable_exchange_indices.insert(index)`).
  2. Record `LiveSessionEvent::UserInterrupted(InterruptionRecord { exchange_index, response_id, detected_at, source: "sideband_final_transcript", .. })`,
     choosing the `InterruptionAction` / `InterruptionStopOutcome` variants in
     `crates/qsf_session/src/exchange.rs` that mean "superseded by a new user turn".
     Do not invent new variants unless none fits.
  3. If `runtime_state.response_id` is known, insert it into `stale_response_ids`. If
     it is unknown (interrupt landed between `response.create` and
     `response.created`), log a warning — see the accepted edge case in Step 4.
  4. Start a clean new exchange:
     `Exchange::new_voice_pending(guard.new_trusted_exchange_index(), SystemTime::now())`
     applied via `ExchangeStarted` (the reducer finalizes the old exchange as
     `Interrupted` and suppresses the old response id), then
     `AudioFinalTranscriptCommitted` for the new index.
  5. Reset all in-flight runtime fields with
     `runtime_state.clear_in_flight_response_state()` (the Step 2 helper), then set
     `runtime_state.active_exchange_index = Some(new_index)`.
  6. Continue with the normal memory-injection + `response.create` path for the new
     exchange; set `AwaitingResponse` and point `pending_response_exchange` at it.

### Step 4: Gate `response.created` and `response.done` against stale and cancelled responses

In the `response.created` arm (`sideband.rs:402-413`):

- Ownership gating, before installing any state: if the event's response id is in
  `stale_response_ids`, do not install it into `runtime_state.response_id`, do not call
  `ensure_authoritative_exchange`, and do not record `ResponseStarted`. Log a warning
  and return. This closes the symmetric corruption path where a late `response.created`
  for an interrupted response would otherwise stamp the old response id onto the fresh
  exchange.
- Accepted, documented edge case (symmetric with `response.done` below): an interrupt
  that landed before the old `response.created` was observed has no known id, so that
  late `response.created` cannot be id-matched and `pending_response_exchange` already
  points at the new exchange. Provider events are serialized per socket and the old
  `response.created` precedes the interrupting transcript in practice, so this remains a
  logged warning, not a guarded path. Step 5 asserts this ordering assumption explicitly.

In `handle_response_done_event`:

- Stale gating, before any exchange mutation: a `response.done` is stale when its
  response id is in `stale_response_ids`, or when `pending_response_exchange` does not
  match the current active exchange index. For a stale event, write a diagnostic-only
  `DiagnosticRecord::StaleProviderEvent { qsf_session_id, response_id, status, exchange_index, at }`
  (the event's real status is the audit trail) and return — no `ProviderEventRecord`
  (the reducer would drop or misattribute it; see "Why the failure happens today"), no
  `ExchangeCompleted`, no `OutputProduced`, no tool execution or continuation, and no
  reset of the new turn's runtime fields. The current active exchange must be left
  completely unchanged.
- Status gating in the `FunctionCallOnly` / `Mixed` branch: if
  `response.status != "completed"` (cancelled/incomplete/failed), do not execute tools
  and do not send a continuation. Instead **finalize the active exchange** so
  `live.active_exchange` becomes `None` — reuse the same finalize shape the Spoken/Empty
  non-`completed` path already uses (`sideband.rs:1084-1096`): apply
  `LiveSessionEvent::ExchangeCompleted { exchange_index, completed_at }`, then
  `guard.non_promotable_exchange_indices.insert(exchange_index)`, then
  `promote_completed_trusted_exchanges`. Record the cancellation in the audit trail (the
  `ProviderEventRecord` for `FunctionCallCompleted` already applied at
  `sideband.rs:838-869` carries the real status, since that exchange is still the active
  one at this point). Then call `runtime_state.clear_in_flight_response_state()`, set
  `runtime_state.active_exchange_index = None`, and set `turn_phase = Idle`. Accumulated
  model-use and request hashes from the cancelled sequence must not survive into a later
  exchange, and no reusable active exchange may be left behind.
- The `Spoken` / `Empty` path keeps its existing non-promotable marking
  (`sideband.rs:1091-1096`) and finalize; replace its inline field clears
  (`sideband.rs:1098-1107`) with `runtime_state.clear_in_flight_response_state()` plus
  the explicit `active_exchange_index = None`, and ensure this reset runs only for
  non-stale events.
- Accepted, documented edge case: a response cancelled before its `response.created`
  was observed has no known id, so a late *successful* `response.done` from it cannot
  be id-matched. Provider events are serialized per socket and `response.created`
  precedes the interrupting transcript in practice, so this is a logged warning, not a
  guarded path (symmetric with the `response.created` edge above).

### Step 5: Regression tests (mocked sideband)

Use the existing test style in `sideband.rs` (`handle_provider_event` with JSON
events). Write the first two failing-first against current behavior — they are the
regression tests for the live failure:

1. Exact live failure: turn starts with a memory-search prompt → `response.done` with a
   `function_call` output (tool executes, continuation sent, phase `ToolLoop`) →
   transcript `"Thank you."` arrives → must be ignored as noise (diagnostic record
   written; `session_state` untouched; no extra `response.create` on the outbound
   channel) → continuation `response.done` completes → persisted `session-state.json`
   turn has `user_input` equal to the original prompt; no turn has
   `user_input = "Thank you."`. Also assert the written
   `DiagnosticRecord::IgnoredContinuationTranscript` serializes with a snake_case
   `turn_phase` (`tool_loop`).
2. Cancelled continuation variant: after the noise transcript, deliver `response.done`
   with status `cancelled` for the in-flight response → exchange is non-promotable; no
   promoted turn pairs `"Thank you."` with the memory answer; **`live.active_exchange`
   is `None` after the cancelled response (equivalently, the next transcript receives a
   fresh exchange index, not the cancelled one)**; accumulated counters and
   `current_request_hash` do not survive into the next turn; a subsequent fresh turn
   still promotes normally.
3. Real interruption while in flight: a non-allow-listed transcript arrives during
   `AwaitingResponse` → old exchange `Interrupted` and non-promotable; new exchange
   starts clean (fresh request hash, `tool_calls_in_turn` 0); a late `response.done`
   carrying the old response id is inert — written as a
   `DiagnosticRecord::StaleProviderEvent`, with the new active exchange's
   `provider_events` unchanged and the new exchange neither completed nor paired with
   the old output.
4. Late stale `response.created`: after an interruption inserts the old response id into
   `stale_response_ids`, a late `response.created` carrying that id does not overwrite
   `runtime_state.response_id` and does not append a `ResponseStarted` provider event to
   the new active exchange. Add a sibling assertion documenting the accepted
   unknown-id ordering assumption (old `response.created` precedes the interrupting
   transcript in the serialized stream).
5. Idle short transcript: `"thanks"` while `Idle` → a normal turn that promotes when
   its own response completes.
6. Promoted-turn invariant across tests 1–4: every promoted turn's
   `output.response_id` belongs to the response created for that turn's own exchange —
   never to a different exchange index.

Verify:

- `cargo test -p qsf_realtime_server realtime::sideband::tests`
- `cargo test -p qsf_realtime_server turn_integrity`
- Full `cargo test`.

### Step 6: Diary entry and gates

Per "Phase Completion and Gates": run `cargo build`,
`cargo clippy --all-targets -- -D warnings`, and `cargo fmt`; add a concise
`docs/EngineeringDiary.md` entry (read the "How to use" header first). Phase 2 touches
no code under `crates/qsf_browser_server/ui/` or `crates/qsf_realtime_server/ui/`, so
the npm gates are not required unless this phase lands together with UI changes.

### External human testing

- Re-run the default-session browser test with the seeded memory store.
- Speak the same four prompts from the Phase 4 live test.
- Leave speakers enabled once, then repeat with headphones.
- Confirm no promoted trusted turn pairs a short noise transcript with the previous
  answer, even if the UI still shows diagnostic browser-relay transcripts.

### Phase 2 acceptance criteria

- While a response or tool continuation is in flight, an allow-listed short transcript
  never mutates `session_state`, never emits a `response.create`, and is observable as
  a diagnostic record.
- A non-allow-listed transcript in flight interrupts cleanly: the old exchange becomes
  `Interrupted` and non-promotable, the new exchange starts with fresh response/tool
  state, and the old response cannot complete against or pair its answer with the new
  exchange.
- `response.done` with a non-`completed` status never executes tools or sends a
  continuation, marks its exchange non-promotable, and **finalizes the active exchange
  so no reusable active exchange is left behind** for the next transcript to inherit.
- A stale `response.done` (or `response.created`) for a superseded response is inert and
  is auditable as a diagnostic-only record without altering the current active exchange.
- Short transcripts while idle still create normal turns.
- The guard is default-on with no opt-out flag.
- The Step 5 tests pass; `cargo build`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt`, and full `cargo test` pass; the EngineeringDiary entry is written.

## Phase 3: Fix Session Inspection Counts

Fix `ToolSessionSnapshot::from_runtime` in
`crates/qsf_realtime_server/src/realtime/tools.rs`.

Implementation direction:

- Replace the ambiguous `exchange_count` computation:
  `live.completed_exchanges.len() + turns.len()`
- Count durable promoted turns once, then optionally include the active exchange:
  - `completed_exchange_count = runtime.session_state.turns.len()`
  - `active_exchange_index = runtime.session_state.live.active_exchange.as_ref().map(...)`
  - If keeping the existing `exchange_count` field, define it as
    `completed_exchange_count + active_exchange_index.is_some() as usize`.
- Consider adding explicit fields to the tool output:
  `completed_exchange_count`, `active_exchange_present`, and `active_exchange_status`.
  This keeps the compact summary auditable without dumping internals.

Verification:

- Add unit tests for:
  - promoted turns plus retained live completed exchanges do not double-count
  - active exchange is counted once
  - no active exchange reports only completed promoted turns
- Add/update `inspect_session_state` tool tests for the JSON output.
- Run `cargo test -p qsf_realtime_server realtime::tools::tests`.
- Include this in the manual browser retest by asking the session inspection prompt
  after two known successful turns and comparing the answer with persisted
  `session-state.json`.

## Phase 4: Documentation and Acceptance

Update documentation after implementation and human verification:

- `docs/EngineeringDiary.md`: per-phase implementation entries are added as each phase is
  completed (see "Phase Completion and Gates"). In this phase, add any consolidated
  closing entry still needed plus one observation entry if the human test reveals
  provider/VAD behavior worth preserving.
- `docs/Experiments/Experiment.LiveToolPerception.md`: add the observed ghost-turn
  failure mode, the retest procedure, and final result once verified.
- `docs/Architecture/Architecture.RealtimeSessionServer.md`: refresh sideband
  exchange-integrity behavior and `Last reviewed`.
- `docs/Architecture/Architecture.StateAndObservability.md`: document ignored
  diagnostic-only transcripts, stale-provider-event diagnostic records, and
  non-promotable interrupted exchanges if added.
- `docs/Research/ResearchQuestions.Audio.md`: record the provider VAD / acoustic
  bleed observation as evidence for future presence and turn-taking work.
- `docs/DecisionLog.md`: only add an entry if the implementation creates a durable
  policy, such as "short continuation transcripts are diagnostic-only while the
  assistant is speaking" or "stale/superseded provider events are audited as
  diagnostic-only records, never as exchange provider events."

Final gates:

- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`
- Because `crates/qsf_realtime_server/ui/` is touched:
  - `npm run check`
  - `npm test`
  - `npm run fmt`

## Acceptance Criteria

- Browser microphone capture requests echo cancellation, noise suppression, and
  automatic gain control by default.
- A cancelled response/tool continuation cannot be promoted under a later transcript,
  and leaves no reusable active exchange behind.
- Short noise-like continuation transcripts are observable but do not become trusted
  turns by default.
- Genuine user interruption during assistant speech starts a clean new trusted exchange
  and leaves the interrupted exchange non-promotable.
- Stale or superseded provider events are inert and auditable as diagnostic-only records.
- `inspect_session_state()` no longer double-counts promoted turns and retained live
  completed exchanges.
- The seeded-memory default-session manual test can complete without a mismatched
  durable turn, even if the diagnostic UI still shows provider-observed stray input.