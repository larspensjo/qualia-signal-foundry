# Experiment Backlog

## Purpose

This backlog collects candidate experiments for Qualia Signal Foundry.

The project is exploratory, so experiments should stay small, observable, and reversible. An experiment should reduce uncertainty, test a concept, compare candidate architectures, or reveal failure modes.

This backlog is not a commitment to implement everything listed here.

## Backlog Rules

- Keep experiments small enough to produce a useful result.
- Prefer experiments that test one idea at a time.
- Record negative results; they are useful.
- Do not promote experiment outcomes into architecture decisions too quickly.
- Link experiments to concepts, architecture documents, and research questions.
- Use `Experiment.Template.md` when an experiment is selected for planning.

## Status Values

```text
Idea
Proposed
Planned
Running
Completed
Paused
Abandoned
Superseded
```

## Priority Values

```text
High
Medium
Low
Later
```

## Candidate Experiments

| Experiment | Priority | Status | Main Question |
|---|---:|---:|---|
| `Experiment.AssociativeMemoryToyModel` | High | Proposed | Can a small weighted memory graph retrieve useful context better than recency-only lookup? |
| `Experiment.FrameworkSkeletonMVP` | High | Proposed | What is the smallest runnable framework needed to support future experiments? |
| `Experiment.EventLogAndTraceMVP` | High | Proposed | What minimal event log and trace format is useful for understanding system behavior? |
| `Experiment.ContextBudgetRetrievalComparison` | High | Proposed | How should the system select memories under a small context budget? |
| `Experiment.SleepPhaseSessionSummary` | High | Proposed | Does a session-end summary improve continuity in the next session? |
| `Experiment.AudioLoopMVP` | Medium | Proposed | Can a minimal audio loop create a stronger sense of presence than text-only interaction? |
| `Experiment.ToolAsPerceptionCalculator` | Medium | Proposed | How should a simple read-only computational tool be represented as perception? |
| `Experiment.MemoryDecayPolicy` | Medium | Proposed | Does memory decay improve relevance or accidentally hide useful older memories? |
| `Experiment.ModelRoleSplitLiveVsSleep` | Medium | Proposed | Is it useful to split live interaction and sleep consolidation into separate model roles? |
| `Experiment.ContextTraceInspection` | Medium | Proposed | Can a researcher understand why specific context was selected? |
| `Experiment.InterruptionHandlingAudio` | Later | Idea | How should the system react when the user interrupts while it is speaking? |
| `Experiment.ExternalInputEventStream` | Later | Idea | How should non-text inputs be normalized into runtime events? |
| `Experiment.MemoryPromotionRules` | Later | Idea | Which events should become durable memories? |
| `Experiment.AssociationReinforcement` | Later | Idea | Which signals should strengthen links between memories? |
| `Experiment.ToolResultMemoryPromotion` | Later | Idea | When should tool observations become long-term memories? |
| `Experiment.CostPerModelRole` | Later | Idea | Does splitting model roles reduce or increase total cost? |
| `Experiment.SleepTraceAudit` | Later | Idea | Are sleep-phase memory changes understandable and appropriate? |
| `Experiment.ReplaySingleRuntimeStep` | Later | Idea | Can a single runtime step be captured well enough for useful replay or inspection? |

## High-Priority Experiments

### Experiment.AssociativeMemoryToyModel

**Priority:** High  
**Status:** Proposed

Build a small toy version of associative memory using simple text memories, weighted links, recency, and reinforcement.

This experiment should compare associative retrieval against simpler baselines such as recency-only lookup or keyword lookup.

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Concepts/Concept.ContextBudget.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The system can retrieve relevant memories from a small controlled set.
- Retrieval decisions are inspectable.
- The result shows whether association weights are useful enough to continue.
- Failure modes are clear.

Suggested baseline:

```text
Recency-only retrieval.
```

Useful observations:

- Which memories were selected?
- Which memories were omitted?
- Were selected memories actually relevant?
- Did association links help or distract?
- How much context budget was needed?

### Experiment.FrameworkSkeletonMVP

**Priority:** High  
**Status:** Proposed

Create the smallest runnable project framework that can host later experiments.

This experiment is less about consciousness simulation and more about making future experiments easy to run.

Related documents:

```text
Architecture/Architecture.Overview.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.ModelRoles.md
```

Possible scope:

- basic runtime loop skeleton
- event type definitions
- event log
- simple trace output
- placeholder model role abstraction
- placeholder memory store
- command-line experiment entry point

Possible success criteria:

- One experiment can be run through the framework.
- Events and traces are written somewhere inspectable.
- The framework does not overcommit to final architecture.

### Experiment.EventLogAndTraceMVP

**Priority:** High  
**Status:** Proposed

Define and test a minimal event log and trace system.

This experiment should answer what must be recorded to understand a runtime step.

Related documents:

```text
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
```

Possible events:

```text
InputReceived
StateUpdated
MemoryRetrieved
ContextAssembled
ModelRoleInvoked
OutputProduced
TraceRecorded
```

Possible success criteria:

- A short interaction can be inspected after the fact.
- The trace explains why output happened.
- The log is useful without being too verbose.

### Experiment.ContextBudgetRetrievalComparison

**Priority:** High  
**Status:** Proposed

Compare several ways of selecting context under a small budget.

Candidate strategies:

```text
recency only
keyword match
semantic similarity
associative weight
hybrid score
manual ideal selection
```

Related documents:

```text
Concepts/Concept.ContextBudget.md
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.MemorySystem.md
```

Possible success criteria:

- The experiment shows which retrieval strategy is most promising for small memory sets.
- The output includes retrieval scores and omitted candidates.
- The result informs the first memory-system implementation.

### Experiment.SleepPhaseSessionSummary

**Priority:** High  
**Status:** Proposed

Run a simple session-end sleep phase that produces a summary, memory candidates, and open questions.

Related documents:

```text
Concepts/Concept.SleepPhase.md
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
```

Possible success criteria:

- The sleep phase produces a useful session summary.
- It extracts plausible memory candidates.
- It identifies unresolved questions.
- It does not silently create accepted decisions.
- The output is inspectable.

## Medium-Priority Experiments

### Experiment.AudioLoopMVP

**Priority:** Medium  
**Status:** Proposed

Build a minimal audio input/output loop.

This may be the first experiment that tests real-time presence directly, but it may require more setup than memory experiments.

Related documents:

```text
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.AudioLoop.md
Architecture/Architecture.RuntimeLoop.md
Research/ResearchQuestions.Audio.md
```

Possible scope:

- microphone capture
- speech-to-text
- model input
- text-to-speech
- speaker output
- latency logging

Possible success criteria:

- The loop works end-to-end.
- End-to-end latency is measured.
- The system can handle at least simple turn-taking.
- Failure modes are logged.

### Experiment.ToolAsPerceptionCalculator

**Priority:** Medium  
**Status:** Proposed

Give the system access to a simple calculator-like tool and represent the result as an observation rather than an action.

Related documents:

```text
Concepts/Concept.ToolsAsPerception.md
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.ContextManagement.md
```

Possible success criteria:

- Tool requests are structured.
- Tool results are normalized.
- Tool use is logged.
- The result enters context only when relevant.

### Experiment.MemoryDecayPolicy

**Priority:** Medium  
**Status:** Proposed

Compare simple memory decay strategies.

Candidate strategies:

```text
no decay
time-based decay
retrieval reinforcement
manual importance only
hybrid decay and reinforcement
```

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Concepts/Concept.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The experiment shows how decay affects retrieval quality.
- It reveals whether important but old memories are lost too easily.
- It produces clear follow-up questions.

### Experiment.ModelRoleSplitLiveVsSleep

**Priority:** Medium  
**Status:** Proposed

Compare a single-model flow with a split between live interaction and sleep consolidation.

Related documents:

```text
Concepts/Concept.MultiModelMind.md
Architecture/Architecture.ModelRoles.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The split produces better summaries or memory candidates.
- The cost and complexity are understood.
- The trace shows which role affected which output.

### Experiment.ContextTraceInspection

**Priority:** Medium  
**Status:** Proposed

Inspect context assembly traces after interactions and evaluate whether they explain the system's behavior.

Related documents:

```text
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
```

Possible success criteria:

- A researcher can understand why context was selected.
- Omitted context is visible.
- The trace helps diagnose failures.

## Later Experiments

### Experiment.InterruptionHandlingAudio

**Priority:** Later  
**Status:** Idea

Explore how the system should react when the user interrupts while it is speaking.

Related documents:

```text
Architecture/Architecture.AudioLoop.md
Concepts/Concept.RealtimePresence.md
Research/ResearchQuestions.Audio.md
```

### Experiment.ExternalInputEventStream

**Priority:** Later  
**Status:** Idea

Normalize external inputs such as audio, file changes, tool observations, or future video signals into runtime events.

Related documents:

```text
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

### Experiment.MemoryPromotionRules

**Priority:** Later  
**Status:** Idea

Test rules for deciding when an event should become durable memory.

Related documents:

```text
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.SleepPhase.md
```

### Experiment.AssociationReinforcement

**Priority:** Later  
**Status:** Idea

Test which signals should strengthen links between memories.

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.MemorySystem.md
```

### Experiment.ToolResultMemoryPromotion

**Priority:** Later  
**Status:** Idea

Test whether tool observations should become memories and under what conditions.

Related documents:

```text
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.MemorySystem.md
```

### Experiment.CostPerModelRole

**Priority:** Later  
**Status:** Idea

Measure whether splitting model roles increases cost too much or reduces cost by allowing cheaper specialized models.

Related documents:

```text
Architecture/Architecture.ModelRoles.md
Architecture/Architecture.ContextManagement.md
```

### Experiment.SleepTraceAudit

**Priority:** Later  
**Status:** Idea

Review sleep-phase traces to determine whether consolidation changes are understandable and appropriate.

Related documents:

```text
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.StateAndObservability.md
```

### Experiment.ReplaySingleRuntimeStep

**Priority:** Later  
**Status:** Idea

Capture enough information to inspect or rerun a single runtime step.

Related documents:

```text
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

## Recommended First Experiment

Recommended first experiment:

```text
Experiment.AssociativeMemoryToyModel
```

Reason:

- It does not require audio devices.
- It does not require real-time infrastructure.
- It tests a central idea.
- It helps design memory, context management, sleep phase, and observability.
- It can be implemented with simple data structures.
- It can produce useful traces early.

Recommended first framework-support experiment:

```text
Experiment.FrameworkSkeletonMVP
```

Reason:

- It creates the minimum structure needed to run future experiments consistently.
- It keeps the project from becoming a collection of disconnected prototypes.

## Parking Lot

Ideas that may become experiments later:

- simulated attention model
- self-model state
- identity continuity across sessions
- emotional or motivational model
- video input
- screen observation
- tool permission escalation
- controlled write-capable tools
- memory graph visualization
- sleep-phase comparison between different models
- replayable conversation sessions
- user-perceived presence scoring
- synthetic benchmark conversations
- local versus remote model roles
- prompt-injection resistance for tool observations
