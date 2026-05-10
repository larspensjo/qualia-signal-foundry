# Decision log

Purpose: durable record of deliberate commitments — the source of truth for what the
project has agreed to do going forward.

How to use:
- One entry per decision. Decisions are commitments, not summaries of work.
- Implementation summaries and bug-fix postmortems belong in `EngineeringDiary.md`.
  A bug fix earns a decision-log entry only when it produces a durable rule, and the
  rule itself is the entry, not the fix.
- Reversals of prior decisions get their own entry referencing the original.
- A plan in itself, or change thereof, is not a decision until it is committed to.
- Keep entries concise and reference concrete artifacts.
- New entries go to the end of the file.

Use the decision log for:
- Architecture commitments
- Technology or library choices
- Naming, structural, or coding conventions adopted project-wide
- Safety and scope boundaries
- Experiment outcomes promoted into accepted design
- Reusable rules derived from incidents

## Entry Template

## YYYY-MM-DD - <decision title>
Decision: <the rule, in present tense>
Context: <why this was decided now>
Consequences: <what this constrains or implies going forward>
Refs: path/to/file.rs, experiment, prior decision (for reversals), etc.

## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). Established in Agents.md and
mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.
Refs: docs/Architecture/Architecture.RuntimeLoop.md, Agents.md

## 2026-05-09 - Diary and decision-log document contracts
Decision: `EngineeringDiary.md` is the chronological "what happened" log (every
submitted code change, plus research, planning, surprises, and observations) at a
granularity of one entry per logical change. `DecisionLog.md` is reserved for deliberate
commitments only.
Context: The two documents had overlapping templates — the decision log accepted
`Implementation` and `Bug Fix` types, which duplicated what the diary covered. The split
makes each document's purpose unambiguous and lets the decision log stay short and
authoritative.
Consequences: Implementation summaries and bug-fix postmortems do not produce
decision-log entries; a fix only earns one when it yields a durable rule, and the rule
itself is the entry. Diary entries that implement a prior decision should reference it
in their Refs line.
Refs: docs/EngineeringDiary.md, docs/DecisionLog.md,
docs/ProjectFrame/ProjectWorkflow.md

## 2026-05-10 - Transcript-first realtime speech integration
Decision: The first real audio provider integration uses streaming transcription as
partial and final transcript events. Full speech-to-speech realtime sessions and live
translation remain separate later experiments.
Context: OpenAI's May 2026 realtime speech models make realtime audio more practical,
but the framework is still centered on observable event flow, pure reducers, and
replaceable side-effect providers. Streaming transcription fits that boundary with
less complexity than a full voice agent. The model IDs were checked against current
OpenAI API documentation on 2026-05-10.
Consequences: `gpt-realtime-whisper` is the first OpenAI realtime speech target.
`gpt-realtime-2` is reserved for later voice-session experiments, and
`gpt-realtime-translate` is reserved for translation-specific experiments. Realtime
providers emit QSF events and do not own runtime state, memory promotion, tool
permissions, or decisions.
Refs: docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
docs/Architecture/Architecture.AudioLoop.md,
https://developers.openai.com/api/docs/guides/realtime-transcription,
https://developers.openai.com/api/docs/models/gpt-realtime-2,
https://developers.openai.com/api/docs/models/gpt-realtime-translate

## 2026-05-10 - Memory schema versioning is per record type and run artifacts are sealed
Decision: `MemoryRecord` and `Association` each carry an independent `schema_version: u16`
field from v1. The live runtime reads and writes only the current version. Past memory
artifacts are immutable and never migrated in place; versioned readers for older artifacts
live in a separate compatibility module used for replay and analysis. Pure additive
changes (new optional fields with serde defaults) do not bump the version; removed,
renamed, or semantically changed fields do.
Context: Phase 4 of Plan.FrameworkMVP introduces memory records. The framework's replay
goal — "would the same input retrieve the same memories?" — is incompatible with
forward-migrating old run artifacts, which would distort historical evidence. Adding the
version field at v1 is cheap; retrofitting it later is not. Memory records and
associations evolve on independent timelines and should not share a version.
Consequences: Every persisted memory record and association carries its own
`schema_version`. The live `MemoryStore` errors loudly on off-version records rather than
attempting to interpret them. Compatibility readers are written only when a schema
actually changes, and are kept out of the live runtime. A future cross-run shared memory
store (for example, from sleep-phase consolidation) is out of scope and may use a
different policy.
Refs: docs/Plans/Design.MemorySchemaVersioning.md,
docs/Plans/Plan.FrameworkMVP.md,
docs/Architecture/Architecture.MemorySystem.md
