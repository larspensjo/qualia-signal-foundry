# Plan: Text-Owned Voice Loop

## Status

Proposed

## Summary

The current realtime voice-session experiment proves that QSF can observe a live
OpenAI realtime voice provider. That experiment is provider-owned: the realtime model
hears speech, decides what to say, and produces audio. QSF records the lifecycle,
transcript, response, timing, and tool-call boundary events.

This plan defines the next durable audio architecture: a text-owned voice loop where
audio is input/output infrastructure and QSF owns the conversational turn.

Target shape:

```text
microphone
  -> transcript provider
  -> AudioFinalTranscript
  -> InputReceived
  -> QSF reducer/state/context/memory/model effects
  -> OutputProduced text
  -> speech output provider
  -> playback lifecycle events
```

In this model, the voice provider does not decide the answer. QSF decides the answer
through the same runtime state, reducer, memory, context, tool, and model-role paths
used by non-audio experiments.

## Why This Matters

The project is not ultimately trying to build a generic realtime voice agent. The
goal is to test whether a QSF-owned runtime can feel present, continuous, and
inspectable when spoken to.

Provider-owned realtime voice is useful as a comparison baseline, but it does not
exercise the simulated consciousness architecture deeply enough:

- memory retrieval is not in the answer path
- context assembly is not in the answer path
- reducer-owned state is not in the answer path
- QSF model roles are not in the answer path
- tool permission boundaries are only observed, not used to shape a QSF response

The text-owned loop makes voice an interface to QSF rather than a replacement for QSF.

## Goals

- Turn spoken input into a normal QSF `InputReceived` event.
- Produce spoken output from a normal QSF `OutputProduced` event.
- Keep transcript, memory, context, tool, model, and state updates inside QSF-owned
  runtime boundaries. The loop should use existing memory, context, model-role,
  and tool-permission infrastructure; it should not introduce new memory or tool
  architecture.
- Preserve the existing no-secret and no-raw-audio logging boundary.
- Record enough timing to compare:
  - transcript-first text-owned voice
  - provider-owned realtime voice
  - text-only model interaction
- Keep the first implementation deterministic by default and live-audio only by
  explicit selection.
- Make the implementation useful even before local speaker playback is polished.

## Non-Goals

- Translation or multilingual audio.
- A production voice assistant.
- Continuous always-listening mode.
- Full duplex interruption handling.
- Speaker diarization.
- Persistent identity.
- New memory architecture.
- Tool execution beyond existing QSF permission boundaries.
- Raw audio retention.
- Replacing the provider-owned realtime voice experiment.
- Using `gpt-realtime-2` to render QSF-owned text. It is a speech-to-speech
  response model, not a render-only speech output adapter.
- Making OpenAI text-to-speech a default dependency before the simulated provider
  boundary is proven.

## Existing Building Blocks

The plan should reuse current framework pieces instead of inventing a parallel audio
stack.

Useful existing modules:

```text
crates/qsf_app/src/audio/transcript_provider.rs
  TranscriptProvider, simulated provider, OpenAI realtime transcription provider,
  WAV and microphone input paths. The OpenAI realtime transcription provider
  should use `gpt-realtime-whisper` for live transcript deltas.

crates/qsf_app/src/audio/voice_session_provider.rs
  Provider-owned realtime voice comparison path and useful timing/event lessons.

crates/qsf_app/src/experiments/streaming_transcription_mvp.rs
  Final transcript to InputReceived bridge.

crates/qsf_app/src/models
  ModelRole, ModelRequest, ModelResponse, MockModelClient, OpenAI adapter.

crates/qsf_app/src/memory and crates/qsf_app/src/context
  Retrieval and context assembly already used by earlier experiments.

crates/qsf_app/src/observability
  JSONL event and trace records.
```

Current comparison experiments:

```text
streaming-transcription-mvp
  Audio input -> transcript events -> InputReceived.

realtime-voice-session
  Provider-owned speech-to-speech session, useful as a baseline.
```

## Current OpenAI Audio Guidance

The May 2026 OpenAI voice model announcement maps cleanly onto this plan when the
models are treated as provider choices rather than architecture owners.

Preferred mapping:

```text
gpt-realtime-whisper
  Preferred live microphone transcription model.
  Text-owned loop input only.

gpt-4o-mini-transcribe
  Lower-cost or request-style transcription fallback when live deltas are not
  required.

gpt-4o-mini-tts
  Architecturally clean text-in/audio-out speech output candidate.
  Optional and explicit only; the simulated provider remains the default until
  the speech-output boundary is proven.

gpt-realtime-2
  Provider-owned speech-to-speech comparison model.
  Keep in realtime-voice-session; do not use it to speak OutputProduced text.

gpt-realtime-translate
  Out of scope for the English-only MVP.
```

The important rule is that OpenAI audio models may transcribe input or render
QSF-owned output, but they must not silently become the answer owner in the
text-owned loop.

## Proposed Experiment

Add a new named experiment:

```text
text-owned-voice-loop
```

Description:

```text
Capture or simulate speech, route finalized text through QSF-owned runtime/model
behavior, then synthesize or simulate speech output from the QSF text response.
```

The experiment should produce:

```text
runs/<run-id>/
  engine.log
  events.jsonl
  traces.jsonl
  Report.md
  text-owned-voice-loop.md
```

## Runtime Ownership Model

The core invariant:

```text
Audio providers emit events or provider results.
QSF owns interpretation, state updates, context, model calls, and output text.
```

Each user turn should carry one voice-loop `session_id` across transcript,
runtime/model, and speech-output records. Provider-specific ids may still exist,
but they should be secondary correlation fields.

Required turn flow:

```text
AudioInputStarted
AudioInputChunkCaptured*
AudioPartialTranscript*
AudioFinalTranscript
AudioInputEnded
LatencyMeasurementRecorded
InputReceived
ContextAssemblyRequested
ContextAssembled
ModelRoleRequested
ModelRoleCompleted
OutputProduced
SpeechPlaybackRequested
SpeechPlaybackStarted
SpeechPlaybackCompleted
LatencyMeasurementRecorded
```

The reducer remains pure where reducer-owned paths exist. The first experiment may
follow the current audio experiment pattern of imperative orchestration plus pure
translation helpers, but provider calls, model calls, file I/O, event writing, and
speech synthesis must stay outside state mutation logic and return results as
events.

## First Useful Behavior

The first text-owned voice loop should answer with a small QSF-owned model role call.

Candidate model role:

```text
ConversationalResponder
```

Concrete role definition:

```text
role_id: ModelRoleId::ConversationalResponder
json: conversational_responder
purpose: Produce short spoken replies from QSF-owned context and user input.
allowed_tools: []
context_budget: ContextBudget::new(4, 600)
default_model: gpt-5.4-nano
output_expectation: Text
```

Request shape:

```text
role: ConversationalResponder
input: final transcript text
context: selected QSF context fragments
expectation: short spoken answer
```

Mock behavior should be deterministic and testable:

```text
Input: "Tell me anything about yourself."
Output: "I am a QSF runtime voice loop. I turn your speech into events, assemble context,
and answer through the framework before speech playback."
```

OpenAI-backed behavior should remain opt-in through the existing model-provider
selection pattern.

## Speech Output Provider Boundary

Add a speech output provider abstraction separate from the realtime session provider.

Candidate trait:

```rust
pub trait SpeechOutputProvider {
    fn provider_name(&self) -> &str;

    fn synthesize(
        &self,
        request: &SpeechOutputRequest,
    ) -> Result<SpeechOutputSession, SpeechOutputProviderError>;
}
```

Candidate request:

```text
SpeechOutputRequest
  session_id
  text
  voice
  output_mode
```

Candidate session result:

```text
SpeechOutputSession
  session_id
  provider_name
  voice
  text_length
  started_at_ms
  first_audio_at_ms
  completed_at_ms
  audio_output_bytes
  playback_adapter
```

Default provider:

```text
SimulatedSpeechOutputProvider
  Does not call a network service.
  Wraps the existing Phase 8 SimulatedAudioSession playback fixtures and timing
  helpers rather than inventing a second deterministic playback emitter.
  Emits deterministic timing and byte-count metadata from one source of truth.
  Lets the experiment run in CI and local default builds.
```

Optional provider:

```text
OpenAI-backed speech output provider
  Should use a render-style text-to-speech endpoint such as `gpt-4o-mini-tts`,
  if adopted explicitly.
  Must not use `gpt-realtime-2` as a speech renderer because it is a response
  owner, not a render-only adapter.
  Must not log raw audio, API keys, or authorization headers.
```

Speaker playback should be optional. The first implementation should ship the
simulated provider only; provider-backed synthesis can follow after the
`OutputProduced -> SpeechPlaybackRequested -> SpeechPlaybackCompleted` boundary is
verified. A provider-backed path may synthesize audio metadata or bytes without
playing them locally if that keeps the boundary easier to verify.

## Event Semantics

### Input Events

`AudioPartialTranscript` remains observability-first. It should not update QSF state
unless a later experiment explicitly enables partial-input state.

`AudioFinalTranscript` is the commit point for spoken input.

`InputReceived` is the runtime-owned version of the final transcript. This event is
what QSF reducers and effects consume.

### Output Events

`OutputProduced` is the commit point for QSF-owned answer text.

`SpeechPlaybackRequested` means QSF has handed text to a speech provider.

`SpeechPlaybackStarted` and `SpeechPlaybackCompleted` are provider lifecycle
observations.

The speech provider may report bytes, timing, and voice configuration, but it does not
change the answer text.

## Trace Requirements

Each run should write traces for:

```text
transcript-provider-session
  session_id, provider, input source, model, partial count, final transcript timing

voice-runtime-input-bridge
  session_id, AudioFinalTranscript -> InputReceived

voice-context-assembly
  session_id, selected and omitted context fragments

voice-model-response
  session_id, model role, provider, model, response length, latency, usage when available

speech-output-provider
  session_id, speech provider, model, voice, output mode, audio byte count,
  playback timing

voice-loop-latency
  capture, transcription, runtime dispatch, context/model, speech synthesis/playback,
  total turn timing
```

Suggested latency breakdown:

```text
capture_started_ms
capture_completed_ms
first_partial_transcript_ms
final_transcript_ms
input_received_ms
context_started_ms
context_completed_ms
model_started_ms
model_completed_ms
output_produced_ms
speech_requested_ms
speech_started_ms
speech_completed_ms
total_turn_ms
```

## Safety Boundaries

- `OPENAI_API_KEY` and authorization headers are never logged.
- Raw audio is not persisted by default.
- The no-raw-audio rule applies to captured user input. Synthesized output bytes
  may be persisted only when an explicit output mode requests a file or playback
  artifact.
- Provider errors are sanitized before event/trace/report output.
- Speech output providers receive only QSF-owned text, not hidden state.
- Tool use remains routed through QSF tool permissions.
- Model outputs are QSF outputs only after `OutputProduced`.
- Partial transcripts do not become memory or state by default.

## Configuration

Use explicit environment selectors. Ambient API keys must not change the default path.

Candidate environment variables:

```text
QSF_TRANSCRIPT_PROVIDER=simulated|openai
QSF_TRANSCRIPT_INPUT_SOURCE=simulated|wav|mic
QSF_TRANSCRIPT_WAV_PATH=<path>
QSF_TRANSCRIPT_MIC_DEVICE=default
QSF_TRANSCRIPT_MIC_DURATION_MS=4000
QSF_TRANSCRIPT_MODEL=gpt-realtime-whisper

QSF_MODEL_PROVIDER=mock|openai

QSF_SPEECH_OUTPUT_PROVIDER=simulated|openai
QSF_SPEECH_OUTPUT_MODEL=gpt-4o-mini-tts
QSF_SPEECH_OUTPUT_VOICE=marin
QSF_SPEECH_OUTPUT_MODE=metadata-only|file|playback
```

The first implementation should reuse the existing `QSF_TRANSCRIPT_*` variables
rather than adding a parallel input-source decoder. Voice-loop-specific aliases can
be added later only if the voice loop needs behavior that differs from generic
transcription.

`QSF_TRANSCRIPT_MODEL` and `QSF_SPEECH_OUTPUT_MODEL` are model override candidates.
The first implementation may keep model ids as constants if that keeps the surface
smaller; the plan-level defaults are `gpt-realtime-whisper` for live transcription
and `gpt-4o-mini-tts` for optional OpenAI text-to-speech.

## Implementation Slices

### Slice 1: Deterministic Text-Owned Loop

Goal:

Run a full voice-loop-shaped turn without live audio or network calls.

Tasks:

```text
1. Add text-owned-voice-loop experiment registration in experiments/registry.rs.
2. Reuse SimulatedTranscriptProvider for input.
3. Convert AudioFinalTranscript into InputReceived.
4. Assemble a small deterministic context bundle.
5. Use a mock ConversationalResponder model role.
6. Emit OutputProduced.
7. Add SpeechOutputProvider in audio/speech_output_provider.rs.
8. Implement SimulatedSpeechOutputProvider by wrapping existing SimulatedAudioSession
   playback fixtures.
9. Emit speech playback lifecycle events.
10. Lift the no-raw-audio payload test helper into shared audio test support.
11. Write text-owned-voice-loop.md.
```

Verification:

```powershell
cargo run -p qsf_app -- experiment text-owned-voice-loop
cargo test -p qsf_app text_owned_voice_loop
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Expected result:

The run proves the event shape and QSF ownership boundary without requiring audio
hardware or credentials.

### Slice 2: QSF Model Role Response

Goal:

Make the response path use the existing model-role boundary rather than inline text.

Tasks:

```text
1. Add ModelRole::ConversationalResponder or equivalent stable domain name.
2. Add mock response behavior.
3. Add OpenAI-backed optional behavior through existing model provider selection.
4. Trace model role request/completion/failure.
5. Keep output text committed through OutputProduced.
```

Verification:

```text
Mock run is deterministic.
OpenAI model run is opt-in.
Failure events sanitize provider errors.
```

Expected result:

QSF owns the answer content through its model abstraction.

### Slice 3: Live Microphone Input

Goal:

Use real speech input while QSF still owns the response.

Tasks:

```text
1. Reuse OpenAI realtime transcript provider with `gpt-realtime-whisper`.
2. Preserve the "Speak now" capture cue.
3. Record partial/final transcript events.
4. Bridge only final transcript to InputReceived.
5. Compare latency against streaming-transcription-mvp and realtime-voice-session.
```

Verification:

```powershell
$env:QSF_TRANSCRIPT_PROVIDER="openai"
$env:QSF_TRANSCRIPT_INPUT_SOURCE="mic"
$env:QSF_TRANSCRIPT_MIC_DURATION_MS="4000"
cargo run -p qsf_app --features openai -- experiment text-owned-voice-loop
```

Expected result:

Live speech becomes QSF-owned input and receives a QSF-owned text answer.

### Slice 4: Speech Output Provider

Goal:

Turn QSF-owned text into speech output lifecycle events.

Tasks:

```text
1. Add SpeechOutputProvider abstraction.
2. Keep simulated provider as default.
3. Support metadata-only output first.
4. Add optional local playback only after synthesis metadata is reliable.
5. Defer OpenAI-backed speech synthesis until the simulated boundary and exact-text
   invariant are tested.
```

Verification:

```text
SpeechPlaybackRequested contains QSF text.
SpeechPlaybackStarted and SpeechPlaybackCompleted reflect provider lifecycle.
OutputProduced text is unchanged by the speech provider.
No raw audio is logged.
```

Expected result:

The system can produce verifiable speech output metadata from QSF-owned text without
giving response ownership to the speech provider. Real spoken audio remains an
explicit provider extension, not a prerequisite for completing the loop.

### Slice 5: Comparison Report

Goal:

Make provider-owned and text-owned voice runs comparable.

Tasks:

```text
1. Add common latency names across voice experiments.
2. Record transcript accuracy notes.
3. Record response ownership: provider-owned vs QSF-owned.
4. Include tool-boundary status.
5. Include memory/context participation.
```

Verification:

Run:

```text
streaming-transcription-mvp
realtime-voice-session
text-owned-voice-loop
```

Compare:

```text
input transcript latency
first response latency
response completion latency
answer ownership
memory/context participation
```

Expected result:

The project can evaluate what is lost or gained when QSF owns the answer instead of
the realtime provider.

## Tests

Minimum tests:

```text
- simulated text-owned voice loop writes AudioFinalTranscript, InputReceived,
  OutputProduced, SpeechPlaybackRequested, SpeechPlaybackCompleted
- final transcript is the only transcript event that creates InputReceived
- model role failure records ModelRoleFailed and does not emit OutputProduced
- speech provider failure records a speech failure event or ErrorOccurred
- speech provider receives exactly the OutputProduced text
- SpeechPlaybackRequested.payload["text"] equals OutputProduced.payload["message"]
- one voice-loop session_id correlates transcript, model, and speech-output records
- no raw-audio-like payload keys appear in events
- no credential-like provider error appears unsanitized
- latency trace contains capture, transcription, model, and speech stages
```

## Documentation Updates

When implemented, update:

```text
docs/EngineeringDiary.md
docs/Experiments/Experiment.TextOwnedVoiceLoop.md
docs/Architecture/Architecture.AudioLoop.md
docs/Architecture/Architecture.RuntimeLoop.md
docs/DecisionLog.md only if a durable rule is accepted
```

Likely decision candidate:

```text
Voice interfaces are adapters around QSF-owned text turns unless an experiment is
explicitly provider-owned, as in realtime-voice-session.
```

Additional decision candidates:

```text
Speech output stays simulated until the exact-text speech-provider boundary is proven.
`gpt-realtime-2` is not used for speech rendering because that would conflate speech
rendering with answer ownership.

One voice-loop session_id propagates across transcript, model role, and speech-output
records within a turn.

Slice 1 reuses QSF_TRANSCRIPT_* env vars; voice-loop-specific aliases are added only
when behavior diverges.
```

## Open Questions

### RQ-VoiceLoop-SpeechOutputProvider

What is the first real speech-output provider after the simulated boundary is proven:
`gpt-4o-mini-tts`, a future OpenAI gpt-5-generation TTS provider, a non-OpenAI
provider, or a local model? What quality, latency, and ownership signal justifies
adopting it?

### RQ-VoiceLoop-LatencyDomain

Should runtime-side stages such as context assembly, model invocation, and output
production be reported under a new `runtime` latency domain, or folded into the
existing audio latency enum?

### RQ-VoiceLoop-DispatcherTiming

Should the AudioFinalTranscript -> InputReceived dispatcher land before the
text-owned voice loop, after Slice 1, or only when another experiment forces the
shared dispatcher boundary?

### RQ-VoiceLoop-Playback

When should local speaker playback become part of the experiment rather than a
separate adapter test?

### RQ-VoiceLoop-PartialTranscripts

Should partial transcripts ever influence short acknowledgments, or should QSF remain
final-transcript-only until interruption behavior is designed?

### RQ-VoiceLoop-ResponseStyle

Should the conversational responder be constrained to brief spoken replies by default?

### RQ-VoiceLoop-MemoryParticipation

How much memory/context should be included in a live spoken turn before response
latency becomes unacceptable?

### RQ-VoiceLoop-ProviderComparison

What latency or response-quality threshold would justify provider-owned realtime voice
for some interactions despite weaker QSF ownership?

## Done Criteria

This plan is complete when:

```text
- A default deterministic text-owned voice-loop experiment runs.
- A live microphone input path can feed QSF-owned response generation.
- QSF emits OutputProduced before any speech provider receives text.
- Speech provider output lifecycle is observable.
- Events and traces distinguish QSF-owned answers from provider-owned answers.
- A comparison report distinguishes QSF-owned and provider-owned answer ownership
  across streaming-transcription-mvp, realtime-voice-session, and
  text-owned-voice-loop.
- No raw audio, API key, or authorization header is logged.
- cargo build passes.
- cargo test passes.
- cargo clippy --all-targets -- -D warnings passes.
- cargo fmt has been run.
```
