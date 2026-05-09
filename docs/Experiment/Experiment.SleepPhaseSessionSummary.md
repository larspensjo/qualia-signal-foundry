# Experiment: Sleep Phase Session Summary

## Experiment ID

`Experiment.SleepPhaseSessionSummary`

## Status

Proposed

## Summary

This experiment tests a minimal sleep-phase process that runs after a session and produces a compact session summary, memory candidates, open questions, and decision candidates.

The goal is not to implement full memory consolidation. The goal is to test whether a controlled post-session pass can create useful continuity material for future sessions.

## Motivation

The sleep phase is a central concept in Qualia Signal Foundry.

The live loop should stay fast and focused. Heavy reflection, summarization, memory extraction, and association updates may be better handled after the session.

This experiment reduces uncertainty around:

- what a minimal sleep phase should produce
- whether session summaries improve continuity
- how memory candidates should be extracted
- how open questions should be detected
- how to avoid silently turning suggestions into decisions
- what sleep-phase traces should contain

## Related Documents

```text
Concepts/Concept.SleepPhase.md
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.ModelRoles.md
```

## Hypothesis

A simple session-end sleep phase that produces a summary, memory candidates, open questions, and future context hints can improve continuity across sessions without requiring the live loop to carry large context.

## Scope

### In Scope

- process a short session transcript or event log
- produce a compact session summary
- extract memory candidates
- extract open questions
- extract decision candidates
- produce future context hints
- write a sleep report
- preserve source references where practical
- manual review of sleep output

### Out of Scope

- automatic accepted decisions
- full associative graph updates
- complex decay logic
- irreversible memory deletion
- autonomous external tool use
- background operation
- production memory persistence
- identity/self-model changes

## Setup

Inputs:

- one short session transcript or event log
- existing project framing documents, if needed
- basic sleep-phase instructions
- optional small memory snapshot

Outputs:

- sleep report
- session summary
- memory candidate list
- open question list
- decision candidate list
- next-session context hints
- trace of what input was used

The first version may be manual or semi-manual.

## Procedure

1. Select a short session or diary entry.
2. Run the sleep-phase process on the selected input.
3. Generate a compact session summary.
4. Extract memory candidates.
5. Extract open questions.
6. Extract decision candidates, clearly marked as candidates.
7. Generate future context hints.
8. Record what source material was used.
9. Manually review the output.
10. Decide whether any results should update docs, memory, experiments, or decisions.

## Baseline

Baseline options:

```text
No sleep phase.
Manual diary note only.
Raw transcript carried forward.
```

The main comparison is whether the sleep output is more useful than simply preserving the raw session or diary entry.

## Measurements

### Quantitative Measurements

- input length
- summary length
- number of memory candidates
- number of open questions
- number of decision candidates
- number of future context hints
- time required
- model cost, if using an LLM
- number of useful items after manual review
- number of rejected items after manual review

### Qualitative Observations

- usefulness of summary
- quality of memory candidates
- whether open questions are meaningful
- whether decision candidates are too aggressive
- whether future context hints would help the next session
- whether source traceability is sufficient
- whether the output feels distorted or overconfident

## Success Criteria

The experiment is successful if:

- the sleep output is useful enough to review
- memory candidates are plausible
- open questions are relevant
- decision candidates remain clearly marked as candidates
- the output can be traced to source material
- the result clarifies what the sleep-phase MVP should include

## Failure Criteria

The experiment is inconclusive if:

- the sleep report is generic or unhelpful
- output cannot be traced to source material
- too many trivial memories are extracted
- decision candidates are confused with accepted decisions
- the process is too expensive or verbose for routine use
- the result does not improve on a simple manual diary note

## Required Observability

The experiment should log:

- sleep trigger
- input source
- source excerpt or reference
- sleep instructions used
- model role used, if any
- generated summary
- extracted memory candidates
- extracted open questions
- extracted decision candidates
- generated context hints
- manual review notes
- accepted/rejected items

## Risks and Confounders

- summaries may distort details
- model may over-infer decisions
- trivial facts may be promoted into memory
- important details may be omitted
- manual review may bias the result
- output may appear more coherent than the source supports
- repeated summarization may cause memory drift

## Expected Output

The experiment should produce:

- sleep report
- reviewed memory candidate list
- open question list
- decision candidate list
- notes on rejected items
- recommendation for sleep-phase MVP
- follow-up experiment suggestions

## Results

To be filled in after running the experiment.

### What Happened

TBD

### Measurements

TBD

### Observations

TBD

### Surprises

TBD

### Failure Modes

TBD

## Interpretation

TBD

Use this distinction:

```text
Observed:
  What happened.

Interpreted:
  What we think it means.

Uncertain:
  What remains unclear.
```

## Follow-Up Questions

- What should become a memory candidate?
- How much source traceability is needed?
- Should sleep phase produce context hints automatically?
- How should decision candidates be reviewed?
- How short should the next-session summary be?
- Should sleep happen manually, at session end, or on checkpoints?

## Follow-Up Experiments

```text
Experiment.MemoryPromotionRules
Experiment.SleepTraceAudit
Experiment.AssociationReinforcement
Experiment.SleepPhaseAssociationUpdate
Experiment.ContextBudgetRetrievalTest
```

## Decision Candidates

- Candidate: The first sleep MVP should produce summaries, memory candidates, open questions, and decision candidates.
- Candidate: Sleep-phase outputs should require review before becoming durable decisions.
- Candidate: Sleep reports should include source references or trace IDs.
- Candidate: Session-end sleep is the safest first trigger mode.

## Final Status

TBD

## Notes

This experiment should remain conservative. The sleep phase should help the project remember and prepare; it should not become hidden autonomy.
