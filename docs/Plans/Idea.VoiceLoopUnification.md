# Idea: Voice Loop Unification with Multi-Turn SessionState

Status: Promoted to `Plan.VoiceLoopUnification.md`.

## Motivation

The multi-turn text loop has cross-session continuity. The voice loop today has
no `SessionState` module and cannot resume. The next plan should design a
shared session model that handles voice's event-driven shape: interrupts,
partial transcripts, and partial responses. The voice loop can then participate
in the same continuity manifest the text loop uses today.

## Prerequisite

`Plan.CrossSessionContinuity.md` must be complete. This idea consumes its
`SessionState`, `ContinuityManifest`, `MemoryStore`, and `SleepCommit`
abstractions.

## Open Problems

- Whether `Turn` is the right unit for voice, or whether voice needs a finer
  `Utterance` or `Exchange` boundary
- How interruption state participates in `prepare_awake_continuation`
- Whether the consolidated brief is read at session start or streamed as the
  user begins speaking
