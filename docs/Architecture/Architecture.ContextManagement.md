# Architecture: Context Management

## Maturity

Candidate

## Implementation Status

A context-assembly pipeline exists with explicit fragment selection under a
fragment-count budget, but ranking and role-specific assembly are still simple.

**Implemented today:**

- `ContextAssembler` that selects fragments and emits `ContextAssemblyRequested` /
  `ContextAssembled` events
  ([context/context_assembler.rs](../../crates/qsf_app/src/context/context_assembler.rs))
- `ContextBudget` for per-turn retrieved-memory fragment selection
  ([context/context_budget.rs](../../crates/qsf_app/src/context/context_budget.rs))
- `ContextFragment` carrying source and selection metadata
  ([context/context_fragment.rs](../../crates/qsf_app/src/context/context_fragment.rs))
- Memory retrieval before context assembly in both voice and text loops
- Prompt assembly with cache-stable byte-identical prefixes across turns, including
  warm-summary ageing and tool-recall paths
  ([conversation/prompt.rs](../../crates/qsf_app/src/conversation/prompt.rs))

**Partial:**

- Ranking is dominated by retrieval order and fixture scores; multi-signal ranking
  (recency, reinforcement, diversity) is not implemented
- The fragment budget is count-based; token and cost budgets are tracked for
  reporting but not enforced at assembly time
- The same assembly shape is used across roles today — there is no per-role
  context-pack mechanism

**Not yet implemented:**

- Context packs as a reusable structure
- Role-specific assembly strategies (Live / Memory Extraction / Sleep / Critic)
- Compression beyond warm-summary ageing
- Attention-driven context selection (no `AttentionState` exists)
- Inspectable omitted-fragment lists beyond the existing reports

Last reviewed: 2026-05-18 against the code on `main`.

## Summary

Context management defines how Qualia Signal Foundry decides what information is available to the live simulation at any given moment.

The project assumes that the live context must remain small, focused, and cost-aware. The system should not load the full memory, full transcript, full documentation set, or full environment state into every model invocation. Instead, it should select a compact working context based on current input, active state, relevant memories, recent events, and available token or cost budgets.

Context management is central to the project because it connects memory, real-time presence, tool use, model routing, and sleep-phase consolidation.

## Purpose

The purpose of context management is to make the simulated mind feel continuous without making every interaction expensive or overloaded.

The system should be able to:

- preserve relevant continuity across turns and sessions
- retrieve useful memories without loading everything
- keep the live loop responsive
- avoid irrelevant context pollution
- control inference cost
- support different model roles with different context needs
- make context selection inspectable for research purposes

The live context should be treated as a scarce resource.

## Design Principle

The system should distinguish between **stored state** and **active context**.

Stored state may be large.

Active context should be small.

A useful rule:

```text
The system may remember more than it is currently thinking about.
```

This allows the project to explore long-term memory and continuity without requiring the full memory system to be present in every model call.

## Candidate Context Flow

A possible context flow is:

```text
Input event
  -> update runtime state
  -> identify current focus
  -> retrieve candidate memories
  -> retrieve relevant tool/environment state
  -> summarize or compress candidates
  -> rank context fragments
  -> assemble model context
  -> invoke selected model role
  -> log used context
```

This flow should remain flexible. Different model roles may use different context assembly strategies.

## Context Sources

The context assembler may draw from several sources.

### Current Input

The immediate user input or external signal.

Examples:

- transcribed speech
- typed text
- audio event
- tool result
- system event
- timer event
- session start event

### Recent Interaction State

Short-term conversational state from the current session.

Examples:

- recent turns
- interrupted response
- current topic
- active unresolved question
- current emotional or interaction tone, if modeled
- recent tool calls
- recent memory retrievals

### Runtime State

The current structured state of the simulation.

Examples:

- active focus
- current goals or tensions
- selected mode
- attention target
- pending response
- uncertainty flags
- latency state
- speaking/listening state

### Memory Candidates

Relevant memories selected from the memory system.

Examples:

- episodic memories
- semantic summaries
- associative nodes
- previous decisions
- recurring user preferences
- unresolved themes
- reinforced concepts

### Tool and Environment Context

Information obtained from external perception-like tools.

Examples:

- file contents
- search results
- calculated values
- local environment state
- audio metadata
- video or sensor-derived observations, if added later

### Project and System Instructions

Stable project-level framing and operating constraints.

Examples:

- project vision
- non-goals
- safety boundaries
- tool permissions
- model role instructions
- experiment instructions

These should be loaded sparingly and preferably through compact summaries or targeted context packs.

For realtime sessions, the stable volition baseline is part of this always-present
frame because it is rendered into the shared base instructions. The per-turn
volition context packet is a separate task/context pack: it is injected after any
retrieved memory item and before the initial `response.create`, and it remains
bounded so it can shape framing without replacing the memory layer.

## Context Layers

A useful candidate model is to divide context into layers.

```text
Always-present frame
  Small, stable identity and purpose information.

Session context
  Recent events and current interaction state.

Retrieved memory
  Selected memories relevant to the current situation.

Task/context pack
  Optional focused information for a specific mode or experiment.

  Realtime volition uses this layer for the bounded per-turn packet, while the
  stable baseline stays in the always-present frame.

Tool observations
  Recent external information obtained through controlled tools.

Output constraints
  Instructions for the current response or simulation step.
```

The architecture should avoid letting any single layer dominate the context unless intentionally configured for an experiment.

## Context Budget Types

The project may need several kinds of budgets.

### Token Budget

How many tokens may be included in a model call.

### Cost Budget

How much money or compute should be spent on a step.

### Latency Budget

How long the system can wait before the sense of real-time presence degrades.

### Attention Budget

How many concepts, memories, or active concerns the system can meaningfully handle at once.

### Retrieval Budget

How many memory candidates or tool observations should be considered before final context assembly.

These budgets may conflict. For example, deeper retrieval may improve relevance but increase latency.

## Context Assembly Strategy

A candidate strategy:

1. Start with a compact project/runtime frame.
2. Add the current input.
3. Add recent session state.
4. Retrieve candidate memories.
5. Score candidates for relevance, recency, reinforcement, and novelty.
6. Compress or summarize candidates when needed.
7. Add only the highest-value fragments.
8. Add tool observations if they are directly relevant.
9. Reserve space for the model response.
10. Log what was included and what was omitted.

The system should not assume that more context is always better.

## Memory Retrieval and Context Selection

Memory retrieval and context assembly are related but not identical.

The memory system may return many candidate memories. The context manager decides which of those candidates should actually enter the model context.

Candidate ranking signals may include:

- semantic relevance
- associative link strength
- recency
- reinforcement count
- importance
- emotional or interaction weight, if modeled
- connection to active research questions
- connection to current user input
- previous usefulness
- diversity of selected memories
- risk of distracting from the current focus

The system should avoid retrieving only the most semantically similar memories if that causes repetitive or narrow behavior.

## Context Compression

The system may need several compression mechanisms.

Examples:

- summarize a long transcript into a short session note
- convert raw events into structured memory records
- compress many related memories into one concept summary
- extract only open questions from a long discussion
- replace detailed tool output with a compact observation
- preserve links to full data outside the active context

Compression should preserve enough traceability that researchers can inspect what was lost.

## Context Packs

A context pack is a reusable bundle of focused information loaded for a specific purpose.

Possible examples:

- audio-loop experiment context
- memory-debugging context
- tool-safety context
- project-manager context
- researcher context
- architecture-review context
- diary-consolidation context

Context packs can help avoid always loading large global instructions.

They should be explicit, inspectable, and preferably small.

## Model Role Differences

Different model roles may need different context.

Examples:

```text
Live interaction model
  Needs current input, recent session state, selected memories, and low-latency constraints.

Memory extraction model
  Needs recent transcript/events and rules for converting them into memory records.

Association model
  Needs candidate memories, concepts, and scoring rules.

Sleep/consolidation model
  Needs session summary, new memories, existing related memories, and open questions.

Research/planning model
  Needs larger project context, concept documents, and experiment history.

Critic/reviewer model
  Needs proposal content, decision history, and evaluation criteria.
```

The context manager should not use one universal prompt shape for all roles unless an experiment intentionally tests that simplification.

## Real-Time Constraints

For real-time interaction, context management must respect latency.

The live audio loop may need a faster and smaller context than offline reflection.

A possible division:

```text
Live loop:
  small context, fast retrieval, immediate response.

Reflection step:
  larger context, deeper retrieval, slower reasoning.

Sleep phase:
  broad context, consolidation, association updates, no real-time pressure.
```

This distinction is important for preserving the feeling of presence.

## Observability

Context selection should be observable.

For research and debugging, the system should be able to show:

- which memories were retrieved
- which memories were included
- which were omitted
- why items were ranked highly
- how much context budget was used
- how long retrieval took
- which model role was invoked
- what context pack was active
- whether compression was applied

Observability is not only a debugging feature. It is part of the research method.

## Risks and Failure Modes

### Context Pollution

The system may include too much irrelevant material, causing confused or generic behavior.

### Over-Retrieval

The system may spend too much time or money retrieving memory candidates.

### Under-Retrieval

The system may fail to retrieve important memories, weakening continuity.

### Recency Bias

Recent events may dominate even when older memories are more relevant.

### Reinforcement Bias

Frequently repeated memories may dominate even when they are not currently useful.

### Prompt Bloat

Project instructions, role descriptions, safety notes, and memory summaries may gradually grow until the live loop becomes expensive and slow.

### Lossy Compression

Summaries may remove details that later become important.

### Hidden Context Effects

If the system cannot explain why context was selected, researchers may be unable to understand behavior.

## Candidate Implementation Shape

A possible implementation could include these components:

```text
ContextRequest
  Describes the current model role, input event, budget, and purpose.

ContextSource
  Provides candidate fragments from memory, state, tools, documentation, or recent events.

ContextFragment
  A candidate piece of context with metadata.

ContextRanker
  Scores and filters fragments.

ContextAssembler
  builds the final prompt/context package.

ContextTrace
  Records what was selected, compressed, omitted, and why.
```

This is a candidate shape, not a final design.

## Relationship to Other Documents

This document connects to:

- `Concept.ContextBudget.md`
- `Concept.AssociativeMemory.md`
- `Concept.SleepPhase.md`
- `Concept.ToolsAsPerception.md`
- `Architecture.MemorySystem.md`
- `Architecture.RuntimeLoop.md`
- `Architecture.ToolSystem.md`
- `Architecture.SleepPhase.md`
- `Architecture.ModelRoles.md`
- `Architecture.StateAndObservability.md`

## Open Questions

### RQ-Context-MinimumContinuity

What is the minimum amount of context needed for the system to feel continuous across sessions?

### RQ-Context-RetrievalRanking

Which ranking signals are most useful for selecting relevant memories?

### RQ-Context-CompressionLoss

How much information can be compressed before continuity becomes noticeably weaker?

### RQ-Context-RealtimeBudget

What token and latency budgets are acceptable for real-time audio interaction?

### RQ-Context-RoleSpecificAssembly

Should each model role have a specialized context assembly strategy?

### RQ-Context-InspectableSelection

How much context-selection reasoning should be logged for research without creating excessive overhead?

## Possible Experiments

### Experiment: Minimal Continuity Context

Test how little remembered information is needed for a user to perceive continuity across sessions.

### Experiment: Retrieval Strategy Comparison

Compare semantic similarity, associative weights, recency, and hybrid ranking for memory retrieval.

### Experiment: Context Compression Levels

Run the same interaction with different summary compression levels and compare perceived continuity.

### Experiment: Real-Time Context Budget

Measure how context size affects latency and conversational flow in the audio loop.

### Experiment: Context Pollution Test

Intentionally add irrelevant but plausible memories and observe whether the system becomes distracted or inconsistent.

## Current Status

Context management is considered a central architectural concern.

The exact mechanisms are not decided. The current working assumption is that the system should use small live context, explicit retrieval, role-specific assembly, and inspectable context traces.
