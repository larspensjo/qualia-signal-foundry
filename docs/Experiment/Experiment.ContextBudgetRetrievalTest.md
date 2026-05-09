# Experiment: Context Budget Retrieval Test

## Experiment ID

`Experiment.ContextBudgetRetrievalTest`

## Status

Proposed

## Summary

This experiment tests how the system selects relevant memories and context fragments under a deliberately small context budget.

The goal is to compare retrieval and ranking strategies while forcing tradeoffs. The experiment should show what gets included, what gets omitted, and whether the selected context would plausibly support a better model response.

## Motivation

Context is scarce in the live loop.

Qualia Signal Foundry should not load all memories, all documents, or all recent transcript into every model invocation. It needs a strategy for selecting a compact context that preserves continuity without causing prompt bloat or latency problems.

This experiment reduces uncertainty around:

- how small the live context can be
- which retrieval signals matter
- how to rank memory candidates
- how to avoid context pollution
- how to make omitted context visible
- whether associative memory improves context selection

## Related Documents

```text
Concepts/Concept.ContextBudget.md
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

## Hypothesis

A hybrid retrieval strategy using relevance, recency, importance, and associative links can select more useful context under a small budget than recency-only retrieval.

## Scope

### In Scope

- fixed test queries
- small memory/context dataset
- several retrieval strategies
- strict context budget
- selected and omitted context logging
- manual relevance review
- latency measurement
- comparison against baselines

### Out of Scope

- full production context manager
- large-scale vector search
- automated prompt optimization
- full model response evaluation
- real-time audio loop
- long-term memory persistence
- dynamic sleep-phase updates

## Setup

Prepare a small dataset of context fragments, such as:

- recent session turns
- memory records
- concept summaries
- architecture notes
- decision-like facts
- open research questions

Each fragment may have metadata:

```text
fragment_id
summary
type
tags
created_at
importance
recency
association_links
estimated_token_cost
source_reference
```

Define several test prompts that require different kinds of context.

Example prompt types:

- current-topic prompt
- old-but-important-memory prompt
- indirect-association prompt
- project-boundary prompt
- architecture-detail prompt
- open-question prompt

## Procedure

1. Prepare a fixed set of context fragments.
2. Define a small context budget.
3. Define test prompts.
4. Run recency-only selection.
5. Run keyword/tag-based selection.
6. Run associative selection.
7. Run hybrid scoring.
8. Optionally create a manual ideal selection.
9. Record selected and omitted fragments.
10. Measure latency and budget use.
11. Manually evaluate relevance.
12. Compare strategies.

## Baseline

Primary baseline:

```text
Recency-only context selection.
```

Optional baselines:

```text
Keyword/tag matching.
Manual ideal selection.
Random selection, for sanity check.
```

## Measurements

### Quantitative Measurements

- number of fragments selected
- total estimated tokens selected
- context budget used
- relevant fragments selected
- relevant fragments omitted
- irrelevant fragments selected
- retrieval latency
- scoring time
- number of association hops
- overlap with manual ideal selection

### Qualitative Observations

- whether selected context feels sufficient
- whether omitted context should have been included
- whether context pollution occurred
- whether old important items were retrieved
- whether association paths were helpful
- whether scoring explanation is understandable

## Success Criteria

The experiment is successful if:

- multiple retrieval strategies can be compared
- the context budget forces meaningful tradeoffs
- selected and omitted fragments are visible
- the experiment identifies at least one promising retrieval strategy
- failure modes are clear
- the result informs `Architecture.ContextManagement.md`

## Failure Criteria

The experiment is inconclusive if:

- the context budget is too generous
- test prompts are too artificial
- scoring is not inspectable
- manual evaluation is impossible
- all strategies perform similarly for unclear reasons
- the dataset is too small to reveal tradeoffs

## Required Observability

The experiment should log:

- input prompt
- selected strategy
- context budget
- candidate fragments
- selected fragments
- omitted fragments
- scores
- score components
- association paths
- estimated token use
- latency
- manual relevance notes

## Risks and Confounders

- artificial dataset may bias the result
- manual ideal selection may be subjective
- scoring weights may be overfit
- estimated token use may differ from actual tokenization
- keyword matching may perform surprisingly well on small data
- hybrid scoring may look better because it has more knobs
- relevance may depend on the eventual model response, not just selected context

## Expected Output

The experiment should produce:

- comparison table
- retrieval traces
- selected/omitted context examples
- notes on scoring behavior
- recommendation for first context manager prototype
- follow-up research questions

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

- Which score components matter most?
- How much context is enough for continuity?
- Should context selection be deterministic?
- Should the live loop use a smaller strategy than offline reflection?
- How should compressed summaries compete with raw memories?
- Should context manager output include omitted-but-relevant warnings?

## Follow-Up Experiments

```text
Experiment.AssociativeMemoryToyModel
Experiment.ContextTraceInspection
Experiment.MemoryDecayPolicy
Experiment.SleepPhaseSessionSummary
Experiment.ReplaySingleRuntimeStep
```

## Decision Candidates

- Candidate: Context assembly must log selected and omitted fragments.
- Candidate: The first context manager should use a strict budget even in prototypes.
- Candidate: Retrieval should compare against recency-only as a standing baseline.
- Candidate: Hybrid scoring is worth implementing only if score components remain inspectable.

## Final Status

TBD

## Notes

This experiment should be run before the context manager becomes too complex. The goal is to learn which retrieval signals matter, not to optimize final performance.
