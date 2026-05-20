# Experiment: Associative Memory Toy Model

## Experiment ID

`Experiment.AssociativeMemoryToyModel`

## Status

Completed.

Implemented as the registered `associative-memory-toy-model` experiment. The
experiment now runs deterministically against the Phase 4 memory fixture, compares
recency-only, keyword/tag, and association-weighted retrieval, writes memory and
context traces, and emits `memory-fixture.json` plus `retrieval-comparison.md` run
artifacts.

## Summary

This experiment tests a small, inspectable associative memory model.

The goal is to determine whether simple weighted links between memories can retrieve more useful context than a baseline such as recency-only lookup or keyword matching.

This is a deliberately small experiment. It should use a controlled set of synthetic or manually written memories rather than a large real memory database.

## Motivation

Associative memory is one of the central ideas in Qualia Signal Foundry.

The live loop cannot afford to load all memory into context. The system needs a way to select relevant memories based on meaning, association, recency, reinforcement, and current focus.

This experiment reduces uncertainty around:

- whether weighted associations are useful
- how retrieval decisions can be made inspectable
- what metadata each memory needs
- how memory retrieval interacts with context budgets
- which retrieval baseline is worth improving upon

## Related Documents

```text
Concepts/Concept.AssociativeMemory.md
Concepts/Concept.ContextBudget.md
Concepts/Concept.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.StateAndObservability.md
```

## Hypothesis

A small associative memory graph with weighted links can retrieve more useful context than simple recency-only lookup for prompts that depend on indirect or thematic associations.

## Scope

### In Scope

- small memory record format
- manually created memory set
- weighted associations between memories
- simple retrieval scoring
- recency-only baseline
- keyword baseline, if easy
- inspectable retrieval trace
- manual relevance evaluation
- small context budget simulation

### Out of Scope

- production memory database
- embeddings, unless trivially available
- large-scale retrieval
- automatic memory extraction
- real sleep-phase consolidation
- complex decay policy
- persistent identity model
- full LLM integration

## Setup

Use a small memory set, for example 20-50 records.

Each memory may include:

```text
memory_id
summary
tags
created_at
importance
recency
reinforcement_count
association_links
source_note
```

Each association may include:

```text
from_memory_id
to_memory_id
weight
reason
last_reinforced_at
```

Example memory themes:

- audio loop
- real-time presence
- tools as perception
- associative memory
- context budget
- sleep phase
- project non-goals
- model roles
- observability
- external inputs

## Procedure

1. Create a small controlled memory set.
2. Add weighted links between related memories.
3. Define several test prompts.
4. For each prompt, retrieve memories using recency-only lookup.
5. Retrieve memories using keyword lookup, if implemented.
6. Retrieve memories using associative scoring.
7. Apply the same context budget to each strategy.
8. Record which memories were selected and omitted.
9. Manually rate relevance.
10. Compare strategies.
11. Record failure modes and surprising retrievals.

## Baseline

Primary baseline:

```text
Recency-only retrieval.
```

Optional baseline:

```text
Keyword/tag-based retrieval.
```

Manual ideal selection can be used as a rough reference.

## Measurements

### Quantitative Measurements

- number of memories selected
- number of relevant memories selected
- number of irrelevant memories selected
- number of important memories omitted
- retrieval score per selected memory
- retrieval latency
- simulated context budget used
- association hops used

### Qualitative Observations

- whether selected memories feel relevant
- whether associations reveal useful indirect connections
- whether associations create distraction
- whether scoring is understandable
- whether the trace explains the result
- whether the model feels too hand-tuned

## Success Criteria

The experiment is successful if:

- associative retrieval can be compared against at least one baseline
- retrieval traces explain why memories were selected
- the experiment reveals whether association weights are useful
- the result informs the first memory-system architecture
- failure modes are clear

It is acceptable if associative retrieval performs poorly, provided the reason is informative.

## Failure Criteria

The experiment is inconclusive if:

- the memory set is too small or too artificial to reveal anything
- scoring is opaque
- selected memories cannot be manually evaluated
- the baseline is not comparable
- the implementation becomes too complex before producing results

## Required Observability

The experiment should log:

- input query
- retrieval strategy
- candidate memories
- selected memories
- omitted memories
- retrieval scores
- association paths
- context budget used
- manual relevance rating
- notes about surprising selections

## Risks and Confounders

- hand-authored memories may bias results
- hand-authored links may make the task too easy
- test prompts may overfit the memory graph
- manual relevance ratings are subjective
- simple scoring may not scale
- recency-only baseline may be too weak
- associative links may amplify irrelevant memories

## Expected Output

The experiment should produce:

- experiment notes
- memory set
- association graph snapshot
- retrieval traces
- comparison table
- failure-mode notes
- follow-up questions
- decision candidates

## Results

Implemented in `crates/qsf_app/src/experiments/memory_and_context.rs`.

### What Happened

- The placeholder experiment was replaced by a real retrieval comparison.
- The run uses a controlled fixture and executes three strategies: recency-only,
  keyword/tag, and association-weighted retrieval.
- Each strategy feeds retrieved memories through the same context budget so selected
  and omitted fragments can be compared.
- The experiment records `MemoryRetrievalRequested`, `MemoryRetrieved`,
  `ContextAssemblyRequested`, and `ContextAssembled` events with trace details.

### Measurements

- Retrieval output records selected and omitted memory ids per strategy.
- Context output records selected and omitted fragment ids, estimated token use, and
  retrieval/context latency fields.
- The generated comparison report records selected memory ids, selected context ids,
  omitted context ids, and token use.
- Manual relevance ratings are still not automated.

### Observations

- Association-weighted retrieval can surface linked context-budget and sleep-phase
  memories even when they are not the newest records.
- Retrieval traces expose score components, matched terms, association paths, and
  omitted candidates for manual review.
- Context assembly can reuse the same selected/omitted-fragment observability across
  retrieval strategies.

### Surprises

- The existing event, trace, memory, and context-budget surfaces were enough to host
  the toy model without adding a separate experiment-specific reporting framework.

### Failure Modes

- The fixture is hand-authored, so score behavior may be overfit to the first memory
  graph.
- Estimated token counts are rough, not tokenizer-derived.
- The result supports a first retrieval direction, but it is not evidence that the
  strategy scales to larger or real memories.

## Interpretation

Observed:
  The experiment can compare multiple retrieval strategies under one budget and make
  selected and omitted memories inspectable.

Interpreted:
  Association-weighted retrieval is promising enough to keep as a first-class
  experiment strategy, while recency-only remains useful as a baseline.

Uncertain:
  The project still needs larger fixtures, manual relevance review, and eventually
  live memory use before treating association-weighted retrieval as the default
  memory policy.

## Follow-Up Questions

- Which metadata is actually useful for retrieval?
- Should association weights be manually set, learned, or sleep-phase generated?
- How many association hops should retrieval allow?
- Should old but strongly reinforced memories beat recent memories?
- How should decay affect retrieval?
- How should retrieval results enter the context manager?

## Follow-Up Experiments

```text
Experiment.ContextBudgetRetrievalTest
Experiment.MemoryDecayPolicy
Experiment.AssociationReinforcement
Experiment.SleepPhaseAssociationUpdate
Experiment.MemoryPromotionRules
```

## Decision Candidates

- Candidate: Use explicit memory records with inspectable metadata in the first prototype.
- Candidate: Store association links separately from memory records.
- Candidate: Require retrieval traces for memory experiments.
- Candidate: Compare all retrieval strategies against recency-only lookup.

## Final Status

Completed as a deterministic toy-model experiment. Keep this document as the
experiment spec plus outcome summary; future memory work should build on the emitted
retrieval traces rather than treating this as a production memory-system decision.

## Notes

This was one of the first useful experiments because it tested a central project idea without requiring audio, external services, or real-time infrastructure.
