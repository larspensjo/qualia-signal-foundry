# Plan: Voice Loop Unification

## Status

Phase 0 complete as a design/inventory pass.

Phase 1 complete (commit 2a20950): shared `Exchange`, `LiveSessionState`,
`RuntimePhase`, interruption/utterance records, the `reduce_live_session` reducer, a
`TryFrom<&Exchange> for Turn` adapter, and a pre-migration `SessionState` serde
fixture landed in `crates/qsf_app/src/session/{exchange,live_state}.rs`, with
`live: LiveSessionState` added to `SessionState` behind `#[serde(default)]`.

Phase 2 is the next implementation step, and is now the most important phase for the
project goal (see below).

Important gap to close in Phase 2: as of Phase 1 the shared core is parallel, not
load-bearing. `reduce_live_session` is `#[cfg_attr(not(test), allow(dead_code))]`,
and `multi_turn_text_loop` still drives `state.turns` directly — nothing in a
production build constructs an `Exchange`. The refined goal below only holds if
Phase 2 puts the text loop on the shared core; otherwise text-loop improvements will
not propagate to voice.

Code drift reviewed on 2026-05-31. The main conclusion is that the plan direction
still holds, but the text-loop baseline is now richer than when the plan was
written. Phase 1 and Phase 2 must preserve live memory capture, live cross-turn
co-retrieval, `processed_ranges` idempotency, session-end cross-turn flush, and the
sleep association proposer boundary while introducing `Exchange`.

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

The current project has two useful but separate shapes:

- `multi-turn-text-loop` owns the mature `SessionState`, cross-session resume,
  memory-store reinforcement, live memory capture, live cross-turn co-retrieval,
  consolidated-brief boot, warm summaries, session-end cross-turn flush, and sleep
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
  reads/reinforcement, live memory capture, live cross-turn co-retrieval,
  manifest updates, consolidated-brief injection, and session-end flush.
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs` is the current voice
  path that routes finalized speech through QSF-owned model behavior and speech
  output. It still uses a voice-specific `VoiceLoopMemorySource` default rather
  than the shared continuity `MemoryStore`.
- `crates/qsf_app/src/experiments/realtime_voice_session.rs` records realtime
  provider lifecycle, tool requests, interruptions, and speech playback events.
- `crates/qsf_app/src/audio/voice_session_provider.rs` already represents
  realtime transcripts, responses, interruptions, and provider tool-call requests.
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
  discipline, the current per-experiment live state, and interruption handling as
  not yet implemented in the live loop.
- `docs/Architecture/Architecture.AudioLoop.md` describes the voice interaction
  controller, simulation bridge, playback controller, and interruption policy.
- `docs/Architecture/Architecture.SleepPhase.md` notes that voice-loop session
  consumption by sleep is not yet implemented, while text-loop sleep already uses
  proposer-based association handling.
- `docs/Architecture/Architecture.MemorySystem.md` notes that voice-loop
  participation in the shared continuity memory store is not yet implemented, and
  documents the current live/sleep association split.

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

Current drift note, reviewed 2026-05-31: the implemented text-loop default is still
`state/text-loop` through `session::resume::state_dir_from_env()`. Moving the default
resolver to `state/session` is still implementation work, not current behavior. Both
loops use this one resolver — the `multi-turn-text-loop` and the voice loop share a
single continuous session. Phase 3 introduces the shared resolver for voice boot;
Phase 6 routes the text loop onto the same resolver and removes any remaining silent
`state/text-loop` defaults. The text loop stays a first-class experiment; only its
persistence directory moves (once, via the read-only fallback below).

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

Phase 0 outcome: no separate `docs/Plans/Design.VoiceLoopUnification.md` is needed
yet. The adapter boundary, event mapping, config split, and payload fields below are
specific enough for Phase 1. Create the design note only if implementation discovers
a deeper conflict than this plan can carry cleanly.

### Phase 0 Adapter Boundary

Provider-specific data must be normalized before it reaches a shared reducer. The
boundary is:

```text
provider request/session structs and raw provider events
  -> provider adapter normalization
  -> shared live-session reducer events
  -> pure reducer state transition
```

Rules:

- Reducers accept QSF-owned event names and payloads only. They must not branch on
  OpenAI event names such as `response.audio_transcript.delta`,
  `input_audio_buffer.speech_started`, or `response.function_call_arguments.done`.
- Provider adapters may keep raw provider event names in traces or sanitized
  observability payload fields when useful for debugging, but reducer payloads use
  stable fields such as `provider_id`, `utterance_id`, `response_id`, `revision_index`,
  `detected_at_ms`, `status`, and `stop_outcome`.
- `TranscriptProviderSession` maps to input-side facts:
  `AudioInputStarted`, `AudioInputChunkCaptured`, `AudioPartialTranscript`,
  `AudioFinalTranscript`, `AudioInputEnded`, `LatencyMeasurementRecorded`, and then
  `InputReceived`.
- `VoiceProviderSession` maps to realtime facts:
  `RealtimeSessionStarted`, audio input facts, `RealtimePreambleProduced`,
  `RealtimeResponseStarted`, provider `ToolRequested`, `OutputProduced`,
  `SpeechPlaybackRequested`, optional `SpeechPlaybackStarted`, `UserInterrupted`,
  `RealtimeResponseCompleted`, `SpeechPlaybackCompleted`, and
  `RealtimeSessionCompleted` / `RealtimeSessionFailed`.
- Tool calls emitted by a provider are reducer-visible requests only. They are not
  executed unless the QSF tool boundary later routes and permits them.
- Speech playback adapters report playback lifecycle facts. They do not own assistant
  cognition, prompt assembly, memory retrieval, or durable session state.

### Phase 0 Shared Reducer Event Inventory

The shared reducer event set should keep existing text-loop behavior recognizable
while widening payloads to carry exchange and modality context. During Phase 1 and
Phase 2, compatibility events can still be recorded in observability and `Turn` can
still be derived for persistence.

| Current or needed fact | Shared reducer event | Phase 0 payload requirement | Notes |
| --- | --- | --- | --- |
| `SessionStarted(SessionConfig)` | `SessionStarted` | `session_id`, `config`, resolved `state_dir`, `resume_mode` when known | Existing event stays; add path/resume context where boot code has it. |
| `SessionResumed` observability event | `SessionResumed` | `session_id`, `resume_mode`, `state_dir`, predecessor/current state path, downgrade reason if any | Already observable, but should become reducer-visible once live state owns resume cleanup. |
| `InputReceived { input }` | `InputReceived` | `session_id`, `exchange_index`, `input: ExchangeInput`, source modality, `utterance_id` for voice | Text maps to `ExchangeInput::Text`; final speech maps to `ExchangeInput::Voice`. |
| `AudioPartialTranscript` | `AudioPartialTranscript` | `session_id`, `exchange_index` if allocated, `utterance_id`, `utterance_index`, `revision_index`, `source_chunk_index`, `received_at_ms`, transcript, `provider_id` | Updates live state only; not a completed durable exchange. |
| `AudioFinalTranscript` | `AudioFinalTranscript` | `session_id`, `exchange_index`, `utterance_id`, `utterance_index`, `received_at_ms`, final transcript, `provider_id` | Adapter should emit this before `InputReceived`; reducer can collapse it into voice input state. |
| `MemoryRetrieved` | `MemoryRetrieved` | `session_id`, `exchange_index`, retrieved block, recalled memory ids/items, source store path when persistent | Current text event mutates nothing; shared event should attach retrieval to the active exchange. |
| `ContextAssembled(ContextAssembly)` | `ContextAssembled` | `session_id`, `exchange_index`, `ContextAssembly` | Existing context payload carries selected/omitted details. |
| `PromptAssembled { ... }` | `PromptAssembled` | `session_id`, `exchange_index`, `full_request_hash`, `message_count`, `total_bytes` | Keeps cache/debug behavior stable. |
| `ModelRoleCompleted { ... }` | `ModelRoleCompleted` | `session_id`, `exchange_index`, `model_id`, provider/model names where known, latency, token counts, response text, optional model tool calls | Captures QSF-owned model use. |
| `OutputProduced` | `OutputProduced` | `session_id`, `exchange_index`, `response_id`, text, output owner/source, target, produced_at_ms | For text and text-owned voice, this follows `ModelRoleCompleted`; provider-owned realtime output may use it only in explicit realtime experiments. |
| `ModelRoleFailed { error_summary }` | `ModelRoleFailed` | `session_id`, `exchange_index`, role id/model id when known, sanitized error summary | Leaves active exchange failed without inventing output. |
| `TurnCompleted(Turn)` | `ExchangeCompleted` | `session_id`, `exchange_index`, completed `Exchange`, completion status, completed_at | Phase 1/2 derive `Turn` from `Exchange`; Phase 3 persists `Exchange` canonically. |
| `TurnSummarized(TurnSummary)` | `ExchangeSummarized` / compatibility `TurnSummarized` | `session_id`, summarized `exchange_index`, summary, model use | Keep `TurnSummarized` as compatibility until sleep reads exchanges. |
| `TurnsAgedAndCoRetrieved { ... }` | `ExchangesAgedAndCoRetrieved` / compatibility `TurnsAgedAndCoRetrieved` | `session_id`, aged exchange range, new/strengthened association counts, persisted timestamp, summaries, processed-range updates | Current text loop uses this for live cross-turn co-retrieval and warm-summary aging; preserve the compatibility event until sleep and reports read exchanges. |
| `ToolCompleted(RecallRecord)` | `ToolCompleted` | `session_id`, `exchange_index`, `call_id`, tool name, category, side-effect level, latency, result summary/verbatim when permitted | Add `exchange_index`; keep recall-specific fields inside the tool result payload. |
| `ToolRequested` observability event | `ToolRequested` | `session_id`, `exchange_index`, `call_id`, tool name, source, arguments summary, `auto_executed=false` for provider requests | Needed for realtime provider tool-call routing. |
| `ToolFailed` observability event | `ToolFailed` | `session_id`, `exchange_index`, `call_id`, tool name, sanitized error, permission outcome | Needed once provider requests are routed through QSF permissions. |
| `SessionLimitReached { ... }` | `SessionLimitReached` | `session_id`, current completed exchange count, max, override flag | Count exchanges, not raw partial audio facts. |
| `SessionEnded { reason }` | `SessionEnded` | `session_id`, reason, runtime phase, active exchange cleanup summary | Awake resume must not restart in `speaking` or `listening`. |
| `SpeechPlaybackRequested` | `SpeechPlaybackRequested` | `session_id`, `exchange_index`, `response_id`, adapter/provider id, voice/model/mode, text hash or text, requested_at_ms | Boundary marker before side-effect playback. |
| `SpeechPlaybackStarted` | `SpeechPlaybackStarted` | `session_id`, `exchange_index`, `response_id`, adapter/provider id, started_at_ms | Moves runtime phase to speaking only if the response is still active. |
| `SpeechPlaybackCompleted` | `SpeechPlaybackCompleted` | `session_id`, `exchange_index`, `response_id`, adapter/provider id, completed_at_ms, status, audio metadata | Completes playback state; does not by itself imply model cognition. |
| `UserInterrupted` | `UserInterrupted` | `session_id`, `exchange_index`, `response_id`, `utterance_id` if new speech exists, detected_at_ms, source, action, `stop_outcome`, partial response text when available | Phase 4 consumes this for deterministic barge-in state. |
| `RealtimeSessionStarted` | `RealtimeSessionStarted` | `session_id`, provider id, provider model, input source summary, voice, output modalities | Adapter/session lifecycle fact; initializes provider observability state only. |
| `RealtimePreambleProduced` | `RealtimePreambleProduced` | `session_id`, `exchange_index`, `response_id`, `provider_id`, text, received_at_ms | Persist as provider event/preamble; never feed into QSF prompt assembly. |
| `RealtimeResponseStarted` | `RealtimeResponseStarted` | `session_id`, `exchange_index`, `response_id`, `provider_id`, started_at_ms, target | Creates/updates active response lifecycle state. |
| `RealtimeResponseCompleted` | `RealtimeResponseCompleted` | `session_id`, `exchange_index`, `response_id`, `provider_id`, completed_at_ms, status, text, audio metadata | Completes provider response lifecycle for realtime experiments. |
| `RealtimeSessionCompleted` | `RealtimeSessionCompleted` | `session_id`, provider id, completed_at_ms, final status | Session adapter lifecycle fact; not a durable exchange boundary by itself. |
| `RealtimeSessionFailed` | `RealtimeSessionFailed` | `session_id`, provider id, sanitized error, failure category, failed_at_ms | Logs provider failure without raw audio or secrets. |

### Phase 0 `SessionConfig` Split

`SessionConfig` currently contains `model_id`, `max_turns`, `warm_threshold`,
`allow_over_limit`, and `memory_source`. For the shared core, keep this as the
modality-neutral session contract until an implementation need proves otherwise.

Shared session fields:

- `model_id`: QSF-owned model used for text and text-owned voice responses. If Phase 5
  stores provider-owned realtime model metadata, that belongs on provider events or
  `ExchangeModelUse`, not as a replacement for this field.
- `max_turns`: rename mentally to an exchange limit; implementation can keep the field
  name until a schema migration is already required.
- `warm_threshold`: shared prompt/summarization pressure threshold for completed
  exchanges.
- `allow_over_limit`: shared limit override behavior.
- `memory_source`: shared retrieval source for text and voice once voice joins
  continuity.

Modality/provider runtime config stays outside `SessionConfig` unless it affects
resume compatibility:

- Text input source details such as stdin/scripted input.
- Transcript provider selection and local input details:
  `QSF_TRANSCRIPT_PROVIDER`, `QSF_TRANSCRIPT_INPUT_SOURCE`, WAV path, microphone
  device/duration, language, and prompt.
- Speech output selection: provider, mode, speech model, and voice.
- Realtime voice session details: provider, realtime model, voice, reasoning effort,
  instructions, output modalities, input transcription model, and realtime input source.

Resume compatibility should compare shared session fields first. Provider config drift
should only force a cold start if persisted live provider state would otherwise be
misinterpreted; normal provider selection changes should be allowed because providers
are adapters around QSF-owned session state.

### Phase 0 Fixed Payload Fields

Phase 1 should add these fields when the corresponding event or structure appears, so
Phase 4 and Phase 5 do not need another payload reshaping pass:

- `session_id`: every reducer event.
- `exchange_index`: every event that mutates or annotates an exchange. Allocate before
  final input enters model context; partial transcript events may carry `None` only
  before an exchange exists.
- `utterance_id`: stable string id for voice input. Keep provider
  `utterance_index` as metadata, but do not use it as the durable id.
- `revision_index`: monotonic counter for partial transcript revisions within an
  utterance.
- `response_id`: every output, realtime response lifecycle, playback, interruption,
  and provider preamble event.
- `provider_id`: stable adapter/provider label for audio and realtime provider facts.
- `received_at_ms`, `started_at_ms`, `completed_at_ms`, or `detected_at_ms`: relative
  session timestamps for event ordering and latency reconstruction.
- `runtime_phase`: persisted live state value such as idle/listening/thinking/speaking;
  awake continuation must clear listening/speaking provider handles.
- `stop_outcome`: interruption result, with values such as `stopped`, `ignored`,
  `already_complete`, or `not_supported`.
- `status`: response/playback/exchange status, using QSF-owned values instead of raw
  provider-specific status strings where reducer behavior depends on it.
- Sanitized `error_summary` / `failure_category` on failure events.
- Text fields that may enter memory or prompts must be explicitly categorized as
  `final_transcript`, `output_text`, `provider_preamble`, or `partial_text`.
  `provider_preamble` and `partial_text` are never prompt inputs by default.

### Phase 0 Review Notes

Ratified:

- `Exchange` remains the right durable unit for the shared live loop.
- Boot-time consolidated brief loading remains before the first finalized input for
  the early implementation.
- `state/session/` remains the planned modality-neutral default, with read-only
  fallback from `state/text-loop/`.
- Provider preambles are first-class provider output events on the active exchange,
  not QSF-owned assistant output.
- Persisted-state compatibility uses serde defaults first, then explicit
  `schema_version`, then a one-shot upgrader when the state directory moves.

Human review still recommended before Phase 1 code lands: confirm the `Exchange`
payload shape and the boot-time brief-loading latency tradeoff. No open Phase 0
ambiguity blocks implementation.

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
- Model live-memory and association-aging compatibility explicitly. The first shared
  state slice should be able to represent current `TurnsAgedAndCoRetrieved`
  behavior, retrieved-memory reinforcement context, live-capture source turn
  references, and `processed_ranges` without moving those side effects into the
  reducer.
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
- Regression test or focused compatibility assertion proving the new shared state
  can describe a current text-loop aging/co-retrieval outcome without losing
  association counts or processed-range intent.

Docs to update:

- Refresh the Implementation Status section in
  `docs/Architecture/Architecture.RuntimeLoop.md` if the shared state module lands.
- Add the required diary entry for the shared-state extraction.

## Phase 2: Move Text Loop Onto The Shared Core

Goal: make the text loop actually run on the shared core so that voice can later reuse
the exact same behavior. This is the phase that delivers the "improve once, benefit
both" property; Phase 1 only created the shared types.

Work:

- Route `multi_turn_text_loop`'s per-turn processing through the shared core so the
  loop builds an `Exchange` first and derives `Turn` via `TryFrom<&Exchange>`, rather
  than keeping a second hand-written `Turn` write path. Remove the
  `#[cfg_attr(not(test), allow(dead_code))]` on `reduce_live_session` once it is on
  the production path; if it stays dead outside tests, this phase's goal is not met.
- Factor the behavior the voice loop must mimic (prompt assembly, memory
  read/reinforce, live capture, cross-turn co-retrieval, warm summaries, persistence,
  manifest commit) into shared functions/modules that both loops call, instead of
  leaving that logic private to `multi_turn_text_loop`. Preserve existing prompt hash,
  warm summary, recall, and manifest behavior; the text loop's external behavior must
  stay byte-for-byte compatible where tests assert it.
- Preserve the current live memory side effects around the text exchange:
  retrieved-memory reinforcement, assistant/user-name capture, remember-this
  capture, warm-threshold and token-budget cross-turn co-retrieval, processed-range
  persistence, and session-end flush deferral to the sleep safety net on failure.
- Keep report output stable enough that previous text-loop diagnostics remain useful.
- Keep `TurnSummary` behavior intact; only rename or reshape it if the shared state
  model makes the old name actively misleading.
- Persist through the existing manifest-last state protocol.

Verification:

- Existing multi-turn text tests pass unchanged where possible.
- Add a resume test proving a completed text exchange survives reload and produces
  the same prompt prefix behavior as the old `Turn` path.
- Add or keep regression coverage proving live memory capture still writes the
  expected `MemoryStore` records and that `processed_ranges` still prevents duplicate
  cross-turn association work after aging/session-end flush.
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
  [Default State Directory](#default-state-directory) for shared voice-session boot
  during this phase, so a voice run and a text run share one continuous session. Do
  not introduce a separate `state/voice-loop` directory, and do not leave the voice
  path on `VoiceLoopMemorySource` as its silent default.
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
- Regression test: a simulated voice run followed by a text run, and a text run
  followed by a voice run, both read and append the same shared `state/session/`
  session history by default rather than producing two continuity universes.
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

## Phase 6: Voice Loop As A Peer Surface

Goal: give voice its own first-class experiment that reuses the shared core, without
changing the status of the `multi-turn-text-loop`. Voice is a peer surface, not the
primary or default loop.

Work:

- Introduce a stable voice experiment name such as `voice-loop` (a stable domain
  name, not a phase name). Keep `multi-turn-text-loop` registered and unchanged.
- Let the voice entry point default to deterministic simulated providers unless real
  providers are explicitly selected, so the default path needs no audio credentials.
- Do not add a `QSF_PRIMARY_LOOP` default that demotes text. If a selector is useful,
  it only chooses which experiment to run and defaults to today's behavior.
- Confirm both loops exercise the same shared behavior code, so a later improvement to
  the text loop is picked up by the voice loop without duplicate edits.
- Route both loops onto the single shared resolver so a voice run and a text run
  continue one session. Move the default to `state/session/` with the read-only
  fallback from `state/text-loop/` defined in
  [Default State Directory](#default-state-directory), and add the `schema_version`
  upgrader described in
  [Persisted State Compatibility](#persisted-state-compatibility).
- If Phase 3 already moved the shared resolver to `state/session/`, Phase 6 only
  audits and removes any remaining silent `state/text-loop` defaults rather than
  performing a second migration.
- Emit a boot event and `engine_logging` record that names the chosen experiment, the
  resolved state directory, and whether the legacy `state/text-loop` fallback was used.
- Keep `multi-turn-text-loop` as a first-class experiment indefinitely.

Verification:

- `cargo run -p qsf_app -- list-experiments` shows the new entry point and existing
  experiments remain discoverable.
- Simulated default run completes without real audio credentials.
- Text input mode and voice input mode produce exchanges in the same persisted
  session shape.
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

- **`Turn` vs `Exchange` migration.** `Turn` stays as the serialized shape
  during Phase 1 and Phase 2; `Exchange` is added alongside it and is the
  single source of truth in code, with `Turn` derived via `From<&Exchange>`.
  Phase 3 promotes `Exchange` to the canonical persisted unit. See
  [Session Unit](#session-unit).
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

- **Phase 2 must put the shared reducer on the production path.** Track that the
  `allow(dead_code)` on `reduce_live_session` is removed and the text loop builds
  `Exchange` values in production, not only in tests. Without this, the shared core
  stays a parallel structure and the "improve once, benefit both" goal fails.

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
