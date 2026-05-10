# Experiment: Audio Loop MVP

## Experiment ID

`Experiment.AudioLoopMVP`

## Status

Proposed

## Summary

This experiment tests the smallest useful real-time audio loop for Qualia Signal Foundry.

The goal is to determine whether the project can capture microphone input, convert speech to text, pass the result into the runtime loop, generate a response, synthesize speech, and play it back while recording enough timing and trace information to evaluate the experience.

This is not intended to create a polished voice assistant. It is an experiment in real-time presence.

This experiment should follow `Experiment.StreamingTranscriptionMVP`. The transcript
event boundary should be working before this experiment adds speech synthesis,
playback, and voice-loop timing.

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
  voice-session experiment, see Phase 10 of `Plan.FrameworkMVP.md`

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

To be filled in after running the experiment.

### What Happened

TBD

### Measurements

TBD

### Observations

TBD

### Surprises

TBD

### Failure Modes

TBD

## Interpretation

TBD

Use this distinction:

```text
Observed:
  What happened.

Interpreted:
  What we think it means.

Uncertain:
  What remains unclear.
```

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

TBD

## Notes

The purpose of this experiment is not to make a polished audio assistant. The purpose is to test whether real-time audio is useful and technically manageable as a presence mechanism.
