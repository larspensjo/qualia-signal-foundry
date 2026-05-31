# Architecture: Runtime Loop

Status: Draft
Maturity: Sketch
Area: Core Architecture

## Implementation Status

The candidate loop described below is implemented as a pure reducer pattern. Most
loop stages exist, but several are aspirational placeholders. Treat the rest of this
document as a candidate design that real experiments incrementally fill in.

**Implemented today:**

- Unidirectional `(State, Event) → State` reducer discipline, enforced across
  experiment runtimes ([runtime/](../../crates/qsf_app/src/runtime/))
- Structured event log with the event types used in production
  ([observability/event_log.rs](../../crates/qsf_app/src/observability/event_log.rs))
- Input normalization for audio (`TranscriptProvider` → `AudioFinalTranscript` →
  `InputReceived`) and for typed text turns
  ([audio/transcript_provider.rs](../../crates/qsf_app/src/audio/transcript_provider.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Voice-turn event sequence matching the shape documented under *Runtime Loop and
  Voice Turns* below
  ([experiments/text_owned_voice_loop.rs](../../crates/qsf_app/src/experiments/text_owned_voice_loop.rs))
- Memory retrieval before context assembly, then model invocation, then output and
  trace emission
- Live memory capture after the model response now persists assistant-name,
  user-name, and remembered-topic candidates before the next turn begins
  ([memory/live_capture.rs](../../crates/qsf_app/src/memory/live_capture.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Retrieval traces now surface omitted candidate skip reasons, so unrelated
  identity memories can be observed rather than silently reinforced
- Warm-turn summarization retries once on truncation and fails closed on a second
  truncation instead of persisting a truncated continuity summary
- `session_id` propagation through transcript, runtime input, model role, output,
  and speech playback for voice turns
- Cross-session boot for the multi-turn text loop: load continuity manifest, classify
  resume mode, emit `SessionResumed`, then enter the normal reducer-driven loop
  ([session/resume.rs](../../crates/qsf_app/src/session/resume.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Multi-turn hot context aging now composes the existing active-turn warm threshold
  with a token-budget high-water policy. Aging side effects run cross-turn
  co-retrieval first, persist association deltas and `processed_ranges`, retry
  once when a warm summary truncates, and only commit the summary if the retry
  finishes normally; a second truncation logs an error and leaves the turn hot
  for a later attempt. Successful aging then feeds `TurnsAgedAndCoRetrieved`
  through the reducer while keeping `state.turns` append-only
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs),
  [session/mod.rs](../../crates/qsf_app/src/session/mod.rs))
- Clean `:quit` and EOF exits run a session-end cross-turn flush for remaining hot
  turns before recording `SessionEnded`; flush failures are logged and deferred to
  the sleep safety-net path rather than blocking exit

**Partial:**

- "Update Attention and Focus" exists in concept only; there is no `AttentionState`
  structure today
- Shared live-session state now exists in `session/exchange.rs` and
  `session/live_state.rs`, including exchange payloads, runtime phase, partial
  transcript, interruption, response, and processed-range state. The runtime loop
  still uses the text-loop reducer directly and has not been fully switched to the
  shared core yet
  ([session/exchange.rs](../../crates/qsf_app/src/session/exchange.rs),
  [session/live_state.rs](../../crates/qsf_app/src/session/live_state.rs),
  [session/mod.rs](../../crates/qsf_app/src/session/mod.rs))
- Output planning is collapsed into model-role output today; there is no separate
  `OutputPlan` step

**Not yet implemented:**

- Scheduled / internal timer events
- The candidate state categories `AttentionState`, `ToolState` aggregator, and
  `AudioState` as a unified module — equivalents are scattered across audio and
  experiment code
- Interruption and turn-taking handling in the live loop

Last reviewed: 2026-05-31 against the shared live-session extraction — the
session module now carries a reusable live exchange state slice, while the text
loop still owns the concrete reducer path.

## Purpose

This document describes a candidate runtime loop for Qualia Signal Foundry.

The runtime loop is the live part of the system: the part that receives input, updates state, selects relevant context, invokes model functions, produces output, and records what happened.

This document is not a final implementation specification. It captures a working architectural shape that can guide early prototypes and experiments.

## Summary

The runtime loop should make the simulation feel continuous, responsive, and inspectable.

A simplified shape is:

```text
External Input
  -> Input Event
  -> State Update
  -> Attention / Focus Update
  -> Memory Retrieval
  -> Context Assembly
  -> Model Invocation
  -> Output Planning
  -> Output Event
  -> Logging
  -> Memory Capture
```

The loop should keep live context small, expose internal state for research, and avoid mixing unrelated responsibilities into one opaque model call.

## Design Intent

The runtime loop should support:

- real-time interaction
- audio-driven presence
- memory-informed responses
- low inference cost
- clear state transitions
- inspectable behavior
- replayable experiments
- gradual introduction of more advanced cognitive components

The loop should not be optimized only for task completion. Its main purpose is to support experiments in simulated presence, continuity, and consciousness-like behavior.

## State Update Model

The runtime loop uses a unidirectional, reducer-style state update model:

- State is updated only through pure functions of the form `(State, Event) → State`.
- Side effects (model calls, tool invocations, logging) are isolated from state update
  functions and fed back into the loop as new events.
- Reducers must remain unit-testable without mocks or external dependencies.
- No meaningful state transition should be hidden inside a side effect.

This is a deliberate architectural commitment recorded in `docs/DecisionLog.md`.
See also: `Agents.md`, which carries this as a coding standard.

## Multi-Turn Boot Continuity

The multi-turn text loop now has a pre-loop boot step:

```text
state/text-loop/continuity-manifest.json
  -> load previous SessionState if present
  -> classify ColdStart | AwakeContinuation | ConsolidatedBrief
  -> emit SessionResumed
  -> SessionStarted event enters the reducer loop
```

`state/text-loop/` is process-working-directory relative unless `QSF_STATE_DIR` is set.
`AwakeContinuation` keeps the same `session_id` and carries turns forward only when the
resume-breaking parts of the stored `SessionConfig` match the new run. Runtime-only
limit overrides such as `allow_over_limit` are recomputed without forcing a cold start.
`ConsolidatedBrief` starts a fresh session with `previous_session_id` set, while Stage 4
owns actual brief injection.

## Candidate Flow

### 1. Receive External Input

Inputs may come from several sources:

- text input
- microphone audio
- transcribed speech
- timing events
- local file inspection
- web/search results
- sensor-like inputs
- future video input
- internal scheduled events

External inputs should be normalized into input events before they affect the simulation state.

### 2. Create Input Event

An input event is a structured representation of something that happened.

Examples:

```text
UserSpoke
UserTyped
ToolResultReceived
TimerElapsed
SessionStarted
SessionEnded
AudioInterruptionDetected
```

Input events should preserve enough detail to support debugging, replay, and later memory extraction.

### 3. Update Live State

The system should update its live state before deciding how to respond.

Live state may include:

- current session information
- recent conversation turns
- current user activity
- active topic
- attention focus
- pending tool requests
- current response state
- latency measurements
- active audio state
- temporary working memory

This state should be explicit and inspectable where practical.

### 4. Update Attention and Focus

The system may need a lightweight attention mechanism that decides what currently matters.

Possible focus signals:

- the latest user utterance
- repeated themes
- unresolved questions
- emotional or urgency signals
- recent memory activations
- current experiment mode
- current audio state
- tool results waiting for interpretation

This does not need to be complicated in early prototypes. A simple focus object may be enough.

### 5. Retrieve Relevant Memory

The runtime loop should retrieve only a small number of relevant memories.

Possible memory sources:

- recent session history
- short-term working memory
- episodic memory
- semantic summaries
- associative memory nodes
- prior decisions
- unresolved research questions
- user preferences or project facts

Retrieval should be budgeted. The system should avoid loading large memory collections directly into the live model context.

### 6. Assemble Model Context

The context assembly step selects what the model sees.

Possible context components:

- current input
- current focus
- recent turns
- selected memories
- active project constraints
- relevant tool results
- current system state summary
- response policy or mode
- experiment instrumentation instructions

This step is central to cost control and should be observable.

A useful early rule:

```text
The live model should receive the smallest context that can plausibly support a coherent response.
```

### 7. Invoke Model Function

The runtime loop may call one or more model functions.

Possible roles:

- live interaction model
- speech-aware response model
- memory extraction model
- tool selection model
- reflection model
- critic/reviewer model
- summarization model

Early prototypes should keep this simple. More roles can be introduced when experiments justify the complexity.

### 8. Plan Output

Before producing external output, the system may create an output plan.

The output plan may include:

- text to speak
- text to display
- whether to pause
- whether to ask a question
- whether to call a read-only tool
- whether to defer a thought
- whether to store a memory candidate
- whether to mark an issue for the sleep phase

For real-time audio, the output plan may also include interruption and timing behavior.

### 9. Emit Output Event

Outputs should also be represented as structured events.

Examples:

```text
AssistantSpoke
AssistantDisplayedText
ToolCallRequested
ToolResultDisplayed
MemoryCandidateCreated
SleepTaskQueued
```

Even when the output is just text or speech, recording it as an event makes the system easier to replay and analyze.

### 10. Log and Observe

Each loop iteration should produce useful logs.

Important things to log:

- input event
- state transition
- retrieved memory candidates
- selected context
- model role invoked
- model latency
- token usage
- tool use
- output event
- memory candidates
- errors and fallback behavior

For this project, observability is not only a debugging feature. It is part of the research method.

### 11. Capture Memory Candidates

The live loop should not necessarily write permanent memory directly.

Instead, it may emit memory candidates such as:

- notable user statement
- repeated topic
- unresolved question
- possible preference
- project decision candidate
- surprising interaction
- failure case
- new association candidate

A later memory process or sleep phase can decide how these candidates should be stored, merged, reinforced, or discarded.

## Runtime Loop and Sleep Phase

The runtime loop and sleep phase should be separate.

The runtime loop should handle live interaction.

The sleep phase should handle slower consolidation work, such as:

- summarizing sessions
- strengthening associations
- decaying weak memories
- merging duplicate memories
- extracting research questions
- updating diary notes
- preparing future context

This separation helps keep live interaction responsive and keeps expensive reflection outside the critical path.

## Runtime Loop and Tools

Tools should initially be treated as controlled perception extensions.

In the runtime loop, a tool call should be represented as an event, not as hidden model behavior.

A possible tool flow:

```text
Model requests observation
  -> Tool permission check
  -> Tool invocation
  -> Tool result event
  -> State update
  -> Context assembly
  -> Model interprets result
```

Early tools should preferably be read-only.

Examples:

- calculator
- file reader
- search
- local metadata inspection
- audio input
- possibly video input

Write-capable tools should be delayed or heavily constrained.

## Runtime Loop and Voice Turns

The text-owned voice loop uses the same runtime ownership rule as text interaction:
providers adapt input and output, while QSF owns interpretation, context assembly,
model-role invocation, and `OutputProduced` text.

The current voice-turn shape is:

```text
AudioFinalTranscript
  -> InputReceived
  -> ContextAssemblyRequested
  -> ContextAssembled
  -> ModelRoleRequested
  -> ModelRoleCompleted
  -> OutputProduced
  -> SpeechPlaybackRequested
```

One `session_id` should correlate the voice turn across transcript, runtime input,
model role, output, speech playback, and latency records. Model role requests may be
used outside voice turns, so the model request metadata carries `session_id` only when
a caller has a turn/session identifier to propagate.

## Runtime Loop and Real-Time Audio

Audio makes the runtime loop more demanding because input and output may overlap.

The loop should eventually account for:

- partial speech input
- voice activity detection
- interruption
- turn-taking
- streaming transcription
- response streaming
- text-to-speech playback
- cancellation
- latency measurement

Early prototypes may use a simpler turn-based audio loop before attempting fully overlapping real-time behavior.

## Candidate State Categories

The runtime state may be organized into categories such as:

```text
SessionState
  Current session identity, start time, active mode, experiment metadata.

InteractionState
  Recent turns, active response, interruption state, current user input.

AttentionState
  Current focus, topic, salience signals, unresolved tensions.

MemoryState
  Retrieved memories, memory candidates, active associations.

ToolState
  Available tools, pending tool calls, recent tool results.

AudioState
  Microphone status, speech detection, transcription status, playback status.

ObservationState
  Logs, metrics, trace identifiers, model/tool latency.
```

These are candidate categories, not final module names.

## Guiding Principles

### Explicit State Over Hidden State

The system should avoid hiding important state inside prompt text or model output alone.

State should be represented explicitly where practical.

### Event-Driven Interaction

The runtime loop should be event-oriented.

Events make it easier to replay sessions, debug behavior, and run controlled experiments.

### Small Live Context

The live model should not receive all available memory.

It should receive a carefully selected working context.

### Observable Decisions

Memory retrieval, tool use, and context assembly should be inspectable.

Researchers should be able to understand why the system saw a particular piece of information.

### Safe Tool Use

The early runtime loop should favor read-only observation over external action.

### Progressive Complexity

The runtime loop should begin simple and become more sophisticated only when experiments show a need.

## Open Questions

### RQ-Runtime-LoopGranularity

How large should one runtime loop iteration be?

Possible options:

- one user turn
- one audio segment
- one partial speech update
- one model response chunk
- one event of any kind

### RQ-Runtime-RealtimeOverlap

How should the runtime loop handle overlapping input and output?

For example, the user may interrupt while the system is speaking.

### RQ-Runtime-ModelRoles

Which decisions should be made by the live interaction model, and which should be delegated to specialized model roles?

### RQ-Runtime-StatePersistence

Which parts of live state should be persisted across sessions, and which should remain temporary?

### RQ-Runtime-Replayability

What information must be logged to replay an interaction accurately enough for research?

### RQ-Runtime-ContextAssembly

How should the system decide which memories, tool results, and state summaries enter the live model context?

### RQ-Runtime-LatencyBudget

What latency is acceptable for the system to feel present during text interaction, voice interaction, and interrupted speech?

## Risks and Failure Modes

### Opaque Behavior

If too much logic is hidden inside prompts or model calls, researchers may not understand why the system behaved a certain way.

### Context Bloat

If every input pulls in too much memory, the system may become expensive, slow, and less focused.

### Premature Complexity

A multi-role cognitive architecture may become difficult to debug before the basic loop is understood.

### Latency

Real-time audio may fail to feel present if the loop is too slow.

### Memory Pollution

If the live loop stores too many low-quality memories directly, long-term memory may become noisy.

### Tool Overreach

If tools are introduced as action mechanisms too early, the project may drift toward agent automation instead of consciousness simulation.

## Possible Early Prototype

A minimal runtime loop could be:

```text
User text input
  -> InputEvent
  -> Update session state
  -> Retrieve recent session summary and a few memory candidates
  -> Assemble compact context
  -> Invoke live model
  -> Emit text response
  -> Log trace
  -> Create memory candidates
```

A later audio prototype could extend this:

```text
Microphone audio
  -> Speech detection
  -> Transcription
  -> InputEvent
  -> Runtime loop
  -> Text response
  -> Speech synthesis
  -> Audio playback
  -> Latency and interruption logging
```

## Related Documents

- `docs/Architecture/Architecture.Overview.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Concepts/Concept.RealtimePresence.md`
- `docs/Concepts/Concept.AssociativeMemory.md`
- `docs/Concepts/Concept.ContextBudget.md`
- `docs/Concepts/Concept.ToolsAsPerception.md`
- `docs/Concepts/Concept.SleepPhase.md`
- `docs/Research/ResearchQuestions.Audio.md`

## Current Status

This document is a sketch.

The proposed runtime loop should be used to guide early prototypes and experiments, not as a fixed architecture. The design should be revised as the project learns more about real-time interaction, memory retrieval, model roles, and observability.
