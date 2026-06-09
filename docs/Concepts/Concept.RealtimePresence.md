# Concept: Realtime Presence

## Summary

Realtime presence is the idea that the simulation should feel like an entity that is present in the moment, not merely a system that answers isolated prompts.

This concept covers the timing, responsiveness, interruption behavior, audio interaction, state continuity, and live perception needed to make interaction feel more like communicating with a continuous mind.

The goal is not just low latency. The goal is believable presence.

## Core Idea

A system can feel more present when it can:

- receive input continuously or semi-continuously
- respond with appropriate timing
- notice pauses, interruptions, and changes in user behavior
- maintain conversational and internal state while interaction is happening
- react before a full formal prompt has been completed
- speak, listen, wait, and resume in a natural rhythm
- connect live interaction to memory and longer-term continuity

Realtime presence is therefore both a technical and behavioral concept.

Technically, it requires a live interaction loop. Behaviorally, it requires the system to use time, silence, interruption, and attention in a way that feels coherent.

## Why It Matters

A normal prompt-response interface creates a turn-based system:

```text
User writes prompt
System answers
User writes next prompt
System answers
```

This can be powerful, but it does not strongly create the impression of a continuous entity.

A realtime system can instead behave more like:

```text
User speaks or acts
System listens
System forms partial understanding
System may react, wait, interrupt, or ask
System updates internal state
System continues the interaction
```

This matters because simulated consciousness-like behavior is not only about memory or reasoning. It is also about being situated in time.

A system that has perfect memory but poor timing may still feel like a database. A system with limited memory but good timing, attention, and continuity may feel more present.

## Important Distinction

Realtime presence is not the same as simply making responses faster.

Low latency helps, but presence also depends on:

- when the system chooses to speak
- when it chooses to stay silent
- how it handles partial input
- how it recovers from interruption
- whether it remembers what was happening before the interruption
- whether it can track the user's current focus
- whether it can maintain a stable interaction state

A fast system can still feel mechanical. A slightly slower system can feel more present if the timing and state transitions are coherent.

## Possible Design Directions

### Audio-First Interaction Loop

An early prototype could focus on realtime audio input and output.

The loop may include:

- microphone input
- voice activity detection
- speech-to-text
- partial transcript handling
- live state update
- model response generation
- text-to-speech
- interruption handling

This does not need to be perfect at first. The first goal is to create a minimal
loop that can be measured, observed, and improved.

The project has already used a transcript-first path to establish partial/final
speech events, state updates, shared continuity, and latency traces. The current
next direction is a browser-based realtime speech-to-speech slice where the
browser owns the WebRTC media plane and QSF owns memory, context, tools, and
observability through the server. That keeps realtime presence connected to the
same event, reducer, state, and trace model as the rest of the framework.

This realtime conversation path is the intended primary operating mode for QSF as
the project matures. Smaller experiments remain useful because they isolate and
measure pieces of presence, but they are scaffolding around the live conversation
goal.

### State-Oriented Interaction

Realtime presence should probably be modeled as state, not as isolated messages.

Possible live state includes:

- current user utterance
- partial interpretation
- current attention focus
- active topic
- recent interruptions
- system speaking/listening state
- pending response intent
- recent memory candidates
- unresolved questions
- emotional or motivational tone, if explored later

This suggests a design where the system has a visible interaction state machine or similar structure.

### Interruptible Output

A present system should be interruptible.

If the user starts speaking while the system is talking, the system should be able to:

- stop speaking
- preserve what it was trying to say
- listen to the interruption
- decide whether to resume, revise, or abandon the previous response

This may be more important for presence than generating polished long answers.

### Partial Understanding

The system may need to operate on incomplete input.

For example, it could form tentative interpretations while the user is still speaking, then revise them when more information arrives.

This creates design questions:

- What should be updated on partial input?
- What should wait for a complete utterance?
- How much speculative interpretation is useful?
- How can incorrect early assumptions be corrected cleanly?

### Timing as a Signal

Silence, hesitation, interruption, and pacing can all carry meaning.

The system may eventually treat timing as part of perception:

- long pause
- quick correction
- repeated hesitation
- sudden interruption
- rapid topic change
- user speaking over the system

At first, this can be logged rather than interpreted deeply.

### Presence Without Full Autonomy

Realtime presence does not require uncontrolled agency.

The system can feel present while still operating inside clear boundaries:

- listen
- observe
- respond
- inspect allowed data
- update internal state
- use read-only tools

External action should remain restricted until the project deliberately revisits that boundary.

## Relationship to Memory

Realtime presence and memory are tightly connected.

The live loop may create many small memory signals:

- what the user talked about
- what the system attended to
- what was repeated
- what seemed important
- what caused confusion
- what was interrupted
- what remained unresolved

Not all of this should become long-term memory.

The system may need a layered approach:

```text
Live interaction state
  -> short-term session memory
  -> candidate memory events
  -> sleep-phase consolidation
  -> long-term associative memory
```

Realtime presence therefore produces raw material for later memory consolidation.

## Relationship to External Inputs

Audio is likely the first external input, but the concept can extend to other inputs:

- video
- screen state
- files
- local environment signals
- sensors
- logs
- user activity events

The important idea is that external inputs should not merely be tool calls. They can become part of the simulation's perceived world.

## Relationship to Tools as Perception

Realtime presence gives the system a live interaction channel.

Tools as perception give the system additional ways to inspect the world.

Together, they suggest a model where the simulation can maintain a changing internal picture of what is happening, rather than waiting passively for complete prompts.

Early tools should remain mostly read-only.

## Open Questions

### RealtimeLoopGranularity

How often should the system update its live state?

Possibilities include:

- per audio frame
- per speech segment
- per partial transcript
- per completed utterance
- per interaction turn

Different layers may need different update frequencies.

### PartialInputPolicy

What should the system do with partial speech-to-text results?

Possible approaches:

- ignore partial results until the utterance is complete
- use partial results only for anticipation
- update live state continuously
- allow early response planning
- allow interruption decisions but not memory formation

### InterruptionSemantics

How should the system interpret interruption?

An interruption may mean:

- the user disagrees
- the user wants to correct something
- the answer is too long
- the topic changed
- the user is impatient
- the interruption was accidental

The first prototype should probably log interruptions without over-interpreting them.

### LatencyBudget

What latency is acceptable for different parts of the loop?

Different latency budgets may apply to:

- detecting speech
- transcribing speech
- deciding whether to interrupt output
- generating a short backchannel response
- generating a full answer
- producing speech output

### PresenceEvaluation

How should realtime presence be evaluated?

Possible evaluation questions:

- Does the system respond at the right time?
- Does it handle interruption gracefully?
- Does it preserve context across interruptions?
- Does it avoid speaking too much?
- Does it recover from recognition errors?
- Does it feel situated in the current interaction?

This may require subjective human evaluation in addition to technical metrics.

### BoundaryBetweenLiveAndSleep

Which updates should happen during the live loop, and which should be postponed to a sleep-like consolidation phase?

The live loop should probably be cheap, fast, and bounded. Deeper reflection and memory restructuring may belong between sessions.

## Risks and Failure Modes

### LatencyBreaksPresence

If the loop is too slow, the system may feel less present even if the answers are good.

Mitigation:

- measure latency early
- separate fast reactions from deeper responses
- use small models or specialized components where appropriate
- avoid unnecessary context loading in the live loop

### OverEagerInterruption

If the system reacts too early, it may interrupt the user or make wrong assumptions.

Mitigation:

- distinguish tentative state from committed interpretation
- use conservative interruption policies
- make early reactions short and reversible

### SpeechInterfaceDominatesDesign

Audio may become so central that the project turns into a voice assistant project.

Mitigation:

- keep the project vision visible
- treat audio as one input channel, not the whole project
- connect realtime behavior to memory, attention, and continuity

### TooMuchLiveCognition

Trying to do too much during the live loop may make the system expensive and slow.

Mitigation:

- keep live cognition small
- defer consolidation to sleep phase
- retrieve only a small amount of context
- log raw signals for later processing

### FakePresenceWithoutContinuity

A system may sound natural in the moment but fail to maintain continuity over time.

Mitigation:

- connect live interaction to memory
- preserve unresolved topics
- consolidate important events
- inspect what the system remembered and forgot

## Possible Experiments

These experiments are presence probes and build slices. They should inform the
eventual realtime conversation mode rather than define the project as a collection
of unrelated demos.

### Experiment: Minimal Audio Loop

Build a minimal loop with:

- microphone input
- speech-to-text
- model response
- text-to-speech

Measure latency and observe interaction quality.

### Experiment: Streaming Transcription MVP

Represent live speech as partial and final transcript events.

Compare simulated transcript input with provider-backed streaming transcription, and
measure whether transcript timing gives useful presence signals before full voice
output is implemented.

### Experiment: Interruptible Speech

Test whether the system can stop or revise a realtime spoken response when the
user starts talking.

Observe whether it can resume or revise its response coherently.

### Experiment: Realtime Browser Voice MVP

Build the first browser-based full-duplex voice slice: user speaks, the realtime
model responds audibly, interruption is exercised, and relayed events are recorded
as diagnostic-only QSF exchanges.

Measure latency, turn-taking quality, interruption behavior, transcript
divergence, and whether the trust boundary stays observable.

### Experiment: Partial Transcript State

Feed partial speech-to-text results into a live state tracker.

Compare behavior with a system that only reacts after complete utterances.

### Experiment: Presence Logging

Log interaction events such as:

- speech start
- speech end
- pause duration
- interruption
- response start
- response stop
- topic shift

Use logs to understand what makes the system feel present or absent.

### Experiment: Short Reactions vs Full Responses

Separate quick responses from deeper answers.

Example:

- fast acknowledgement or clarification
- slower reflective answer

Evaluate whether this improves perceived presence.

## Related Concepts

- Concept.AssociativeMemory.md
- Concept.ToolsAsPerception.md
- Concept.ExternalInputs.md
- Concept.SleepPhase.md
- Concept.ContextBudget.md
- Concept.MultiModelMind.md
- docs/Plans/Design.RealtimeVoiceConversation.md
- docs/Architecture/Architecture.RealtimeSessionServer.md

## Current Status

Exploratory.

Realtime presence is a central concept. The full behavior is still exploratory,
but the first browser speech-to-speech architecture has an accepted Phase-0
decision baseline as of 2026-06-09.

The next useful step is the realtime browser voice MVP with detailed logging,
followed by a focused review of latency, interruption behavior, transcript
divergence, and state handling.
