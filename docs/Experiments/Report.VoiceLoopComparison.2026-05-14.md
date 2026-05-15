# Report: Voice Loop Comparison

## Compared Runs

This report compares the current voice-loop evidence from 2026-05-14:

| Path | Role |
|---|---|
| `runs/2026-05-14-133230-streaming-transcription-mvp` | Live microphone transcript-only baseline |
| `runs/2026-05-14-133853-realtime-voice-session` | Provider-owned realtime speech-to-speech baseline |
| `runs/2026-05-14-140617-text-owned-voice-loop` | Corrected QSF-owned text turn with memory context and simulated speech output |

The streaming transcription and realtime voice-session baselines used this prompt:

```text
Tell me something funny and unexpected about yourself.
```

The corrected text-owned run used a memory-oriented prompt to validate retrieval in the
answer path:

```text
What should you remember about context budget and memory retrieval?
```

Historical context:

| Path | Role |
|---|---|
| `runs/2026-05-14-113743-text-owned-voice-loop` | Earlier same-prompt text-owned run before memory retrieval and corrected total latency |
| `runs/2026-05-14-075918-realtime-voice-session` | Earlier realtime voice-session run with a different spoken input |

## Summary

The corrected text-owned voice loop is now the strongest architecture baseline. Live
speech enters as `AudioFinalTranscript`, becomes `InputReceived`, triggers memory
retrieval, assembles QSF context, invokes `ConversationalResponder`, emits
`OutputProduced`, and only then reaches the speech output provider.

The run proves memory participation in a live spoken answer: association-weighted
retrieval selected `memory.context-budget`, context assembly carried that fragment into
the model request, and the answer reflected the selected memory and fixed voice-loop
boundary context.

The realtime voice session remains the speech-native baseline because it produces
provider audio bytes and can overlap response generation with final input
transcription. It is weaker as a QSF architecture test because the provider owns the
answer content.

The streaming transcription MVP remains the cleanest input-only baseline. It validates
live transcript latency and partial/final transcript events without model response,
memory, context, or speech output.

## Comparison Table

| Dimension | Streaming Transcription | Realtime Voice Session | Text-Owned Voice Loop |
|---|---:|---:|---:|
| Run | `2026-05-14-133230-streaming-transcription-mvp` | `2026-05-14-133853-realtime-voice-session` | `2026-05-14-140617-text-owned-voice-loop` |
| Prompt class | Same-prompt baseline | Same-prompt baseline | Memory-context validation |
| Input provider | `openai-realtime-transcript-provider` | `openai-realtime-session-provider` | `openai-realtime-transcript-provider` |
| Response owner | None | Realtime provider | QSF model role |
| Model role | None | Provider-owned realtime response | `conversational_responder` |
| Model provider | None | `gpt-realtime-2` | OpenAI `gpt-5.4-nano-2026-03-17` |
| Speech output | None | Provider audio bytes observed | Simulated metadata-only |
| Final transcript | `Tell me something funny and unexpected about yourself.` | `Tell me something funny and unexpected about yourself.` | `Should you remember about context budget and memory retrieval.` |
| Partial transcript revisions | 9 | Not recorded as partial transcript events | 10 |
| First partial latency | 3334 ms | N/A | 4253 ms |
| Final transcript latency | 4688 ms | 10283 ms final transcript timestamp | 5321 ms |
| Memory retrieval latency | N/A | N/A | 1 ms |
| Context assembly latency | N/A | No QSF context assembly | 6 ms |
| Model/response latency | N/A | 2518 ms response latency | 2304 ms model-role latency |
| First audio latency | N/A | 726 ms | Simulated: 14 ms after speech request |
| Total run/turn latency | 4688 ms transcript latency | 12555 ms provider turn latency | 7772 ms observed text-owned turn latency |
| Context assembly visible | No | No QSF context assembly | Yes, 4 fragments selected |
| Memory retrieval visible | No | No QSF memory retrieval | Yes, `memory.context-budget` selected |
| Tool boundary visible | No tool path | Tool auto-execution disabled, 0 calls | Model role allowed tools empty |
| Raw audio logged | false | false | false |

## Ownership

### Streaming Transcription

Streaming transcription has no answer owner because it does not answer. Its useful
boundary is:

```text
AudioFinalTranscript -> InputReceived
```

It is the best baseline for input timing and final-transcript commit semantics.

### Realtime Voice Session

The realtime voice-session provider owns the spoken response. QSF records provider
facts as events:

```text
RealtimeResponseStarted
RealtimeResponseCompleted
OutputProduced(source = realtime_provider_response)
SpeechPlaybackCompleted
```

This is useful for measuring provider-native speech behavior, but the response text is
not produced by QSF context assembly or a QSF model role.

### Text-Owned Voice Loop

The corrected text-owned loop owns the response through QSF:

```text
AudioFinalTranscript
  -> InputReceived
  -> MemoryRetrievalRequested
  -> MemoryRetrieved
  -> ContextAssemblyRequested
  -> ContextAssembled
  -> ModelRoleRequested(conversational_responder)
  -> ModelRoleCompleted(provider = openai)
  -> OutputProduced
  -> SpeechPlaybackRequested
```

The speech provider receives exactly the `OutputProduced` text.

## Latency Interpretation

The original same-prompt text-owned run remains useful for comparing transcript timing
against the streaming transcription baseline:

| Metric | Streaming | Earlier Same-Prompt Text-Owned |
|---|---:|---:|
| First partial transcript | 3334 ms | 3610 ms |
| Final transcript | 4688 ms | 4851 ms |

The corrected text-owned run should be used for current text-owned latency conclusions
because it includes memory retrieval and model-role runtime in the total:

| Stage | Corrected Text-Owned |
|---|---:|
| Final transcript | 5321 ms |
| Memory retrieval | 1 ms |
| Context assembly | 6 ms |
| OpenAI model role | 2304 ms |
| Simulated speech output | 140 ms |
| Total observed turn latency | 7772 ms |

The current total is still not a full audible-response latency because speech output is
metadata-only simulation. It is useful for comparing transcript, memory, context, and
model timing, not final speaker playback.

The same-prompt realtime voice session took 12555 ms end to end, but it produced real
provider audio bytes and began response generation before final input transcription
completed. Its response start offset from final transcript was `-246 ms`. That overlap
is a speech-native advantage the text-owned loop does not try to match yet.

## Context And Memory Participation

Streaming transcription does not assemble context.

Realtime voice session records provider response lifecycle and maps provider tool-call
requests to QSF events, but QSF context assembly is not in the answer path.

The corrected text-owned voice loop selected four context fragments:

- `memory.context-budget`
- `voice-loop-runtime-boundary`
- `voice-loop-output-boundary`
- `voice-loop-user-turn`

The answer used that context naturally:

```text
Yes--remember to respect the context budget and use compact memory retrieval. Also,
only finalized speech counts: AudioFinalTranscript is the commit point, and
InputReceived happens then. Finally, ensure OutputProduced exists before any speech
output providers receive the text.
```

This proves memory retrieval is now part of the live spoken response path, not only a
standalone memory experiment.

## Tool Boundary

The realtime voice-session run requested zero provider tool calls, but the experiment
records that tool auto-execution is disabled.

The text-owned voice loop uses `ConversationalResponder` with `allowed_tools: []`.
That is appropriate for the current slice: spoken response generation stays inside the
model role boundary and cannot silently call tools.

## Safety Boundary

All three compared runs recorded `raw_audio_logged: false`.

The text-owned and streaming runs use `AudioSafetyMarkers` on transcript events. The
text-owned loop also records safety markers on speech playback request/start/completion
events.

No API key or authorization value is present in the inspected event or trace summaries.

## What This Shows

The architecture now supports three distinct voice research modes:

- **Input-only:** live speech can become runtime input without response generation.
- **Provider-owned voice:** a realtime speech provider can own the response for
  speech-native comparison.
- **QSF-owned voice:** live speech can become a QSF-owned text turn, with response
  ownership, memory retrieval, context, model role, and exact speech handoff visible in
  events.

The strongest architectural result is the corrected text-owned voice loop. It proves
that voice can be an interface around QSF rather than a replacement for QSF.

## Recommended Next Step

Do not add OpenAI TTS yet.

The generated `text-owned-voice-loop.md` report now includes a diagnostics section for
response ownership, selected memory context, memory source, model provider and latency,
exact speech handoff status, raw-audio logging status, and corrected total observed
turn latency.

The voice loop now has an opt-in file-backed memory source through
`QSF_VOICE_MEMORY_SOURCE=file` and `QSF_VOICE_MEMORY_FILE=<path>`. Next, use that
boundary with approved sleep-phase memory candidates or a small session memory store,
then compare retrieval quality against the Phase 4 fixture.
