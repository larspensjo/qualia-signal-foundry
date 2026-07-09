# Concept: External Inputs

## Summary

External inputs are the signals that allow the simulated system to sense something beyond the current text prompt.

This may include audio, video, files, screen state, system events, sensors, web search results, user activity, and other controlled read-only observations. These inputs are important because the project is not only trying to answer prompts. It is trying to explore simulated presence, continuity, perception, attention, and consciousness-like behavior over time.

External inputs should initially be treated as perception, not agency.

The early design should focus on safe, inspectable, read-only input streams that can be observed, summarized, filtered, and selectively brought into the live context.

## Core Idea

A simulated mind needs some way to receive signals from its environment.

In a conventional chatbot, the main external input is the user’s text message. In this project, that is too narrow. A system intended to simulate presence may need to perceive timing, speech, interruptions, silence, nearby context, tool results, environmental changes, and possibly visual information.

The system should therefore support external input channels that feed into an internal perception pipeline.

Possible examples include:

- microphone audio
- speech-to-text transcripts
- voice activity detection
- timing and latency signals
- screen or application state
- files selected by the user
- web search results
- local document inspection
- camera or video input
- system status
- calendar-like time signals
- structured experiment signals
- synthetic test inputs

The key design question is not only how to capture these inputs, but how to decide which signals become part of attention, memory, reflection, and response.

## Why It Matters

External inputs are central to the feeling of presence.

A system that only responds to typed prompts can be useful, but it may feel disconnected from the world. A system with carefully managed external inputs can appear more situated and continuous.

External inputs may support:

- real-time interaction
- interruption handling
- awareness of silence or hesitation
- richer session context
- better memory formation
- environmental grounding
- tool-mediated perception
- experiment replay
- multimodal research

They also create important constraints:

- input streams can become noisy
- too much input can overwhelm the context budget
- privacy risks increase
- real-time latency becomes more important
- the system needs attention mechanisms
- memory storage must be selective

External inputs therefore connect directly to several other concepts, especially realtime presence, tools as perception, context budgeting, associative memory, and sleep-phase consolidation.

## Possible Design Directions

### Unified Input Event Stream

One possible direction is to convert external inputs into a common event format.

Examples:

```text
AudioStarted
AudioStopped
TranscriptFragmentReceived
UserInterrupted
ScreenRegionChanged
FileObserved
SearchResultReceived
SensorReadingReceived
ToolObservationReceived
```

This would allow the system to reason over many input types without hard-coding every modality into the main cognition loop.

The event stream should probably include metadata such as:

- timestamp
- source
- confidence
- latency
- privacy level
- importance estimate
- raw payload reference
- summarized payload
- whether the event is eligible for memory

### Perception Pipeline

External inputs should probably pass through a perception pipeline before they reach the main model.

A possible pipeline:

```text
Capture
  -> Normalize
  -> Filter
  -> Summarize
  -> Score salience
  -> Route to attention, memory, or archive
```

This avoids sending every raw signal directly into the live context.

### Read-Only First

The early project should keep external inputs read-only.

This means the system may observe, but should not freely act on the outside world.

Examples of read-only inputs:

- listen to audio
- read a file selected by the user
- inspect a controlled directory
- query a search tool
- observe a camera feed in a controlled experiment
- read system time
- read experiment state

This boundary keeps the project safer and more focused. The purpose is to explore perception and simulated consciousness, not uncontrolled external agency.

### Raw Data vs Interpreted Perception

The system may need to separate raw input from interpreted perception.

For example, microphone audio might produce several layers:

```text
Raw audio buffer
  -> voice activity events
  -> transcript fragments
  -> speaker turn estimate
  -> emotional or prosodic hints
  -> summarized conversational event
```

The live cognition loop should normally receive interpreted perception, not raw high-volume input.

Raw input may still be useful for debugging, replay, or later analysis.

### Attention-Gated Inputs

Not every input should reach the main context.

The system may need an attention gate that decides:

- what is currently relevant
- what should be ignored
- what should be summarized
- what should be stored as memory
- what should be deferred to sleep-phase processing
- what should trigger an immediate response

This attention gate may be rule-based at first, then later model-assisted.

### Replayable Input Sessions

For research, external input sessions should ideally be replayable.

A replayable session allows researchers to run the same input sequence through different versions of the system and compare behavior.

This may require logging:

- input event sequence
- timestamps
- model outputs
- memory writes
- retrieval decisions
- tool observations
- state transitions

Replayability is especially useful when evaluating realtime behavior, memory formation, and attention mechanisms.

## Open Questions

- Which external inputs are needed for the first meaningful prototype?
- Should audio be the first real external input, or should a simpler synthetic event stream come first?
- How should noisy or uncertain input be represented?
- How much raw data should be retained?
- How should privacy-sensitive input be marked and protected?
- Should external inputs be stored directly, summarized, or both?
- What should make an input eligible for long-term memory?
- Should attention scoring be deterministic, model-driven, or hybrid?
- How should the system handle simultaneous input streams?
- How should interruptions be represented?
- Should visual input be part of the early prototype or delayed?
- How can input processing stay within a small context budget?
- How can input sessions be replayed for research?

## Risks and Failure Modes

### Input Overload

External inputs can produce too much information.

Continuous audio, video, file monitoring, or system events could overwhelm the live context and increase cost. The system needs filtering, summarization, and salience scoring.

### False Sense of Awareness

If the system receives partial or low-quality input, it may appear more aware than it really is.

The design should preserve uncertainty. The system should distinguish between observed facts, inferred interpretations, and speculative guesses.

### Privacy Leakage

External inputs may contain sensitive personal information.

This is especially important for audio, video, screen capture, files, and environment sensors. The project should make input sources explicit and inspectable.

### Premature Complexity

It may be tempting to add many input channels early.

That could distract from the core research questions. A smaller number of well-instrumented inputs is better than many poorly understood ones.

### Tight Coupling to Specific Devices

The project should avoid making the core simulation depend directly on a specific microphone, camera, operating system API, or hardware device.

Device adapters should feed normalized events into the system.

### Confusing Perception with Action

External inputs should not be confused with external agency.

A system that can observe a web page is different from a system that can post to it. Early designs should keep this boundary clear.

## Possible Experiments

### Experiment: Synthetic Input Stream

Create a controlled event stream with artificial user speech events, pauses, interruptions, and environment changes.

Goal:

- test the input event model
- test attention gating
- test memory eligibility
- test replayability

### Experiment: Audio Transcript Input

Use microphone input and speech-to-text to feed transcript fragments into the system.

Goal:

- test realtime interaction
- measure latency
- detect turn-taking problems
- observe how partial transcripts affect response quality

### Experiment: Voice Activity Without Transcription

Feed only voice activity events into the system.

Goal:

- test whether silence, speaking, interruption, and timing signals improve perceived presence
- avoid early dependence on transcription quality

### Experiment: Tool Observation Input

Represent search results, file reads, or calculator outputs as external perception events.

Goal:

- unify tool output with other perception sources
- test whether tool results should enter memory
- explore tools as perception rather than commands

### Experiment: Replay Session

Record a short session and replay it through two different input-processing strategies.

Goal:

- compare behavior
- debug state transitions
- evaluate repeatability
- support researcher review

### Experiment: Salience Filter

Compare simple rule-based filtering against model-assisted salience scoring.

Goal:

- reduce context usage
- identify important input events
- avoid overloading the live loop

## Related Concepts

- Concept.RealtimePresence.md
- Concept.ToolsAsPerception.md
- Concept.AssociativeMemory.md
- Concept.ContextBudget.md
- Concept.SleepPhase.md
- Concept.MultiModelMind.md

## Current Status

Exploratory.

The first offline substrate for a read-only external corpus is implemented in
`qsf_corpus`: `qsf_app ingest-world` (or `qsf.ps1 world-ingest`) reads a producer-owned corpus
directory, validates its marker and article provenance, and persists a content-hash ledger with a
deterministic lexical index. This is preparation for perception, not yet a live input channel:
no corpus article reaches the realtime model or durable memory in the current implementation.

External inputs are clearly important to the project, especially for realtime presence and simulated perception. However, the exact set of input channels should not be locked down too early.

The likely early direction is:

```text
Start with a normalized input event stream.
Add audio-related events first.
Keep tools and sensors read-only.
Use filtering and summarization before input reaches the live context.
Log enough information for replay and research review.
```

The main unresolved question is how much perception the first prototype needs in order to feel meaningfully present without becoming too complex or expensive.
