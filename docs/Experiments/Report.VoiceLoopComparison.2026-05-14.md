# Report: Voice Loop Comparison

## Compared Runs

This report compares three concrete runs from 2026-05-14:

| Path | Role |
|---|---|
| `runs/2026-05-14-133230-streaming-transcription-mvp` | Live microphone transcript-only baseline |
| `runs/2026-05-14-133853-realtime-voice-session` | Provider-owned realtime speech-to-speech baseline |
| `runs/2026-05-14-113743-text-owned-voice-loop` | QSF-owned text turn with live microphone input and simulated speech output |

All three primary comparison runs used the same spoken prompt:

```text
Tell me something funny and unexpected about yourself.
```

An earlier realtime voice-session run remains useful as historical context but is no
longer the primary baseline because it used a different spoken input:

```text
runs/2026-05-14-075918-realtime-voice-session
```

## Summary

The text-owned voice loop now proves the intended architecture: live speech enters as
`AudioFinalTranscript`, becomes `InputReceived`, goes through QSF context assembly and
the `ConversationalResponder` model role, emits `OutputProduced`, and only then reaches
the speech output provider. This gives stronger QSF ownership and inspectability than
the provider-owned realtime voice session, at the cost of not yet producing real
audible output.

The realtime voice session remains the better speech-native baseline because it
produces provider audio bytes and can begin a response before final input transcription
arrives. It is weaker as a QSF architecture test because the provider owns the answer
content.

The streaming transcription MVP remains the cleanest input-only baseline. It validates
live transcript latency and partial/final transcript events without model response or
speech output.

## Comparison Table

| Dimension | Streaming Transcription | Realtime Voice Session | Text-Owned Voice Loop |
|---|---:|---:|---:|
| Run | `2026-05-14-133230-streaming-transcription-mvp` | `2026-05-14-133853-realtime-voice-session` | `2026-05-14-113743-text-owned-voice-loop` |
| Input provider | `openai-realtime-transcript-provider` | `openai-realtime-session-provider` | `openai-realtime-transcript-provider` |
| Response owner | None | Realtime provider | QSF model role |
| Model role | None | Provider-owned realtime response | `conversational_responder` |
| Model provider | None | `gpt-realtime-2` | OpenAI `gpt-5.4-nano-2026-03-17` |
| Speech output | None | Provider audio bytes observed | Simulated metadata-only |
| Final transcript | `Tell me something funny and unexpected about yourself.` | `Tell me something funny and unexpected about yourself.` | `Tell me something funny and unexpected about yourself.` |
| Partial transcript revisions | 9 | Not recorded as partial transcript events | 9 |
| First partial latency | 3334 ms | N/A | 3610 ms |
| Final transcript latency | 4688 ms | 10283 ms final transcript timestamp | 4851 ms |
| Model/response latency | N/A | 2518 ms response latency | 1937 ms model latency |
| First audio latency | N/A | 726 ms | Simulated: 14 ms after speech request |
| Total run/turn latency | 4688 ms transcript latency | 12555 ms provider turn latency | 4997 ms text-owned loop latency |
| Context assembly visible | No | No QSF context assembly | Yes, 3 fragments selected |
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

The text-owned loop owns the response through QSF:

```text
AudioFinalTranscript
  -> InputReceived
  -> ContextAssemblyRequested
  -> ContextAssembled
  -> ModelRoleRequested(conversational_responder)
  -> ModelRoleCompleted(provider = openai)
  -> OutputProduced
  -> SpeechPlaybackRequested
```

The speech provider receives exactly the `OutputProduced` text.

## Latency Interpretation

The text-owned voice loop and streaming transcription runs are comparable for transcript
input because they used the same phrase and both used `gpt-realtime-whisper`:

| Metric | Streaming | Text-Owned |
|---|---:|---:|
| First partial transcript | 3334 ms | 3610 ms |
| Final transcript | 4688 ms | 4851 ms |

The text-owned loop adds the QSF response path:

| Stage | Text-Owned |
|---|---:|
| Context assembly trace | 6 ms |
| OpenAI model role | 1937 ms |
| Simulated speech output | 140 ms |
| Total loop latency | 4997 ms |

The current total loop latency is not a full audible-response latency because speech
output is still metadata-only simulation. It is useful for comparing transcript,
context, and model timing, not final speaker playback.

The same-prompt realtime voice session took longer end to end than the text-owned loop,
but it produced real provider audio bytes and began response generation before final
input transcription completed. Its response start offset from final transcript was
`-246 ms`. That response overlap is a speech-native advantage the current text-owned
loop does not try to match yet.

## Context And Memory Participation

Streaming transcription does not assemble context.

Realtime voice session records provider response lifecycle and maps provider tool-call
requests to QSF events, but QSF context assembly is not in the answer path.

Text-owned voice loop includes QSF context assembly in the answer path. In the compared
run it selected three deterministic context fragments:

- the final-transcript commit boundary
- the output-before-speech boundary
- the current finalized spoken input

This proves the architecture boundary, but it does not yet prove memory retrieval in a
live spoken response. Memory/context participation should be deepened after the
comparison baseline is stable.

Follow-up implementation note: the text-owned loop now retrieves one
association-weighted Phase 4 memory candidate after `InputReceived` and before context
assembly. The next live comparison run should validate the new `MemoryRetrievalRequested`
and `MemoryRetrieved` events, the selected memory id in `text-owned-voice-loop.md`, and
whether the responder uses that memory naturally in the answer.

Latency note: the live memory-context run exposed that the generated
`text-owned-voice-loop.md` total undercounted model-role runtime. The experiment has
been updated so future reports list memory retrieval, context assembly, model role,
speech output, and total observed turn latency separately.

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
  ownership, context, model role, and exact speech handoff visible in events.

The strongest architectural result is the text-owned voice loop. It proves that voice
can be an interface around QSF rather than a replacement for QSF.

## Recommended Next Step

Do not add OpenAI TTS yet.

First, run the updated text-owned loop with a memory-oriented live prompt and compare
latency plus answer quality against the prior text-owned run. The run should prove that
memory retrieval is observable and that selected memory context reaches
`ConversationalResponder`. Use the corrected generated latency fields for future
comparisons rather than the older `Total deterministic turn latency` line.

After that, add a short comparison section to the generated
`text-owned-voice-loop.md` report so future runs show response ownership and
model/speech timing automatically. Only after the text/context path remains stable
should the project add a render-only speech output provider such as OpenAI TTS.
