# Experiment: Text-Owned Voice Loop

## Status

Implemented as a deterministic first-pass experiment path.

## Purpose

Test a voice-loop-shaped turn where audio providers only adapt input and output, while
QSF owns the transcript-to-context-to-model-to-output path.

## Implementation Shape

- Experiment id: `text-owned-voice-loop`
- Default transcript provider: deterministic `SimulatedTranscriptProvider`.
- Default model provider: deterministic `MockModelClient`.
- Default speech output provider: deterministic `SimulatedSpeechOutputProvider`.
- Transcript provider selection reuses `QSF_TRANSCRIPT_PROVIDER`.
- Model provider selection reuses `QSF_MODEL_PROVIDER`.
- Speech output selection uses `QSF_SPEECH_OUTPUT_PROVIDER`, but OpenAI speech output
  is intentionally unavailable until the simulated exact-text boundary is proven.

Default flow:

```text
SimulatedTranscriptProvider
  -> AudioFinalTranscript
  -> InputReceived
  -> voice context assembly
  -> ConversationalResponder model role
  -> OutputProduced
  -> SimulatedSpeechOutputProvider
  -> SpeechPlaybackStarted / SpeechPlaybackCompleted
```

## Observability

The experiment records:

- audio input lifecycle and partial/final transcripts
- `AudioFinalTranscript -> InputReceived` bridge
- context assembly request and result
- `ConversationalResponder` model role request/completion/failure
- `OutputProduced` before speech output receives text
- speech playback request/start/completion metadata
- latency trace for capture, transcription, runtime bridge, context/model, and speech
  output stages

Raw audio, API keys, and authorization headers are not written to events, traces, or
reports.

## Verification

Default deterministic run:

```powershell
cargo run -p qsf_app -- experiment text-owned-voice-loop
```

Targeted regression tests:

```powershell
cargo test -p qsf_app text_owned_voice_loop
```

The regression tests assert that only final transcripts create `InputReceived`, one
session id correlates the turn, `SpeechPlaybackRequested.payload["text"]` equals
`OutputProduced.payload["message"]`, model failure prevents `OutputProduced`, and
speech-provider failure sanitizes credential-like errors.
