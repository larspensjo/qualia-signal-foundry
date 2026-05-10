# Research Questions: Audio

## Purpose

This document captures open research questions related to audio interaction in Qualia Signal Foundry.

Audio is not only an input/output mechanism. In this project, audio is treated as a major part of simulated presence. Voice, timing, interruption, latency, hesitation, and turn-taking may all influence whether the system feels like a continuous communicating entity rather than a text interface.

This document should remain exploratory. Questions listed here should not be treated as requirements until they have been investigated, tested, and promoted through architecture or decision documents.

## Current Status

Status: Exploratory

The project has discussed audio as an important early capability, especially for real-time interaction. The exact implementation approach is still open.

Relevant related documents:

- `docs/Concepts/Concept.RealtimePresence.md`
- `docs/Concepts/Concept.ExternalInputs.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Experiments/Experiment.StreamingTranscriptionMVP.md`
- `docs/Experiments/Experiment.AudioLoopMVP.md`

Current implementation direction:

- Start with streaming transcription as partial and final transcript events.
- Use `gpt-realtime-whisper` as the first OpenAI-backed realtime speech target.
- Defer `gpt-realtime-2` full speech-to-speech sessions until transcript events,
  latency traces, and runtime bridging are working.
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

#### Status

Open

## Research Theme: Turn-Taking and Interruption

### RQ-Audio-TurnDetection

How should the system decide when the user has finished speaking?

#### Why It Matters

Turn detection affects flow. If the system responds too early, it interrupts the user. If it waits too long, it feels slow.

#### Current Thinking

A useful early approach may combine voice activity detection, silence thresholds, and optional explicit controls such as push-to-talk.

#### Possible Experiments

- Compare push-to-talk, silence-based turn detection, and continuous streaming.
- Test different silence thresholds.
- Track false starts, premature responses, and awkward pauses.

#### Status

Open

### RQ-Audio-InterruptionHandling

How should the system react when the user interrupts while it is speaking?

#### Why It Matters

Interruption handling is central to real-time presence. A system that continues speaking over the user may feel non-present or scripted.

#### Current Thinking

The system should eventually support barge-in, where user speech can stop or modify the current output. Early prototypes may log interruptions before fully handling them.

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
- Realtime model session with audio input, after transcript-first experiments
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

## Research Theme: Audio Memory

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
- memory events created from audio

#### Status

Open

## Early Candidate Questions for the First Audio MVP

The first audio-adjacent experiment should focus on streaming transcription before
the full microphone-to-speaker loop:

1. Can the system represent live speech as partial and final transcript events?
2. What is the measured latency to first partial transcript and final transcript?
3. Should partial transcripts affect live state or only traces?
4. How often do partial transcripts revise meaningfully before finalization?
5. What provider errors and fallback paths need explicit events?
6. What information should be logged before adding TTS and playback?

## Not Yet Decided

The following should remain undecided for now:

- provider choice for full speech-to-speech voice sessions
- local versus cloud transcription after the first OpenAI-backed streaming test
- local versus cloud text-to-speech
- always-listening versus push-to-talk
- voice identity
- whether to store raw audio
- whether to use realtime multimodal models directly
- whether audio should be part of the core loop or an optional interface module

## Possible Follow-Up Documents

This research question document may lead to:

- `docs/Experiments/Experiment.StreamingTranscriptionMVP.md`
- `docs/Experiments/Experiment.AudioLoopMVP.md`
- `docs/Architecture/Architecture.AudioDeviceAbstraction.md`
- `docs/Architecture/Architecture.RealtimeSession.md`
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
