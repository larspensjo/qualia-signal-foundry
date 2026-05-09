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
