# Architecture: Sleep Phase

## Maturity

Candidate

## Implementation Status

A minimal session-end sleep flow exists: a sleep pass produces a structured report,
and a separate reviewed-memory pipeline can promote that report into a memory file
through an explicit acceptance command. Most richer consolidation behavior
(decay, reinforcement, associations, automatic triggers) is not built.

**Implemented today:**

- `sleep_phase_session_summary` experiment that produces a structured `SleepReport`
  from a session
  ([experiments/sleep_phase_session_summary.rs](../../crates/qsf_app/src/experiments/sleep_phase_session_summary.rs),
  [sleep/sleep_report.rs](../../crates/qsf_app/src/sleep/sleep_report.rs),
  [sleep/session_summary.rs](../../crates/qsf_app/src/sleep/session_summary.rs))
- `reviewed_memory_draft` experiment that converts a sleep report into a memory
  draft file
  ([experiments/reviewed_memory_draft.rs](../../crates/qsf_app/src/experiments/reviewed_memory_draft.rs))
- `accept_reviewed_memory` experiment that promotes an inspected draft into a
  file-backed memory source the voice/text loops can load
  ([experiments/accept_reviewed_memory.rs](../../crates/qsf_app/src/experiments/accept_reviewed_memory.rs))
- Session summarization model call routed through the summarizer model role
- Sleep-to-memory conversion is explicit and separate; live loops never auto-promote
  (per the 2026-05-16 decision)

**Partial:**

- Manual trigger only; checkpoint and periodic sleep are not implemented
- Memory candidate extraction is basic — derived from session content, not from
  scoring across events

**Not yet implemented:**

- Decay of weak memories
- Reinforcement of repeated themes
- Association building or strengthening
- Open-question extraction as a structured output
- Decision-candidate extraction
- Future-context-hint preparation
- Tool-trace review
- Replayable sleep with deterministic comparison

Last reviewed: 2026-05-18 against the code on `main`.

## Summary

The sleep phase is an offline or between-session process that reviews recent activity, updates memory, strengthens useful associations, weakens less relevant material, extracts open questions, and prepares future context.

In Qualia Signal Foundry, the sleep phase is not intended to be uncontrolled background autonomy. It is a controlled consolidation process that helps the simulated system maintain continuity without loading all prior context into the live loop.

The sleep phase connects memory, context management, tool traces, decision logging, research questions, and long-term identity modeling.

## Purpose

The purpose of the sleep phase is to improve continuity and reduce live-loop cost.

The system should use sleep-like processing to:

- summarize recent interactions
- convert raw events into memory candidates
- create or update associative links
- reinforce recurring or important memories
- decay weak or unused memories
- extract unresolved questions
- identify possible decisions
- prepare compact context for future sessions
- review tool use and failures
- support researcher inspection

The live loop should remain fast and focused. The sleep phase can be slower, broader, and more reflective.

## Design Principle

The core principle is:

```text
The live loop experiences; the sleep phase consolidates.
```

The live loop should not perform all heavy memory organization during interaction. It should capture enough structured data for later processing.

The sleep phase should then transform raw experience into more durable and useful memory structures.

## What Sleep Phase Is Not

The sleep phase is not:

- uncontrolled autonomous action
- hidden self-modification
- unsupervised external communication
- a replacement for explicit decisions
- a place to silently change project goals
- a reason to avoid logging live-loop behavior

The early sleep phase should be observable, repeatable, and bounded.

## Candidate Sleep Flow

A possible sleep-phase flow is:

```text
Session ends or checkpoint triggers
  -> collect recent event log
  -> summarize interaction
  -> extract memory candidates
  -> score importance and salience
  -> update associative memory
  -> decay weak memories
  -> reinforce repeated themes
  -> extract open questions
  -> identify decision candidates
  -> prepare future context hints
  -> write sleep-phase trace
```

This flow should be treated as a candidate design, not a final architecture.

## Sleep Triggers

The sleep phase may be triggered in several ways.

### Session-End Sleep

Runs after a user session ends.

Useful for:

- summarizing the session
- storing important memories
- identifying unresolved issues
- preparing the next session

### Periodic Sleep

Runs on a schedule or after a fixed amount of activity.

Useful for:

- long-running systems
- continuous audio experiments
- ongoing memory maintenance

### Manual Sleep

Triggered explicitly by a researcher or project operator.

Useful for:

- controlled experiments
- debugging
- comparing different consolidation strategies

### Checkpoint Sleep

Runs after significant events.

Examples:

- important decision
- experiment completion
- long conversation
- major tool use
- new concept introduced
- high uncertainty detected

Early implementation should probably begin with manual or session-end sleep.

## Sleep Inputs

The sleep phase may consume several input streams.

### Event Log

The raw record of what happened.

Examples:

- user input
- model output
- audio events
- interruptions
- state transitions
- tool calls
- tool results
- context assembly traces
- memory retrievals
- errors

### Recent Session Summary

A compact summary of the latest interaction.

This may be generated during or after the session.

### Runtime State Snapshot

A structured snapshot of the simulation state at the time sleep begins.

Examples:

- active focus
- unresolved questions
- current goals or tensions
- recent memories retrieved
- current identity/self-model state, if present
- active context pack

### Memory State

Existing memory records and associations that may be updated.

Examples:

- episodic memories
- semantic summaries
- associative links
- reinforcement counts
- decay timestamps
- importance scores
- open research themes

### Tool Traces

Records of external perception and computation.

Examples:

- search results used
- files inspected
- calculations performed
- audio transcription confidence
- tool errors
- tool latency

### Research and Project Documents

Selected documentation may be available during sleep, especially when the goal is to update research state.

Examples:

- concept documents
- architecture sketches
- experiment logs
- decision records
- open research questions

The sleep phase should not load all documentation blindly. It should use explicit context packs or targeted retrieval.

## Sleep Outputs

The sleep phase may produce several kinds of output.

### Session Summary

A compact record of what happened.

Useful for future continuity.

### Memory Updates

New, modified, reinforced, weakened, merged, or archived memories.

### Association Updates

New or changed links between memories, concepts, questions, and decisions.

### Open Questions

Questions that should be investigated later.

### Decision Candidates

Potential decisions that may need human review.

The sleep phase should not silently convert every candidate into an accepted decision.

### Future Context Hints

Small notes that help the next session start with relevant continuity.

Examples:

- current project focus
- unresolved topic
- next likely task
- important warning
- recently reinforced concept

### Experiment Observations

If the session was part of an experiment, the sleep phase may extract observations and metrics.

### Sleep Trace

A detailed log of what the sleep phase did.

The trace should support inspection, replay, and debugging.

## Memory Consolidation

A key sleep-phase responsibility is memory consolidation.

Possible steps:

```text
Raw events
  -> candidate memories
  -> memory scoring
  -> duplicate detection
  -> merge or split memories
  -> create associations
  -> update reinforcement
  -> apply decay
  -> store compact summaries
```

The system should distinguish between:

- raw transcript
- session summary
- episodic memory
- semantic memory
- associative link
- durable identity/self-model note
- temporary context hint

Not all events deserve long-term storage.

## Associative Updates

The sleep phase is a natural time to update associative memory.

Possible association signals:

- concepts mentioned together
- repeated themes
- user corrections
- decisions linked to reasons
- experiments linked to results
- tool observations linked to questions
- memories retrieved together
- concepts that resolved uncertainty
- unresolved tensions

Associations may have weights, timestamps, reinforcement counts, and decay behavior.

The sleep phase can update these without slowing down the live loop.

## Decay and Forgetting

Memory decay should be handled deliberately.

The sleep phase may:

- reduce weight of unused memories
- archive stale low-value memories
- preserve highly reinforced memories
- keep decisions stable unless explicitly changed
- retain traceability to raw logs when useful
- avoid deleting research-relevant material too aggressively

For this project, forgetting does not necessarily mean physical deletion. It may mean that a memory becomes less likely to enter active context.

A useful distinction:

```text
Forgotten for retrieval
  Less likely to be selected.

Archived for traceability
  Still stored, but not active.

Deleted
  Removed entirely, usually only by explicit policy.
```

## Reinforcement

The sleep phase may reinforce memories that appear important.

Reinforcement signals may include:

- repeated mention
- explicit user importance
- connection to project vision
- connection to active research questions
- use in decisions
- successful retrieval
- emotional or interaction significance, if modeled
- relation to unresolved work
- relation to experiment results

Reinforcement should not be purely frequency-based. Repetition can indicate importance, but it can also indicate noise.

## Open Question Extraction

The sleep phase should detect unresolved questions.

Examples:

- questions explicitly asked but not answered
- tensions between competing designs
- uncertainties mentioned in architecture notes
- experiment ideas not yet run
- assumptions that need validation
- safety boundaries needing clarification

Open questions should be stored in a form that can later feed `ResearchQuestions.*.md` documents or experiment planning.

## Decision Candidate Extraction

The sleep phase may identify decision candidates.

Examples:

- repeated preference for read-only tools first
- emerging agreement on session-end sleep
- selection of Rust as implementation language
- preference for small live context
- rejection of uncontrolled agency

However, the sleep phase should not silently promote a candidate to a formal decision.

Recommended flow:

```text
Sleep phase identifies candidate
  -> candidate appears in sleep trace
  -> human/researcher reviews it
  -> accepted decision becomes ADR or DecisionLog entry
```

This keeps the project from hardening too early.

## Context Preparation

The sleep phase can prepare compact context for future runs.

Possible outputs:

- next-session brief
- active research themes
- recently important memories
- unresolved questions
- current architectural assumptions
- experiment status
- reminders about safety boundaries

These should be small enough to help the next session without becoming prompt bloat.

## Tool Trace Review

The sleep phase may review tool use.

Questions it can ask:

- Were tools used too often?
- Were tools used too late?
- Did tool results improve output quality?
- Did slow tool calls hurt real-time presence?
- Did tool output become memory?
- Were there repeated tool errors?
- Did any tool create unexpected side effects?
- Should tool permissions be adjusted?

Tool trace review is especially important because tools are the bridge between perception and agency.

## Relationship to Runtime Loop

The runtime loop should capture events and state transitions.

The sleep phase should interpret and consolidate them.

```text
Runtime loop:
  fast, event-driven, latency-sensitive.

Sleep phase:
  slower, reflective, memory-oriented, inspectable.
```

The runtime loop should not depend on sleep completing immediately.

## Relationship to Memory System

The memory system stores and retrieves memories.

The sleep phase maintains and reorganizes them.

Possible responsibilities:

```text
Memory system:
  storage, retrieval, scoring interfaces.

Sleep phase:
  summarization, decay, reinforcement, association updates, promotion decisions.
```

This boundary may evolve.

## Relationship to Context Management

The sleep phase can prepare future context hints, but the context manager decides what enters a live model invocation.

```text
Sleep output
  -> context hint or memory update
  -> context manager ranks it later
  -> active context includes it only if useful
```

This prevents sleep from overloading the next live session.

## Relationship to Research Documentation

The sleep phase may produce information that should later be reflected in documentation.

Examples:

- new concept candidate
- new research question
- experiment result
- decision candidate
- recurring risk
- architecture inconsistency

A researcher or project manager should decide which outputs become formal documents.

## Relationship to Multi-Model Mind

The sleep phase may use different model roles than the live loop.

Possible sleep-related roles:

```text
Session summarizer
  Produces compact account of recent events.

Memory extractor
  Converts events into memory candidates.

Association builder
  Updates links between memories and concepts.

Critic/reviewer
  Looks for inconsistencies, risks, or unsupported assumptions.

Research question extractor
  Identifies unresolved questions.

Context pack builder
  Prepares future compact context.
```

These roles may use cheaper, slower, or more specialized models than the live interaction role.

## Observability

The sleep phase should be highly observable.

The system should record:

- when sleep ran
- what triggered it
- what inputs were used
- what summaries were produced
- which memories were created
- which memories were updated
- which memories were decayed
- which associations changed
- which open questions were extracted
- which decision candidates were found
- which context hints were prepared
- which model roles were used
- cost and latency
- errors and uncertainties

Researchers should be able to inspect the sleep result without guessing what happened.

## Replayability

A useful research feature is replayable sleep.

Given the same input event log, memory snapshot, and sleep configuration, the system should be able to rerun or compare consolidation strategies.

Full determinism may be difficult when using probabilistic models, but the system can still preserve:

- input logs
- model names
- prompts
- configuration
- selected context
- outputs
- timestamps
- random seeds where applicable

Replayability helps compare memory strategies.

## Candidate Implementation Shape

A possible implementation could include:

```text
SleepTrigger
  Describes why sleep starts.

SleepInputBundle
  Contains event logs, summaries, state snapshots, memories, and traces.

SleepPlan
  Defines which sleep steps to run.

SleepStep
  A single consolidation operation.

SleepProcessor
  Executes the sleep plan.

MemoryUpdateSet
  Proposed memory changes.

AssociationUpdateSet
  Proposed association changes.

SleepTrace
  Full observable record of the run.

SleepReport
  Human-readable summary of what changed.
```

This is only a candidate shape.

## Candidate Sleep Steps

Early sleep could be split into explicit steps:

```text
1. Gather recent events.
2. Summarize session.
3. Extract memory candidates.
4. Score memory candidates.
5. Update associative links.
6. Apply decay and reinforcement.
7. Extract open research questions.
8. Extract decision candidates.
9. Prepare next-session context hints.
10. Write sleep report and trace.
```

Each step should be individually inspectable and replaceable.

## Minimal MVP

A minimal sleep-phase MVP could be:

```text
Input:
  Recent session transcript or event log.

Process:
  Generate session summary.
  Extract important memory candidates.
  Extract open questions.
  Write a sleep report.

Output:
  Session summary.
  Candidate memory list.
  Open question list.
  Sleep trace.
```

This would already be useful before implementing full associative memory decay.

## Risks and Failure Modes

### Over-Consolidation

The sleep phase may convert too many transient events into durable memories.

### Premature Commitment

The sleep phase may make ideas feel settled before they have been tested.

### Memory Drift

Repeated summarization may distort what actually happened.

### Reinforcement Loops

The system may over-reinforce its own mistaken summaries or assumptions.

### Loss of Detail

Important details may be lost when raw events become summaries.

### Hidden Behavior

If sleep changes memory without clear traces, researchers may not understand future behavior.

### Cost Growth

Sleep may become expensive if it reviews too much history too often.

### Context Pack Bloat

Prepared future context may grow until it harms the live loop.

### False Continuity

The system may appear continuous by overusing summaries rather than retrieving genuinely relevant memory.

### Unsafe Autonomy

Sleep may gradually become a place where the system plans actions rather than consolidates observations.

## Safety Boundaries

Early sleep-phase boundaries:

- no external communication
- no write-capable tools unless explicitly approved
- no silent project-goal changes
- no silent accepted decisions
- no irreversible memory deletion without policy
- all memory updates logged
- all decision candidates marked as candidates
- all generated summaries traceable to source events where practical

These boundaries can be revisited later.

## Open Questions

### RQ-Sleep-Triggering

When should sleep run: session-end, periodic, manual, checkpoint-based, or a combination?

### RQ-Sleep-MinimalUsefulMVP

What is the smallest sleep phase that improves continuity?

### RQ-Sleep-MemoryPromotion

Which events should become durable memories?

### RQ-Sleep-AssociationUpdate

Which signals should strengthen or weaken associations?

### RQ-Sleep-DecayPolicy

How should memory decay work without losing important long-term continuity?

### RQ-Sleep-Traceability

How much source trace is needed to trust sleep-generated summaries?

### RQ-Sleep-ModelRoles

Should sleep be one model call or a pipeline of specialized model roles?

### RQ-Sleep-Replayability

How important is deterministic or semi-deterministic replay for research?

### RQ-Sleep-DecisionCandidates

How should the system distinguish between a recurring idea and an actual decision?

## Possible Experiments

### Experiment: Session-End Summary

After each session, generate a compact summary and use it to seed the next session. Compare perceived continuity against a baseline without sleep.

### Experiment: Memory Candidate Extraction

Extract memory candidates from a session and manually review which are useful.

### Experiment: Association Reinforcement

Track repeated concepts across sessions and strengthen links between related memories. Observe whether retrieval improves.

### Experiment: Decay Policy Comparison

Compare no decay, time-based decay, retrieval-based reinforcement, and hybrid strategies.

### Experiment: Sleep Trace Review

Have a researcher review sleep traces and identify whether the system made understandable consolidation choices.

### Experiment: Replay Same Session With Different Sleep Strategies

Use the same session log and compare multiple sleep-phase approaches.

## Current Status

The sleep phase is considered a central architecture concept, but the implementation should start small.

The current working assumption is that early sleep should be manual or session-end triggered, read-only, heavily logged, and focused on summarization, memory candidate extraction, open question extraction, and future context preparation.
