# Research Questions: Audio

## Purpose

This document captures open research questions related to audio interaction in Qualia Signal Foundry.

Audio is not only an input/output mechanism. In this project, audio is treated as a major part of simulated presence. Voice, timing, interruption, latency, hesitation, and turn-taking may all influence whether the system feels like a continuous communicating entity rather than a text interface.

This document should remain exploratory. Questions listed here should not be treated as requirements until they have been investigated, tested, and promoted through architecture or decision documents.

## Current Status

Status: Exploratory

The project has implemented transcript-first audio paths and browser-based
realtime speech-to-speech infrastructure. The browser realtime path is still
experimental and needs continued human verification, but it is no longer only a
plan/design artifact.

Long term, realtime voice conversation is the intended primary operating mode of
QSF. The research questions here should therefore evaluate and shape that mode,
not treat it as just another isolated experiment.

The live-memory extraction and presence diagnostics work adds direct evidence for
the presence questions by extracting memory from trusted continuity roots and
logging live-loop latency and interruption diagnostics for later review.

Relevant related documents:

- `docs/Concepts/Concept.RealtimePresence.md`
- `docs/Concepts/Concept.ExternalInputs.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Architecture/Architecture.RealtimeSessionServer.md`
- `docs/Experiments/Experiment.StreamingTranscriptionMVP.md`
- `docs/Experiments/Experiment.AudioLoopMVP.md`
- `docs/Plans/Design.RealtimeVoiceConversation.md`
- `docs/Plans/Plan.RealtimeVoiceConversation.md`

Current implementation direction:

- Keep the existing streaming transcription and text-owned voice paths as the
  deterministic, inspectable foundation.
- Use `gpt-realtime-whisper` for transcript-only realtime speech-to-text work.
- Build browser speech-to-speech slices with `gpt-realtime-2.1`, voice
  `marin`, medium reasoning effort, audio output, and provider `server_vad` with
  automatic response creation and interruption enabled.
- Treat browser-relayed provider events as diagnostic-only; use the server-side
  sideband as the authoritative source for trusted continuity.
- Keep `gpt-realtime-translate` as a separate translation experiment.

## Research Theme: Audio and Presence

### RQ-Audio-PresenceContribution

How much does audio interaction contribute to the impression of simulated presence compared with text-only interaction?

#### Why It Matters

The project aims to explore communication with a simulated mind-like system. Audio may create a stronger sense of presence because it introduces real-time timing, voice, rhythm, interruption, and conversational flow.

#### Current Thinking

Audio is likely to matter significantly, but it should be tested rather than assumed. Some aspects of presence may come from latency and continuity rather than from voice quality alone.

#### Possible Experiments

- Compare text-only interaction with voice interaction using the same underlying model.
- Compare batch voice interaction with streaming voice interaction.
- Evaluate whether users perceive the system as more continuous when it can respond in real time.

#### Status

Open

## Research Theme: Latency

### RQ-Audio-LatencyThreshold

What end-to-end latency is low enough for the system to feel present and responsive?

#### Why It Matters

Latency can strongly affect whether an audio system feels alive. Even high-quality responses may feel mechanical if the delay is too long or poorly timed.

#### Current Thinking

The target latency may depend on the interaction mode. A reflective response can tolerate more delay than a quick backchannel, interruption, or acknowledgment.

#### Possible Experiments

- Measure microphone-to-response latency in the minimal audio loop.
- Compare perceived presence at different artificial latency levels.
- Separate latency targets for short acknowledgments, normal replies, and deep-thinking replies.

#### Status

Open

### RQ-Audio-LatencyBudget

How should the latency budget be divided between audio capture, speech detection, transcription, model inference, text-to-speech, and playback?

#### Why It Matters

A real-time system may fail because many small delays accumulate. Understanding the budget makes it easier to improve the correct part of the pipeline.

#### Possible Areas to Measure

- microphone capture delay
- voice activity detection delay
- speech-to-text delay
- model response delay
- text-to-speech delay
- audio playback delay
- orchestration overhead
- WebRTC setup and SDP rendezvous overhead
- server-side sideband attach and context-injection overhead

#### Status

Open

## Research Theme: Turn-Taking and Interruption

### RQ-Audio-TurnDetection

How should the system decide when the user has finished speaking?

#### Why It Matters

Turn detection affects flow. If the system responds too early, it interrupts the user. If it waits too long, it feels slow.

#### Current Thinking

The accepted browser realtime MVP starts with provider `server_vad` because it is
simple, directly exercises automatic response creation, and supports barge-in.
`semantic_vad` remains a later comparison point if the first human tests show
awkward turn endings or if lower eagerness is worth the extra latency.

#### Possible Experiments

- Compare provider `server_vad` with `semantic_vad` after the baseline browser MVP
  is measurable.
- Test different silence/eagerness settings.
- Track false starts, premature responses, and awkward pauses.

#### Status

Open

### RQ-Audio-InterruptionHandling

How should the system react when the user interrupts while it is speaking?

#### Why It Matters

Interruption handling is central to real-time presence. A system that continues speaking over the user may feel non-present or scripted.

#### Current Thinking

The first browser realtime slice should exercise provider barge-in through
automatic interruption. QSF should still record interruption facts explicitly and
avoid over-interpreting them until the behavior has been observed in human tests.

#### Possible Design Directions

- Stop speaking immediately when user speech is detected.
- Fade out current speech and switch to listening mode.
- Let short user sounds pass without interrupting.
- Keep a transcript marker showing that the system was interrupted.

#### Status

Open

### RQ-Audio-Backchannels

Should the system use short audio responses such as acknowledgments, hesitation sounds, or brief confirmations while thinking?

#### Why It Matters

Human conversation often uses small timing signals to show attention. Simulated equivalents may increase presence, but they may also feel artificial or annoying.

#### Possible Experiments

- Compare silent thinking with short spoken acknowledgments.
- Test text-to-speech backchannels versus non-verbal audio cues.
- Evaluate whether backchannels improve perceived responsiveness.

#### Status

Open

## Research Theme: Streaming vs Batch Processing

### RQ-Audio-StreamingVsBatch

Should audio be processed continuously, in chunks, or only after a detected turn boundary?

#### Why It Matters

Streaming audio may support better real-time behavior, but it can also increase complexity and cost. Batch processing is simpler but may feel less alive.

#### Candidate Modes

- Push-to-talk batch transcription
- Voice activity based chunks
- Continuous streaming transcription through transcript events
- Realtime model session with audio input/output through the browser media plane
- Hybrid mode with cheap detection and selective deeper processing

#### Status

Open

### RQ-Audio-IncrementalUnderstanding

Does the system need to understand speech incrementally before the user finishes speaking?

#### Why It Matters

Incremental understanding may allow earlier reactions, interruption handling, and better timing. However, it may also create unstable partial interpretations.

#### Possible Experiments

- Feed partial transcripts into the interaction loop.
- Compare final-only transcription with incremental transcription.
- Measure whether partial understanding improves naturalness.

#### Status

Open

### RQ-Audio-TranscriptDivergence

How should QSF handle divergence between the user ASR transcript and what the
realtime model appears to have understood from native audio?

#### Why It Matters

In a speech-to-speech session, the model hears audio directly. The transcript is
an observability artifact and memory candidate source, but it may not perfectly
match the model's internal audio interpretation. Treating the transcript as exact
truth could create bad memories or confusing continuity.

#### Current Thinking

Store the raw provider event stream and a normalized transcript, but treat the
transcript as approximate. Memory extraction should prefer completed trusted turns
and should preserve enough source metadata to audit recognition errors.

#### Possible Experiments

- Compare user-perceived intent with stored input transcripts after live sessions.
- Track cases where the spoken response implies the model understood something
  different from the transcript.
- Evaluate whether memory extraction should exclude low-confidence or disputed
  transcript segments.

#### Status

Open

## Research Theme: Audio Memory

### RQ-Audio-ContextInjectionRelevance

What is the smallest working-memory packet that improves spoken continuity without
overloading the realtime session?

#### Why It Matters

The realtime model should not receive a full memory dump. QSF needs to inject just
enough identity, recent context, and retrieved memory to make the conversation feel
continuous while preserving latency and avoiding irrelevant influence.

#### Current Thinking

Use existing association-weighted retrieval, but send a small packet per session
start and per user turn through the server-side sideband. Relevance matters more
than volume.

#### Possible Experiments

- Compare no injection, one selected memory, and a small ranked packet across the
  same spoken continuity prompt.
- Measure whether injected memory appears in the spoken reply without derailing the
  current turn.
- Inspect traces for selected and omitted memories to see whether the packet was
  explainable.

#### Status

Open

### RQ-Audio-TranscriptMemory

Should audio interactions be stored as exact transcripts, summaries, memory events, or all three?

#### Why It Matters

The project is concerned with continuity and memory. Audio conversations may generate large amounts of data, so the system needs a strategy for deciding what is remembered.

#### Possible Design Directions

- Store raw audio only temporarily.
- Store full transcripts for debugging.
- Store compressed summaries for long-term memory.
- Extract memory events during or after the session.
- Use sleep-phase consolidation to convert conversation into associations.

#### Status

Open

### RQ-Audio-ProsodyMemory

Should the system preserve information about tone, pace, pauses, emotion, or emphasis?

#### Why It Matters

A transcript loses much of the information present in speech. Prosody might be useful for memory salience, emotional interpretation, or continuity.

#### Current Thinking

This is probably not needed for the first MVP, but it may become important later.

#### Status

Open

## Research Theme: Voice Output

### RQ-Audio-VoiceIdentity

Should the system have a stable voice identity?

#### Why It Matters

A consistent voice may strengthen continuity. However, voice identity may also create expectations about personality, embodiment, and agency.

#### Possible Questions

- Should the voice remain stable across sessions?
- Is the accepted initial `marin` default perceived as a stable identity or merely
  as a provider quality default?
- Should the voice change with system state?
- Should different internal model roles have different voices?
- Should voice customization be part of the experiment or avoided initially?

#### Status

Open

### RQ-Audio-SpeakingStyle

What speaking style best supports the project goal: neutral assistant speech, reflective speech, conversational speech, or something else?

#### Why It Matters

Speaking style can influence whether the system feels like a tool, assistant, companion, narrator, or simulated mind.

#### Status

Open

## Research Theme: External Audio Environment

### RQ-Audio-EnvironmentListening

Should the system listen only to direct speech, or should it also interpret background sounds?

#### Why It Matters

Background audio could make the system more situated, but it also raises complexity, privacy, and relevance concerns.

#### Current Thinking

Early prototypes should probably focus on direct speech only. Environmental listening can be treated as a later external input concept.

#### Status

Open

### RQ-Audio-AlwaysListeningBoundary

Should the system support always-listening mode, or should audio capture require explicit activation?

#### Why It Matters

Always-listening may increase presence but also introduces privacy, safety, trust, and implementation concerns.

#### Possible Design Directions

- Push-to-talk only
- Wake-word mode
- Session-limited always-listening
- Explicit visible recording state
- Debug-only continuous capture

#### Status

Open

## Research Theme: Evaluation

### RQ-Audio-PresenceMetrics

How can the project evaluate whether audio improves simulated presence?

#### Why It Matters

Presence is subjective, but the project still needs ways to compare experiments.

#### Possible Measures

- perceived responsiveness
- interruption quality
- conversational flow
- user willingness to continue speaking
- perceived continuity across sessions
- awkward pauses per minute
- end-to-end latency
- number of failed turn detections
- number of unnecessary interruptions

#### Status

Open

### RQ-Audio-DebugObservability

What should be logged or visualized to make the audio loop researchable?

#### Why It Matters

If audio behavior cannot be inspected, it will be difficult to improve. The system should expose enough internal timing and state to support research.

#### Possible Observability Data

- captured audio segments
- detected speech regions
- transcript timestamps
- partial transcript revisions
- model request timing
- response generation timing
- TTS timing
- playback timing
- interruption events
- ephemeral-token and `call_id` binding lifecycle events
- context-injection payload size and selected-memory IDs
- trusted vs diagnostic event source markers
- memory events created from audio

#### Status

Open

## Early Candidate Questions for the Realtime Browser Voice MVP

The first transcript-only MVP has already established the speech-to-text boundary.
The next live audio validation slice should focus on the browser speech-to-speech
loop:

1. Can a user speak in the browser and hear a realtime spoken reply?
2. Does provider `server_vad` create acceptable turn boundaries for normal speech?
3. Does barge-in stop or revise the active response quickly enough to feel present?
4. Do diagnostic browser-relayed events map cleanly into QSF exchanges without
   entering sleep or continuity?
5. Does the SDP `Location` header provide a reliable `call_id` for server-side
   sideband attachment?
6. How often do stored transcripts diverge from what the model appeared to
   understand?
7. What is the smallest memory-injection packet that improves spoken continuity?

## Not Yet Decided

The following should remain undecided for now:

- local versus cloud transcription after the first OpenAI-backed streaming test
- local versus cloud text-to-speech
- always-listening versus push-to-talk
- voice identity beyond the initial `marin` quality default
- whether to store raw audio
- whether `semantic_vad` is worth added latency after baseline human tests
- whether audio should be part of the core loop or an optional interface module

## Possible Follow-Up Documents

This research question document may lead to:

- `docs/Experiments/Experiment.StreamingTranscriptionMVP.md`
- `docs/Experiments/Experiment.AudioLoopMVP.md`
- `docs/Experiments/Experiment.RealtimeBrowserVoiceMVP.md`
- `docs/Experiments/Experiment.LiveContextInjection.md`
- `docs/Architecture/Architecture.AudioDeviceAbstraction.md`
- `docs/Architecture/Architecture.RealtimeSessionServer.md`
- `docs/DecisionLog.md` entries for accepted audio input and provider-boundary decisions

## Summary

Audio is a central research area because it may strongly affect simulated presence. However, the project should avoid treating audio as merely a user interface feature.

The key research question is not only:

```text
How do we send speech to a model and play speech back?
```

The deeper project question is:

```text
How does real-time voice interaction change the perceived continuity, presence, and mind-like quality of the system?
```
