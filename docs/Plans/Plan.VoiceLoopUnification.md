# Plan: Voice Loop Unification

## Status

Phases 0-5 are complete. Phase 6 is the next implementation step.

Completed phase placeholders are intentionally short below; the durable design
contracts are kept in [Design Choices For This Plan](#design-choices-for-this-plan),
[Phase 0 Resolutions](#phase-0-resolutions), the architecture docs, and the code.

Phase 5 (commit 673b902) bridged realtime provider facts into the shared
live-session core: realtime sessions now boot shared continuity, normalize provider
lifecycle/interruption/tool-call facts through the shared reducer, and persist them
as durable voice exchanges. Provider tool calls stay inert (`auto_executed=false`)
unless routed through a QSF-owned path, and provider preambles are persisted as a
separate output category that never feeds QSF prompt assembly.

Remaining gap before Phase 6: the multi-turn text loop still defaults to
`state/text-loop` through `session::resume::state_dir_from_env()` and a
single-directory boot path. Both voice experiments already resolve through the shared
`session::state_directory::resolve_shared_state_directory_from_env` (with its
read-only `state/text-loop` -> `state/session` fallback) and boot via
`session::boot_session`. Phase 6 routes the text loop onto the same resolver so a
voice run and a text run become one continuous session.

This plan promotes `Idea.VoiceLoopUnification.md` into an incremental implementation
path.

Refined goal (2026-05-31): the `multi-turn-text-loop` stays as it is today, both in
external behavior and as a first-class experiment. It is not renamed, demoted, or
turned into a mere input mode of a voice loop. Instead, the behavior the text loop
owns today (reducer-driven session state, prompt assembly, memory read/reinforce,
live capture, cross-turn co-retrieval, warm summaries, persistence, sleep handoff) is
extracted into shared code that both the text loop and the voice loop call. The voice
loop mimics the text loop by reusing that common code, so improving the text loop
automatically improves the voice loop rather than the two drifting apart.

A voice run and a text run are one continuous session over a single shared
`state/session/` directory. They are separate entry points, not separate continuity
universes: a voice run followed by a text run (and vice versa) reads and appends the
same session history by default. The text loop's persistence directory moves to that
shared path once (via the read-only fallback below); its status as a first-class
experiment is unchanged.

## Background

The current project now has a shared text/text-owned-voice session core plus a
separate realtime provider experiment:

- `multi-turn-text-loop` owns the mature `SessionState`, cross-session resume,
  memory-store reinforcement, live memory capture, live cross-turn co-retrieval,
  consolidated-brief boot, warm summaries, session-end cross-turn flush, and sleep
  handoff.
- `text-owned-voice-loop` now reuses the shared session runtime and live reducer for
  durable voice turns.
- `realtime-voice-session` now feeds provider boundaries, realtime interruption facts,
  response lifecycle events, and provider tool-call requests into the shared
  live-session reducer and persists them as durable voice exchanges (Phase 5).

The next implementation step gives voice a first-class peer experiment and routes
both loops onto the single shared `state/session` resolver so a voice run and a text
run continue one session instead of two continuity universes.

## Current Anchors

Code anchors:

- `crates/qsf_app/src/session/mod.rs` defines the text-biased `SessionState`,
  `Turn`, `TurnSummary`, `SessionConfig`, and `SessionEvent`.
- `crates/qsf_app/src/session/{manifest,resume,persistence,continuation}.rs` own
  continuity boot, atomic state persistence, resume classification, and awake
  continuation cleanup.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` is the reference
  implementation for reducer-driven session state, prompt assembly, memory-store
  reads/reinforcement, live memory capture, live cross-turn co-retrieval,
  manifest updates, consolidated-brief injection, and session-end flush.
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs` routes finalized speech
  through QSF-owned model behavior, shared session boot, shared memory retrieval,
  live-session exchange reduction, persistence, manifest commit, and speech output.
- `crates/qsf_app/src/experiments/realtime_voice_session.rs` boots shared session
  continuity and routes realtime provider lifecycle, tool requests, interruptions,
  preambles, and speech playback events through the shared reducer into durable voice
  exchanges (Phase 5).
- `crates/qsf_app/src/audio/voice_session_provider.rs` represents realtime
  transcripts, responses, interruptions, and provider tool-call requests, with a
  typed provider interruption enum mapped into shared interruption enums.
- `crates/qsf_app/src/session/state_directory.rs` already implements
  `resolve_shared_state_directory_from_env`, the shared resolver with the read-only
  `state/text-loop` -> `state/session` fallback. Both voice experiments already use it;
  Phase 6 routes the multi-turn text loop onto it too.
- `crates/qsf_app/src/memory/{live_capture,co_retrieval,processed_ranges}.rs` and
  `crates/qsf_memory/src/processed_range.rs` now carry important continuity
  behavior that must survive the `Turn` -> `Exchange` migration.
- `crates/qsf_app/src/sleep/proposers/{llm_candidate,safety_net_co_retrieval}.rs`
  and `crates/qsf_app/src/sleep/auto_promote.rs` now define the split between
  live mechanical association coverage and sleep-side candidate/safety-net work.
- `crates/qsf_app/src/observability/event_log.rs` already includes the audio,
  realtime, speech playback, `SessionResumed`, and `TurnsAgedAndCoRetrieved`
  event names that the shared reducer should either consume directly or bridge from.

Documentation anchors:

- `docs/Architecture/Architecture.RuntimeLoop.md` records the reducer/event/state
  discipline, shared session runtime helpers, text/voice shared exchange state, and
  the remaining realtime bridge gap.
- `docs/Architecture/Architecture.AudioLoop.md` describes the voice interaction
  controller, simulation bridge, playback controller, and interruption policy.
- `docs/Architecture/Architecture.SleepPhase.md` notes that voice-loop session
  consumption by sleep is not yet implemented, while text-loop sleep already uses
  proposer-based association handling.
- `docs/Architecture/Architecture.MemorySystem.md` documents the current live/sleep
  association split and the text-owned voice loop's shared memory-store path.

Open prerequisite note: `Idea.VoiceLoopUnification.md` references
`Plan.CrossSessionContinuity.md`, but that plan file is not present in
`docs/Plans/` now. Treat the implemented code, the decision log entries from
2026-05-20, and the architecture status sections as the current source of truth for
cross-session continuity.

## Target Shape

The shared runtime should converge on this flow:

```text
Voice or text input event
  -> shared live-session reducer
  -> memory retrieval and context assembly
  -> model role invocation
  -> output event
  -> optional speech rendering
  -> session persistence and manifest update
  -> sleep commit consumes the same session state
```

The audio subsystem remains an adapter around the live-session core. Audio providers
may emit partial transcripts, final transcripts, response lifecycle facts,
interruptions, and playback facts, but they do not mutate durable session state
directly.

## Non-Goals

Not in scope for this plan:

- Provider-owned cognition as the default voice loop. Provider-owned realtime
  responses may remain explicit experiments, but the shared loop keeps QSF-owned
  state, memory, tools, and prompt assembly authoritative.
- Multi-speaker diarization, speaker identification, or meeting-style transcript
  attribution.
- Tuning the final barge-in policy beyond the first deterministic interruption
  behavior needed to make state transitions inspectable.
- Raw audio persistence. Audio observability should continue to record metadata,
  transcripts, timing, and safety markers rather than storing raw audio by default.
- Renaming, demoting, or behavior-changing the `multi-turn-text-loop`. It stays a
  first-class experiment with its current behavior; the voice loop reuses shared code
  rather than absorbing the text loop.
- Making voice the primary/default loop. Voice is an added surface over the shared
  core, not a replacement default, and there is no `QSF_PRIMARY_LOOP=voice` default.
  (Both loops do share one `state/session/` directory and one continuous session; see
  [Default State Directory](#default-state-directory). That is a shared-continuity
  decision, not a demotion of the text loop.)

## Cross-Cutting Acceptance Criteria

These criteria apply to every implementation phase that touches runtime behavior:

- Reducers stay pure and unit-testable; provider adapters convert side-effect facts
  into shared events before state changes.
- New runtime modules use stable domain names such as `exchange.rs`,
  `runtime_phase.rs`, `state_directory.rs`, or `session_state.rs`. Do not create
  modules named after this plan, a phase number, or "unification".
- Entry-point files (`main.rs`, `mod.rs`, `lib.rs`) stay thin. In particular,
  `crates/qsf_app/src/session/mod.rs` should only expose module boundaries and
  shared types, not grow orchestration logic.
- Every new reducer event or state transition has enough `engine_logging` context
  to debug a run after the fact: at minimum `session_id`; where applicable
  `exchange_index`, `utterance_id`, `response_id`, operation/provider name,
  resolved state directory, and stop-vs-ignore outcome for interruptions.
- `SessionResumed`, state-directory selection, schema migration, interruption, and
  provider failure paths log at info/warn/error level as appropriate and avoid raw
  audio or secrets.
- Defaults exercise the new code path in the phase that introduces it. Compatibility
  and fixture paths remain explicit opt-ins, not silent defaults.
- Shared-session extraction must preserve the current text-loop continuity
  contracts: live memory capture, retrieved-memory reinforcement, cross-turn
  co-retrieval, `processed_ranges`, session-end flush, sleep-side safety-net
  coverage, and manifest-last commits.
- Each phase that changes application behavior ends with a short
  `docs/EngineeringDiary.md` entry before the work is considered complete.

## Design Choices For This Plan

These choices guide the implementation phases below. They are not DecisionLog
commitments unless a later phase explicitly promotes one.

### Session Unit

Use `Exchange` as the shared durable unit, not raw `Turn`.

`Turn` works for typed request/response interactions, but voice needs to preserve
finer event structure without turning every partial transcript into a durable memory
candidate. The plan should introduce a new durable structure along these lines:

```rust
struct Exchange {
    index: usize,
    input: ExchangeInput,
    output: Option<ExchangeOutput>,
    context_assembly: ContextAssembly,
    retrieved_memory_block: String,
    recalled_items: Vec<RecallRecord>,
    model: ExchangeModelUse,
    interruptions: Vec<InterruptionRecord>,
    status: ExchangeStatus,
}

enum ExchangeInput {
    Text { text: String },
    Voice { final_transcript: String, utterances: Vec<UtteranceRecord> },
}
```

Migration shape (updated after Phase 5): `Turn` remains the durable serialized
compatibility shape for completed text-owned turns. `Exchange` is the shared runtime
source of truth: text and text-owned voice build an `Exchange` first, then derive a
`Turn` via `TryFrom<&Exchange>` for current persistence and legacy sleep/report
readers. Phase 5 added persisted exchange records that carry realtime-specific details
which cannot be represented by `Turn`; Phase 7 removes the last direct `Turn` consumer.

This avoids two parallel write paths while keeping the existing text test
suite, sleep summarization, and persisted state files stable until voice has
proven the shared shape.

### Partial Events

Partial transcripts and partial responses are live state, not durable completed
exchanges by default.

Persist enough live state to explain resume behavior after a crash:

- latest partial transcript text, revision, and timestamp
- active response id, partial response text/audio marker, and status
- current listening/thinking/speaking phase
- interruption records already committed by events

On normal completion, collapse finalized data into the `Exchange`. On awake resume,
clear provider handles and volatile playback/listening state, keep already committed
interruption records, and mark abandoned partials explicitly when they matter for
observability.

### Interruption Resume Semantics

`prepare_awake_continuation` should become modality-aware without embedding audio
provider behavior.

Rules:

- If a response was completed before shutdown, keep the completed `Exchange` as-is.
- If a response was interrupted and a new finalized user input exists, keep the
  interrupted exchange and process the new input as the next exchange.
- If a response was interrupted but only partial speech exists, persist an
  `InterruptionRecord` and clear the active partial input on resume unless a later
  provider can prove the partial was finalized.
- Never resume in `speaking` or `listening` phase. Awake continuation starts from an
  idle/listening-ready runtime phase after state cleanup.

### Consolidated Brief Timing

Load the consolidated brief at session boot before processing the first finalized
input.

For the early implementation, this keeps prompt assembly deterministic and mirrors
the current text-loop behavior. A later full-duplex optimization may start audio
capture while the brief is loading, but no finalized input should enter model context
until the resume mode and boot brief are resolved.

### Default State Directory

Resolved in Phase 0: the new default is `state/session` (modality-neutral),
not `state/live-loop`. The state lives in one directory regardless of whether
input arrives as voice or text, which matches the goal of a single continuity
universe.

Current drift note, reviewed 2026-06-03: the shared resolver now exists as
`session::state_directory::resolve_shared_state_directory_from_env`, implementing the
read-only `state/text-loop` -> `state/session` fallback below. Both voice experiments
already resolve through it and boot via `session::boot_session`. The
`multi-turn-text-loop` is the one runtime loop still off the resolver: it defaults to
`state/text-loop` through `session::resume::state_dir_from_env()` on a single-directory
boot path. Phase 6 routes the text loop through the shared resolver so the
`multi-turn-text-loop` and the voice loop share a single continuous session. The text
loop stays a first-class experiment; only its persistence directory moves (once, via
the read-only fallback below).

Compatibility behavior on boot:

- If `QSF_STATE_DIR` is set, use it unchanged.
- Else if `state/session/` exists, use it.
- Else if `state/text-loop/` exists, read from it for the current run and emit
  an info-level `engine_logging` record naming the legacy path. The first
  successful sleep commit writes the new state to `state/session/`. Do not
  copy or rewrite `state/text-loop/` in place; that risks data loss on
  partial writes.
- Else create `state/session/` fresh.

Every boot logs the resolved state path so post-hoc inspection of `runs/`
is sufficient to tell which directory was used.

### Provider Preambles

Resolved in Phase 0: realtime provider preambles are a separate output
category attached to the active `Exchange`, not partial assistant output and
not pure metadata.

Concretely, `Exchange` carries an optional `provider_preamble` (or a
`provider_events: Vec<ProviderEvent>` collection) with `text`, `received_at`,
and `provider_id`. Preambles are persisted and observable for latency and
barge-in debugging, but they are never fed back into QSF prompt assembly and
are never treated as QSF-owned cognition. This preserves the QSF/provider
boundary that Phase 5 defends.

Revisit only if an experiment explicitly opts into provider-owned cognition.

### Persisted State Compatibility

Resolved in Phase 0: serde-forward compatibility for one schema cycle, then a
one-shot upgrader.

Rules:

- Phase 1 adds new fields with `#[serde(default)]` so existing `SessionState`
  files load unchanged. A checked-in fixture of today's `SessionState` JSON
  must round-trip through serde in a regression test.
- Phase 2 introduces an explicit `schema_version: u32` on `SessionState`
  (default = 1 for existing files, 2 going forward). Reducers and migration
  code branch on `schema_version`, never on field presence.
- Already implemented (Phase 2 follow-through): `load_session_state`
  (`crates/qsf_app/src/session/persistence.rs`) refuses any `schema_version`
  newer than the binary supports, and `load_resume_inputs`
  (`crates/qsf_app/src/session/resume.rs`) upgrades a loaded legacy state in
  memory via `upgrade_schema_version()` and logs the upgrade on resume. The
  newer-version guard does not need to be newly designed in Phase 6.
- Phase 6 (default state directory move) only needs to make the in-memory
  upgrade durable. Do not "rewrite v1 -> v2 in place" unconditionally: a direct
  rewrite of the loaded file is only allowed when
  `resume_state_dir == persist_state_dir`. When `legacy_fallback_used` is true
  (`resume_state_dir == state/text-loop`, `persist_state_dir == state/session`),
  keep the legacy file untouched and persist the upgraded copy to
  `persist_state_dir` through the normal manifest-last commit. A regression test
  must assert the legacy v1 file is byte-for-byte unchanged after such a boot
  while the new `state/session` copy is v2.
- No back-compat promise for partial/live state (active transcripts, playback
  markers, listening/speaking phase). Those are cleared on awake resume per
  the interruption rules above.

## Phase 0: Design Tightening And Inventory — COMPLETE

Complete. The design pass resolved the shared session unit, migration shape, default
state directory, provider preambles, persisted-state compatibility, adapter boundary,
`SessionConfig` split, and fixed payload fields. The current source of truth is the
ratified design choices above plus the implemented reducer/event types in
`crates/qsf_app/src/session/`.

## Phase 1: Extract Shared Live-Session State — COMPLETE

Complete (commit 2a20950). Shared exchange/live-state types, the pure reducer, serde
compatibility, `Turn` conversion, and focused reducer/fixture tests landed under
`crates/qsf_app/src/session/`.

## Phase 2: Move Text Loop Onto The Shared Core — COMPLETE

Complete (commit 00a476d). The multi-turn text loop builds exchanges through the
shared reducer and derives persisted `Turn` records from finalized exchanges while
keeping text-loop behavior and persistence stable.

## Phase 3: Give Text-Owned Voice The Shared Session Core — COMPLETE

Complete (commit f68196b). Text-owned voice now boots through shared session runtime
helpers, uses the shared `MemoryStore` by default, records voice exchanges through
the live reducer, persists derived turns through the manifest-last protocol, and
keeps fixture/file memory modes as explicit opt-ins.

Remaining scope is unchanged: full voice-to-text round-trip continuity still waits
for Phase 6, when the text loop moves onto the shared `state/session` resolver.

## Phase 4: Add Live Interruption State — COMPLETE

Complete (commit b598655). `InterruptionRecord`, action/outcome enums, active
response state, runtime phase cleanup, `LiveSessionEvent::UserInterrupted`, and
awake-continuation cleanup now make interruption a first-class shared live-session
state transition.

Viability check on 2026-06-01: the implementation is viable for Phase 5. A matching
`UserInterrupted` event records interruption details on the active exchange, marks
non-ignored interruptions as interrupted, clears volatile active response state, and
returns the runtime phase to idle; ignored outcomes keep an active response speaking.
Awake continuation clears partial transcript and active response state and never
resumes in listening or speaking. Realtime provider interruptions remain
observability-first until Phase 5 bridges provider facts into the shared core.

Targeted verification passed on 2026-06-01:

- `cargo test session --lib`
- `cargo test realtime_voice_session --lib`
- `cargo test text_owned_voice_loop --lib`

## Phase 5: Bridge Realtime Voice Into The Shared Core — COMPLETE

Complete (commit 673b902). Realtime voice sessions now boot shared session
continuity and persist provider facts as durable voice exchanges instead of remaining
observability-only. New persisted exchange records on `SessionState`, realtime
provider-event and tool-request records on `Exchange`, and live-session reducer events
for provider lifecycle facts and provider tool requests route final transcripts,
preambles, response lifecycle, interruptions, and provider tool calls through the
shared reducer, then persist the completed exchange through the manifest-last state
path. Provider interruption actions use a typed provider enum mapped into the shared
interruption enums, and provider-relative audio timestamp conversion is hoisted into
the shared audio module. Provider tool calls stay inert (`auto_executed=false`) and do
not append turns or trigger side effects without a QSF-owned route, with regression
coverage on that boundary. Provider preambles are persisted as the separate
`provider_preamble` / `provider_events` output category and never feed QSF prompt
assembly, satisfying the [Provider Preambles](#provider-preambles) exit criterion.

Targeted verification passed on 2026-06-01: `cargo test session --lib`,
`cargo test realtime_voice_session --lib`,
`cargo test audio::voice_session_provider --lib`,
`cargo test text_owned_voice_loop --lib`, `cargo build`, and
`cargo clippy --all-targets -- -D warnings`.

## Phase 6: Voice Loop As A Peer Surface

Goal: give voice its own first-class experiment that reuses the shared core, without
changing the status of the `multi-turn-text-loop`. Voice is a peer surface, not the
primary or default loop. Route all loops onto the single shared `state/session`
resolver so a voice run and a text run continue one session.

Current state going in:

- The shared resolver `session::state_directory::resolve_shared_state_directory_from_env`
  already exists with the read-only `state/text-loop` -> `state/session` fallback and
  unit tests. Both voice experiments already call it from their public `run` entry
  points (`crates/qsf_app/src/experiments/text_owned_voice_loop.rs:64`,
  `crates/qsf_app/src/experiments/realtime_voice_session.rs:39`) and boot through
  `session::boot_session` with separate `resume_state_dir`/`persist_state_dir`. The
  remaining hardcoded `state/session` paths in those files are `#[cfg(test)]`
  helpers that build explicit `StateDirectoryResolution` values, not runtime paths.
- The multi-turn text loop is the one runtime loop still off the resolver: its `run`
  uses `session::resume::state_dir_from_env()` (default `state/text-loop`) and a
  single-directory boot path (`crates/qsf_app/src/experiments/multi_turn_text_loop.rs:70`).
- `boot_session` already emits the `SessionResumed` event and an `engine_info` record
  naming `resume_state_dir`, `persist_state_dir`, and `legacy_fallback_used`
  (`crates/qsf_app/src/session/runtime.rs`).
- `SessionState` already carries `schema_version` with `upgrade_schema_version()`;
  `load_session_state` refuses newer versions and `load_resume_inputs` upgrades legacy
  state in memory and logs it. The remaining gap is making that upgrade durable on a
  legacy-fallback boot (see [Persisted State Compatibility](#persisted-state-compatibility)),
  not designing the guard.
- Experiments register through the `ExperimentName` enum in
  `crates/qsf_app/src/experiments/registry.rs`; `list-experiments` already exists as a
  CLI subcommand in `crates/qsf_app/src/cli.rs`.

This phase has two separable, independently testable steps. Land and verify them in
order:

### Phase 6a: Route The Text Loop Onto The Shared Resolver

- Move `multi_turn_text_loop.rs` off `state_dir_from_env()` and its single-directory
  boot onto `resolve_shared_state_directory_from_env` + `session::boot_session`, so it
  honors `resume_state_dir` vs `persist_state_dir` from `StateDirectoryResolution`: the
  legacy `state/text-loop` directory is read-only and the first commit writes
  `state/session`. The voice public entry points already do this; do not rewrite them.
- Make the in-memory schema upgrade durable per
  [Persisted State Compatibility](#persisted-state-compatibility): persist the upgraded
  state to `persist_state_dir` through the normal manifest-last commit, and never
  rewrite the legacy file in place when `legacy_fallback_used` is true. (The
  newer-version guard and the in-memory `upgrade_schema_version()` already exist; this
  step only makes the upgraded copy land in `state/session`.)
- Audit for and remove any remaining silent `state/text-loop` default in runtime
  paths (the resolver, not `state_dir_from_env`, becomes the single source of truth;
  retire or redirect `state_dir_from_env` if it has no remaining runtime callers).
  Test helpers and fixtures may still target explicit directories.
- No new boot event/log is needed for the resolved directory: confirm the text loop
  now flows through `boot_session`, which already emits it.

### Phase 6b: Add The Voice Loop Peer Experiment

- Add a stable `voice-loop` variant to the `ExperimentName` enum (a stable domain
  name, not a phase name) with its id, description, and construction wired through the
  registry. Keep `multi-turn-text-loop`, `text-owned-voice-loop`, and
  `realtime-voice-session` registered and unchanged.
- Let the voice entry point default to deterministic simulated providers unless real
  providers are explicitly selected, so the default path needs no audio credentials.
- Do not add a `QSF_PRIMARY_LOOP` default that demotes text. If a selector is useful,
  it only chooses which experiment to run and defaults to today's behavior.
- Confirm both loops exercise the same shared behavior code, so a later improvement to
  the text loop is picked up by the voice loop without duplicate edits.
- Emit a boot event and `engine_logging` record naming the chosen experiment.
- Keep `multi-turn-text-loop` as a first-class experiment indefinitely.

Verification:

- Phase 6a: `cargo test session --lib` and a new regression test proving a voice run
  followed by a text run AND a text run followed by a voice run both read and append
  the same shared `state/session/` history rather than producing two continuity
  universes (relocated from Phase 3). Add a boot integration test for the legacy
  fallback: a v1 `SessionState` under `state/text-loop` is upgraded and the upgraded v2
  copy is written to `state/session`, while the original `state/text-loop` file stays
  byte-for-byte unchanged. (The newer-than-supported refusal is already covered at the
  `load_session_state` level; extend it to the boot path if not already exercised.)
- Phase 6b: `cargo run -p qsf_app -- list-experiments` shows the new `voice-loop`
  entry and existing experiments remain discoverable; a simulated default run
  completes without real audio credentials; text input mode and voice input mode
  produce exchanges in the same persisted session shape.
- `cargo build`
- `cargo test`

Docs to update:

- Update `README.md` run instructions when the voice experiment is added as a
  surfaced experiment.
- Update `docs/Architecture/Architecture.Overview.md` when the voice loop surface is
  added alongside the text loop.
- Update `docs/Architecture/Architecture.AudioLoop.md`,
  `docs/Architecture/Architecture.RuntimeLoop.md`, and
  `docs/Architecture/Architecture.SleepPhase.md` status sections.
- Update `docs/Architecture/Architecture.StateAndObservability.md`, which still
  documents `state/text-loop` as the default state path, so the user-visible
  state/observability contract reflects the `state/session` default.
- Add a `docs/DecisionLog.md` entry for the shared `state/session/` directory move
  (one continuous session across both loops), since it changes a user-visible default.
- Add the required diary entry for the voice loop surface change.

Rollback:

- The read-only fallback means `state/text-loop/` is never rewritten in place, so the
  directory move can be backed out by pointing `QSF_STATE_DIR` at the old path. If the
  voice experiment misbehaves it can be disabled independently while the text loop and
  shared core stay intact.

## Phase 7: Sleep Consumes Voice Sessions

Goal: close the continuity loop by making sleep summarize and commit voice exchanges
the same way it handles text sessions today.

Work:

- Teach sleep session summarization to read shared `Exchange` records, including
  voice transcripts, interrupted responses, and speech-output metadata.
- Ensure routine voice memory candidates can auto-promote as observations, while
  decision/preference-like candidates still use reviewed drafts.
- Include interruption and latency summaries as inspectable sleep context but avoid
  promoting raw partial transcripts into durable memory.
- Commit consolidated brief and memory-store updates through the existing
  manifest-last protocol.

Verification:

- Sleep over a voice session writes `memory-store.json`, `consolidated-brief.json`,
  archive brief, and updated `continuity-manifest.json`.
- Sleep over a session containing only interrupted exchanges with no completed
  assistant output produces a coherent brief and does not crash on empty response
  text.
- Next voice run resumes from `ConsolidatedBrief` and injects the brief into the
  first model context.
- `cargo test sleep --lib`
- `cargo test text_owned_voice_loop --lib`
- `cargo test realtime_voice_session --lib`

Docs to update:

- Update `docs/Architecture/Architecture.SleepPhase.md` Implementation Status.
- Update `docs/Architecture/Architecture.MemorySystem.md` if voice memories now use
  the shared store end to end.
- Confirm the auto-promote vs reviewed-draft boundary for voice candidates still
  matches `docs/Architecture/Architecture.MemorySystem.md`.
- Add the required diary entry for sleep consuming voice sessions.

## Phase 0 Resolutions

These were originally tracked as open questions and are now ratified by this
plan. They are implementation-plan resolutions, not DecisionLog commitments. The
detailed rationale lives in [Design Choices For This Plan](#design-choices-for-this-plan).

- **`Turn` vs `Exchange` migration.** `Exchange` is now the shared runtime source of
  truth for text and text-owned voice. Completed text-owned exchanges still derive
  persisted `Turn` records for compatibility until realtime-specific exchange
  persistence and sleep consumption land. See [Session Unit](#session-unit).
- **Default state directory.** One shared `state/session/` directory for both loops,
  with a read-only fallback to `state/text-loop/` until the next sleep commit. A voice
  run and a text run are one continuous session over this directory. The text loop
  stays a first-class experiment; only its persistence path moves. See
  [Default State Directory](#default-state-directory).
- **Realtime provider preambles.** Separate output category on the active
  `Exchange` (`provider_preamble` / `provider_events`); persisted and
  observable, but never fed into QSF prompt assembly. See
  [Provider Preambles](#provider-preambles).
- **Persisted state compatibility.** Serde defaults in Phase 1, explicit
  `schema_version` in Phase 2, one-shot upgrader in Phase 6; no back-compat
  for partial/live state. See
  [Persisted State Compatibility](#persisted-state-compatibility).

Open items remaining:

- **Phase 6 shared resolver move.** Both voice experiments already route through
  `resolve_shared_state_directory_from_env`; full voice <-> text round-trip continuity
  over one `state/session/` directory still waits for the multi-turn text loop to move
  off the silent `state/text-loop` default onto the same resolver.
- **Phase 7 sleep consumption.** Sleep still needs to consume shared `Exchange` records
  containing voice transcripts, interruptions, and speech-output metadata.

## Human Testing Points

- During Phase 5, inspect a simulated realtime session and confirm provider
  interruption facts become shared exchange state rather than observability-only
  records.
- After Phase 6, decide whether the new default experiment name and CLI behavior
  make voice feel like the main loop while keeping typed text comfortable.

## Final Verification For The Full Plan

When the implementation is complete, run:

```text
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Also run one deterministic simulated voice session, one typed-text session through
the shared loop, and one sleep pass over a voice session. Real microphone/OpenAI
testing is recommended but should remain opt-in through provider environment
variables.

## Documentation Checklist

Per `docs/ProjectFrame/ProjectWorkflow.md`, implementation phases should update:

- `docs/EngineeringDiary.md` for each logical application code change.
- Affected experiment specs or reports, especially
  `docs/Experiments/Experiment.TextOwnedVoiceLoop.md` and
  `docs/Experiments/Experiment.RealtimeVoiceSessionMVP.md` if those files are
  present when implementation begins.
- `docs/Concepts/Concept.RealtimePresence.md` when implemented interruption
  behavior changes the concept-level framing.
- `docs/Architecture/Architecture.RuntimeLoop.md` when shared state lands.
- `docs/Architecture/Architecture.AudioLoop.md` when voice adopts continuity and
  interruption policy.
- `docs/Architecture/Architecture.MemorySystem.md` when voice uses the shared memory
  store by default.
- `docs/Architecture/Architecture.SleepPhase.md` when sleep consumes voice sessions.
- `docs/Architecture/Architecture.Overview.md` and `README.md` when voice becomes the
  primary surfaced loop.
- `docs/ProjectFrame/DocumentStatus.md` only if this work changes document kind,
  maturity-tag, or status-section conventions; otherwise follow its existing
  guidance when updating Architecture docs.
- `docs/DecisionLog.md` only for durable commitments, not for ordinary plan progress.
  Phase 6's shared `state/session/` directory move is a user-visible default change and
  needs a decision entry.

## Risks

- Migrating `Turn` too aggressively could destabilize the mature text-loop tests.
  Prefer adapters or serde-compatible intermediate structures if needed.
- Persisting too much partial audio state could turn noisy provider artifacts into
  false memory. Keep partials live/observability-first unless finalized.
- Realtime provider-owned responses can blur the QSF-owned model boundary. Keep that
  as an explicit experiment mode, not the default shared loop.
- Moving the default state directory can confuse existing local continuity. Add
  clear compatibility behavior and event payloads that reveal the state path used.
- Realtime provider behavior could leak into shared reducers if provider payloads are
  converted ad hoc. Keep the Phase 0 adapter boundary as the only reducer entry path.
- Defaults can drift back to fixture/compatibility paths during incremental work.
  Each phase must name its default configuration and test that the default exercises
  the new code path.
- Text-loop memory behavior is now broad enough that a "state only" migration can
  accidentally regress side effects. Treat live capture, reinforcement,
  processed-range idempotency, and sleep safety-net handoff as part of the behavior
  surface when comparing old `Turn` and new `Exchange` paths.

## Refs

- `docs/Plans/Idea.VoiceLoopUnification.md`
- `docs/Plans/Plan.MultiTurnTextLoop.md`
- `docs/Architecture/Architecture.RuntimeLoop.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Architecture/Architecture.SleepPhase.md`
- `docs/ProjectFrame/ProjectWorkflow.md`
- `docs/DecisionLog.md`
- `crates/qsf_app/src/session/`
- `crates/qsf_app/src/memory/live_capture.rs`
- `crates/qsf_app/src/memory/co_retrieval.rs`
- `crates/qsf_app/src/memory/processed_ranges.rs`
- `crates/qsf_memory/src/processed_range.rs`
- `crates/qsf_app/src/sleep/proposers/`
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs`
- `crates/qsf_app/src/experiments/realtime_voice_session.rs`
- `crates/qsf_app/src/audio/voice_session_provider.rs`
