# Experiment: Audio Loop MVP

## Experiment ID

`Experiment.AudioLoopMVP`

## Status

Superseded.

This broad all-in-one audio-loop proposal has been split into narrower experiment
paths. Transcript-first input is covered by `Experiment.StreamingTranscriptionMVP`,
provider-owned realtime voice sessions are covered by
`Experiment.RealtimeVoiceSessionMVP`, and QSF-owned spoken-turn behavior is covered by
`Experiment.TextOwnedVoiceLoop`. Keep this document as historical context for the
original audio-loop question, not as the active implementation plan.

## Summary

This experiment tests the smallest useful real-time audio loop for Qualia Signal Foundry.

The goal is to determine whether the project can capture microphone input, convert speech to text, pass the result into the runtime loop, generate a response, synthesize speech, and play it back while recording enough timing and trace information to evaluate the experience.

This is not intended to create a polished voice assistant. It is an experiment in real-time presence.

This experiment was originally intended to follow `Experiment.StreamingTranscriptionMVP`.
That work has since been decomposed into narrower voice-session and text-owned voice
experiments so the project can evaluate provider ownership, QSF response ownership,
memory retrieval, and speech handoff separately.

## Motivation

Audio is a central part of simulated presence because it introduces timing, turn-taking, interruption, hesitation, and latency.

This experiment reduces uncertainty around:

- whether a minimal audio loop is practical on the target development machine
- how much latency appears in each stage
- what observability is needed for real-time interaction
- whether audio meaningfully changes the feeling of interacting with the system
- which parts should be real in the MVP and which can initially be mocked

## Related Documents

```text
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.AudioLoop.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.ModelRoles.md
Research/ResearchQuestions.Audio.md
Experiments/Experiment.StreamingTranscriptionMVP.md
```

## Hypothesis

A minimal microphone-to-model-to-speech loop can create a noticeably stronger sense of presence than text-only interaction if the system captures timing, supports simple turn-taking, and keeps latency within an acceptable range.

## Scope

### In Scope

- microphone capture
- speech-to-text through the transcript provider boundary
- passing finalized text into a runtime loop
- generating a simple model response
- text-to-speech output
- speaker playback
- basic event logging
- latency measurement
- simple error handling
- manual testing

### Out of Scope

- polished voice UX
- emotional voice modeling
- advanced interruption handling
- video input
- long-term memory integration
- complex tool use
- multi-party conversation
- always-on background operation
- production audio device management
- full duplex `gpt-realtime-2` sessions; these belong in a separate realtime
  voice-session experiment

## Setup

Possible setup:

- Windows development machine
- Rust project skeleton
- microphone and speaker/headphones
- speech-to-text provider or local transcription prototype
- text-to-speech provider or local TTS prototype
- simple runtime loop
- event log output
- trace output for timing

The first implementation may use placeholder model responses if needed. The experiment is still useful if the audio path and timing traces can be tested independently.

Prerequisite:

```text
Experiment.StreamingTranscriptionMVP has established partial transcript events,
final transcript events, and transcript latency traces.
```

## Procedure

1. Start the audio loop.
2. Capture microphone input.
3. Detect a spoken utterance boundary, manually or through basic voice activity detection.
4. Convert speech to text.
5. Emit a `SpeechFinalText` or equivalent runtime event.
6. Pass the event into the runtime loop.
7. Generate a short response.
8. Send the response to text-to-speech.
9. Play the synthesized speech.
10. Record timing for each stage.
11. Repeat with several short spoken prompts.
12. Record subjective observations about presence, awkwardness, latency, and failure modes.

## Baseline

The baseline is a text-only interaction loop with the same model response logic.

Comparison questions:

- Does audio feel more present?
- Does latency make the interaction worse?
- Does speech input introduce too many transcription errors?
- Does speech output improve continuity or merely add friction?

## Measurements

### Quantitative Measurements

- audio capture start time
- speech boundary detection time
- transcription start and completion time
- model request start and completion time
- TTS request start and completion time
- playback start and completion time
- end-to-end latency
- time to first audible response
- transcription confidence, if available
- number of failed turns
- number of manual retries

### Qualitative Observations

- perceived presence
- perceived responsiveness
- naturalness of turn-taking
- annoyance from latency
- transcription quality
- speech output quality
- whether the interaction feels more continuous than text
- whether the system feels too slow for real-time use

## Success Criteria

The experiment is successful if:

- speech can be captured and transcribed
- a response can be generated and spoken
- the loop can complete several simple turns
- latency is measured per stage
- event logs and traces are sufficient to diagnose delays
- the result clarifies whether audio should remain an early implementation priority

The experiment can still be successful if the audio experience is poor, provided the failure modes are clear.

## Failure Criteria

The experiment is inconclusive if:

- timing is not recorded
- failures cannot be diagnosed
- the loop cannot complete even simple turns
- transcription or TTS is too unreliable to evaluate presence
- the implementation becomes too large before producing a useful result

## Required Observability

The experiment should log:

- audio input started
- audio input ended
- partial transcript, if available
- final transcript
- model role invocation
- generated response
- TTS started
- playback started
- playback completed
- errors
- latency per stage
- total end-to-end latency

## Risks and Confounders

- network latency
- provider model latency
- microphone quality
- room noise
- TTS voice quality
- transcription errors
- unclear turn boundaries
- subjective evaluation of presence
- too much implementation work before the first useful result

## Expected Output

The experiment should produce:

- short experiment notes
- event log
- timing trace
- sample transcript
- latency summary
- failure-mode notes
- recommendation for next audio experiment

## Results

This exact all-in-one experiment was not run as a single implementation path. Its
scope was decomposed into smaller experiments that proved parts of the audio loop
with clearer ownership boundaries.

### What Happened

- `Experiment.StreamingTranscriptionMVP` implemented the transcript provider boundary,
  partial/final transcript events, latency traces, and final transcript bridge into
  runtime input.
- `Experiment.RealtimeVoiceSessionMVP` implemented a full realtime voice-session
  provider path while keeping provider-requested tools routed into QSF events instead
  of direct execution.
- `Experiment.TextOwnedVoiceLoop` implemented a QSF-owned transcript-to-memory-to-
  context-to-model-to-output path with speech output metadata.
- Render-only live spoken TTS and robust interruption/barge-in remain not yet
  implemented.

### Measurements

- Streaming transcription records first partial, final transcript, and runtime input
  dispatch latency.
- Realtime voice-session runs record provider session timing, response start, first
  audio, response completion, speech playback metadata, interruption events, and tool
  request routing.
- Text-owned voice-loop runs record transcript dispatch, memory retrieval, context
  assembly, model runtime, speech output, and total observed turn latency.

### Observations

- Splitting the audio loop made ownership boundaries clearer than a single broad MVP
  would have.
- QSF can own transcript interpretation, memory retrieval, context assembly, model
  response, and output text while audio providers remain side-effect adapters.
- Provider-owned realtime voice is useful to evaluate, but it must not bypass QSF
  memory, reducer, or tool-permission boundaries.

### Surprises

- Speech-output metadata was enough to prove exact QSF text handoff before adding
  live render-only TTS.

### Failure Modes

- The project still lacks live spoken answer rendering through a real TTS provider.
- Interruption/barge-in behavior is still mostly unimplemented.
- Full-duplex always-listening behavior remains out of scope for the implemented
  slices.

## Interpretation

Observed:
  The broad audio-loop question is better served by smaller experiments with explicit
  ownership boundaries.

Interpreted:
  Future audio work should continue to distinguish transcript input, provider-owned
  realtime sessions, QSF-owned answer generation, speech output rendering, and
  interruption handling.

Uncertain:
  The next open audio questions are live spoken output quality, end-to-end perceived
  presence, and interruption behavior under real conversational timing.

## Follow-Up Questions

- What latency is acceptable for perceived presence?
- Is streaming transcription required?
- Is push-to-talk enough for early testing?
- How should interruptions be represented?
- Should audio run through the same runtime event system as text?
- Should the live model produce text, speech directives, or both?

## Follow-Up Experiments

```text
Experiment.InterruptionHandlingAudio
Experiment.AudioLatencyTimeline
Experiment.RealtimePresenceTextVsAudio
Experiment.ExternalInputEventStream
```

## Decision Candidates

- Candidate: Use a simple push-to-talk or manual utterance boundary for the first audio MVP.
- Candidate: Treat transcription output as runtime events.
- Candidate: Keep advanced interruption handling out of the first audio loop.
- Candidate: Require latency tracing for all real-time audio experiments.

## Final Status

Superseded by `Experiment.StreamingTranscriptionMVP`,
`Experiment.RealtimeVoiceSessionMVP`, and `Experiment.TextOwnedVoiceLoop`. Do not use
this document as the active audio implementation plan; use it as historical context
for the original microphone-to-response-to-speech question.

## Notes

The purpose of this experiment is not to make a polished audio assistant. The purpose is to test whether real-time audio is useful and technically manageable as a presence mechanism.
