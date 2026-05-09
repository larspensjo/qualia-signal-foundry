# Project Workflow

## Purpose

This document describes how ideas, questions, experiments, architecture notes, and decisions should move through Qualia Signal Foundry.

The project is exploratory. The workflow should help the project make progress without locking down ideas too early. It should also prevent useful thoughts from remaining as vague notes forever.

The goal is a lightweight research-and-engineering rhythm:

```text
Capture ideas early.
Clarify them when they matter.
Test them when possible.
Only then promote them into architecture or decisions.
```

## Core Workflow

The normal flow is:

```text
Idea or discussion
  -> Engineering diary
  -> Concept note
  -> Research question
  -> Experiment backlog
  -> Planned experiment
  -> Experiment result
  -> Architecture update
  -> Decision log entry
```

Not every idea must pass through every stage.

Some ideas may stay in the diary. Some may become concept notes. Some may become experiments. Only a few should become accepted architecture or formal decisions.

## Stage 1: Capture

Use:

```text
docs/EngineeringDiary.md
```

Purpose:

Capture rough thoughts, discussion summaries, surprises, concerns, and emerging ideas.

Use this for:

- informal notes
- brainstorming summaries
- observations from coding
- questions that are not yet well-formed
- surprising behavior
- small implementation discoveries
- early experiment impressions

The diary is allowed to be messy. It should preserve thinking before it is organized.

Good diary entry pattern:

```markdown
## 2026-05-08

Discussed the first framework structure.

Important idea:
- The live loop should stay small and fast.

Open question:
- How much context is enough for continuity?

Possible next step:
- Add a small associative memory experiment.
```

## Stage 2: Clarify Concepts

Use:

```text
docs/Concepts/Concept.*.md
```

Purpose:

Turn recurring or important ideas into clear concept notes.

Concept documents should explain:

- what the idea is
- why it matters
- possible design directions
- risks and failure modes
- open questions
- possible experiments

Concept documents should stay exploratory.

They should usually avoid final language such as:

```text
The system must...
The implementation will...
This is the final architecture...
```

Better language:

```text
The system may...
A possible direction is...
This suggests an experiment...
One candidate design is...
```

## Stage 3: Form Research Questions

Use:

```text
docs/Research/ResearchQuestions.Index.md
docs/Research/ResearchQuestions.*.md
```

Purpose:

Capture uncertainty explicitly.

Research questions should be used when the project needs to investigate something before committing to design.

Examples:

```text
RQ-Memory-MinimalContinuity
What is the smallest amount of remembered context needed for the system to feel continuous across sessions?

RQ-Audio-LatencyThreshold
What latency is low enough for audio interaction to feel present?

RQ-Context-RetrievalRanking
Which ranking signals are most useful when selecting memories for a small context budget?
```

A research question can have a status:

```text
Open
Investigating
Tentative Answer
Needs Experiment
Answered
Parked
```

## Stage 4: Add Experiments to the Backlog

Use:

```text
docs/Experiments/Experiment.Backlog.md
```

Purpose:

Collect candidate experiments before committing to detailed planning.

An experiment belongs in the backlog if it can reduce uncertainty or expose a failure mode.

Backlog entries should be short:

```text
Experiment.AssociativeMemoryToyModel
Question: Can weighted associative links retrieve better context than recency-only lookup?
Priority: High
Status: Proposed
```

The backlog is not a promise to implement everything.

## Stage 5: Plan an Experiment

Use:

```text
docs/Experiments/Experiment.Template.md
```

Create a concrete experiment file when a backlog item is selected.

Example:

```text
docs/Experiments/Experiment.AssociativeMemoryToyModel.md
```

A planned experiment should define:

- hypothesis
- scope
- setup
- procedure
- baseline
- measurements
- success criteria
- required observability
- risks and confounders
- expected output

Keep experiments small.

A good experiment should be able to produce useful results even if the idea fails.

## Stage 6: Run and Observe

During an experiment, capture what happened.

Use:

```text
experiment notes
event logs
trace output
metrics
manual observations
EngineeringDiary.md
```

Separate:

```text
Observed:
  What happened.

Interpreted:
  What it might mean.

Uncertain:
  What remains unclear.
```

For this project, observability is part of the experiment. A result is less useful if it cannot be inspected.

Useful things to record:

- inputs
- selected memories
- omitted memories
- context assembly
- model role used
- tool calls
- latency
- cost
- state transitions
- sleep-phase changes
- failure modes
- surprising behavior

## Stage 7: Interpret Results

After running an experiment, update the experiment document.

Fill in:

```text
Results
Interpretation
Follow-up Questions
Follow-up Experiments
Decision Candidates
Final Status
```

Important distinction:

An experiment result can suggest a decision, but it should not automatically become a decision.

Use wording like:

```text
Candidate: Use weighted associative links in the first memory prototype.
```

Do not write it as accepted architecture until reviewed.

## Stage 8: Update Architecture

Use:

```text
docs/Architecture/Architecture.*.md
```

Purpose:

Update candidate architecture when experiments or decisions affect system structure.

Architecture documents should describe current working design, not every idea ever considered.

Good times to update architecture:

- an experiment supports or weakens a design approach
- an implementation detail becomes important
- a boundary between subsystems becomes clearer
- a risk or failure mode needs to be documented
- a concept is ready to become a candidate structure

Architecture maturity labels may be useful:

```text
Sketch
Candidate
Accepted
Deprecated
```

Architecture should not be treated as final unless the decision log says so.

## Stage 9: Record Decisions

Use:

```text
docs/DecisionLog.md
```

Purpose:

Capture commitments.

A decision belongs in the decision log when the project should treat something as settled for now.

Examples:

```text
Decision:
  Early tools are read-only by default.

Context:
  The project wants external perception without uncontrolled agency.

Consequences:
  Write-capable tools are postponed or require explicit approval.
```

Decisions may later be reversed, but reversals should also be recorded.

Use the decision log for:

- project boundaries
- architecture commitments
- safety boundaries
- technology choices
- experiment conclusions promoted into design
- naming or structural conventions

Do not use the decision log for casual ideas.

## Stage 10: Continue the Loop

After updating architecture or decisions, add new follow-up questions or experiments.

Typical follow-up flow:

```text
Experiment result
  -> new question
  -> backlog item
  -> next experiment
```

The project should evolve through repeated small loops, not one large upfront design.

## Document Responsibilities

### `README.md`

Public entry point.

Should explain:

- what the project is
- current status
- how to build and run
- where to find documentation

### `docs/ProjectFrame/ProjectVision.md`

Stable project anchor.

Should explain:

- purpose
- research motivation
- guiding ideas
- long-term direction

### `docs/ProjectFrame/NonGoals.md`

Project boundaries.

Should explain what the project is intentionally not trying to become.

### `docs/EngineeringDiary.md`

Chronological thinking log.

Should capture informal notes, observations, and evolving thoughts.

### `docs/Concepts/`

Exploratory idea documents.

Should explain ideas without overcommitting to implementation.

### `docs/Research/`

Open questions and investigation areas.

Should capture uncertainty explicitly.

### `docs/Experiments/`

Practical tests.

Should turn uncertainty into runnable investigations.

### `docs/Architecture/`

Candidate system structure.

Should describe how the system may be built, with maturity labels where useful.

### `docs/DecisionLog.md`

Accepted decisions.

Should capture commitments and their reasoning.

### `docs/Plans/`

Implementation plans.

Should describe concrete work needed to build framework pieces or run experiments.

## Recommended First Build Loop

For the first implementation phase, use this loop:

```text
1. Create Plan.FrameworkMVP.md.
2. Select Experiment.FrameworkSkeletonMVP or Experiment.AssociativeMemoryToyModel.
3. Implement only the minimum framework needed for that experiment.
4. Log events and traces from the experiment.
5. Update the experiment document with results.
6. Update architecture only where the result clarifies design.
7. Add decisions only if something should now be treated as settled.
```

Recommended first experiment:

```text
Experiment.AssociativeMemoryToyModel
```

Recommended first framework-support experiment:

```text
Experiment.FrameworkSkeletonMVP
```

## Promotion Rules

Use these rules to decide when something moves between document types.

### Diary to Concept

Promote when an idea appears repeatedly or seems central.

Example:

```text
Memory should work through associations rather than transcript replay.
```

### Concept to Research Question

Promote when an idea contains uncertainty that needs investigation.

Example:

```text
Which association signals are useful for memory retrieval?
```

### Research Question to Experiment

Promote when the question can be tested practically.

Example:

```text
Compare recency-only retrieval with weighted associative retrieval.
```

### Experiment to Architecture

Promote when results affect system structure.

Example:

```text
Memory retrieval needs trace output showing why candidates were selected.
```

### Architecture to Decision

Promote when the project should commit to a direction for now.

Example:

```text
The MVP will use explicit event logs and trace IDs.
```

## Decision Discipline

Avoid premature decisions.

A statement should not become a decision just because it appears in:

- a diary entry
- a concept note
- an architecture sketch
- an experiment hypothesis
- a model-generated suggestion

A decision should be deliberate and recorded.

## Experiment Discipline

Prefer small experiments.

A good experiment:

- tests one main idea
- has a clear baseline
- produces inspectable output
- can fail usefully
- records surprises
- creates follow-up questions

Avoid experiments that try to test the entire consciousness simulation at once.

## Architecture Discipline

Architecture documents should stay useful and current.

Avoid:

- turning every speculation into architecture
- hiding open questions inside architecture text
- treating candidate designs as final
- allowing architecture to drift away from experiment results

Prefer:

- maturity labels
- explicit assumptions
- links to related experiments
- clear risks and failure modes
- decision references when something is accepted

## Research Discipline

Keep research questions visible.

If the project feels uncertain, that is acceptable. Capture the uncertainty.

Good research questions prevent vague discomfort from becoming hidden design risk.

## Diary Discipline

Use the diary for chronology, not final truth.

The diary is where the project remembers how thinking evolved.

If a diary entry becomes important, promote it into a concept, question, experiment, architecture note, or decision.

## Handling AI-Generated Suggestions

AI-generated suggestions should be treated as proposals.

They may be useful for:

- drafting documents
- generating experiment ideas
- reviewing plans
- identifying risks
- summarizing discussions
- comparing alternatives

They should not be treated as accepted project truth until reviewed and, where appropriate, recorded in the decision log.

## Handling Open Issues

Open issues should not be hidden.

Put them in one of these places:

```text
Small or fresh issue:
  EngineeringDiary.md

Conceptual uncertainty:
  Concept.*.md

Research uncertainty:
  ResearchQuestions.*.md

Implementation uncertainty:
  Plan.*.md

Experiment uncertainty:
  Experiment.*.md

Accepted risk or tradeoff:
  DecisionLog.md
```

## Handling Contradictions

Contradictions are expected in an exploratory project.

When documents disagree:

1. Check whether one is a concept and the other is architecture.
2. Check whether a decision log entry exists.
3. Prefer the decision log for accepted commitments.
4. Prefer newer experiment results for evidence.
5. Update or annotate stale documents.
6. Record important resolution in the decision log.

## Lightweight Review Checklist

Before starting implementation work, check:

```text
- Is the work tied to a concept, architecture document, experiment, or plan?
- Is the scope small enough?
- Is there a way to observe the result?
- Are open questions captured?
- Are we avoiding premature commitment?
- Are safety boundaries clear?
```

Before accepting a design decision, check:

```text
- What evidence supports it?
- What alternatives were considered?
- What are the consequences?
- Is it reversible?
- Where is it recorded?
```

## Current Working Assumption

The project should use documentation as a research instrument.

Documentation is not just for explaining finished design. It is part of how the project thinks, tests, remembers, and avoids premature certainty.
