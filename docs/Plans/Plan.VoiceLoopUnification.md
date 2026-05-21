# Plan: Voice Loop Unification

## Status

Planned. No implementation has started yet.

This plan promotes `Idea.VoiceLoopUnification.md` into an incremental implementation
path. The strategic direction is that voice becomes the primary live loop while typed
text becomes an optional input surface over the same session model.

## Background

The current project has two useful but separate shapes:

- `multi-turn-text-loop` owns the mature `SessionState`, cross-session resume,
  memory-store reinforcement, consolidated-brief boot, warm summaries, and sleep
  handoff.
- `text-owned-voice-loop` and `realtime-voice-session` prove audio provider
  boundaries, transcript events, speech-output events, realtime interruption facts,
  and provider tool-call routing, but they do not yet own durable session state.

The next implementation should not bolt persistence onto each voice experiment. It
should extract a shared live-session core that can accept text and voice input events,
then let the voice loop adopt that core as the normal runtime path.

## Current Anchors

Code anchors:

- `crates/qsf_app/src/session/mod.rs` defines the text-biased `SessionState`,
  `Turn`, `TurnSummary`, `SessionConfig`, and `SessionEvent`.
- `crates/qsf_app/src/session/{manifest,resume,persistence,continuation}.rs` own
  continuity boot, atomic state persistence, resume classification, and awake
  continuation cleanup.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` is the reference
  implementation for reducer-driven session state, prompt assembly, memory-store
  reads/reinforcement, manifest updates, and consolidated-brief injection.
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs` is the current voice
  path that routes finalized speech through QSF-owned model behavior and speech
  output.
- `crates/qsf_app/src/experiments/realtime_voice_session.rs` records realtime
  provider lifecycle, tool requests, interruptions, and speech playback events.
- `crates/qsf_app/src/audio/voice_session_provider.rs` already represents
  realtime transcripts, responses, interruptions, and provider tool-call requests.

Documentation anchors:

- `docs/Architecture/Architecture.RuntimeLoop.md` records the reducer/event/state
  discipline and notes interruption handling as not yet implemented in the live loop.
- `docs/Architecture/Architecture.AudioLoop.md` describes the voice interaction
  controller, simulation bridge, playback controller, and interruption policy.
- `docs/Architecture/Architecture.SleepPhase.md` notes that voice-loop session
  consumption by sleep is not yet implemented.
- `docs/Architecture/Architecture.MemorySystem.md` notes that voice-loop
  participation in the shared continuity memory store is not yet implemented.

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

Migration shape (resolved in Phase 0): `Turn` stays as the durable serialized
shape during Phase 1 and Phase 2. `Exchange` is introduced alongside it, and the
text loop populates both from a single source (an `Exchange` is built first,
then a `Turn` is derived via `From<&Exchange>`). Phase 3 makes `Exchange` the
canonical persisted unit and reduces `Turn` to a read-only adapter for any
legacy callers. Phase 7 removes the last direct `Turn` consumer.

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
- Phase 6 (default state directory move) adds an `upgrade_state_if_needed()`
  step at boot that rewrites v1 -> v2 in place and logs the migration. The
  binary refuses to read schema versions newer than it supports.
- No back-compat promise for partial/live state (active transcripts, playback
  markers, listening/speaking phase). Those are cleared on awake resume per
  the interruption rules above.

## Phase 0: Design Tightening And Inventory

Goal: remove ambiguities before code moves.

Work:

- Confirm the resolutions captured in [Design Choices For This Plan](#design-choices-for-this-plan):
  session unit and migration shape, default state directory and compatibility,
  provider preambles, and persisted state compatibility / `schema_version`.
  Record any deviation here before Phase 1 starts.
- Write `docs/Plans/Design.VoiceLoopUnification.md` if implementation reveals more
  than this plan should settle.
- Define an adapter boundary for provider facts before they enter reducers:
  provider/session data in, shared reducer events out. The reducer must not depend
  on OpenAI-specific event names, timing quirks, or provider payload structure.
- Produce a mapping table from existing `SessionEvent` variants in
  `crates/qsf_app/src/session/mod.rs` (`SessionStarted`, `InputReceived`,
  `MemoryRetrieved`, `ContextAssembled`, `PromptAssembled`,
  `ModelRoleCompleted`, `ModelRoleFailed`, `TurnCompleted`, `TurnSummarized`,
  `ToolCompleted`, `SessionLimitReached`, `SessionEnded`) to the shared
  reducer event set, plus the new variants needed for voice
  (`AudioPartialTranscript`, `AudioFinalTranscript`, `OutputProduced`,
  `SpeechPlayback*`, `UserInterrupted`, realtime response lifecycle).
- Define which fields in `SessionConfig` are shared and which are modality-specific.
- Capture event payload fields that Phase 4 / Phase 5 will need (`response_id`,
  `exchange_index`, `utterance_id`, revision counter) so the shape is fixed once.

Verification:

- No code required.
- Human review recommended: confirm `Exchange` is the right durable unit and that
  boot-time brief loading is acceptable for early voice latency experiments.
- Phase 0 exits only when every item in [Phase 0 Resolutions](#phase-0-resolutions)
  is either ratified or explicitly overridden in this plan.

Docs to update:

- This plan only, unless a separate design note is created.

## Phase 1: Extract Shared Live-Session State

Goal: create a shared state model that can represent both text and voice without
changing runtime behavior yet.

Work:

- Add shared structs in `crates/qsf_app/src/session/`, likely in new modules such as
  `exchange.rs` and `runtime_phase.rs` or `session_state.rs`, while keeping
  `mod.rs` thin.
- Add `Exchange`, `ExchangeInput`, `ExchangeOutput`, `UtteranceRecord`,
  `InterruptionRecord`, `RuntimePhase`, and status enums.
- Add reducer events for voice-shaped lifecycle facts, or add conversion helpers
  from existing `EventType` payloads into `SessionEvent` values. Event payloads
  must include the Phase 0 fields needed later by interruption/realtime work:
  `exchange_index`, `utterance_id`, revision counter, and `response_id` where
  relevant.
- Keep reducers pure and unit-test the state transitions directly.
- Preserve existing text `SessionState` serialization during this phase, or add
  serde defaults so old state files still load.

Verification:

- `cargo test session --lib`
- `cargo test multi_turn_text_loop --lib`
- Regression tests for: text exchange completion, partial transcript update, final
  transcript commit, interrupted response cleanup, and serde round-trip.
- Regression test with a checked-in pre-migration `SessionState` JSON fixture that
  proves existing persisted text state still loads.

Docs to update:

- Refresh the Implementation Status section in
  `docs/Architecture/Architecture.RuntimeLoop.md` if the shared state module lands.
- Add the required diary entry for the shared-state extraction.

## Phase 2: Move Text Loop Onto The Shared Core

Goal: prove the shared model can preserve the mature text behavior before voice uses
it.

Work:

- Adapt `multi_turn_text_loop` to create completed text `Exchange` records, while
  preserving existing prompt hash, warm summary, recall, and manifest behavior.
- Keep report output stable enough that previous text-loop diagnostics remain useful.
- Keep `TurnSummary` behavior intact; only rename or reshape it if the shared state
  model makes the old name actively misleading.
- Persist through the existing manifest-last state protocol.

Verification:

- Existing multi-turn text tests pass unchanged where possible.
- Add a resume test proving a completed text exchange survives reload and produces
  the same prompt prefix behavior as the old `Turn` path.
- `cargo test multi_turn_text_loop --lib`
- `cargo test sleep --lib`
- `cargo build`

Docs to update:

- Update `docs/Architecture/Architecture.RuntimeLoop.md` with the new shared state
  shape and any compatibility notes.
- Add the required diary entry for the text-loop migration.

Rollback:

- Keep the old `Turn` serialization path until Phase 3 promotes `Exchange`, so Phase
  2 can revert to the previous prompt/session behavior without migrating local state
  back from a new schema.

## Phase 3: Give Text-Owned Voice The Shared Session Core

Goal: make the deterministic voice path resumable before tackling full realtime
complexity.

Work:

- Replace the local one-shot session handling in `text_owned_voice_loop` with shared
  live-session boot: load manifest, classify resume mode, emit `SessionResumed`, and
  apply `SessionStarted` through the reducer.
- Convert `AudioFinalTranscript` into a voice `ExchangeInput` through the shared
  reducer, then complete an `Exchange` after `OutputProduced` and
  `SpeechPlaybackCompleted`.
- Load memory from the shared `MemoryStore` under the chosen state directory instead
  of the voice-only fixture by default. Keep fixture/file options for deterministic
  experiments when explicitly configured.
- Use the `state/session` resolution rules from
  [Default State Directory](#default-state-directory) during this phase. Do not
  introduce a separate `state/voice-loop` directory.
- Persist session state and update the manifest after a successful exchange.
- Inject `ConsolidatedBrief` into first voice prompt/context using the same contract
  the text loop uses.

Verification:

- Regression test: simulated transcript produces one completed voice exchange and a
  manifest pointing at the persisted state.
- Regression test: a second simulated run resumes or starts from consolidated brief
  according to the manifest.
- Regression test: voice uses the cross-session memory store by default and still
  supports explicit fixture mode.
- Acceptance criterion: with no voice-specific memory env vars set, the simulated
  voice path loads the shared `MemoryStore`, so the default path exercises shared
  continuity.
- Regression test: a simulated voice run followed by a text run with the same state
  directory produces one continuous session history rather than two continuity
  universes.
- `cargo test text_owned_voice_loop --lib`
- `cargo test session --lib`
- `cargo build`

Docs to update:

- Update `docs/Architecture/Architecture.AudioLoop.md` Implementation Status to say
  the text-owned voice loop participates in shared session continuity.
- Update `docs/Architecture/Architecture.MemorySystem.md` to remove or narrow the
  voice-loop continuity gap.
- Update `docs/Experiments/Experiment.TextOwnedVoiceLoop.md` if present, or add a
  follow-up report if the experiment doc does not exist.
- Add the required diary entry for voice adopting shared continuity.

Rollback:

- Keep fixture/file voice memory sources explicit and tested, so a broken shared-store
  voice path can be disabled for investigation without removing the shared state
  structs.

## Phase 4: Add Live Interruption State

Goal: make interruption a first-class session state transition, not only a realtime
provider report artifact.

Work:

- Route `UserInterrupted` into the shared reducer.
- Represent interrupted output on the active exchange with response id, detected
  time, source, action, and preserved partial response text when available.
- Add deterministic policy for the first implementation:
  `user speech while speaking -> stop/mark interrupted -> capture next finalized input`.
- Ensure `prepare_awake_continuation` clears volatile active audio state and keeps
  durable interruption records.
- Add event/log payload context sufficient to identify response id, exchange index,
  and whether playback was stopped, ignored, or already complete.
- Emit `engine_logging` records for interruption detection and outcome with
  `session_id`, `exchange_index`, `response_id`, source, and stop-vs-ignore result.

Verification:

- Reducer tests for interruption during speaking, interruption after completion,
  and interruption followed by a new finalized input.
- Resume test for shutdown after interruption with only partial user speech.
- `cargo test realtime_voice_session --lib`
- `cargo test text_owned_voice_loop --lib`

Docs to update:

- Update `docs/Architecture/Architecture.AudioLoop.md` interruption section with
  implemented policy and remaining limitations.
- If the deterministic interruption policy survives Phase 5 review, add a
  `docs/DecisionLog.md` entry before treating it as a project rule.
- Update `docs/Concepts/Concept.RealtimePresence.md` if the implemented behavior
  changes the concept's expectations for presence or barge-in handling.
- Add the required diary entry for live interruption state.

## Phase 5: Bridge Realtime Voice Into The Shared Core

Goal: keep realtime provider richness while ensuring QSF owns session continuity,
memory, tools, and durable state.

Work:

- Convert realtime provider session facts into shared reducer events instead of only
  recording observability events.
- Keep provider tool calls as `ToolRequested` records routed through QSF permission
  boundaries; do not auto-execute them from the provider session.
- Treat provider-owned response text/audio as an adapter output unless an experiment
  explicitly tests provider-owned cognition.
- Store final transcript, response lifecycle, interruptions, and tool-call requests
  in the durable exchange shape.
- Use the provider adapter boundary defined in Phase 0 so provider-specific quirks
  remain outside pure reducers.
- Implement realtime provider preambles as the separate `provider_preamble` /
  `provider_events` output category on the active `Exchange`, as resolved in
  [Provider Preambles](#provider-preambles). Preambles are persisted but never
  fed back into QSF prompt assembly.

Verification:

- Simulated realtime session creates a persisted voice exchange with interruption
  and tool-request records.
- Tool-call requests remain non-executed unless routed through the existing QSF tool
  path.
- Regression test: a provider-emitted tool call does not mutate session state and
  does not trigger side effects when no QSF route is configured.
- Phase exit criterion: provider preambles are implemented as the separate output
  category described in [Provider Preambles](#provider-preambles), or this plan is
  updated with the replacement design before Phase 5 is marked complete.
- `cargo test realtime_voice_session --lib`
- `cargo build`

Docs to update:

- Update `docs/Architecture/Architecture.AudioLoop.md` and
  `docs/Architecture/Architecture.RuntimeLoop.md` to reflect realtime participation
  in shared state.
- Update `docs/Experiments/Experiment.RealtimeVoiceSessionMVP.md` if present, or add
  a follow-up report if the experiment doc does not exist.
- Add the required diary entry for the realtime bridge.

## Phase 6: Make Voice The Primary Loop Surface

Goal: rename and organize runtime entry points so voice is the normal loop and text
is an optional input mode.

Work:

- Introduce a stable experiment/runtime name such as `live-loop` or `voice-loop`,
  avoiding temporary phase names.
- Let the new entry point default to voice-capable behavior with deterministic
  simulated providers unless real providers are explicitly selected.
- Add a transition override such as `QSF_PRIMARY_LOOP=voice|text`, with `voice` as
  the default once Phase 6 lands so the default path exercises the new primary loop.
- Keep typed text as an input source for the same loop, not a separate continuity
  universe.
- Migrate default state directory to `state/session` with the compatibility
  behavior defined in [Default State Directory](#default-state-directory),
  and add the `schema_version` upgrader described in
  [Persisted State Compatibility](#persisted-state-compatibility).
- Emit a boot event and `engine_logging` record that names the chosen loop mode,
  state directory, and whether legacy `state/text-loop` compatibility was used.
- Keep `multi-turn-text-loop` as a compatibility or focused text experiment until
  its unique test value is exhausted.

Verification:

- `cargo run -p qsf_app -- list-experiments` shows the new entry point and existing
  experiments remain discoverable.
- Simulated default run completes without real audio credentials.
- Text input mode and voice input mode produce exchanges in the same persisted
  session shape.
- `cargo build`
- `cargo test`

Docs to update:

- Update `README.md` run instructions if the primary experiment changes.
- Update `docs/Architecture/Architecture.Overview.md` once the main-loop surface
  changes.
- Update `docs/Architecture/Architecture.AudioLoop.md`,
  `docs/Architecture/Architecture.RuntimeLoop.md`, and
  `docs/Architecture/Architecture.SleepPhase.md` status sections.
- Add a `docs/DecisionLog.md` entry for the primary-loop rename/default and default
  state-directory migration if Phase 6 makes those user-visible defaults.
- Add the required diary entry for the primary-loop surface change.

Rollback:

- Keep `QSF_PRIMARY_LOOP=text` and the old experiment registration available during
  the transition so the primary-loop default can be backed out without losing shared
  persisted state.

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

- **`Turn` vs `Exchange` migration.** `Turn` stays as the serialized shape
  during Phase 1 and Phase 2; `Exchange` is added alongside it and is the
  single source of truth in code, with `Turn` derived via `From<&Exchange>`.
  Phase 3 promotes `Exchange` to the canonical persisted unit. See
  [Session Unit](#session-unit).
- **Default state directory.** `state/session/` (modality-neutral), with a
  read-only fallback to `state/text-loop/` until the next sleep commit. See
  [Default State Directory](#default-state-directory).
- **Realtime provider preambles.** Separate output category on the active
  `Exchange` (`provider_preamble` / `provider_events`); persisted and
  observable, but never fed into QSF prompt assembly. See
  [Provider Preambles](#provider-preambles).
- **Persisted state compatibility.** Serde defaults in Phase 1, explicit
  `schema_version` in Phase 2, one-shot upgrader in Phase 6; no back-compat
  for partial/live state. See
  [Persisted State Compatibility](#persisted-state-compatibility).

Open items remaining (track here as they appear; none at the start of Phase 1):

- _none_

## Human Testing Points

- After Phase 3, run a simulated voice session twice and inspect whether the second
  run feels like continuation rather than a fresh one.
- After Phase 4, manually test interruption timing with a simulated interruption
  first, then with a real microphone path when local device setup is available.
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
  Phase 6's primary-loop/default-state-dir change is expected to need a decision
  entry if it changes user-visible defaults.

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
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs`
- `crates/qsf_app/src/experiments/realtime_voice_session.rs`
- `crates/qsf_app/src/audio/voice_session_provider.rs`
