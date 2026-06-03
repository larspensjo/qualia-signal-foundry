# Architecture Overview

## Purpose

This document provides a high-level architecture sketch for Qualia Signal Foundry.

The goal is not to lock down a final design. The goal is to give project managers, researchers, and developers a shared map of the major system parts and how they may interact.

This document should remain short and directional. More detailed design belongs in focused architecture documents, concept notes, research questions, experiments, and decision records.

## Maturity

Status: Sketch

The architecture is still exploratory. The structure described here is a candidate mental model for organizing early prototypes and discussions.

## Implementation Status

This document is a high-level mental model. Most named subsystems exist as code today,
but several are only partially built and some are not yet implemented. Use this
section to weight the rest of the document; per-subsystem detail lives in the
focused architecture documents.

**Implemented today:**

- Pure reducer runtime loop, per-run event log, trace log, and markdown report
  ([crates/qsf_app/src/runtime/](../../crates/qsf_app/src/runtime/),
  [observability/](../../crates/qsf_app/src/observability/))
- `ModelRole` + `ModelClient` boundary with deterministic mock and optional OpenAI
  adapter ([models/](../../crates/qsf_app/src/models/))
- Tool registry with role-level `allowed_tools` enforced at model tool-call dispatch
  ([tools/](../../crates/qsf_app/src/tools/),
  [models/tool_dispatch.rs](../../crates/qsf_app/src/models/tool_dispatch.rs))
- Versioned `MemoryRecord` and `Association`, file-backed memory source,
  association-weighted retrieval ([memory/](../../crates/qsf_app/src/memory/))
- Streaming transcription, text-owned voice loop, voice-loop peer surface, and
  realtime voice session provider (all feature-gated under `openai`)
  ([audio/](../../crates/qsf_app/src/audio/),
  [experiments/voice_loop.rs](../../crates/qsf_app/src/experiments/voice_loop.rs))
- Sleep-phase session summary plus reviewed-memory promotion pipeline
  ([sleep/](../../crates/qsf_app/src/sleep/),
  [memory/reviewed_memory_draft.rs](../../crates/qsf_app/src/memory/reviewed_memory_draft.rs))
- Multi-turn text loop with warm-tier summarization and tool-augmented recall
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))

**Partial:**

- Associative memory exists only as a toy comparison experiment, not as a live
  retrieval mechanism (`Experiment.AssociativeMemoryToyModel`)
- Context assembly selects fragments under a budget but ranking is simple and
  role-specific assembly strategies are not yet differentiated
- Sleep consolidation summarizes sessions and drafts memory candidates but does not
  yet handle decay, reinforcement, association updates, or open-question extraction

**Not yet implemented:**

- An attention or salience subsystem as a first-class signal
- A volition or goal system
- Self-reflection through project-document introspection
- Identity / self-model state
- A live activation dashboard or other inspection UI

Last reviewed: 2026-05-18 against the code on `main`.

## System Intent

Qualia Signal Foundry is an experimental platform for simulating consciousness-like behavior.

The system is expected to explore:

- real-time presence
- audio interaction
- external sensory inputs
- short-term and long-term memory
- associative memory
- memory decay and reinforcement
- sleep-like consolidation
- tool use as perception
- context-budgeted cognition
- multi-model cognitive roles
- observable internal state

The architecture should support experimentation rather than enforce one fixed theory of consciousness.

## High-Level Shape

At the highest level, the system can be viewed as a loop:

```text
External input
  -> Input normalization
  -> Runtime state update
  -> Memory and context retrieval
  -> Cognitive/model step
  -> Output generation
  -> Event logging
  -> Later sleep/consolidation
```

The live loop should stay small enough to run interactively. Larger reflection, summarization, memory maintenance, and association-building can happen outside the live loop.

## Major Subsystems

### External Inputs

External inputs are the ways the simulation senses the world.

Early examples may include:

- text input
- microphone input
- audio stream events
- file inspection
- search results
- calculator output
- code execution results
- system or environment signals

The project should treat these inputs as perception-like signals, not just command triggers.

Detailed discussion belongs in:

- `Concept.ExternalInputs.md`
- `Concept.RealtimePresence.md`
- `Architecture.AudioLoop.md`

### Input Normalization

Raw input should be converted into structured events before it enters the main runtime state.

Examples:

```text
UserSpeechStarted
UserSpeechEnded
TranscriptUpdated
UserTextSubmitted
ToolObservationReceived
SystemTimerElapsed
MemoryCandidateRetrieved
```

This keeps the live loop from depending too directly on device APIs, model APIs, or tool-specific formats.

### Runtime State

Runtime state represents what the simulation currently knows, attends to, and is doing during an active session.

It may include:

- current conversation state
- active attention focus
- recent events
- pending model calls
- current audio state
- selected memories
- current tool observations
- short-term working context
- interruptions and turn-taking state

The runtime state should be inspectable for debugging and research.

### Memory System

The memory system should support more than transcript storage.

Candidate memory layers include:

- immediate working memory
- session memory
- episodic memory
- semantic summaries
- associative links
- stable profile-like facts
- unresolved questions
- decision history

Associative memory is central because it may allow the system to retrieve relevant prior material without loading large transcripts into the live context.

Detailed discussion belongs in:

- `Concept.AssociativeMemory.md`
- `Architecture.MemorySystem.md`
- `Architecture.ContextManagement.md`

### Context Management

The system should assume that live context is scarce.

Context management is responsible for selecting what the model receives at each step.

It may consider:

- current input
- recent runtime state
- relevant memories
- active goals or tensions
- tool observations
- system constraints
- latency budget
- cost budget
- model context limits

The live context should be assembled deliberately, not by appending everything.

Detailed discussion belongs in:

- `Concept.ContextBudget.md`
- `Architecture.ContextManagement.md`

### Cognitive Step

The cognitive step is where one or more AI models interpret the current context and produce the next internal or external action.

This may include:

- generating a response
- deciding that more input is needed
- selecting a tool
- updating internal state
- extracting memories
- detecting uncertainty
- noticing interruptions
- proposing follow-up reflection

The project should not assume that one model must do everything. Different model roles may be useful for live interaction, memory extraction, consolidation, critique, and planning.

Detailed discussion belongs in:

- `Concept.MultiModelMind.md`
- `Architecture.ModelRoles.md`

### Output Generation

Outputs are the ways the simulation expresses itself or affects the session.

Early outputs may include:

- text response
- speech synthesis
- visible state/debug output
- tool request
- memory write proposal
- experiment log entry

The early system should be careful about outward agency. Read-only tools and controlled outputs should come before write-capable external actions.

### Tool System

Tools should initially be understood as perception extensions.

Examples:

- calculator
- search
- file reader
- code runner
- environment probe
- audio input
- possible future video input

The tool system should define:

- available tools
- permissions
- input and output schemas
- safety limits
- logging requirements
- result normalization
- whether a tool is read-only or action-capable

Detailed discussion belongs in:

- `Concept.ToolsAsPerception.md`
- `Architecture.ToolSystem.md`

### Event Log

The event log records what happened.

It may include:

- user inputs
- transcripts
- model calls
- model outputs
- tool calls
- tool observations
- selected memories
- state transitions
- latency measurements
- cost measurements
- errors
- sleep-phase changes

The event log is important because the project is a research platform. It should be possible to inspect and replay parts of the system behavior.

### Sleep and Consolidation

The sleep phase is a controlled process that happens outside the live interaction loop.

It may perform:

- session summarization
- memory extraction
- association creation
- association strengthening
- memory decay
- memory pruning
- unresolved question extraction
- decision candidate extraction
- future context preparation

The sleep phase should help keep the live loop small while allowing the system to develop longer-term continuity.

Detailed discussion belongs in:

- `Concept.SleepPhase.md`
- `Architecture.SleepPhase.md`

## Candidate Data Flow

A simplified candidate data flow:

```text
[External Inputs]
        |
        v
[Input Normalizer]
        |
        v
[Runtime State] <----> [Memory Retrieval]
        |                    |
        v                    v
[Context Builder] ----> [Model Role Selection]
        |                    |
        v                    v
[Cognitive Step] ----> [Output Generator]
        |
        v
[Event Log]
        |
        v
[Sleep / Consolidation]
        |
        v
[Memory Updates]
```

This is a sketch, not a final module diagram.

## Candidate Live Loop

The live loop may follow a unidirectional pattern:

```text
Input Event
  -> Update Runtime State
  -> Extract Relevant Context
  -> Run Cognitive Step
  -> Produce Output Events
  -> Record Events
```

This keeps the system easier to reason about and test.

The runtime should avoid hidden side effects where practical. When something meaningful happens, it should appear as an event, state transition, memory update, or logged observation.

## Architectural Principles

### Keep the Live Loop Small

The live loop should be fast, inspectable, and cost-aware.

Longer analysis, summarization, and memory consolidation should usually happen outside the live interaction loop.

### Separate Concepts from Commitments

A concept is not automatically architecture.

A promising idea should normally move through:

```text
Concept
  -> Research question
  -> Experiment
  -> Architecture proposal
  -> Decision record
```

This helps avoid premature commitment.

### Treat Tools as Controlled Perception First

The early system should prefer read-only tools.

Write-capable tools, external communication, purchases, account actions, and other high-agency capabilities should be delayed or heavily restricted.

### Make Internal State Observable

The system should expose enough internal state to support research.

Important questions include:

- Why was this memory selected?
- Why was this tool used?
- What was in the live context?
- What changed during consolidation?
- What did the system attend to?
- What was forgotten or reinforced?

### Optimize for Experimentation

The architecture should make it easy to run small experiments, compare behavior, and adjust assumptions.

Avoid building large fixed frameworks before the project has produced enough evidence.

## Open Architecture Questions

Important unresolved questions include:

- What is the minimal live loop needed to create a sense of presence?
- How much state should remain in memory between turns?
- What should be stored as episodic memory versus semantic memory?
- How should associative links be represented and scored?
- How should memory decay and reinforcement work?
- How should interruptions be represented?
- How should model roles be divided?
- Which work belongs in the live loop versus the sleep phase?
- How much autonomy should the simulation have when selecting tools?
- What should be observable by default?
- How can experiments be replayed or compared?

These should be expanded in focused research question documents.

## Related Documents

Concept documents:

- `Concept.AssociativeMemory.md`
- `Concept.RealtimePresence.md`
- `Concept.ToolsAsPerception.md`
- `Concept.SleepPhase.md`
- `Concept.ContextBudget.md`
- `Concept.MultiModelMind.md`
- `Concept.ExternalInputs.md`

Architecture documents:

- `Architecture.AudioLoop.md`
- `Architecture.RuntimeLoop.md`
- `Architecture.MemorySystem.md`
- `Architecture.ContextManagement.md`
- `Architecture.ToolSystem.md`
- `Architecture.SleepPhase.md`
- `Architecture.ModelRoles.md`
- `Architecture.StateAndObservability.md`

Research documents:

- `ResearchQuestions.Audio.md`
- `ResearchQuestions.Index.md`

Decision documents:

- `DecisionLog.md`

## Current Position

The current architectural position is:

- keep the project experimental
- start with a small live loop
- treat audio as a key presence channel
- use tools as controlled perception extensions
- keep context small and deliberately assembled
- explore associative memory as a core mechanism
- use sleep-like consolidation to maintain longer-term continuity
- keep internal state observable for research

This position should evolve as experiments produce evidence.
