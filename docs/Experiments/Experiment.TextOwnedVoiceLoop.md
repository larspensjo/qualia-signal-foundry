# Experiment: Text-Owned Voice Loop

## Status

Implemented as a deterministic first-pass experiment path. Live microphone input and
one retrieved memory context fragment now flow through the same QSF-owned answer path.

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
- Empty final transcripts are treated as transcription failures and do not become
  `InputReceived`.
- Memory retrieval uses the existing Phase 4 fixture and association-weighted
  retrieval, selecting one memory candidate into the four-fragment voice context
  budget.

Default flow:

```text
SimulatedTranscriptProvider
  -> AudioFinalTranscript
  -> InputReceived
  -> association-weighted memory retrieval
  -> voice context assembly
  -> ConversationalResponder model role
  -> OutputProduced
  -> SimulatedSpeechOutputProvider
  -> SpeechPlaybackStarted / SpeechPlaybackCompleted
```

Live microphone input keeps the same QSF-owned response path:

```text
QSF_TRANSCRIPT_PROVIDER=openai
QSF_TRANSCRIPT_INPUT_SOURCE=mic
  -> OpenAI realtime transcript provider
  -> AudioFinalTranscript
  -> InputReceived
  -> association-weighted memory retrieval
  -> ConversationalResponder
  -> OutputProduced
  -> simulated speech output metadata
```

## Observability

The experiment records:

- audio input lifecycle and partial/final transcripts
- `AudioFinalTranscript -> InputReceived` bridge
- `MemoryRetrievalRequested` and `MemoryRetrieved`
- context assembly request and result
- `ConversationalResponder` model role request/completion/failure
- `OutputProduced` before speech output receives text
- speech playback request/start/completion metadata
- latency trace for capture, transcription, runtime bridge, context/model, and speech
  output stages
- generated turn reports list final transcript, memory retrieval, context assembly,
  model role, speech output, and total observed turn latency separately
- generated turn reports include a diagnostics section for response ownership,
  selected memory context, exact speech handoff, model latency, total observed latency,
  and raw-audio logging status

Successful runs also print the QSF-owned response text to stdout so live microphone
tests can be checked without opening the run artifact first. Run artifacts include
`memory-fixture.json` and the selected memory context id in `text-owned-voice-loop.md`.

Raw audio, API keys, and authorization headers are not written to events, traces, or
reports.

If the live provider returns an empty final transcript, the experiment records
`AudioTranscriptionFailed` and stops before runtime input, context assembly, model
role invocation, or speech playback.

## Verification

Default deterministic run:

```powershell
cargo run -p qsf_app -- experiment text-owned-voice-loop
```

Targeted regression tests:

```powershell
cargo test -p qsf_app text_owned_voice_loop
```

OpenAI transcript-provider compile check:

```powershell
cargo test -p qsf_app --features openai audio::transcript_provider::tests::openai_realtime_provider_validates_local_inputs_before_network_call
```

Live microphone evaluation:

```powershell
$env:QSF_TRANSCRIPT_PROVIDER="openai"
$env:QSF_TRANSCRIPT_INPUT_SOURCE="mic"
$env:QSF_TRANSCRIPT_MIC_DEVICE="default"
$env:QSF_TRANSCRIPT_MIC_DURATION_MS="4000"
$env:QSF_MODEL_PROVIDER="mock"
$env:QSF_SPEECH_OUTPUT_PROVIDER="simulated"
$env:QSF_SPEECH_OUTPUT_MODE="metadata-only"
cargo run -p qsf_app --features openai -- experiment text-owned-voice-loop
```

The regression tests assert that only final transcripts create `InputReceived`, one
session id correlates the turn, one retrieved memory fragment participates in selected
context, `SpeechPlaybackRequested.payload["text"]` equals
`OutputProduced.payload["message"]`, model failure prevents `OutputProduced`, and
speech-provider failure sanitizes credential-like errors. A latency regression test
uses a deliberately delayed mock model to ensure total turn latency includes model-role
runtime.

## Live Evaluation Notes

2026-05-14:

- The OpenAI realtime transcript provider compiled with the current text-owned loop
  refactor.
- Two live microphone runs reached `openai-realtime-transcript-provider` and returned
  final transcript events, but the transcript text was empty.
- The loop was updated so empty final transcripts fail as `AudioTranscriptionFailed`
  and do not create `InputReceived`.
- Guarded failure artifact: `runs/2026-05-14-112211-text-owned-voice-loop`.
- A later live microphone run succeeded with the final transcript
  "Tell me something about yourself."
- Successful live artifact: `runs/2026-05-14-113329-text-owned-voice-loop`.
- The successful run emitted six partial transcript revisions, one final transcript,
  `InputReceived`, `ContextAssembled`, `ModelRoleRequested`,
  `ModelRoleCompleted`, `OutputProduced`, `SpeechPlaybackRequested`, and
  `SpeechPlaybackCompleted`.
- Measured first partial transcript latency was 1648 ms, final transcript latency was
  2923 ms, and total text-owned voice-loop latency was 3069 ms with simulated speech
  output.
- A follow-up live run used `QSF_MODEL_PROVIDER=openai` for the
  `ConversationalResponder` while keeping speech output simulated.
- OpenAI model artifact: `runs/2026-05-14-113743-text-owned-voice-loop`.
- The follow-up run transcribed "Tell me something funny and unexpected about
  yourself.", completed the model role through OpenAI
  (`gpt-5.4-nano-2026-03-17`), and produced QSF-owned output text before simulated
  speech output received it.
- The OpenAI model call reported 1937 ms model latency, 89 input tokens, and 45 output
  tokens.
- The next implementation pass wired association-weighted memory retrieval into the
  same path before context assembly. New runs should show `MemoryRetrievalRequested`,
  `MemoryRetrieved`, and a `Selected memory context` entry in
  `text-owned-voice-loop.md`.
- The latency report was corrected after the first memory-context live run exposed that
  total turn latency did not include model-role runtime. New reports should show
  `Model role latency` and `Total observed turn latency`.

2026-05-15:

- Generated `text-owned-voice-loop.md` reports now include a `Diagnostics` section so
  response ownership, selected memory context, exact speech handoff, model latency,
  total observed latency, and raw-audio logging status are visible without manually
  cross-checking events and traces.
