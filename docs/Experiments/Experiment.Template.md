# Experiment Template

## Experiment ID

`Experiment.<ShortName>`

Example:

```text
Experiment.AssociativeMemoryToyModel
Experiment.StreamingTranscriptionMVP
Experiment.SleepPhaseSessionSummary
```

## Status

Choose one:

```text
Proposed
Planned
Running
Completed
Paused
Abandoned
Superseded
```

## Summary

Briefly describe the experiment in one or two paragraphs.

The summary should explain what concept, architecture question, or research question the experiment is meant to explore.

## Motivation

Explain why this experiment is worth doing.

Good prompts:

- What uncertainty does this reduce?
- Which concept does it test?
- Which architecture decision might this inform?
- What would we learn if the experiment succeeds?
- What would we learn if it fails?

## Related Documents

List related concept, architecture, research, and decision documents.

Example:

```text
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Research/ResearchQuestions.Index.md
```

## Hypothesis

State the hypothesis clearly.

Example:

```text
A small associative memory graph with weighted links and decay can retrieve more useful context than simple recency-based transcript lookup.
```

The hypothesis should be testable, even if the result is partly subjective.

## Scope

Describe what is included and excluded.

### In Scope

- Item 1
- Item 2
- Item 3

### Out of Scope

- Item 1
- Item 2
- Item 3

Out-of-scope items are important because experiments should stay small.

## Setup

Describe the environment, tools, models, data, and configuration needed.

Possible items:

- operating system
- language/runtime
- local services
- model provider
- model role configuration
- input data
- test scripts
- sample memories
- audio devices
- feature flags

## Procedure

Describe the experiment steps.

Example:

```text
1. Prepare a small set of synthetic memories.
2. Create associations between memories.
3. Run a set of test prompts.
4. Retrieve candidate memories using the experimental method.
5. Compare against a baseline retrieval method.
6. Record selected memories, omitted memories, latency, and observations.
```

## Baseline

Describe what the experiment is compared against.

Examples:

- no memory
- recency-only memory
- keyword search
- semantic search
- manual selection
- text-only interaction
- no sleep phase
- no tool access

If there is no baseline, explain why.

## Measurements

List what should be measured or observed.

### Quantitative Measurements

Examples:

- latency
- token usage
- cost
- number of memories retrieved
- retrieval precision
- retrieval recall
- number of tool calls
- transcription delay
- interruption delay

### Qualitative Observations

Examples:

- perceived continuity
- perceived presence
- relevance of retrieved memories
- clarity of behavior
- researcher confidence
- failure modes
- surprising behavior

## Success Criteria

Define what would make the experiment successful.

Success does not need to mean that the idea works perfectly. An experiment can succeed by producing useful negative evidence.

Example:

```text
The experiment is successful if it shows whether weighted associative retrieval provides visibly better context selection than recency-only retrieval for at least a small controlled memory set.
```

## Failure Criteria

Define what would make the experiment fail or become inconclusive.

Examples:

- result cannot be reproduced
- observations are too subjective
- logging is insufficient
- latency is too high to evaluate interaction quality
- test data is too small or too artificial
- implementation complexity overwhelms the concept being tested

## Required Observability

List what must be logged or inspectable.

Examples:

- input events
- selected context
- omitted context
- retrieved memories
- retrieval scores
- model role invocations
- tool calls
- latency
- cost
- generated summaries
- memory updates
- sleep-phase changes

### Trace Completeness Contract

Fill this in when the experiment depends on traces to explain a behavioral chain, such
as selection, arbitration, reducer/action flow, effect boundaries, replay, or "why did
this happen?" review. If it does not apply, write "Not applicable" and explain why.

Required trace fields by stage:

```text
Stage / turn:
  input:
  events_applied:
  selector_output:
  omitted_or_suppressed_candidates:
  arbitration_result:
  bounded_or_external_output:
  dynamic_state_snapshot:
  artifact_or_report_reference:
```

Artifact boundary:

```text
events.jsonl:
  Chronological facts that occurred.

trace records:
  Structured causal/reasoning chain needed for replay and review.

human-readable report:
  Summary and review checklist, derived from structured artifacts.
```

Automated verification:

```text
- Parse generated artifacts and assert required trace fields exist.
- If replay determinism is claimed, compare stable meaningful trace fields.
- Do not count status strings, event counts, or free-form report text as sufficient
  evidence for trace completeness.
```

## Risks and Confounders

List anything that could distort the result.

Examples:

- model variability
- prompt wording
- poor test data
- hidden context effects
- overfitting to examples
- subjective evaluation
- audio device quality
- network latency
- provider model changes

## Expected Output

Describe what artifacts the experiment should produce.

Examples:

- experiment notes
- event log
- trace file
- trace schema/completeness check output
- metrics table
- memory graph snapshot
- comparison summary
- follow-up research questions
- decision candidate

## Results

Fill this in after running the experiment.

### What Happened

Describe the actual result.

### Measurements

Record observed metrics.

### Observations

Record qualitative observations.

### Surprises

Record anything unexpected.

### Failure Modes

Record what did not work.

## Interpretation

Explain what the result seems to mean.

Separate observed facts from interpretation.

Useful distinction:

```text
Observed:
  What happened.

Interpreted:
  What we think it means.

Uncertain:
  What remains unclear.
```

## Follow-Up Questions

List new or remaining questions.

Example:

```text
- Does the retrieval method still work with 1,000 memories?
- Does decay improve relevance or hide useful old memories?
- Should association updates happen live or only during sleep phase?
```

## Follow-Up Experiments

List possible next experiments.

Example:

```text
Experiment.AssociativeMemoryDecayPolicy
Experiment.ContextBudgetRetrievalTest
Experiment.SleepPhaseAssociationUpdate
```

## Decision Candidates

List possible decisions suggested by this experiment.

Important: these are only candidates until reviewed and recorded in the decision log.

Example:

```text
- Candidate: Use weighted associative links in the first memory prototype.
- Candidate: Keep memory retrieval outside the real-time audio path for the MVP.
```

## Final Status

Choose one after evaluation:

```text
Useful Result
Inconclusive
Needs Rerun
Superseded
Abandoned
```

## Notes

Free-form notes, links, diagrams, or comments.
