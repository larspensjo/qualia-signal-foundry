# Concept: Context Budget

## Summary

Context budget is the idea that the live simulation should treat active model context as a scarce and managed resource.

Qualia Signal Foundry should not assume that every relevant memory, document, tool result, reflection, or transcript fragment can be loaded into the live interaction loop. Instead, the system should deliberately decide what information deserves attention right now, what can remain outside context, and what should be summarized, retrieved, ignored, or deferred.

This concept connects memory, cost control, realtime presence, tool use, sleep-phase consolidation, and system coherence.

## Core Idea

A consciousness-like simulation needs continuity, but continuity does not require loading everything.

The system may have access to a large amount of information over time:

- recent conversation
- long-term memory
- associative memory links
- user preferences
- project notes
- tool outputs
- audio transcripts
- external observations
- internal reflections
- open questions
- self-model state
- experiment logs
- previous decisions

Only a small subset of that information should be active at any given moment.

The context budget is the available space for active information in the model call or live reasoning loop. Managing that budget means deciding what should enter the immediate working context and what should remain in storage.

The goal is not merely to reduce token usage. The goal is to create a selective attention system that supports coherent behavior under constraints.

## Why It Matters

Without context budgeting, the project risks two opposite failures.

The first failure is statelessness. If too little prior information is included, the simulation forgets important context and feels disconnected.

The second failure is context flooding. If too much information is included, the system becomes expensive, slow, noisy, and less focused.

A useful live system needs a middle path:

```text
Enough context to feel continuous.
Not so much context that the system becomes slow, expensive, or distracted.
```

Context budget is therefore not just an implementation detail. It is part of the simulated mind.

A system that chooses what to remember, what to ignore, and what to bring into attention may feel more coherent than a system that simply loads a large transcript.

## Relation to Consciousness Simulation

Human-like consciousness appears to involve limited attention. Many things exist in memory or perception, but only a few are active at once.

Qualia Signal Foundry does not need to imitate human cognition exactly, but the idea of limited active context is useful.

The system may eventually distinguish between:

- immediate sensory input
- active conversation state
- short-term working memory
- recently retrieved memories
- long-term background memory
- unresolved questions
- stable self-model information
- low-priority archived material

Context budget is the mechanism that decides which of these layers participate in the next moment of reasoning.

## Possible Context Layers

The project may eventually use several layers of context.

### Live Input

This is the current user input, audio transcript, visible environment signal, tool event, or other immediate perception.

It should usually have high priority because it represents what is happening now.

### Short-Term Session Context

This includes recent turns, recent audio events, recent tool results, and current interaction state.

It helps the system stay coherent within the current session.

### Working Memory

Working memory contains information that is currently relevant but not necessarily part of the latest input.

Examples:

- the current topic
- unresolved subquestions
- active goals
- assumptions being used
- recent corrections
- the user's current intent
- temporary experiment state

Working memory should be compact and frequently updated.

### Retrieved Memory

Retrieved memory consists of selected long-term memories brought into active context because they appear relevant.

This is where associative memory interacts with the context budget.

The system should not retrieve everything that matches weakly. It should retrieve only the memories that are likely to improve coherence or behavior.

### Stable Identity or Self-Model Context

The simulation may eventually have a compact self-model: what kind of entity it is simulating, its current role, current limitations, and important continuity facts.

This should be compact and carefully maintained. If it grows too large, it may crowd out live interaction.

### Tool Results

Tool outputs can be large and noisy.

The system may need to summarize, filter, or extract only relevant fragments before tool results enter the main context.

A tool result should not automatically become active memory.

### Sleep-Prepared Context

The sleep phase may prepare compact summaries or hints for the next session.

These could include:

- important recent events
- reinforced memories
- open loops
- expected future topics
- warnings about unresolved contradictions
- candidate associations

This layer can help a new session start with continuity without loading the full previous session.

## Budget Types

The project may need to manage several different budgets, not just one.

### Token Budget

The number of tokens available in the live model context.

This is the most obvious budget, but not the only one.

### Latency Budget

Realtime presence requires responses within acceptable time limits.

Even if more context could be loaded, retrieving, summarizing, and reasoning over it may make the system feel slow.

### Cost Budget

Large context and repeated model calls increase cost.

Since one goal is to keep inference cost low, the system should avoid brute-force memory scans during live interaction.

### Attention Budget

Too many facts can distract the model.

Even if context technically fits, irrelevant or weakly relevant information can reduce answer quality and simulated coherence.

### Complexity Budget

Every context-management mechanism adds implementation complexity.

Early versions should prefer simple mechanisms that can be observed and tested.

## Candidate Design Directions

### Context Assembly Pipeline

The system could build each live prompt through a deliberate pipeline.

Example:

```text
Live input
  -> classify intent and salience
  -> update working memory
  -> retrieve candidate memories
  -> rank candidate memories
  -> summarize or compress selected items
  -> assemble final context
  -> run live model call
```

This would make context construction inspectable and testable.

### Explicit Context Slots

The system could reserve separate slots for different information types.

Example:

```text
System frame
Current input
Recent interaction
Working memory
Retrieved memories
Tool observations
Open questions
Response instructions
```

This prevents one category from consuming the whole context.

### Relevance Scoring

Potential context items could be scored before inclusion.

Signals might include:

- semantic similarity
- associative weight
- recency
- repetition
- explicit importance
- current topic match
- emotional or conceptual salience
- relationship to open questions
- relationship to current user intent
- confidence in the memory

The scoring model should be simple at first and improved through experiments.

### Compression Before Inclusion

Not all retrieved material should be included verbatim.

The system may choose between:

- full item
- short summary
- one-line reminder
- structured fact
- link to item only
- excluded

This is especially important for transcripts, tool results, and long documents.

### Budget-Aware Memory Retrieval

Memory retrieval should know the available budget.

A small live interaction may only allow a few short memory reminders. A deeper reflective mode may allow more.

This suggests that the same memory system may support several retrieval modes:

```text
Realtime mode: small, fast, high precision
Reflective mode: larger, slower, exploratory
Sleep mode: broad, offline, consolidation-oriented
Research mode: explicit inspection and analysis
```

### Context Debug View

Because this is a research platform, the assembled context should be inspectable.

A debug view could show:

- what was included
- what was excluded
- why each item was included
- relevance scores
- token estimates
- source memory IDs
- compression level
- which subsystem requested the item

This would make context budget decisions easier to evaluate.

## Open Questions

### What Should Be Always Present?

Some information may need to be present in every live interaction.

Open issue:

```text
What is the minimum stable context required for coherent simulated identity?
```

Too little stable context may make the system inconsistent. Too much may make it rigid or expensive.

### How Much Recent Conversation Should Be Included?

Recent turns are often useful, but transcripts grow quickly.

Open issue:

```text
When should recent transcript be kept verbatim, summarized, or discarded?
```

Realtime audio makes this harder because the system may receive many partial or noisy inputs.

### How Should Memory Compete With Live Input?

A retrieved memory can enrich the response, but it can also distract from the current moment.

Open issue:

```text
How should the system decide when memory is helpful enough to enter live context?
```

### Should the System Know What Was Excluded?

There may be value in telling the model that additional information exists outside context.

Example:

```text
There are older memories related to this topic, but they were not loaded.
```

Open issue:

```text
Does awareness of excluded context improve behavior, or does it create confusion?
```

### How Should Context Budget Affect Personality or Presence?

A system that is constantly context-starved might behave differently from one with rich context.

Open issue:

```text
Should the simulation have observable uncertainty when memory retrieval is weak?
```

This could make the simulation feel more honest and coherent.

### How Aggressive Should Summarization Be?

Summaries save context, but they also lose details.

Open issue:

```text
Which information should be preserved exactly, and which can be compressed?
```

### Should Context Budget Be Static or Dynamic?

Different situations need different context sizes.

A casual interaction may need little memory. A deep research session may need more. A sleep phase may use broad retrieval.

Open issue:

```text
Should the system dynamically choose context budget based on mode, urgency, cost, and user intent?
```

## Risks and Failure Modes

### Over-Retrieval

The system may retrieve too many memories and become distracted.

Symptoms:

- responses feel unfocused
- irrelevant old information appears
- the system over-associates
- current user intent is diluted
- costs increase without better behavior

### Under-Retrieval

The system may retrieve too little and feel stateless.

Symptoms:

- repeated explanations
- forgotten prior decisions
- weak continuity
- user corrections are needed too often
- the simulation does not seem to learn over time

### Summary Drift

Repeated summarization may distort original meaning.

Symptoms:

- memories become simplified incorrectly
- details disappear
- repeated summaries amplify errors
- the self-model changes for the wrong reasons

### Context Pollution

Low-quality information may enter active context and influence behavior.

Examples:

- noisy audio transcripts
- unreliable tool results
- old assumptions
- stale research notes
- incorrect memories
- unverified summaries

### Hidden Decision Logic

If context assembly is opaque, researchers cannot understand why the simulation behaved a certain way.

This is a serious risk for a research platform.

### Premature Optimization

The project may over-engineer context management too early.

Early versions should remain simple enough to build, inspect, and revise.

## Possible Experiments

### Experiment: Fixed Context Slots

Test whether fixed context slots make responses more coherent than an unstructured prompt.

Compare:

```text
Unstructured recent transcript + retrieved memories
```

against:

```text
Structured slots for live input, working memory, retrieved memories, and open questions
```

Measure:

- perceived continuity
- relevance
- response focus
- failure cases
- prompt size

### Experiment: Memory Retrieval Limits

Test different limits for retrieved memories.

Example variants:

```text
0 memories
3 short memories
8 short memories
3 longer memories
summary only
```

Measure whether more memory actually improves simulated continuity.

### Experiment: Realtime Budget Mode

Create a mode where the system must respond with a strict latency and token budget.

Measure:

- response delay
- perceived presence
- memory relevance
- number of retrieval mistakes
- cost per interaction

### Experiment: Compression Levels

Take the same retrieved memory and include it at different compression levels.

Example:

```text
full original
paragraph summary
one-line memory
structured fact
association tag only
```

Compare which level gives the best balance between continuity and cost.

### Experiment: Context Debug View

Build a simple debug view that shows assembled context before model calls.

Evaluate whether this helps researchers diagnose bad responses.

### Experiment: Sleep-Prepared Startup Context

Compare a new session started with:

```text
no prior context
last session summary
sleep-prepared context
associative retrieval from first user input
```

Measure which version creates the best sense of continuity.

## Relationship to Other Concepts

### Associative Memory

Associative memory provides candidate material for context.

Context budget decides how much of that material is actually loaded.

### Realtime Presence

Realtime presence requires low latency and focused attention.

Context budgeting helps prevent memory retrieval and tool use from making interaction sluggish.

### Tools as Perception

Tool results are external observations.

Context budgeting decides which observations should influence the live response and which should be stored or ignored.

### Sleep Phase

The sleep phase can prepare compact context, update memory priorities, and reduce future live-context cost.

It may also repair context pollution and summarize recent experience.

### External Inputs

Audio, video, files, and sensors can produce large amounts of information.

Context budgeting is necessary to prevent external input streams from overwhelming the simulation.

### Multi-Model Mind

Different model roles may have different context budgets.

For example:

```text
fast live model: small context
memory retrieval model: focused context
reflection model: larger context
sleep model: broad offline context
research model: inspectable context
```

## Current Status

Status: Exploratory

The context budget concept is central enough that it should influence early architecture, but the exact mechanisms should not be locked down yet.

The first implementation should probably be simple:

- keep recent session context compact
- retrieve only a small number of candidate memories
- summarize long tool results before inclusion
- make assembled context inspectable
- record what was included and why

The project should avoid building an overly sophisticated context manager before the first memory and realtime experiments have produced evidence.

## Initial Working Assumptions

These assumptions are not final decisions.

- The live loop should use a small, carefully assembled context.
- Memory retrieval should be selective, not exhaustive.
- Tool outputs should usually be filtered or summarized before entering active context.
- Sleep-phase consolidation should reduce future context cost.
- Researchers should be able to inspect assembled context.
- Different operating modes may need different context budgets.
- Context budget is part of the simulated attention system, not only a cost-control mechanism.

## Possible Future Architecture Notes

If this concept proves useful, later architecture documents may define:

- context item types
- context scoring rules
- token estimation
- memory retrieval limits
- compression strategies
- context assembly pipeline
- debug inspection format
- budget policies for different modes
- logging of included and excluded context items

Those details should remain outside this concept document until the project has enough experimental evidence.
