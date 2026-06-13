# Concept: Realtime Audio

## Summary

Realtime audio is the use of live speech input, live spoken output, turn-taking,
interruption, and timing as part of Qualia Signal Foundry's simulated presence.

The project should not treat a realtime voice model as the whole simulated mind.
The useful shape is a three-plane split:

```text
Media plane          = realtime voice transport and audible speech
Control/context plane = QSF memory, instructions, and working context injection
Tool plane           = QSF-permitted perception tools and returned results
```

In that split, the realtime model is the live conversational surface. QSF owns
continuity, memory, associations, tools, state, and observability.

## Status

Exploratory concept with one accepted implementation direction:

- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`
- `docs/Architecture/Architecture.RealtimeSessionServer.md`

The browser speech-to-speech defaults for the first realtime slice were accepted
on 2026-06-09. This concept remains broader than that plan and should not be read
as an implementation commitment unless the decision log says so.

Realtime audio is also the path toward QSF's eventual primary operating mode. The
named experiments below are validation steps for that mode, not an indication that
live voice should remain only an experiment-runner feature.

## Why It Matters

Audio changes the interaction from isolated prompt/response turns into something
closer to a live encounter. Voice, interruption, pauses, response timing, and
recovery from overlap can all affect whether the system feels present.

Realtime audio also produces research signals that text alone does not:

- turn boundaries
- hesitation and silence
- interruption timing
- transcript instability
- speech-response latency
- perceived continuity across sessions

The important project question is not just how to play audio. It is how live
speech should connect to memory, attention, state, and observable continuity.

The live-memory extraction and presence diagnostics work moves one of those
questions into implementation by extracting trusted realtime continuity roots in
`qsf_app` and recording live-loop latency and interruption diagnostics in the
realtime server.

## Three Planes

### Media Plane

The media plane handles microphone capture, speaker playback, WebRTC/WebSocket
transport, provider VAD, and barge-in behavior.

For the accepted browser MVP, the browser owns WebRTC media. Audio flows between
the browser and the realtime provider. The QSF server owns session setup and
observability, but it does not proxy raw media.

Early media-plane questions:

- Is provider `server_vad` good enough for natural turn-taking?
- How quickly does barge-in stop or revise an active spoken response?
- What audio state should the UI expose without becoming a product surface?

### Control and Context Plane

The control/context plane is where QSF makes the voice session part of the larger
system. It can inject:

- compact working-memory packets
- identity or tone guidance
- relevant retrieved memories
- tool results
- current session state
- unresolved questions or active focus

The guiding rule is relevance over volume. A realtime session should receive a
small packet that helps the current turn, not a broad memory dump.

### Tool Plane

Realtime voice tool use should remain a QSF-controlled perception channel.

The model may request allow-listed read-only tools. QSF decides permission, runs
the tool server-side, records the permission and result, and returns the result
to the realtime session. A provider tool-call request is not execution evidence
by itself.

Write-capable and outbound-action tools remain outside the realtime voice scope
until explicitly revisited.

## Memory Formation

Realtime audio can feed memory, but it should not automatically become durable
truth.

A useful loop is:

```text
spoken interaction
  -> provider event stream
  -> normalized transcript and lifecycle records
  -> trusted QSF exchange
  -> live retrieval / context injection
  -> sleep or review consolidation
  -> durable memory and associations
```

The trust boundary matters. Browser-relayed events can help diagnose the media
loop, but durable memory should begin only from an authoritative server-side
source.

## Transcript Caveat

In speech-to-speech mode, the model hears audio natively. The user transcript is
an observability artifact and memory candidate source, not guaranteed proof of
what the model internally understood.

The system should preserve enough event and source metadata to inspect cases
where:

- the transcript is wrong,
- the model replies as though it heard something different,
- memory extraction would preserve a questionable phrase,
- interruption truncates what the user actually heard.

## Candidate Experiments

These experiments are how the project builds evidence for the primary realtime
conversation mode.

### Streaming Transcription Foundation

Represent live speech as partial and final transcript events. Measure latency and
partial-transcript instability.

Status: implemented as the transcript-first foundation.

### Text-Owned Voice Loop

Let QSF own interpretation, context, memory retrieval, and response text, then
send the completed response to a speech-output adapter.

Status: implemented as the deterministic QSF-owned voice path.

### Realtime Browser Voice MVP

Use browser WebRTC for speech-to-speech conversation, with provider VAD and
barge-in. Record browser-relayed events as diagnostic-only QSF exchanges.

Questions it should answer:

- Can the user speak and hear a reply in one browser session?
- Does interruption feel usable?
- Does the event mapping survive overlap and out-of-order events?
- Does the API key stay server-side?
- Does the `call_id` binding support later sideband attachment?

### Live Context Injection

Attach a server-side sideband to the active realtime call. Inject a small
working-memory packet per session start and user turn.

Questions it should answer:

- What amount of memory improves continuity?
- How much injection increases latency?
- Can the spoken model use memory without derailing the current turn?

### Live Tool Perception

Allow the realtime model to request read-only QSF tools, execute them server-side,
record permission/result, and return the result to the session.

Questions it should answer:

- Does the model call the right tool?
- Does it use the result in speech?
- Are denial and failure paths observable?

## Open Questions

- How should QSF represent overlapping user and assistant speech in reducer state?
- How should interrupted assistant speech affect future context?
- When should partial transcript state influence attention but not memory?
- What latency thresholds matter for spoken presence?
- How much working memory should be injected into the realtime session?
- How should ASR-vs-model-understanding divergence affect memory extraction?
- Should voice choice become part of identity, or remain a provider quality
  default?
- Should audio eventually be a core loop capability or an optional interface?

## Risks

### Audio Becomes the Product

Realtime audio can pull the project toward a polished voice assistant. The
mitigation is to keep the memory, state, tools, and observability planes central.

### Provider Lock-In

Realtime audio APIs shape architecture strongly. Provider-specific code should
stay at the media/sideband boundary, while QSF session state and memory stay
provider-agnostic.

### Untrusted Facts Enter Memory

Browser-relayed provider facts are useful diagnostics but should not become
sleep-eligible memory. Trusted live exchanges require the authoritative server
sideband.

### Over-Injection

Injecting too much remembered context can slow the loop and distort the current
conversation. Small, explainable packets are safer and easier to evaluate.

### Transcript Divergence

Speech transcripts can be wrong or incomplete. Memory extraction and continuity
must preserve enough evidence to audit what was actually observed.

## Related Documents

- `docs/Concepts/Concept.RealtimePresence.md`
- `docs/Research/ResearchQuestions.Audio.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.RealtimeSessionServer.md`
- `docs/Architecture/Architecture.ToolSystem.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`
