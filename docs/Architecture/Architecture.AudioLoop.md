# Architecture: Audio Loop

## Status

Maturity: Sketch

This document captures an early candidate architecture for real-time audio interaction in Qualia Signal Foundry. It is not a finalized design. The purpose is to preserve the implementation investigation, identify unresolved questions, and provide a starting point for prototypes.

## Implementation Status

The transcript-first pipeline, text-owned voice loop, and realtime provider bridge
described later in this document are implemented. A browser-based full-duplex
realtime voice conversation now has dedicated server/UI infrastructure, with
human-verified end-to-end barge-in evaluation still pending.

**Implemented today (all under the optional `openai` Cargo feature):**

- `TranscriptProvider` trait with a simulated provider for deterministic tests and
  a `gpt-realtime-whisper` adapter for live streaming transcription
  ([audio/transcript_provider.rs](../../crates/qsf_app/src/audio/transcript_provider.rs))
- Microphone capture and prerecorded WAV evaluation paths, both opt-in via
  `QSF_TRANSCRIPT_INPUT_SOURCE`
- `TranscriptEventEmitter` producing partial and final transcript events into the
  runtime
  ([audio/transcript_event_emitter.rs](../../crates/qsf_app/src/audio/transcript_event_emitter.rs))
- `SpeechOutputProvider` (metadata-only by default), preserving the invariant that
  `SpeechPlaybackRequested.text` equals `OutputProduced.message`
  ([audio/speech_output_provider.rs](../../crates/qsf_app/src/audio/speech_output_provider.rs))
- `VoiceSessionProvider` for `gpt-realtime-2` speech-to-speech sessions, with tool
  requests routed into QSF events and persisted shared exchange records rather than
  executed directly
  ([audio/voice_session_provider.rs](../../crates/qsf_app/src/audio/voice_session_provider.rs))
- `qsf_realtime_server` plus its dedicated `ui/` preview surface for the browser
  WebRTC transport slice, including server-side SDP rendezvous and diagnostic
  browser relay
  ([crates/qsf_realtime_server/src](../../crates/qsf_realtime_server/src),
  [crates/qsf_realtime_server/ui](../../crates/qsf_realtime_server/ui))
- The browser realtime preview now runs with `create_response = false` so the
  server-side sideband can inject context before issuing `response.create`
- Text-owned voice loop where QSF owns interpretation, shared session continuity,
  memory retrieval, context assembly, model-role invocation, and `OutputProduced`
  text
  ([experiments/text_owned_voice_loop.rs](../../crates/qsf_app/src/experiments/text_owned_voice_loop.rs))
- Peer `voice-loop` surface that reuses the same QSF-owned voice pipeline without
  changing the text-owned loop behavior
  ([experiments/voice_loop.rs](../../crates/qsf_app/src/experiments/voice_loop.rs))
- The text-owned voice loop now boots through the shared session runtime, records a
  voice `Exchange` through the live-session reducer, persists a derived `Turn`, and
  updates the continuity manifest after successful simulated or provider-backed
  transcript runs
  ([session/runtime.rs](../../crates/qsf_app/src/session/runtime.rs),
  [session/live_state.rs](../../crates/qsf_app/src/session/live_state.rs))
- The realtime voice-session experiment now boots through the shared session
  runtime, bridges provider final transcript, preamble, response lifecycle,
  interruption, and tool-call facts into live-session reducer events, and persists
  the resulting voice `Exchange` in `SessionState.exchanges`
  ([experiments/realtime_voice_session.rs](../../crates/qsf_app/src/experiments/realtime_voice_session.rs),
  [session/exchange.rs](../../crates/qsf_app/src/session/exchange.rs))
- Completed voice exchanges are consumed by sleep consolidation through the shared
  normalized sleep view. Final transcripts and completed response text can become
  sleep context, while provider preamble text stays out of promotable sleep input
  and remains diagnostic-only
  ([session/sleep_records.rs](../../crates/qsf_app/src/session/sleep_records.rs),
  [experiments/sleep_phase_session_summary.rs](../../crates/qsf_app/src/experiments/sleep_phase_session_summary.rs))
- Latency reporting across transcript dispatch, memory retrieval, context assembly,
  model runtime, and speech output
- `session_id` propagation across the voice turn and across resumed text-owned voice
  sessions

**Partial:**

- Voice Activity Detection is at the provider boundary; no separate VAD module
- Operating modes: turn-based is implemented; the browser realtime preview path
  exists, but human-verified end-to-end barge-in evaluation is still pending

**Not yet implemented:**

- Render-only TTS for live spoken answers (current speech output is metadata-only)
- Human-verified browser WebRTC conversation with barge-in acceptance remains
  pending
- Full interruption / barge-in policy beyond persisted provider interruption facts
- Always-listening mode outside explicit browser session start/stop
- Translation provider (`gpt-realtime-translate`) integration
- A live debug UI for audio state

Last reviewed: 2026-06-10 against the implemented manual-response
preview path and the still-pending human audio verification.

## Summary

The audio loop is the part of the system that allows the simulation to listen, interpret speech, respond with voice, and handle timing-sensitive interaction.

Audio is not only an input/output feature. In this project, audio is part of simulated presence. A system that can hear the user, react at the right time, pause, be interrupted, and continue coherently may feel more like a continuous entity than a text-only prompt-response loop.

The early architecture should therefore prioritize:

- low enough latency to feel conversational
- clear separation between audio capture, interpretation, cognition, and playback
- observability of timing and state transitions
- support for interruption and turn-taking
- controlled experimentation rather than premature optimization

## Related Concepts

This architecture is related to:

- `Concept.RealtimePresence.md`
- `Concept.ExternalInputs.md`
- `Concept.ToolsAsPerception.md`
- `Concept.ContextBudget.md`
- `Concept.MultiModelMind.md`

The concept documents explain why audio matters. This document describes how an audio loop might be structured.

## Goals

The audio loop should support experiments with:

- microphone input
- real-time or near-real-time speech recognition
- conversational turn-taking
- user interruption while the system is speaking
- speech synthesis output
- timing-sensitive presence
- latency measurement
- transcript capture
- event logging
- controlled degradation when services fail

The first implementation does not need to be sophisticated. It should be simple, observable, and easy to replace.

## Non-Goals

The early audio loop is not intended to:

- implement a complete voice assistant
- optimize for production-grade audio quality
- support every audio device configuration
- support autonomous outbound communication
- hide internal timing or decision state
- solve all interruption and barge-in behavior immediately
- lock the project into one model provider or audio backend

The first version should be a research instrument, not a polished voice product.

## High-Level Flow

A simple audio interaction loop can be described as:

```text
Microphone
  -> Audio Capture
  -> Voice Activity Detection
  -> Transcript Provider or Realtime Session Provider
  -> Partial or Final Transcript Event
  -> Simulation State Update
  -> Context Selection
  -> Model Response
  -> Speech Synthesis
  -> Speaker Output
  -> Transcript and Event Log
```

This should be treated as a pipeline of events, not as one monolithic function.

The first provider-backed implementation used streaming transcription as the
boundary into the runtime loop. The next planned live voice slice is a browser
WebRTC speech-to-speech session that still maps provider facts back into the same
QSF event stream and keeps provider-owned media separate from QSF-owned memory,
tools, and observability.

The current text-owned voice-loop experiment adds a second, deterministic boundary:
speech output is a renderer for QSF-owned `OutputProduced` text, not the owner of the
answer. The default loop therefore treats audio providers as adapters around a normal
QSF live-session exchange:

```text
TranscriptProvider
  -> AudioFinalTranscript
  -> InputReceived
  -> ContextAssembled
  -> ConversationalResponder
  -> OutputProduced
  -> SpeechOutputProvider
  -> SpeechPlaybackCompleted
  -> persisted SessionState + continuity manifest
```

Once persisted, finalized voice exchanges participate in the same sleep and
cross-session continuity path as text turns. Sleep reads the shared turn/exchange
view, writes the shared memory store and consolidated brief, and the next voice run
can resume from that brief. Provider preambles and provider lifecycle metadata remain
observable diagnostics rather than QSF-owned cognition or promotable memory text.

## Candidate Runtime Model

The audio loop should probably be event-driven.

Important event types may include:

```text
AudioInputStarted
AudioInputChunkCaptured
SpeechDetected
SpeechSegmentStarted
SpeechSegmentEnded
TranscriptPartialReceived
TranscriptFinalized
UserTurnStarted
UserTurnEnded
ModelResponseStarted
ModelResponseTokenReceived
SpeechOutputStarted
SpeechOutputEnded
UserInterruptedOutput
AudioErrorOccurred
LatencyMeasurementRecorded
```

These events make the system easier to inspect, replay, test, and debug.

## Candidate Components

### Audio Device Layer

Responsible for microphone and speaker access.

Responsibilities:

- enumerate audio devices
- select input and output devices
- capture microphone samples
- play synthesized audio
- handle device errors
- expose audio data in a normalized internal format

This layer should be replaceable. The rest of the system should not depend directly on a specific audio library.

### Audio Capture Buffer

Responsible for temporarily holding captured audio.

Responsibilities:

- store recent audio chunks
- preserve timestamps
- support streaming to downstream components
- allow debug recording when enabled
- avoid unbounded memory growth

The buffer should preserve enough timing information to measure end-to-end latency.

### Voice Activity Detection

Responsible for identifying when the user appears to be speaking.

Responsibilities:

- detect speech start
- detect speech end
- reduce unnecessary transcription calls
- support push-to-talk or always-listening modes
- provide interruption signals while the system is speaking

The first version may use a simple heuristic or an external service. It does not need to be perfect.

### Speech Recognition

Responsible for converting user audio into text or structured input.

Possible modes:

- batch transcription after a detected speech segment
- streaming transcription with partial results
- direct realtime model input, if supported by the selected provider

The architecture should not assume only one mode.

Current provider direction:

```text
TranscriptProvider
  -> simulated transcript provider for tests
  -> gpt-realtime-whisper adapter for streaming speech-to-text

SpeechOutputProvider
  -> simulated speech output provider for exact-text boundary tests
  -> future render-only TTS provider after the simulated boundary is proven

RealtimeSessionProvider
  -> existing gpt-realtime-2 one-shot adapter
  -> browser realtime voice preview transport via qsf_realtime_server

TranslationProvider
  -> gpt-realtime-translate adapter for separate translation experiments
```

The transcript provider is the first real audio integration point. It should emit
partial and final transcript events while leaving state updates, memory promotion,
and tool permissions to the normal QSF runtime.

### Interaction Controller

Responsible for turning audio-derived events into interaction turns.

Responsibilities:

- decide when a user turn starts
- decide when a user turn ends
- handle partial transcripts
- handle user interruption
- decide whether to cancel, pause, or continue model output
- send finalized user input into the simulation loop

This is likely to become one of the most important components for perceived presence.

### Simulation Loop Bridge

Responsible for connecting the audio loop to the rest of the simulation.

Responsibilities:

- submit user input to the live simulation loop
- include relevant session state
- request memory retrieval when needed
- receive model output
- expose response state back to the audio loop

This bridge should prevent the audio subsystem from becoming the whole application.

### Speech Synthesis

Responsible for converting model output into audio output.

Possible modes:

- synthesize the full response after it is complete
- synthesize sentence by sentence
- synthesize streaming output as text is generated

The first implementation may start with full-response synthesis. Later experiments can test whether streaming synthesis improves perceived presence.

### Playback Controller

Responsible for speaker output and interruption behavior.

Responsibilities:

- play synthesized audio
- stop playback when interrupted
- support pause/resume if useful
- record output timing
- expose whether the system is currently speaking

The playback controller should be connected to interruption detection.

The first `SpeechOutputProvider` implementation is metadata-only by default. It
records provider, voice, timing, and byte-count metadata while preserving the invariant
that `SpeechPlaybackRequested.text` is exactly the `OutputProduced.message`.

### Transcript and Event Log

Responsible for preserving what happened.

Responsibilities:

- store user transcripts
- store system responses
- store timestamps
- store interruption events
- store latency measurements
- store audio-loop errors
- optionally link events to later memory formation

The event log is important because the project is research-oriented. The system should be inspectable after a session.

## Candidate Data Flow

The system should distinguish between raw audio, transcript text, interaction events, and memory records.

```text
Raw audio chunk
  -> timestamped audio event
  -> speech segment
  -> transcript candidate
  -> finalized user turn
  -> simulation event
  -> response event
  -> synthesized speech event
  -> session log entry
  -> possible memory candidate
```

Not every audio event should become memory. The sleep phase or memory system should later decide what is worth preserving.

## Operating Modes

### Push-to-Talk Mode

The simplest early mode.

Advantages:

- easier to implement
- avoids accidental listening
- simplifies turn boundaries
- easier to debug

Disadvantages:

- less natural
- weaker sense of continuous presence
- does not test interruption as well

This may be the best first milestone.

### Turn-Based Voice Mode

The system listens for a speech segment, transcribes it, responds, and then listens again.

Advantages:

- closer to natural conversation
- still relatively simple
- supports latency measurement

Disadvantages:

- turn boundary detection matters
- interruption behavior may still be limited

This is a likely second milestone.

### Full Duplex Realtime Mode

The user and system can overlap, and the system can react while audio is still arriving.

Advantages:

- strongest sense of real-time presence
- enables interruption, backchannels, and more natural timing

Disadvantages:

- much harder to implement
- more provider-dependent
- harder to debug
- higher risk of confusing state transitions

This is the current planned browser realtime voice direction. The simpler
transcript-first and text-owned voice boundaries remain the deterministic test
foundation; the full-duplex browser path must still prove its event mapping,
trust boundary, latency, and interruption behavior.

## Interruption Handling

Interruption is central to realtime presence.

A useful early rule:

```text
If the user speaks while the system is speaking, treat this as a possible interruption event.
```

Possible responses:

- stop speech output immediately
- continue listening but finish the current sentence
- pause output and wait for clarification
- ignore very short noises
- ask the model whether the output should be cancelled

The first version should probably use a simple deterministic policy:

```text
User speech detected during system speech
  -> stop playback
  -> mark current response as interrupted
  -> capture the new user turn
  -> include interruption context in the next model input
```

This should be treated as an experiment, not a final behavior.

## Latency Considerations

The audio loop should measure latency explicitly.

Useful latency measurements:

- time from user speech start to speech detection
- time from user speech end to transcript finalized
- time from transcript finalized to model response start
- time from model response start to synthesized audio start
- time from user speech end to audible system response
- time from interruption speech start to playback stopped

The project should avoid vague claims such as “fast enough” without measurements.

The early question is not only technical latency, but perceived latency:

```text
At what delay does the system stop feeling present?
```

That belongs in `ResearchQuestions.Audio.md`, but the architecture should make the question measurable.

## Context and Memory Interaction

Audio sessions can generate a lot of data. The system should not automatically treat the full transcript as always-relevant context.

Possible handling:

- keep a short rolling transcript in live context
- summarize completed turns
- mark notable events as memory candidates
- preserve interruption events as part of interaction history
- allow sleep-phase consolidation to decide what matters
- avoid loading entire prior audio session transcripts into the live loop

This connects the audio loop to the context budget and associative memory design.

## Observability Requirements

The audio loop should be easy to inspect.

At minimum, it should expose:

- current mode
- selected input and output devices
- whether the system is listening
- whether speech is currently detected
- whether the system is thinking
- whether the system is speaking
- last finalized transcript
- current partial transcript, if available
- current response text
- interruption state
- latency measurements
- recent audio-loop events

A debug UI or structured log will be important even for early prototypes.

## Error Handling

The audio loop should degrade gracefully.

Possible error cases:

- no microphone available
- no speaker available
- microphone permission denied
- speech recognition unavailable
- speech synthesis unavailable
- model provider unavailable
- audio buffer overflow
- transcription timeout
- synthesis timeout
- playback failure

The system should prefer explicit degraded modes over silent failure.

Examples:

```text
Audio input unavailable -> allow typed input
Speech synthesis unavailable -> show text output only
Realtime model unavailable -> fall back to turn-based transcription
```

## Security and Control Boundaries

The early audio system should not create uncontrolled agency.

Audio input is a perception channel. It should not automatically grant permission to act externally.

Important boundaries:

- listening mode should be explicit
- debug recording should be explicit
- outbound communication should remain out of scope for early versions
- external tool access should remain controlled
- transcripts should be inspectable
- memory formation from audio should be reviewable or at least observable

## Candidate First Prototype

A minimal first prototype could be:

```text
Simulated or microphone audio source
  -> streaming transcript provider
  -> partial transcript events
  -> final transcript event
  -> runtime input event
  -> report with latency trace
```

This should be implemented before a microphone-to-speaker loop because it validates
the event boundary, partial/final transcript semantics, and latency traces with less
hardware and playback complexity.

Useful success criteria:

- partial transcript events are logged
- final transcript events enter the runtime loop as input
- latency is measured for transcript deltas and finalization
- failures are visible
- code structure does not lock in one provider

## Candidate Second Prototype

After streaming transcription works, a minimal voice loop could be:

```text
Push-to-talk microphone input
  -> record speech segment
  -> transcribe after release
  -> send transcript to model
  -> synthesize complete response
  -> play response
  -> log transcript, response, and timings
```

This prototype would test the basic loop without requiring full duplex audio.

Useful success criteria:

- user can speak instead of type
- system responds with voice
- transcript and response are logged
- latency is measured
- failures are visible
- code structure does not lock in one provider

## Candidate Third Prototype

The next prototype could add automatic turn detection:

```text
Always-listening input
  -> detect speech start/end
  -> transcribe completed speech segments
  -> respond with voice
  -> return to listening
```

This would test whether the system starts to feel more present when the user does not need to press a key.

## Candidate Fourth Prototype

A later prototype could test interruption:

```text
System speaking
  -> user starts speaking
  -> playback stops
  -> new user speech is captured
  -> next model input includes the interruption context
```

This would test whether interruption handling significantly improves the feeling of realtime presence.

## Open Questions

- Should the first voice-output prototype use push-to-talk or automatic
  automatic voice activity detection?
- How much partial transcript should the simulation see before the user turn is finalized?
- Should the system be allowed to respond before the user has fully stopped speaking?
- How should interruption be represented in the model input?
- Should interrupted model responses be stored as memories?
- What latency thresholds matter for perceived presence?
- Should audio timing become part of the self-model or only the session log?
- How much raw audio, if any, should be preserved after transcription?
- Should voice choice be part of identity simulation?

## Design Risks

### Audio Becomes the Product

There is a risk that the project drifts into building a polished voice assistant. Audio is important, but it is only one part of the broader consciousness-simulation platform.

### Provider Lock-In

Realtime audio APIs can strongly shape architecture. The design should avoid making the whole platform depend on one provider-specific audio model.

### Hidden State

Realtime systems can become difficult to debug if state transitions are implicit. The audio loop should expose events and timing information.

### Excessive Context Growth

Audio interaction can generate large transcripts quickly. The system needs summarization, memory filtering, and context-budget discipline.

### Poor Interruption Behavior

A system that cannot be interrupted may feel less present. A system that interrupts itself too aggressively may feel unstable. This should be tested experimentally.

### Privacy Confusion

Always-listening behavior can feel sensitive. The system should make listening, recording, and memory formation explicit and observable.

## Suggested Next Documents

This architecture document should eventually be supported by:

```text
docs/Research/ResearchQuestions.Audio.md
docs/Experiments/Experiment.StreamingTranscriptionMVP.md
docs/Experiments/Experiment.AudioLoopMVP.md
docs/Experiments/Experiment.InterruptionHandlingAudio.md (planned)
docs/Architecture/Architecture.StateAndObservability.md
```

## Current Recommendation

Keep the streaming transcription and deterministic text-owned voice-loop boundaries
as the regression-safe foundation. They prove QSF-owned interpretation, context,
memory retrieval, response text, shared continuity, and latency tracing.

The next live audio step is the browser realtime voice MVP described in
`docs/Plans/Plan.RealtimeVoiceConversation.md` and
`docs/Architecture/Architecture.RealtimeSessionServer.md`: WebRTC media in the
browser, `qsf_realtime_server` for server-side SDP rendezvous, diagnostic-only
browser relay events, and authoritative sideband control.

Do not let the realtime model become the whole simulated mind. The media plane may
be provider-owned, but memory, context injection, tool permission, event logs, sleep
eligibility, and continuity remain QSF-owned.
