# Plan: Voice Loop Unification

## Status

Phases 0-6 are complete. Phase 7 (sleep consumes voice sessions) is the next and
final implementation step.

Completed phase placeholders are intentionally short below; the durable design
contracts are kept in [Design Choices For This Plan](#design-choices-for-this-plan),
[Phase 0 Resolutions](#phase-0-resolutions), the architecture docs, and the code.

Phase 6 (commit d177ba5) routed the multi-turn text loop onto the shared
`state/session` resolver and added a first-class `voice-loop` peer experiment, so a
voice run and a text run now read and append one continuous session over a single
shared `state/session/` directory, with legacy `state/text-loop/` continuity kept
read-only.

Remaining gap before Phase 7: sleep summarization still reads only the legacy
`SessionState.turns` records. Both `session_sleep_input`
(`crates/qsf_app/src/experiments/sleep_phase_session_summary.rs`) and the safety-net
co-retrieval proposer (`crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs`)
iterate `session.turns` directly. Realtime voice sessions persist their content as
`SessionState.exchanges` (carrying voice transcripts, interruptions, provider
preambles, and speech-output metadata) and never derive `Turn` records, so sleep is
currently blind to realtime voice content. Phase 7 teaches sleep to read shared
`Exchange` records so voice sessions consolidate the same way text sessions do.

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

Phase 6 gave voice a first-class peer experiment and routed both loops onto the
single shared `state/session` resolver, so a voice run and a text run already
continue one session instead of two continuity universes. The remaining
implementation step makes the sleep phase consume voice sessions: sleep must read
shared `Exchange` records (voice transcripts, interruptions, and speech-output
metadata), not only the legacy `Turn` records, so a voice run consolidates into the
memory store and consolidated brief the same way a text run does.

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
- `crates/qsf_app/src/session/state_directory.rs` implements
  `resolve_shared_state_directory_from_env`, the shared resolver with the read-only
  `state/text-loop` -> `state/session` fallback. As of Phase 6 all runtime loops
  (multi-turn text, text-owned voice, realtime voice, voice-loop, and sleep) resolve
  through it.
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
which cannot be represented by `Turn` (interruptions, provider preambles, voice
utterances), and realtime voice persists them only as `Exchange`. Phase 7 makes
sleep — the last `Turn`-only reader in the sleep consolidation path — read `Exchange`
records too, so realtime voice content stops being invisible to consolidation. (Other
direct `Turn` consumers and writers remain and are required: the multi-turn text loop
and text-owned voice loop still write derived `Turn` records, the reducer records
`TurnCompleted` into `state.turns`, and awake continuation reads `state.turns.len()`.
Collapsing those paths to exchanges-only is out of Phase 7 scope.)

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

Resolved in Phase 6 (commit d177ba5): the shared resolver
`session::state_directory::resolve_shared_state_directory_from_env` implements the
read-only `state/text-loop` -> `state/session` fallback below, and every runtime loop
(multi-turn text, both voice experiments, the `voice-loop` peer, and sleep) now boots
through it via `session::boot_session`. The `multi-turn-text-loop` stayed a
first-class experiment; only its persistence directory moved (once, via the read-only
fallback below). The historical drift note is retained for context: before Phase 6,
the text loop defaulted to `state/text-loop` through
`session::resume::state_dir_from_env()` on a single-directory boot path.

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
- Implemented in Phase 6 (default state directory move): the in-memory upgrade is
  durable, and v1 -> v2 is never rewritten in place unconditionally. A direct rewrite
  of the loaded file happens only when `resume_state_dir == persist_state_dir`. When
  `legacy_fallback_used` is true (`resume_state_dir == state/text-loop`,
  `persist_state_dir == state/session`), the legacy file is left untouched and the
  upgraded copy is persisted to `persist_state_dir` through the normal manifest-last
  commit. Regression tests assert the legacy v1 file is byte-for-byte unchanged after
  such a boot while the new `state/session` copy is v2.
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

Resolved in Phase 6: full voice-to-text round-trip continuity now works, because the
text loop moved onto the shared `state/session` resolver and both loops share one
continuous session.

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

## Phase 6: Voice Loop As A Peer Surface — COMPLETE

Complete (commit d177ba5). The multi-turn text loop now boots through the shared
`session::state_directory::resolve_shared_state_directory_from_env` resolver and
`session::boot_session`, so legacy `state/text-loop/` continuity is read-only and the
first manifest-last commit writes `state/session/`. The in-memory schema upgrade is
made durable by persisting the upgraded copy to `persist_state_dir` without rewriting
the legacy file when `legacy_fallback_used` is true; incomplete `state/session/`
directories no longer mask legacy continuity, and legacy memory-store records are
merged forward through continuity persistence (text, voice, realtime voice, and sleep)
rather than copied eagerly at boot. Sleep also routes through the shared resolver so
legacy reads and shared writes stay separate.

A stable `voice-loop` experiment was added as a thin peer surface that reuses the
text-owned voice pipeline (so the loops share one behavior code path), registered
alongside the unchanged `multi-turn-text-loop`, `text-owned-voice-loop`, and
`realtime-voice-session` experiments. There is no `QSF_PRIMARY_LOOP` default and the
text loop stays first-class. A `docs/DecisionLog.md` entry for the shared
`state/session/` directory move landed in the same commit.

Targeted verification passed on 2026-06-03: `cargo build`, `cargo test`,
`cargo nextest run`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt`;
`cargo run -p qsf_app -- list-experiments` shows `voice-loop`, and a simulated
`voice-loop` run completed with an isolated `QSF_STATE_DIR`. Regression coverage added
for the cross-surface voice<->text shared session, no boot-time shared-directory
materialization, text/voice memory copy-forward, incomplete shared-directory fallback,
and sleep legacy-fallback writes (legacy v1 file byte-for-byte unchanged, upgraded v2
copy plus migrated memory store written to `state/session/`).

Rollback remains available: because `state/text-loop/` is never rewritten in place,
the directory move can be backed out by pointing `QSF_STATE_DIR` at the old path, and
the `voice-loop` experiment can be disabled independently.

## Phase 7: Sleep Consumes Voice Sessions

Goal: close the continuity loop by making sleep summarize and commit voice exchanges
the same way it handles text sessions today. This is the last phase, and it removes
the last `Turn`-only reader in the sleep consolidation path: when it lands, a realtime
or text-owned voice session consolidates into the memory store and consolidated brief,
and the next voice run resumes from that brief. (Non-sleep `Turn` consumers and writers
remain and are required — awake-continuation limit recomputation reads
`state.turns.len()`, the reducer records `SessionEvent::TurnCompleted` into
`state.turns`, and both text loops still derive `Turn` records for persistence. Phase 7
does not touch those.)

### Current state going in

- Sleep reads `SessionState.turns` in exactly two runtime places, and both ignore
  `SessionState.exchanges`:
  - `session_sleep_input` -> `session_sleep_input` builds the summarizer transcript by
    iterating `session.turns` only
    (`crates/qsf_app/src/experiments/sleep_phase_session_summary.rs`, around the
    `Completed turns:` loop). It also renders `session.summarized_turns` warm
    summaries.
  - The safety-net co-retrieval proposer builds its mechanical cross-turn association
    coverage from `session.turns` and per-turn `recalled_turns`
    (`crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs`).
- Realtime voice (`crates/qsf_app/src/experiments/realtime_voice_session.rs`) persists
  its content **only** as `SessionState.exchanges` and never derives `Turn` records
  (its tests assert `persisted_state.turns.is_empty()` and
  `persisted_state.exchanges.len() == 1`). So realtime voice transcripts,
  interruptions, provider preambles, and speech-output metadata are invisible to sleep
  today.
- The multi-turn text loop and the text-owned voice loop still persist derived `Turn`
  records (`session.turns`), not exchanges. They are unaffected by this gap but must
  keep working unchanged.
- `Exchange` (`crates/qsf_app/src/session/exchange.rs`) already carries everything
  sleep needs: `ExchangeInput::Voice { final_transcript, utterances }` /
  `ExchangeInput::Text`, `ExchangeOutput { text, response_id, audio_marker, .. }`,
  `interruptions: Vec<InterruptionRecord>`, `provider_events: Vec<ProviderEventRecord>`
  (preambles + lifecycle), `recalled_items`, `retrieved_memory_block`, `model`
  (latency/tokens), and a `status` (`Completed`, `Interrupted`, `Failed`, etc.).
  `output` is `Option`, so an interrupted exchange may have no completed assistant
  text.
- `commit_cross_session_sleep` already commits through the manifest-last protocol and,
  as of Phase 6, persists continuity to `persist_state_dir` on legacy fallback before
  loading the memory store and building the promotion plan
  (`crate::sleep::auto_promote::build_promotion_plan`). The commit path itself does not
  need re-plumbing; only the *reading* of session content does.

### Open questions to resolve before/while implementing

1. **Read both `turns` and `exchanges`, or unify writes first?** Realtime voice writes
   exchanges; text loops write turns. The pragmatic, in-scope choice is for sleep to
   read a **unified view of both** so no content is dropped regardless of which loop
   produced it. Do **not** order by vector index: indexes are not globally unique across
   the two vectors. Realtime voice assigns `exchange_index = state.turns.len() +
   state.exchanges.len()`, while text and text-owned voice derive turn indexes from
   `state.turns.len()` alone, so after a voice-first session a later text turn can reuse
   index `0`. Order the merged view **chronologically by `started_at` (tie-break on
   `completed_at`, then a stable kind/index tie-breaker)** — both `Turn` and `Exchange`
   carry timestamps. Collapsing the text/text-owned-voice write path to exchanges-only
   (so `turns` disappears) is explicitly **out of scope** here — call it out but do not
   attempt it in Phase 7. Recommended: add a shared read-only helper (e.g.
   `SessionState::sleep_records()` or a small iterator over a normalized `SleepRecord`)
   so both `session_sleep_input` and the safety-net proposer consume one chronological
   representation instead of duplicating the turns-vs-exchanges branch.
2. **Does the safety-net proposer need exchange coverage in this phase, or only the
   summarizer transcript?** Voice exchanges carry `recalled_items` (the `Exchange`
   analogue of `Turn.recalled_turns`), so they can participate in mechanical
   association safety-net coverage. Decide whether Phase 7 extends
   `safety_net_co_retrieval` to exchanges now, or whether that is deferred (and, if
   deferred, record it so voice association coverage is not silently missing). Default
   recommendation: extend it now via the same shared normalized view, since "preserve
   sleep-side safety-net coverage" is a cross-cutting acceptance criterion.
3. **Warm summaries for voice.** `summarized_turns` is text-loop machinery. Voice
   exchanges are not warm-summarized today. Phase 7 should still surface completed
   voice exchanges in the transcript even when they were never warm-summarized; decide
   whether any exchange-side warm summary is needed (recommended: no, out of scope —
   the summarizer sees full exchange text).

### Work

This phase has two separable, independently testable steps. Land and verify them in
order.

#### Phase 7a: Make Sleep Read Shared Exchange Records

- Introduce one normalized, read-only sleep view over `SessionState` that yields both
  derived-`Turn` content and `Exchange` content in **chronological order by `started_at`**
  (tie-break on `completed_at`, then a stable kind/index tie-breaker), not by vector
  index — see open question 1 for why index order is unreliable across a mixed session.
  Expose for each record: user input text, assistant output text (may be empty),
  retrieved memory block, recalled item references, and—for voice—`final_transcript`,
  interruption records, and provider-preamble/lifecycle metadata (the last surfaced only
  to the non-promotable diagnostic channel, never to promotable transcript text). Keep it
  a pure function/iterator on the session type (no I/O), consistent with the
  reducer/state discipline.
- Update `session_sleep_input` to build the summarizer transcript from that view:
  render completed text turns as today, and add a `Voice exchange N:` section that
  includes the final transcript, the assistant/spoken response (or an explicit
  `(no completed response)` marker when `output` is `None`), and interruption count and
  outcomes.
- Keep provider preambles out of the promotable path entirely. `SleepInputBundle.session_text`
  is inserted verbatim into the sleep summarizer user prompt
  (`crates/qsf_app/src/sleep/session_summary.rs`), and the summarizer report is then
  committed into promoted memory records and the consolidated brief
  (`commit_cross_session_sleep` in
  `crates/qsf_app/src/experiments/sleep_phase_session_summary.rs`). So merely labeling a
  preamble "provider context" inside `session_text` does **not** enforce the
  [Provider Preambles](#provider-preambles) boundary — the summarizer could still echo it
  into `memory_candidates`, `future_context_hints`, or
  `ConsolidatedBrief.previous_session_summary`. Route provider preambles (and raw partial
  transcripts) to a **separate non-promotable channel** — either as review/diagnostic
  notes that are not fed into the summarizer prompt at all, or, if they must reach the
  summarizer for latency context, through an explicit filter that strips preamble text
  from the promotable report fields before commit. Either way add regression tests
  proving provider-preamble text cannot appear in `memory_candidates`,
  `future_context_hints`, or `ConsolidatedBrief.previous_session_summary`.
- Extend the safety-net co-retrieval proposer to the same normalized view so voice
  exchanges contribute `recalled_items` to mechanical association coverage (or
  explicitly defer per open question 2, with a recorded note).
- Guard the empty/interrupted case: an exchange with `status = Interrupted` and
  `output = None` must produce coherent transcript text and must not panic on empty
  response strings anywhere in the sleep path.
- Add `engine_logging` context for the voice-aware path: at minimum `session_id`, and
  per record `exchange_index`/turn index, `status`, and interruption count, so a sleep
  run over a voice session is debuggable after the fact.

Verification (7a):

- A new unit test on the normalized sleep view: a `SessionState` carrying one
  completed text `Turn` and one completed voice `Exchange` yields both in the
  transcript, with the voice final transcript and assistant response present.
- Chronology regression coverage for **both** orderings: a text-then-voice persisted
  state and a voice-then-text persisted state each yield records in true session order
  (by timestamp), proving index-based ordering bugs (reused index `0`) are caught.
- A boundary regression test: a session whose voice exchange carries a provider
  preamble runs through the full sleep path, and the test asserts the preamble text is
  absent from `memory_candidates`, `future_context_hints`, and
  `ConsolidatedBrief.previous_session_summary`.
- A regression test: sleep over a session whose only content is an interrupted voice
  exchange with `output = None` produces a non-empty, coherent `session_text` and does
  not panic.
- `cargo test sleep --lib`
- `cargo test realtime_voice_session --lib`

#### Phase 7b: Promote Voice Memories And Commit Through Manifest-Last

- Confirm the auto-promote vs reviewed-draft split (`crate::sleep::auto_promote`) works
  for candidates derived from voice content: routine voice memory candidates
  auto-promote as observations, while decision/preference-like candidates still go to
  reviewed drafts. The candidate source is the summarizer report, so this mostly means
  verifying the boundary holds once voice content reaches the summarizer — add coverage
  rather than new branching unless a voice-specific gap appears.
- Keep raw partial transcripts and provider preambles out of durable memory: only
  finalized exchange content may become a memory candidate.
- Include interruption and latency summaries as inspectable sleep context (review
  notes / report fields), not as promoted memories.
- Commit the consolidated brief and memory-store updates through the existing
  manifest-last protocol (`commit_cross_session_sleep`) with no new write path; verify
  it already persists continuity correctly when the input session is voice-only.

Verification (7b):

- Sleep over a completed voice session writes `memory-store.json`,
  `consolidated-brief.json`, the archive brief, and an updated
  `continuity-manifest.json`.
- A voice memory candidate classified as routine auto-promotes as an observation; a
  decision/preference-like candidate lands as a reviewed draft (assert against the
  promotion plan).
- The next voice run resumes from `ConsolidatedBrief` and injects the brief into the
  first model context (extend the existing voice resume coverage).
- `cargo test sleep --lib`
- `cargo test text_owned_voice_loop --lib`
- `cargo test realtime_voice_session --lib`

### Whole-phase verification

- `cargo build`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt`
- Human testing point (recommended, opt-in): run one deterministic simulated
  `realtime-voice-session` (or `voice-loop`), then a `sleep-phase-session-summary` over
  it, and inspect `runs/` plus the written `memory-store.json` /
  `consolidated-brief.json` to confirm voice transcripts, interruptions, and latency
  appear as sleep context and that only finalized content became durable memory.

### Docs to update

- Update `docs/Architecture/Architecture.SleepPhase.md` Implementation Status (it
  currently notes voice-loop session consumption by sleep is not yet implemented).
- Update `docs/Architecture/Architecture.MemorySystem.md` to record that voice
  exchanges now flow through the shared memory store end to end, and confirm the
  auto-promote vs reviewed-draft boundary for voice candidates still matches that doc.
- Update `docs/Architecture/Architecture.RuntimeLoop.md` if the normalized sleep view
  changes the documented `Turn`/`Exchange` consumption story.
- Update the experiment specs/reports if present when implementation begins:
  `docs/Experiments/Experiment.RealtimeVoiceSessionMVP.md` and
  `docs/Experiments/Experiment.TextOwnedVoiceLoop.md`.
- Add the required `docs/EngineeringDiary.md` entry for sleep consuming voice sessions.
- No `docs/DecisionLog.md` entry is expected unless implementation surfaces a durable
  commitment (e.g. dropping the `turns` write path), which is out of scope here.

## Phase 0 Resolutions

These were originally tracked as open questions and are now ratified by this
plan. They are implementation-plan resolutions, not DecisionLog commitments. The
detailed rationale lives in [Design Choices For This Plan](#design-choices-for-this-plan).

- **`Turn` vs `Exchange` migration.** `Exchange` is the shared runtime source of
  truth. Realtime voice persists realtime-specific exchanges directly (Phase 5);
  text and text-owned voice still derive persisted `Turn` records for compatibility.
  Phase 7 makes sleep — the last `Turn`-only reader in the sleep consolidation path —
  read `Exchange` records too; the text-side `Turn` write path and other `Turn`
  consumers (reducer, awake continuation) stay. See [Session Unit](#session-unit).
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

- **Phase 7 sleep consumption.** Sleep still reads only `SessionState.turns`
  (`session_sleep_input` and the safety-net co-retrieval proposer). It needs to consume
  shared `Exchange` records containing voice transcripts, interruptions, provider
  preambles, and speech-output metadata, since realtime voice persists content only as
  exchanges. See [Phase 7](#phase-7-sleep-consumes-voice-sessions).

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
