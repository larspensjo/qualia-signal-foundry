# Experiment: Streaming Transcription MVP

## Experiment ID

`Experiment.StreamingTranscriptionMVP`

## Status

Proposed

## Summary

This experiment tests the first real-time audio integration boundary for Qualia Signal
Foundry: streaming speech-to-text as structured runtime events.

The goal is to observe live speech without building the full speech-to-speech loop yet.
Partial and final transcripts should be logged, traced, and optionally fed into the
runtime loop according to explicit rules.

## Motivation

Realtime presence depends on timing, interruption, and turn-taking, but full duplex
voice adds too much complexity for the first audio step. Streaming transcription is a
smaller test because it produces text events that fit the existing framework:

```text
audio input
  -> transcript provider
  -> partial transcript event
  -> final transcript event
  -> runtime input event
  -> reducer/state/report
```

This lets the project test audio timing and partial input semantics while preserving
the unidirectional runtime flow.

## Related Documents

```text
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.AudioLoop.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
Research/ResearchQuestions.Audio.md
Plans/Plan.FrameworkMVP.md
```

## Hypothesis

Streaming transcription can provide a useful first sense of live presence if partial
and final transcript events are observable, latency is measured, and only finalized
transcripts enter committed runtime state by default.

## Scope

### In Scope

- transcript provider abstraction
- deterministic simulated transcript provider
- OpenAI-backed streaming transcription adapter using `gpt-realtime-whisper`
- optional comparison runs with `gpt-4o-transcribe` for accuracy-sensitive
  transcription
- partial transcript events
- final transcript events
- transcript latency traces
- explicit handling of transcript provider errors
- report output summarizing transcript timing and failure modes

### Out of Scope

- speech synthesis
- speaker playback
- full speech-to-speech realtime sessions
- realtime translation
- interruption handling beyond recording overlapping transcript timing
- long-term memory creation from transcripts
- always-listening production behavior
- speaker diarization
- language detection

## Procedure

1. Run the experiment with a simulated transcript provider.
2. Emit a short sequence of partial transcript events.
3. Emit a final transcript event.
4. Feed the final transcript into the runtime loop as an input event.
5. Record latency and event-order traces.
6. If `OPENAI_API_KEY` and the provider adapter are available, repeat with
   `gpt-realtime-whisper`.
7. Review events, traces, and report output.

## Measurements

- time from audio input start to first partial transcript
- time from audio input start to final transcript
- number of partial transcript revisions
- final transcript text length
- transcription error count
- provider connection/setup latency
- failed or timed-out transcription sessions

## Proposed Required Events

These names describe the required event semantics. The implementation may refine the
exact enum names if it preserves the distinction between audio lifecycle, partial
transcript, final transcript, failure, latency, and runtime input events.

```text
AudioInputStarted
AudioInputChunkCaptured
AudioPartialTranscript
AudioFinalTranscript
AudioInputEnded
AudioTranscriptionFailed
LatencyMeasurementRecorded
InputReceived
```

## Proposed Required Traces

These trace names are also proposed and may be refined during implementation.

```text
TranscriptProviderTrace
TranscriptLatencyTrace
TranscriptRuntimeBridgeTrace
```

## Success Criteria

The experiment is successful if:

- simulated transcript events can run without external services
- final transcript events can enter the runtime loop as normal input
- partial transcript events are logged without mutating committed state
- latency is measured for first partial and final transcript
- provider failures are visible in events, traces, and reports
- API keys and authorization headers are not logged

## Failure Criteria

The experiment is inconclusive if:

- partial and final transcript events cannot be distinguished
- latency cannot be reconstructed from traces
- the provider adapter bypasses the runtime event flow
- reducers depend on provider-specific objects
- raw audio or secrets are logged accidentally

## Interpretation Guidance

Use this distinction:

```text
Observed:
  What transcript events, timings, and failures occurred.

Interpreted:
  What the result suggests about realtime presence and next audio steps.

Uncertain:
  What remains unclear until microphone, playback, or full realtime sessions are tested.
```

## Follow-Up Questions

- Should partial transcripts ever update live state, or only traces?
- What latency target should first partial transcript events meet?
- How much transcript revision is acceptable before finalization?
- Should transcript provider errors fall back to typed input?
- Should the first voice-output loop use the same transcript provider boundary?

## Follow-Up Experiments

```text
Experiment.AudioLoopMVP
Experiment.RealtimeVoiceSessionMVP
Experiment.InterruptionHandlingAudio
Experiment.RealtimeTranslationMVP
```

## Decision Candidates

- Candidate: Finalized transcript events enter the runtime loop as normal input events.
- Candidate: Partial transcripts are observable but do not mutate committed state by default.
- Candidate: `gpt-realtime-whisper` is the first provider-backed audio integration.
- Candidate: Full speech-to-speech realtime sessions wait until transcript events and traces work.

## Final Status

TBD
