# Architecture: State and Observability

## Maturity

Candidate

## Implementation Status

Per-run observability — event log, trace log, engine log, markdown report — is
implemented and is the backbone of every experiment. State categories are partial:
session and runtime state exist where experiments need them, and the multi-turn
text loop now has a manifest-backed cross-session state directory. Several
candidate categories listed below still have no shared module.

**Implemented today:**

- `RunContext` owning the per-run output directory and structured JSONL writers
  ([runtime/run_context.rs](../../crates/qsf_app/src/runtime/run_context.rs))
- `EventType` covering the event-log catalogue actually used in production
  (input, transcript, context, model role, tool, memory, sleep, co-retrieval,
  persistence, error)
  ([observability/event_log.rs](../../crates/qsf_app/src/observability/event_log.rs))
- Trace records for context assembly, model-role invocation, tool calls, and recall
  ([observability/trace.rs](../../crates/qsf_app/src/observability/trace.rs))
- Markdown experiment reports per run
  ([reports/markdown_report.rs](../../crates/qsf_app/src/reports/markdown_report.rs))
- `session_id`, model-role timing, prompt-hash, and tool-call lifecycle metadata on
  events
- `SessionResumed` events that record the text-loop boot decision, previous session id,
  config-drift downgrade status, and pending brief path
- Cross-session session state persisted under `state/session/` by default:
  `continuity-manifest.json` and `session-state.json`, with a read-only legacy
  fallback from `state/text-loop/` when needed
- Cross-session sleep and memory state persisted in the same state directory:
  `memory-store.json`, `consolidated-brief.json`, and archived sleep briefs
  ([memory/store.rs](../../crates/qsf_app/src/memory/store.rs),
  [sleep/commit.rs](../../crates/qsf_app/src/sleep/commit.rs))
- Live-loop memory reinforcement events: `CoRetrievalAssociationsProposed`,
  `MemoryReinforced`, and `MemoryStorePersisted`
  ([observability/event_log.rs](../../crates/qsf_app/src/observability/event_log.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Live-memory-capture traces record remembered-topic captures and explicit
  remember-this skip reasons so the excerpt source turn stays inspectable
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs),
  [memory/live_capture.rs](../../crates/qsf_app/src/memory/live_capture.rs))
- Memory retrieval traces include selected and omitted candidates, with
  `RetrievedMemory.skip_reason` explaining relevance-gated omissions and retrieval
  limit omissions.
- `engine.log` initialization redirected to `runs/<run-id>/engine.log` per run
- `qsf_realtime_server` diagnostic artifacts for session allocation, server-side
  SDP rendezvous, browser relay events, call binding, and untrusted diagnostic
  exchanges with explicit source/trust markers
  ([crates/qsf_realtime_server/src/diagnostics.rs](../../crates/qsf_realtime_server/src/diagnostics.rs),
  [crates/qsf_realtime_server/src/realtime/routes.rs](../../crates/qsf_realtime_server/src/realtime/routes.rs))

**Partial:**

- **Runtime State** lives in experiments rather than a shared module
- **Session State** is implemented for the multi-turn text loop and shared voice
  surfaces in `qsf_session`, with `qsf_app` retaining thin compatibility wrappers
  ([qsf_session/src/state.rs](../../crates/qsf_session/src/state.rs),
  [qsf_session/src/live_state.rs](../../crates/qsf_session/src/live_state.rs),
  [qsf_app/src/session/mod.rs](../../crates/qsf_app/src/session/mod.rs))
- **Memory State** is partial: records, associations, decay inputs, and live
  reinforcement are maintained for the text loop and shared voice surfaces, but
  graph inspection and contradiction handling are not implemented
- **Tool State** is exposed through registry metadata on `ToolRequested` /
  `ToolCompleted` / `ToolFailed`; there is no separate `ToolState` summary

**Not yet implemented:**

- **Identity / Self-Model State** — no module exists
- **Research State** as a structured surface — experiment metadata lives in the
  report but not as an inspectable runtime category
- **Realtime Session State** for browser speech-to-speech: provider `call_id`
  bindings, browser relay trust/source markers, diagnostic-only exchange records,
  and the future authoritative sideband attachment / context-injection /
  tool-result correlation surfaces are split between `qsf_session` and
  `qsf_realtime_server`.
- Observability views beyond the per-run markdown report (no timeline UI, no
  memory-graph view, no cost dashboard)
- `experiment_id` and `memory_update_id` correlation across runs

Last reviewed: 2026-06-09 against explicit remember-this capture observability,
retrieval skip reasons, and the implemented Phase-2 realtime diagnostic surface.

## Summary

State and observability define how Qualia Signal Foundry represents internal system state and how researchers can inspect, trace, and understand what the system is doing.

For this project, observability is not only a debugging concern. It is part of the research method. A platform for experimenting with consciousness-like behavior must make its internal processes visible enough to study.

The system should expose how inputs become state changes, how memories are retrieved, how context is assembled, how model roles are invoked, how tools are used, and how sleep-phase consolidation changes long-term memory.

## Purpose

The purpose of state and observability is to make the simulation understandable, debuggable, and researchable.

The system should help answer questions such as:

- What did the simulation perceive?
- What did it attend to?
- What state changed?
- Which memories were retrieved?
- Which memories entered active context?
- Which model role was invoked?
- Which tools were used?
- Why was a response produced?
- What happened during sleep-phase consolidation?
- Which assumptions, summaries, or decisions influenced behavior?
- How much did each step cost?
- How long did each step take?

A system that appears continuous but cannot be inspected is difficult to research.

## Design Principle

The core principle is:

```text
Behavior should be traceable from input to state change to context to output.
```

The project should avoid hidden state transitions where possible.

Not every internal detail needs to be displayed constantly, but the system should preserve enough trace information to support later inspection.

## State Versus Observability

State and observability are related but different.

```text
State:
  What the system currently believes, remembers, tracks, or maintains.

Observability:
  How humans can inspect what happened and why.
```

For example:

```text
State:
  The active focus is "audio loop architecture".

Observability:
  A trace shows when the focus changed, which input caused it, which memories were retrieved, and which model role updated it.
```

## Candidate State Categories

The system may maintain several categories of state.

### Runtime State

Short-lived state used by the live loop.

Examples:

- listening state
- speaking state
- current interaction mode
- active focus
- pending input
- partial audio transcription
- current response draft
- interruption status
- active context budget
- current latency state
- selected model role

### Session State

State that persists during one session.

Examples:

- recent turns
- session summary draft
- active topics
- unresolved questions from the session
- recent tool calls
- recent memory retrievals
- user corrections
- experiment mode
- session metrics

### Memory State

Longer-lived state managed by the memory system.

Examples:

- episodic memories
- semantic summaries
- associative links
- memory weights
- decay values
- reinforcement counts
- memory timestamps
- source references
- retrieval history

### Identity or Self-Model State

Possible future state describing the simulated entity itself.

Examples:

- stable project role
- current self-description
- remembered commitments
- behavioral style
- active long-term tensions
- known limitations
- simulated preferences or motivations, if explored

This area should be handled carefully because it can easily become vague or over-claimed.

### Tool State

State related to external tools.

Examples:

- available tools
- tool permissions
- recent tool requests
- tool results
- tool failures
- tool latency
- tool side-effect class
- tool output confidence

### Research State

State used by the project as a research platform.

Examples:

- active experiment
- current hypothesis
- measured metrics
- open research questions
- decision candidates
- known risks
- architecture maturity labels
- experiment observations

## State Lifetime

State should be categorized by lifetime.

```text
Ephemeral
  Exists only during one step or event.

Turn-level
  Exists during one user/system exchange.

Session-level
  Exists during a session.

Cross-session
  Persists between sessions.

Durable
  Intended to remain stable until explicitly changed.

Archived
  Preserved for traceability but not normally active.
```

This helps prevent accidental promotion of short-lived observations into long-term memory.

The multi-turn text loop and shared voice surfaces now persist cross-session state
in a local `state/session/` directory, or in `QSF_STATE_DIR` when that environment
variable is set. The legacy `state/text-loop/` directory remains readable as a
fallback for continuity. The manifest is the observable commit record for whether
the next boot should be a cold start, awake continuation, or consolidated-brief
resume.

## State Ownership

Each state type should have an owner or subsystem.

Examples:

```text
Runtime loop:
  live interaction state and event processing.

Audio loop:
  listening, speaking, transcription, interruption state.

Memory system:
  memory records, associations, retrieval history.

Context manager:
  context fragments, budgets, selected context.

Tool system:
  tool registry, tool requests, tool results, tool traces.

Sleep phase:
  consolidation state, memory update proposals, sleep reports.

Experiment system:
  experiment configuration, measurements, observations.
```

State ownership helps avoid unclear dependencies and hidden coupling.

## State Transition Model

State changes should be explicit where practical.

A candidate model:

```text
Input event
  -> state transition
  -> trace record
  -> optional context update
  -> optional model role invocation
  -> output event
```

This is compatible with a unidirectional-flow style architecture.

A possible flow:

```text
External input
  -> normalize event
  -> update runtime state
  -> retrieve memory/context
  -> invoke model role
  -> apply structured output
  -> emit observable output
  -> append trace
```

## Event Log

The event log is the chronological record of what happened.

Possible event types:

```text
UserTextInput
AudioInputStarted
AudioPartialTranscript
AudioFinalTranscript
UserInterrupted
SystemStartedSpeaking
SystemStoppedSpeaking
ToolRequested
ToolCompleted
ToolFailed
MemoryRetrieved
ContextAssembled
ModelRoleInvoked
ModelRoleCompleted
StateChanged
SleepStarted
SleepCompleted
MemoryUpdated
DecisionCandidateCreated
ExperimentMetricRecorded
ErrorOccurred
```

The event log should be structured enough to support later analysis.

## Trace Records

A trace record explains one operation or transition.

Examples:

- context assembly trace
- memory retrieval trace
- model role trace
- tool invocation trace
- sleep-phase trace
- audio latency trace
- state transition trace

Traces may be linked by a shared trace ID.

A useful trace should answer:

```text
What happened?
Why did it happen?
What inputs were used?
What outputs were produced?
How long did it take?
What did it cost?
What state changed?
What uncertainty remains?
```

## Context Observability

Context observability is especially important.

The system should be able to show:

- active context budget
- selected context fragments
- omitted candidate fragments
- memory candidates considered
- compression applied
- source references
- role-specific context shape
- final prompt or prompt reference
- response budget reserved

This helps diagnose continuity problems and context pollution.

## Memory Observability

Memory observability should expose:

- memory records
- memory type
- source event or source document
- creation time
- last retrieval time
- reinforcement count
- decay state
- association links
- retrieval score
- reason for retrieval
- reason for omission
- update history
- sleep-phase modifications

For associative memory, the system should ideally show not only a memory but also the path by which it became relevant.

## Tool Observability

Tool observability should expose:

- requested tool
- requesting role
- purpose
- input arguments
- permission check result
- side-effect level
- latency
- cost
- raw result reference
- summarized result
- confidence or trust level
- whether result entered context
- whether result became memory

This is important because tools are the bridge between perception and agency.

## Model Role Observability

Model role observability should expose:

- role invoked
- model profile used
- context budget
- input summary
- output summary
- structured output
- latency
- cost
- fallback behavior
- errors
- downstream effects

This helps researchers understand how the multi-model architecture behaves as a coordinated system.

## Sleep Phase Observability

Sleep-phase observability should expose:

- trigger
- input bundle
- sleep plan
- steps executed
- model roles used
- summaries generated
- memory candidates created
- memory updates proposed
- associations changed
- memories decayed or reinforced
- open questions extracted
- decision candidates created
- future context hints prepared
- cost and latency
- errors and uncertainties

Sleep should not silently rewrite long-term state.

## Audio and Real-Time Observability

Real-time audio work needs dedicated metrics.

Possible observations:

- audio capture start/stop
- voice activity detection events
- partial transcription timing
- final transcription timing
- transcription confidence
- response generation start
- response generation completion
- speech synthesis start
- playback start
- playback completion
- interruption events
- cancellation latency
- end-to-end response latency

Presence depends heavily on timing, so the system should record timing in detail.

## Experiment Observability

Experiments should produce structured observations.

Examples:

- hypothesis
- configuration
- model roles used
- context strategy
- memory strategy
- tool permissions
- measured latency
- measured cost
- user-perceived quality
- failure cases
- researcher notes
- follow-up questions

This allows experiments to be compared over time.

## Candidate Observability Views

The project may eventually benefit from several inspection views.

### Timeline View

Shows chronological events and state transitions.

### Context View

Shows the context assembled for a model role invocation.

### Memory View

Shows memories, associations, weights, and retrieval paths.

### Tool Trace View

Shows tool requests, results, errors, and side effects.

### Model Role View

Shows model role invocations and outputs.

### Sleep Report View

Shows what sleep-phase consolidation changed.

### Experiment Dashboard

Shows metrics and observations for a controlled experiment.

### Cost and Latency View

Shows where time and money are spent.

These views do not all need to exist in the MVP.

## Logging Levels

Different levels of logging may be useful.

```text
Minimal
  Basic events, errors, and outputs.

Normal
  Events, state transitions, tool calls, model role invocations, summaries.

Research
  Detailed traces, context selection, memory retrieval reasons, sleep changes.

Replay
  Enough information to reproduce or compare runs where possible.

Debug
  Verbose internal data useful during implementation.
```

The project may need to balance trace detail against storage, privacy, cost, and readability.

## Replayability

Replayability means the ability to reconstruct or rerun parts of the system.

Useful replay targets:

- a single runtime step
- a model role invocation
- memory retrieval
- context assembly
- a tool call, if result was captured
- sleep-phase consolidation
- a whole session

Full deterministic replay may not always be possible with remote AI models, but the system can still preserve:

- model name or profile
- prompt/context
- selected memories
- tool results
- output
- timestamps
- configuration
- random seeds where applicable
- source references

Replayability is especially useful for research and regression testing.

## Privacy and Data Minimization

Observability can create sensitive logs.

The system should avoid collecting unnecessary information.

Possible strategies:

- store summaries instead of raw content where appropriate
- mark sensitive records
- separate raw logs from research summaries
- allow explicit deletion or redaction
- avoid logging secrets
- limit external tool outputs stored by default
- keep write-capable actions disabled early

The project should not trade safety for observability without deliberate review.

## Candidate Implementation Shape

A possible implementation could include:

```text
StateStore
  Holds current structured state.

EventBus
  Accepts normalized events and routes them to interested subsystems.

EventLog
  Persists chronological events.

TraceStore
  Stores detailed traces for model roles, tools, memory, context, and sleep.

StateSnapshot
  Captures state at important checkpoints.

Observer
  Produces human-readable views of state and traces.

MetricRecorder
  Records latency, cost, confidence, and other measurements.

ExperimentRecorder
  Links traces and metrics to experiment runs.
```

This is a candidate shape, not a final design.

## Candidate State Snapshot

A snapshot could include:

```text
Snapshot
  timestamp
  session_id
  active_focus
  runtime_mode
  recent_events_reference
  active_context_reference
  selected_memories
  current_model_role
  tool_state_summary
  memory_state_summary
  experiment_id
  trace_id
```

Snapshots should be compact. They should point to detailed logs rather than duplicating everything.

## Candidate Trace Identifiers

Trace IDs can connect related records.

Example:

```text
trace_id:
  One interaction step.

session_id:
  One session.

experiment_id:
  One experiment run.

memory_update_id:
  One memory change set.

tool_call_id:
  One tool invocation.

model_call_id:
  One model role invocation.

sleep_run_id:
  One sleep-phase run.
```

Consistent identifiers make later inspection much easier.

## Metrics

Important metrics may include:

### Latency Metrics

- audio capture to transcript
- transcript to model request
- model request to first response
- text to speech playback
- interruption cancellation time
- tool call duration
- memory retrieval duration
- context assembly duration
- sleep-phase duration

### Cost Metrics

- tokens in
- tokens out
- model cost estimate
- tool cost estimate
- total session cost
- sleep-phase cost
- cost per experiment

### Quality Metrics

Some quality metrics may need human judgment.

Examples:

- perceived continuity
- perceived presence
- relevance of retrieved memory
- usefulness of sleep summary
- appropriateness of tool use
- coherence across sessions

### Reliability Metrics

- tool failure rate
- transcription uncertainty
- fallback frequency
- model parsing failures
- memory retrieval misses
- context budget overruns

## Research Notes Versus Logs

The system should distinguish raw logs from interpreted research notes.

```text
Raw logs:
  What happened.

Traces:
  How the system processed what happened.

Research notes:
  Human or model-assisted interpretation of what the results may mean.
```

This distinction helps avoid treating interpretation as fact.

## Human-Readable Reports

The system may produce reports from traces.

Examples:

- session report
- sleep report
- memory update report
- tool-use report
- experiment report
- cost report
- continuity report

Reports should summarize trace data without replacing it.

## Relationship to Other Documents

This document connects to:

- `Architecture.Overview.md`
- `Architecture.RuntimeLoop.md`
- `Architecture.AudioLoop.md`
- `Architecture.MemorySystem.md`
- `Architecture.ContextManagement.md`
- `Architecture.ToolSystem.md`
- `Architecture.SleepPhase.md`
- `Architecture.ModelRoles.md`
- `Concept.RealtimePresence.md`
- `Concept.AssociativeMemory.md`
- `Concept.SleepPhase.md`
- `Concept.ToolsAsPerception.md`

## Risks and Failure Modes

### Invisible State

The system may behave in ways that cannot be explained later.

### Excessive Logging

Too much logging may make the system slow, expensive, or hard to inspect.

### Misleading Traces

A trace may look explanatory while omitting important causes.

### Privacy Leakage

Logs may accidentally store sensitive information.

### Observability Overhead

Detailed tracing may interfere with real-time responsiveness.

### State Drift

Runtime state, memory state, and logged state may diverge.

### Premature Formalization

The project may over-design state structures before experiments show what matters.

### Research Bias

Observers may over-interpret traces and infer more coherence than the system actually has.

### Replay Illusion

Replay may appear deterministic even when model behavior or external tools have changed.

## Safety Boundaries

Early safety boundaries:

- all write-capable external actions should be logged and require approval
- sleep-phase state changes should be traceable
- memory updates should preserve source references where practical
- raw logs should be protected from unnecessary exposure
- sensitive data should be redacted or marked
- observability views should not silently alter state
- accepted decisions should not be created automatically from traces
- model-generated interpretations should be labeled as interpretations

## Open Questions

### RQ-State-MinimalRuntimeState

What is the smallest runtime state needed to support a convincing live interaction loop?

### RQ-State-ContinuityState

Which state elements are most important for perceived continuity?

### RQ-State-TraceDepth

How much tracing is needed for useful research without creating too much overhead?

### RQ-State-MemoryVisibility

How should associative memory retrieval paths be visualized?

### RQ-State-Replayability

Which parts of the system need replay support?

### RQ-State-Privacy

How should logs balance research usefulness against privacy and data minimization?

### RQ-State-ExperimentMetrics

Which metrics best indicate presence, continuity, and useful memory behavior?

### RQ-State-HumanInterpretation

How should human researcher notes be separated from raw system traces?

## Possible Experiments

### Experiment: Minimal Trace Review

Run a short interaction with only basic event logs and evaluate whether researchers can explain the behavior.

### Experiment: Context Trace Inspection

Log full context assembly decisions and review whether selected memories improved continuity.

### Experiment: Memory Retrieval Path View

Show why memories were retrieved and evaluate whether the explanation is useful.

### Experiment: Audio Latency Timeline

Create a timeline of audio capture, transcription, model response, and speech playback to find latency bottlenecks.

### Experiment: Sleep Report Audit

Review sleep-phase memory updates and determine whether they are understandable and appropriate.

### Experiment: Cost Trace

Track cost per model role and decide whether role separation is economically reasonable.

### Experiment: Replay a Session Step

Capture enough information to rerun or inspect one model role invocation.

## Current Status

State and observability are considered central architectural concerns.

The current working assumption is that the system should use explicit state categories, structured event logs, trace IDs, role-specific traces, memory retrieval traces, context assembly traces, tool traces, and sleep reports.

The MVP should start small, but it should not treat observability as an afterthought.
