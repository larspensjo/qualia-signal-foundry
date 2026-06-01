# Experiment: Realtime Voice Session MVP

## Status

Implemented as an MVP experiment path.

## Purpose

Evaluate a full realtime voice-session provider after transcript-first observability
exists, without letting the provider own QSF runtime state, memory updates, or tool
execution.

## Implementation Shape

- Experiment id: `realtime-voice-session`
- Default provider: deterministic simulated realtime session provider.
- Optional provider: OpenAI realtime WebSocket session provider targeting
  `gpt-realtime-2` behind the `qsf_app/openai` feature.
- Provider selection: `QSF_REALTIME_SESSION_PROVIDER=simulated|openai`
- Input source selection:
  - `QSF_REALTIME_SESSION_INPUT_SOURCE=simulated`
  - `QSF_REALTIME_SESSION_INPUT_SOURCE=wav` with `QSF_REALTIME_SESSION_WAV_PATH`
  - `QSF_REALTIME_SESSION_INPUT_SOURCE=mic` with optional
    `QSF_REALTIME_SESSION_MIC_DEVICE` and `QSF_REALTIME_SESSION_MIC_DURATION_MS`

The simulated provider always emits deterministic synthetic chunks. WAV and microphone
source selections only affect real provider runs; if they are paired with the simulated
provider, the run stays simulated and records the requested source label for
configuration visibility.

The experiment now also boots through the shared session runtime. Provider facts are
adapted into live-session reducer events, then persisted in `SessionState.exchanges`
as a voice `Exchange`. The persisted exchange records the final transcript,
provider preamble, response lifecycle events, typed interruption records, and
provider tool-call requests. Exchange-level timestamps use wall-clock time to match
the other voice paths; provider sub-events keep provider-relative timing facts.
Provider-owned response text remains adapter output; this experiment does not route
it into QSF prompt assembly as QSF-owned cognition.

## Observability

The experiment records:

- realtime session start/completion/failure
- audio input lifecycle
- finalized transcript and `InputReceived` bridge
- response preamble, response start, response completion
- speech playback request/start/completion metadata
- interruption events
- provider tool-call requests routed to the QSF tool permission boundary
- latency measurements for capture, transcript, response start, first audio,
  response completion, and interruption timing
- a shared persisted voice exchange containing preamble/lifecycle, interruption,
  and inert tool-request records

Raw audio, API keys, and authorization headers are not written to events, traces, or
reports.

Provider tool calls are recorded with `auto_executed=false`. They do not append
`Turn` records, mutate memory, or invoke tool side effects unless a later QSF-owned
tool route explicitly handles them.

The shared boot path may load continuity metadata and a pending consolidated brief,
but realtime provider-owned cognition deliberately does not inject that brief into a
provider prompt. Phase 7 remains responsible for sleep-side consumption of realtime
voice exchanges.

## Verification

Default deterministic run:

```powershell
cargo run -p qsf_app -- experiment realtime-voice-session
```

Feature-gated OpenAI compile check:

```powershell
cargo test -p qsf_app --features openai audio::voice_session_provider::tests::openai_realtime_session_provider_validates_local_inputs_before_network_call
```

Live provider evaluation requires `OPENAI_API_KEY`, the `openai` feature, and an
explicit provider selector.
