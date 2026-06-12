# Plan: Realtime Voice Noise Filtering and Exchange Integrity

## Status

In progress. Phase 1 is complete (commit `0badf97`). Phase 2 is complete (commit
`0cab263`). Phase 3 is expanded below and ready for implementation. Phase 4 remains
outlined and will be expanded when its turn comes.

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

This plan fixes those issues without changing the Phase 4 tool scope. Issues 1 and 2
are fixed (Phases 1–2). Issue 3 is Phase 3, below.

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

Resolved into Phase 2 (shipped in `0cab263`; recorded in the `docs/DecisionLog.md`
entry "Continuation noise and stale provider events are diagnostic-only", 2026-06-12):

- Continuation-scoped filtering only: short phrases are filtered solely while an
  assistant response/tool continuation is in flight; idle short turns are unaffected.
- Filtered noise transcripts are persisted as diagnostic-only records, observable
  without becoming trusted turns.
- Stale and superseded provider events are audited through diagnostic-only records,
  never through `ProviderEventRecord` (the live reducer would drop or misattribute
  them).

Resolved into Phase 3 (see "Decision adopted" in Phase 3):

- `inspect_session_state` reports `completed_exchange_count` plus explicit
  active-exchange fields instead of a single ambiguous `exchange_count`.

Still open (decided in Phase 4, with live-test evidence):

- Whether to widen the in-flight noise allow-list to one-word fillers (for example
  "uh", "hmm", "okay"). They currently interrupt — deliberately, so "stop" / "wait"
  keep working. Record any widening in `docs/DecisionLog.md`.
- Whether non-promotable retained sideband exchanges need durable auditability — a
  durable sideband skipped-exchange diagnostic record and/or a model-facing
  skipped-exchange count in `inspect_session_state`. Phase 3 deliberately leaves them
  durably unauditable: they exist only in the `#[serde(skip)]` `live.completed_exchanges`,
  and the promotion skip path only logs a warning (see the Phase 3 "Decision adopted"
  auditability caveat). Decide in Phase 4 from live-test evidence whether the model
  needs to observe skipped exchanges; record any addition in `docs/DecisionLog.md`.
  (Raised by the Phase 3 structured review, finding M1.)

## Phase Completion and Gates

Each phase is implemented and verified on its own. To keep the repository compliant if a
phase is committed independently of the others:

- Run that phase's listed automated checks (UI and/or Rust) as completion gates.
- Add a concise `docs/EngineeringDiary.md` entry for that phase's implementation, after
  reading the "How to use" instructions at the top of that file.

Phases 1 and 2 ran their gates and added their diary entries when they landed. Phase 3
is the remaining Rust change and runs the cargo gates itself. Phase 4 runs the final
consolidated gates, including the npm gates for the Phase 1 UI changes.

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
  authoritative protection is the Phase 2 sideband guard.
- Surfacing the applied capture settings in the Diagnostics panel remains an optional
  follow-up if manual testing shows the need.
- Manual-test observations about provider/VAD or hardware constraint behavior belong in
  the Phase 4 diary observation entry and `docs/Research/ResearchQuestions.Audio.md`,
  not in the Phase 1 diary entry.

## Phase 2: Sideband Turn Integrity Guard (Completed)

Shipped in commit `0cab263` ("Guard realtime sideband turn integrity"), with all cargo
gates run, a `docs/EngineeringDiary.md` entry dated 2026-06-12 ("Realtime
turn-integrity guard"), and a `docs/DecisionLog.md` entry ("Continuation noise and
stale provider events are diagnostic-only").

What landed:

- `crates/qsf_realtime_server/src/realtime/turn_integrity.rs`: a pure, unit-tested
  classifier with `TurnPhase` (`Idle` / `AwaitingResponse` / `ToolLoop`) and
  `classify_final_transcript`. While a response or tool continuation is in flight,
  normalized transcripts matching the courtesy allow-list (`cheers`, `thanks`,
  `thank you`) or empty input are ignored as noise; anything else — including "stop"
  and "wait" — interrupts. Idle transcripts always start a turn. Default-on, no config
  flag.
- `SidebandRuntimeState` tracks `turn_phase`, `pending_response_exchange`, and
  `stale_response_ids`, with a shared `clear_in_flight_response_state()` helper used by
  every terminal path.
- The transcript handler gates on disposition: noise becomes a diagnostic-only
  `DiagnosticRecord::IgnoredContinuationTranscript` (no `session_state` mutation, no
  `response.create`); a genuine interruption finalizes the old exchange as
  `Interrupted` and non-promotable, then starts a clean exchange with fresh in-flight
  state.
- `response.created` / `response.done` are gated against stale response ids. Stale
  events become `DiagnosticRecord::StaleProviderEvent` and leave the current active
  exchange untouched. A `response.done` with non-`completed` status in the
  function-call path executes no tools, sends no continuation, finalizes its exchange
  as non-promotable, and clears in-flight state so no reusable active exchange is left
  behind.
- Regression tests cover the live mispairing, the cancelled continuation, interruption
  freshness, stale-event inertness, idle short turns, and the promoted-turn response
  ownership invariant.

Carried-forward notes for later phases:

- `promote_completed_trusted_exchanges` (`sideband.rs:724`) clones promoted exchanges
  into durable `turns` but **retains them in `live.completed_exchanges`**, advancing
  only the `trusted_promoted_exchange_count` watermark. Non-promotable exchanges
  (interrupted, cancelled, degraded-window) are also retained there and never become
  turns. This retention is the direct cause of the Phase 3 overcount.
- Accepted edge case (logged warning, not a guarded path): a response interrupted
  before its `response.created` was observed cannot be id-matched; safety relies on
  per-socket provider event serialization. Revisit only if Phase 4 live testing shows
  misordering.
- The Phase 2 manual speaker/headphones retest was not yet performed; it is
  consolidated into the Phase 4 human verification (same four prompts, seeded store,
  confirm no promoted turn pairs a noise transcript with an earlier answer).
- The allow-list widening question stays open for Phase 4 evidence; see "Open
  Questions".

## Phase 3: Fix Session Inspection Counts

Make `inspect_session_state()` report an auditable, non-duplicated exchange count.
Rust-only. Production changes live entirely in
`crates/qsf_realtime_server/src/realtime/tools.rs` (snapshot, tool output), with no
reducer, persistence-schema, or UI changes. Tests live in `tools.rs`; the **only**
permitted change outside it is an optional, test-only (`#[cfg(test)]`) AppState
constructor lifted into `state.rs` if the Step 3 helper duplication grows (see Step 3)
— that is a test-support move, not a production behavior change.
`ToolSessionSnapshot::from_runtime` stays a pure derivation over `&SessionRuntime` —
no I/O, no locks — so it remains unit-testable like a reducer.

### Why the count is wrong today (code-level)

- `ToolSessionSnapshot::from_runtime` (`tools.rs:39-59`) computes
  `exchange_count = live.completed_exchanges.len() + turns.len()`.
- Promotion (`promote_completed_trusted_exchanges`, `sideband.rs:724`) clones each
  promotable completed exchange into a durable `Turn` via
  `SessionEvent::ExchangeRecorded` + `SessionEvent::TurnCompleted`, but leaves the
  exchange in `live.completed_exchanges`; `trusted_promoted_exchange_count` is only a
  watermark. Every promoted exchange is therefore counted twice.
- Non-promotable retained exchanges (Phase 2 interrupted/cancelled exchanges, degraded
  windows) stay in `live.completed_exchanges` and never become turns, so they inflate
  the count with exchanges that are explicitly not trusted.
- The snapshot already exposes `active_exchange_index` and `active_exchange_status`
  (`tools.rs:32-33`); only the count is ambiguous.

### Decision adopted (resolves the plan-level open question)

- Report `completed_exchange_count = runtime.session_state.turns.len()` — durable
  promoted turns, each counted exactly once — plus explicit active-exchange fields,
  instead of a single blended number.
- Add `active_exchange_present: bool` (derived from `active_exchange.is_some()`)
  alongside the existing `active_exchange_index` / `active_exchange_status`, so the
  model-facing JSON states presence explicitly rather than via a nullable index.
- **Remove** the `exchange_count` field rather than redefining it. Its only consumers
  are inside `tools.rs` itself (the tool JSON at `tools.rs:429` and the
  `observation_summary` at `tools.rs:446` — verified by workspace grep; no UI, route,
  or cross-crate consumer). Keeping a field whose meaning silently changed would be
  worse for auditability than removing it, and realtime model sessions read the tool
  output fresh each session, so there is no compatibility window.
- Non-promotable retained exchanges are intentionally excluded from
  `completed_exchange_count`: the tool summarizes trusted durable state.

  **Auditability caveat (corrects the earlier rationale; review finding M1).** An
  earlier draft claimed these exchanges "remain auditable via diagnostic records and
  `session-state.json`." That is not true on either count, and the rationale must not
  rely on it:
  - `live.completed_exchanges` is `#[serde(skip)]`
    (`crates/qsf_session/src/live_state.rs:84-85`), so retained exchanges are never
    written to `session-state.json`. A skipped exchange never reaches durable
    `turns`/`exchanges` either (the promotion skip path at `sideband.rs:741-758` only
    logs a `log::warn!` and `continue`s).
  - `DiagnosticRecord::DiagnosticExchangeRecorded` is written exclusively for the
    **browser-relay** path (`persist_completed_diagnostic_exchanges`,
    `routes.rs:736-748`, `source: "browser_relay"`), never for skipped sideband
    exchanges. The sideband-path diagnostics that do exist —
    `IgnoredContinuationTranscript` (`sideband.rs:328`) and `StaleProviderEvent`
    (`sideband.rs:963`) — record individual transcripts/events, not an
    interrupted-but-skipped exchange as a whole.

  Net effect: a non-promotable skipped sideband exchange is currently **not durably
  auditable after process exit**. Phase 3 does **not** close this gap. It keeps
  `completed_exchange_count` defined as trusted durable turns and adds no
  skipped-exchange counter or diagnostic record — the tool stays a summary of trusted
  durable state, and the count is provably correct. Whether to add a durable sideband
  skipped-exchange diagnostic record and/or a model-facing skipped count is deferred to
  Phase 4, gated on live-test evidence (see "Open Questions").

### Step 1: Failing-first unit tests for the snapshot

Add tests to the existing `realtime::tools::tests` module. Build runtimes directly —
no async, no sockets:

```rust
let tempdir = TempDir::new().expect("tempdir");
let diagnostics = DiagnosticWriter::create(tempdir.path().join("diagnostics.jsonl"))
    .expect("diagnostics");
let mut runtime = SessionRuntime::new(
    "test-session".to_string(),
    BrowserSessionConfig::default(),
    diagnostics,
);
```

(`tempfile` is already a dev-dependency used by the sideband tests in this crate;
`BrowserSessionConfig::default()` exists at `state.rs:293`.) Shape
`runtime.session_state` per case; build the durable twin of a retained exchange with
`Turn::try_from(&exchange)`, the same conversion promotion uses (`sideband.rs:749`).
A small local `completed_exchange(index, ...)` fixture like the one in the sideband
tests is fine; do not try to share the sideband test module's private helpers.

Test cases:

1. Promoted-and-retained (the regression test for the live overcount): one completed
   exchange present in both `live.completed_exchanges` and, as its converted `Turn`,
   in `turns`; no active exchange → `completed_exchange_count == 1`,
   `active_exchange_present == false`. Under the old arithmetic this state reported 2.
2. Active exchange present: additionally set
   `live.active_exchange = Some(Exchange::new_voice_pending(1, SystemTime::now()))` →
   `completed_exchange_count` unchanged, `active_exchange_present == true`,
   `active_exchange_index == Some(1)`, and `active_exchange_status` equal to the
   constructed exchange's `status`.
3. Empty session: no turns, no completed exchanges, no active exchange → count 0,
   present false, index and status `None`.
4. Non-promotable retained exchange: a completed exchange in `live.completed_exchanges`
   with no corresponding turn → `completed_exchange_count == 0` (retained but
   untrusted exchanges do not inflate the count).

Note on the red state: these tests reference the new fields, so before Step 2 they
fail to compile — that is the failing-first checkpoint in Rust for a field rename.
Land Steps 1–2 together in one change.

### Step 2: Implement the snapshot fix

In `tools.rs`:

- Replace `exchange_count: usize` on `ToolSessionSnapshot` with
  `completed_exchange_count: usize` and `active_exchange_present: bool`. Keep
  `runtime_phase`, `active_exchange_index`, `active_exchange_status`, `trust`, and
  `degraded` unchanged.
- In `from_runtime`: `completed_exchange_count` from
  `runtime.session_state.turns.len()`; `active_exchange_present` from
  `runtime.session_state.live.active_exchange.is_some()`; existing index/status
  mappings unchanged.
- Update `InspectSessionStateTool::execute` (`tools.rs:425-451`): the JSON summary
  emits `completed_exchange_count` and `active_exchange_present` instead of
  `exchange_count`, and the `observation_summary` reports the completed count and
  active status (for example
  `"inspect_session_state reported phase={:?} completed_exchanges={} active={:?} degraded={}"`).

The snapshot construction site in the sideband tool loop (`sideband.rs:1013`) needs no
change — it already passes the whole `&guard`.

### Step 3: Tool-output contract test

Exercise `InspectSessionStateTool::execute` end-to-end through a
`RealtimeToolContext` and parse `output_text` as JSON. Build the context from a
**non-empty** snapshot — a runtime with one promoted-and-retained completed turn plus
an active exchange, mirroring Step 1 case 2 — so the test catches a wrong value mapping
from `ToolSessionSnapshot` to tool output. The original bug is a value bug, not a key
bug; a presence-only assertion would still pass with a wrong number. Assert both:

- **Keys (presence/absence):** the parsed object contains `completed_exchange_count`,
  `active_exchange_present`, `active_exchange_index`, `active_exchange_status`,
  `runtime_phase`, `trust`, and `degraded`, and does **not** contain `exchange_count`.
- **Concrete values:** `completed_exchange_count == 1`, `active_exchange_present == true`,
  `active_exchange_index == 1`, and `active_exchange_status` equal to the active
  exchange's constructed status. Also assert the `observation_summary` string reports
  the completed count (not the old blended total).

`RealtimeToolContext` requires an `AppState`; mirror the `state(&tempdir)` helper the
sideband tests use. If that duplication exceeds a few lines, lift a `#[cfg(test)]`
constructor into `state.rs` instead of copying it — one source of truth for test
AppState construction. This is the single sanctioned change outside `tools.rs` for this
phase (test-only, no production behavior change); see the Phase 3 scope note.

### Step 4: Gates and diary

Per "Phase Completion and Gates":

- `cargo test -p qsf_realtime_server realtime::tools::tests`
- Full `cargo test`
- `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
- Add a concise `docs/EngineeringDiary.md` entry (read the "How to use" header first).

Phase 3 touches no UI files, so the npm gates are not required when it lands alone.

### External human testing (consolidated into the Phase 4 retest)

- After two known successful turns in the default-session browser test, ask the
  session inspection prompt and compare the reported `completed_exchange_count` with
  the `turns` array length in the persisted `session-state.json`.
- Confirm the report distinguishes the in-flight exchange (`active_exchange_present`)
  from completed turns while a response is being produced.

### Phase 3 acceptance criteria

- An exchange present in both durable `turns` and retained `live.completed_exchanges`
  is counted exactly once.
- The active exchange is reported only via the explicit `active_exchange_*` fields and
  is never folded into `completed_exchange_count`.
- Non-promotable retained exchanges do not inflate the reported count.
- The ambiguous `exchange_count` field no longer appears in the tool output or
  snapshot.
- The Step 3 contract test asserts concrete values (not just key presence), so a wrong
  snapshot-to-output value mapping fails the test.
- `ToolSessionSnapshot::from_runtime` remains a pure, synchronous derivation, and no
  production code outside `tools.rs` changes (any `state.rs` touch is `#[cfg(test)]`
  test support only).
- The Step 1 and Step 3 tests pass; `cargo build`,
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, and full `cargo test`
  pass; the EngineeringDiary entry is written.

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
  non-promotable interrupted exchanges. Record explicitly that non-promotable retained
  sideband exchanges are **not** durably persisted today (they live only in the
  `#[serde(skip)]` `live.completed_exchanges`, and the promotion skip path only logs),
  and note any durable skipped-exchange record/count added as a result of the Phase 4
  open question (review finding M1).
- `docs/Research/ResearchQuestions.Audio.md`: record the provider VAD / acoustic
  bleed observation as evidence for future presence and turn-taking work.
- `docs/DecisionLog.md`: only add an entry if the implementation creates a durable
  policy beyond the Phase 2 entry already recorded — for example "session inspection
  counts report completed promoted turns plus explicit active-exchange fields", a
  widening of the continuation noise allow-list, or a decision on durable auditability
  / model-facing counting for non-promotable skipped exchanges (the Phase 4 open
  question raised by the Phase 3 review).

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