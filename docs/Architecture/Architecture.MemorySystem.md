# Architecture: Memory System

Status: Draft
Maturity: Sketch
Area: Core Architecture

## Implementation Status

The memory system has a working schema, file-backed retrieval, cross-session
storage, sleep-side promotion, live-loop reinforcement, and live cross-turn
association coverage for turns leaving hot context. The remaining gaps are mostly
richer semantics around memory types, full contradiction handling, and
future retrieval backends.

**Implemented today:**

- `MemoryRecord` and `Association` with per-record `schema_version`, frozen at v1
  ([memory/memory_record.rs](../../crates/qsf_app/src/memory/memory_record.rs),
  [memory/association.rs](../../crates/qsf_app/src/memory/association.rs))
- Association-weighted retrieval with a small fixture
  ([memory/retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs),
  [memory/fixtures.rs](../../crates/qsf_app/src/memory/fixtures.rs))
- Retrieval scoring now lives in `qsf_memory`, and the context-assembly domain
  now lives in `qsf_context`; the realtime server can use both without depending
  on the full app runtime.
- Relevance-gated keyword/tag retrieval with explicit skip reasons for omitted
  candidates, plus a narrow identity/profile allowance for name-shaped queries
  ([memory/retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs))
- Time-based recency decay against `MemoryRecord.last_reinforced_at`, falling back
  to `created_at` for legacy records
  ([memory/retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs),
  [memory/memory_record.rs](../../crates/qsf_app/src/memory/memory_record.rs))
- Additive, schema-v1-compatible `MemoryRecord.provenance` and `trust_tier`
  fields. Legacy records default to `first_party_internal` / `trusted`; external
  world observations use `world_observation_external` / `untrusted_external`.
- Time-sensitive decay support: world observations use the provisional
  `WORLD_OBSERVATION_DECAY_HALFLIFE_DAYS` default (7 days) rather than the
  ordinary 30-day half-life, while an optional record-level override supports
  later evidence-driven tuning. The correct value remains an open question for
  the sleep world-memory consolidation experiment.
- World-got-newer supersession-lite: a world observation with `superseded_by`
  is omitted from retrieval with an explicit reason, while its successor remains
  eligible. This deliberately does not infer or resolve general contradictions.
- Sleep world-memory consolidation now refreshes the shared `qsf_corpus` ledger and promotes
  only a conservative, traceable content-hash delta. Durable world observations retain full
  structured external attribution (`content_hash`, title, URL, source domain, and fetch time),
  use `untrusted_external` trust and the 7-day time-sensitive decay profile, and are the only
  route by which world facts enter the store. The provisional rule admits substantive articles
  (at least 60 non-whitespace body characters), capped at the two newest per run. Cap-only
  rejections remain pending for a later sleep run, while rule-based rejections are marked seen;
  every decision has a recorded reason in the authoritative `WorldMemoryConsolidated` run
  artifact. Identical content hashes are deduplicated within a run, and only the newest article
  for a URL in one delta is eligible for promotion.
  The unconfigured bundled-fixture default remains promotion-capable. If an explicitly configured
  corpus path degrades to the fixture, sleep records the degradation and eligibility results but
  suppresses all fallback promotions so fixture content cannot silently replace operator-selected
  external input.
  A later fetched article at the same URL supersedes its predecessor only after promotion-time
  successor/self/mutual-link validation. Realtime recall labels such material as recalled,
  untrusted external source claims with source attribution.
- Cross-session memory store via `MemoryStore`, backed by
  `state/text-loop/memory-store.json`, `state/session/memory-store.json` for the
  text-owned voice shared-continuity path, or `QSF_STATE_DIR/memory-store.json`
  ([memory/store.rs](../../crates/qsf_app/src/memory/store.rs)); store contents now
  carry `processed_ranges` as the idempotency ledger for cross-turn association
  coverage
  ([processed_range.rs](../../crates/qsf_memory/src/processed_range.rs),
  [memory/processed_ranges.rs](../../crates/qsf_app/src/memory/processed_ranges.rs))
- File-backed memory source, opt-in via `QSF_VOICE_MEMORY_SOURCE=file` /
  `QSF_SESSION_MEMORY_SOURCE=file`; the text-owned voice default now reads the shared
  `MemoryStore`, while `QSF_VOICE_MEMORY_SOURCE=phase_four_fixture` remains an
  explicit deterministic memory-and-context fixture mode
- Reviewed-memory draft workflow that converts a sleep report into a memory file
  through an explicit acceptance command for manual review paths
  ([memory/reviewed_memory_draft.rs](../../crates/qsf_app/src/memory/reviewed_memory_draft.rs),
  [experiments/reviewed_memory_draft.rs](../../crates/qsf_app/src/experiments/reviewed_memory_draft.rs),
  [experiments/accept_reviewed_memory.rs](../../crates/qsf_app/src/experiments/accept_reviewed_memory.rs))
- Sleep-side auto-promotion of routine memory candidates and safety-net
  cross-turn associations for session anchors not already covered by live
  processing, including voice exchanges through the shared normalized sleep view
  ([sleep/auto_promote.rs](../../crates/qsf_app/src/sleep/auto_promote.rs),
  [experiments/sleep_phase_session_summary.rs](../../crates/qsf_app/src/experiments/sleep_phase_session_summary.rs))
- The live-memory extraction experiment adds a pass in `qsf_app` that reads trusted
  realtime continuity roots, builds extraction input from promoted turns, and
  applies the existing warm-turn ageing path before routing candidates through
  the existing review and commit path.
- Trusted realtime sideband exchanges are eligible for memory extraction and
  consolidation, while browser-relayed realtime voice exchanges remain
  diagnostic-only and excluded from sleep/continuity.
- Live-loop co-retrieval association formation and retrieved-memory reinforcement
  ([memory/co_retrieval.rs](../../crates/qsf_app/src/memory/co_retrieval.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Text-owned voice participates in the shared continuity memory store by default:
  finalized transcripts retrieve from the resolved store, successful responses run
  live retrieved-memory reinforcement and live memory capture, and the session state
  is committed through the same manifest-last path used by the text loop
  ([experiments/text_owned_voice_loop.rs](../../crates/qsf_app/src/experiments/text_owned_voice_loop.rs),
  [session/runtime.rs](../../crates/qsf_app/src/session/runtime.rs))
- Narrow live-loop capture of accepted assistant-name assignments into the durable
  memory store, so simple identity continuity can survive later cold starts
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Narrow live-loop capture of explicit remember-this turns into remembered-topic
  memories with bounded source excerpts, prior-user topic tags, and source-turn
  metadata
  ([memory/live_capture.rs](../../crates/qsf_app/src/memory/live_capture.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Shared live-memory reinforcement and capture now sit in `session/live_memory.rs`
  so text and voice loops reuse the same persistence and trace logic
  ([session/live_memory.rs](../../crates/qsf_app/src/session/live_memory.rs),
  [experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs),
  [experiments/text_owned_voice_loop.rs](../../crates/qsf_app/src/experiments/text_owned_voice_loop.rs))
- Live-loop cross-turn co-retrieval when turns age out through the warm threshold
  or token-budget batch policy, plus a clean-exit session-end flush for remaining
  hot turns
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))
- Session-local warm summaries and append-only verbatim recall in the multi-turn
  text loop
  ([experiments/multi_turn_text_loop.rs](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs))

**Partial:**

- Explicit episodic / semantic / preference / decision typing of memory items is
  still shallow: the schema has record kinds, but the system mostly promotes
  routine observations and leaves decision candidates in reviewed drafts.
- Sleep promotion deduplicates by normalized text, not by semantic equivalence.
- Associative memory exists in the live retrieval and reinforcement path, while
  richer graph inspection and editing remain future work.
- Supersession only represents a newer replacement for an external world
  observation; general contradiction detection and resolution remain unbuilt.

**Not yet implemented:**

- General contradiction representation
- Vector index, embedding store, or graph store
- Promotion of session summaries or recall records into durable memory beyond
  sleep-generated candidates
- Sleep-side consolidation over voice exchanges is now shared with text turns,
  but richer semantic typing of the resulting memories is still shallow

Last reviewed: 2026-07-19 against the provenance/trust-tier memory substrate,
the live-memory extraction pass,
the shared live-memory runtime, and the implemented trusted sideband path.
Live-loop co-retrieval handles mechanical edges, sleep
contributes safety-net and LLM-candidate associations through the proposer
interface, and both text and text-owned voice sessions flow through the shared
memory store, shared live-memory runtime, and sleep consolidation path by
default.

## Purpose

This document describes a candidate memory-system architecture for Qualia Signal Foundry.

The memory system is responsible for preserving continuity across time without requiring the live runtime loop to load all past information into context. It should support short-term interaction, long-term recall, associative retrieval, memory decay, reinforcement, sleep-like consolidation, and research observability.

This document is not a final implementation specification. It captures an architectural direction for early prototypes and experiments.

## Summary

The memory system should not be treated as a simple transcript archive.

A candidate shape is:

```text
Runtime events
  -> Memory capture
  -> Working/session memory
  -> Memory candidates
  -> Consolidation
  -> Long-term memory items
  -> Associative links
  -> Retrieval and ranking
  -> Context package
  -> Live runtime loop
```

The system should keep the live context small while still allowing relevant past material to reappear when it matters.

## Design Intent

The memory system should support:

- continuity across sessions
- low-cost context selection
- associative recall
- memory decay and reinforcement
- sleep-like consolidation
- inspectable memory retrieval
- replayable experiments
- separation between raw logs and usable memories
- gradual evolution from simple storage to richer memory behavior

The memory system should remain experimental. Early versions should be simple enough to build and inspect, while leaving room for richer models later.

## Relationship to Concepts

This architecture is closely related to:

- `Concept.AssociativeMemory.md`
- `Concept.ContextBudget.md`
- `Concept.SleepPhase.md`
- `Concept.RealtimePresence.md`
- `Concept.MultiModelMind.md`

The concept documents explain why memory matters. This document describes one candidate way the memory system could be structured.

## Memory Is Not One Thing

The project should avoid treating memory as a single database table or transcript file.

Different memory layers may have different lifetimes, retrieval rules, and consolidation behavior.

Candidate memory layers:

```text
Immediate working memory
Session memory
Raw event log
Memory candidates
Episodic memory
Semantic memory
Associative memory graph
Stable project facts
Decision history
Research questions
User or environment facts
```

Not all of these need to exist in the first prototype. The important architectural idea is that memory has layers.

## Candidate Memory Layers

### Immediate Working Memory

Immediate working memory exists inside the live runtime loop.

It may contain:

- the current user input
- recent turns
- current focus
- active audio state
- pending tool results
- unresolved interruption state
- short-lived reasoning state

This memory should be small and temporary.

It may be cleared or compressed frequently.

### Session Memory

Session memory represents what happened during the current interactive session.

It may contain:

- recent input and output events
- summaries of topics discussed
- tool observations
- important user corrections
- apparent unresolved questions
- temporary state transitions
- candidate memories to consider later

Session memory is more durable than immediate working memory, but it should not automatically become long-term memory.

### Raw Event Log

The raw event log is the historical record of what happened.

It may contain:

- input events
- output events
- transcript fragments
- audio state changes
- tool calls
- tool results
- memory retrieval decisions
- model calls
- model outputs
- latency and cost measurements
- errors and interruptions

The raw event log is useful for replay, debugging, and experiment analysis.

However, raw logs are not the same as memory. The live system should not normally load raw logs directly into model context.

### Memory Candidates

A memory candidate is something that may become a long-term memory after review or consolidation.

Examples:

- a repeated theme
- a useful project idea
- a new research question
- a design assumption
- a confirmed preference
- a surprising failure mode
- a decision candidate
- a concept link

The runtime loop may create memory candidates during interaction, but the sleep phase may decide whether and how to promote them.

### Episodic Memory

Episodic memory represents remembered events or sessions.

Examples:

- a discussion about associative memory
- a prototype test of the audio loop
- a user correction about project scope
- an experiment result
- a significant design discussion

Episodic memory may preserve the sense that something happened at a particular time and in a particular context.

### Semantic Memory

Semantic memory represents more general knowledge extracted from episodes.

Examples:

- the project treats tools as perception extensions
- live context should stay small
- sleep-phase consolidation is a controlled process
- read-only tools are preferred early
- audio latency affects perceived presence

Semantic memory is less tied to a specific moment than episodic memory.

### Associative Memory

Associative memory connects memory items through weighted links.

A memory item may be linked to:

- related concepts
- source sessions
- open questions
- decisions
- experiments
- architecture documents
- contradictions
- superseding ideas
- examples

The goal is to make relevant memories easier to retrieve without searching everything.

### Stable Project Facts

Some information should be treated as relatively stable and protected from ordinary decay.

Examples:

- project vision
- non-goals
- accepted decisions
- safety boundaries
- core terminology
- repository conventions

These facts may still change, but changes should be deliberate and traceable.

### Decision History

Decision history should probably live primarily in decision records, but memory retrieval may need access to it.

Examples:

- accepted architecture decisions
- rejected alternatives
- reasons for earlier choices
- consequences already observed

Decision history should be retrievable when the system is discussing architecture or planning.

## Candidate Data Model

An early memory item could be represented with a compact structure.

Possible fields:

```text
id
type
title
summary
source references
created time
last accessed time
importance
confidence
status
decay state
retrieval cues
links
embedding reference
raw log references
```

The exact storage format is not decided.

Important early principle:

```text
Long-term memory items should usually be compact summaries with references back to richer source material.
```

The memory system should avoid copying large transcripts into every memory item.

## Memory Item Types

Possible memory item types:

```text
Episode
Fact
Concept
Question
Decision
Experiment
Observation
Preference
FailureMode
Assumption
Reflection
ToolObservation
ArchitectureNote
```

The first implementation does not need all of these. A small enum or typed label set may be enough.

## Associations

Associations are links between memory items.

A candidate association may contain:

```text
source memory id
target memory id
association type
weight
created time
last reinforced time
creation reason
confidence
status
```

Possible association types:

```text
related-to
supports
contradicts
supersedes
elaborates
example-of
part-of
depends-on
caused-by
question-for
experiment-for
decision-about
source-of
reminds-of
```

Early prototypes may start with simple weighted `related-to` links and add typed associations later.

## Retrieval Flow

A candidate retrieval flow:

```text
Current input and runtime state
  -> Extract retrieval cues
  -> Query recent/session memory
  -> Query semantic memory
  -> Query associative memory
  -> Follow selected links
  -> Rank candidates
  -> Apply context budget
  -> Produce memory context package
  -> Record retrieval explanation
```

The retrieval process should be observable. A researcher should be able to inspect why a memory was retrieved.

## Retrieval Signals

Possible retrieval signals:

- semantic similarity
- keyword match
- active topic
- recent focus
- current user intent
- current experiment mode
- recency
- importance
- confidence
- association strength
- reinforcement history
- prior successful retrieval
- explicit user reference
- decision relevance
- unresolved question relevance

Retrieval should not be based only on one signal.

A memory that is semantically similar but stale and low-confidence may be less useful than a slightly less similar memory that is recent, reinforced, and tied to an accepted decision.

## Retrieval Output

Retrieval should produce a compact context package, not an unstructured dump.

A context package may include:

```text
selected memory summaries
reason each memory was selected
source references
confidence levels
relevance scores
warnings or contradictions
budget used
candidate memories rejected due to budget
```

This makes retrieval useful for both runtime behavior and research analysis.

## Context Budget Interaction

The memory system should cooperate with context management.

Memory retrieval should respect limits such as:

- maximum number of memories
- maximum token budget
- maximum retrieval depth
- maximum association traversal depth
- latency budget
- model-specific context limits
- experiment-specific constraints

A useful early rule:

```text
Memory retrieval should return fewer, better memories rather than many loosely related memories.
```

## Memory Capture

Memory capture is the process of identifying material that may be worth remembering.

Candidate sources:

- user statements
- system outputs
- repeated topics
- tool observations
- experiment results
- contradictions
- decisions
- failures
- corrections
- open questions
- internal reflections

Early memory capture should be conservative. It is better to capture candidates and let consolidation decide than to immediately promote everything to long-term memory.

## Promotion to Long-Term Memory

A memory candidate may be promoted when:

- it appears repeatedly
- it is explicitly marked important
- it affects project direction
- it answers or creates a research question
- it changes architecture
- it captures a decision
- it explains a failure mode
- it is likely to be useful in future sessions

Promotion should record why the item was promoted.

## Decay

Decay reduces the retrieval strength of old or weak memories.

Decay should not necessarily delete information.

Possible decay inputs:

- age
- time since last access
- low importance
- low confidence
- weak associations
- failed retrieval usefulness
- supersession by newer memory
- lack of reinforcement

Some memories should be protected from ordinary decay, such as accepted decisions, project vision, non-goals, and safety boundaries.

## Reinforcement

Reinforcement increases the retrieval strength of a memory.

A memory may be reinforced when:

- it is retrieved and used successfully
- the user confirms it
- it appears in several sessions
- it becomes linked to an experiment
- it becomes linked to a decision
- sleep consolidation identifies it as a recurring theme
- it helps explain current state or behavior

The system should avoid reinforcing memories merely because they were retrieved. Retrieval alone is not proof of usefulness.

## Contradictions and Supersession

The memory system should preserve the history of changing ideas without confusing old and current positions.

Possible mechanisms:

- mark memory as superseded
- link newer memory to older memory with `supersedes`
- reduce retrieval priority of obsolete memory
- keep old memory available for historical explanation
- prefer accepted decisions over old proposals

This is important because the project is exploratory and many ideas will change.

## Sleep-Phase Integration

The sleep phase may be the main place where memory becomes more organized.

Sleep-phase tasks may include:

- summarize recent sessions
- extract memory candidates
- promote useful candidates
- merge duplicates
- create associations
- adjust weights
- apply decay
- reinforce repeated themes
- identify contradictions
- extract open research questions
- prepare suggested future context

The sleep phase should be controlled, inspectable, and reproducible where practical.

It should not become uncontrolled background autonomy.

## Tool Observations as Memory

Tool outputs may become memories, but they should be handled carefully.

Examples:

- search results
- file inspection summaries
- calculation results
- code execution results
- audio transcription events
- future video observations

Tool observations should usually store:

- what was observed
- when it was observed
- which tool produced it
- source or provenance
- confidence or uncertainty
- whether it is likely to become stale

Some tool observations may be highly time-sensitive and should decay or expire faster than project decisions.

## Audio and Real-Time Memory

The audio loop may generate memory-relevant events that are not present in text-only systems.

Examples:

- interruption occurred
- user hesitated
- user corrected transcription
- system response latency felt too high
- turn-taking failed
- user repeated a topic verbally
- audio channel was noisy

The memory system should eventually be able to preserve selected interaction dynamics, not only transcript content.

For early prototypes, this can be limited to simple structured events.

## Observability

Memory behavior should be inspectable.

Researchers should be able to see:

- what memories exist
- what associations exist
- which memories were retrieved
- why they were retrieved
- which memories were rejected
- how much context budget was used
- which memories were reinforced
- which memories decayed
- which candidates were promoted
- which memories were superseded

This observability is part of the research platform, not just debugging support.

## Replay and Experiment Support

The memory system should support replay where practical.

Replay may help answer questions such as:

- Would the same input retrieve the same memories?
- Did a sleep phase change future behavior?
- Did a memory decay rule improve relevance?
- Did associative links reduce context cost?
- Did the system become too sticky around old themes?

For early experiments, deterministic replay may not be perfect because model calls may vary. However, event logs, retrieval inputs, retrieval outputs, and memory state changes should be recorded clearly.

## Storage Options

The storage design is not yet decided.

Possible approaches:

### File-Based Markdown or JSON

Useful for early inspection and low complexity.

Benefits:

- easy to inspect
- easy to version
- simple to prototype
- works well with documentation-driven research

Risks:

- may not scale
- harder to query efficiently
- concurrency may be awkward

### SQLite

Useful for structured local storage.

Benefits:

- simple deployment
- good querying
- durable
- easy to backup
- suitable for local experiments

Risks:

- graph traversal may need extra design
- schema changes need migrations

### Vector Store

Useful for semantic retrieval.

Benefits:

- similarity search
- useful for fuzzy recall
- common LLM integration pattern

Risks:

- retrieval may be hard to explain
- may miss explicit relationships
- stale embeddings may need maintenance

### Graph Store

Useful for associative memory.

Benefits:

- natural relationship modeling
- explicit links
- easier association traversal

Risks:

- may be overkill early
- operational complexity
- harder to combine with semantic retrieval

### Hybrid Approach

A likely long-term direction is hybrid:

```text
raw event log
  + structured memory records
  + vector index
  + explicit association links
```

The first prototype should probably choose the simplest option that allows useful experiments.

## Early MVP Direction

A practical MVP could use:

- append-only event log
- compact session summaries
- memory candidates
- manually inspectable memory records
- simple tags and keywords
- simple weighted associations
- basic retrieval by keyword and semantic similarity
- explicit retrieval explanation
- sleep-phase consolidation pass

A possible MVP flow:

```text
1. Record runtime events.
2. Summarize session into memory candidates.
3. Store selected compact memory items.
4. Add simple associations between related items.
5. Retrieve a small set of memories for new input.
6. Log why each memory was selected.
7. Review retrieval quality manually.
```

This is enough to begin learning without overbuilding the memory system.

## Risks and Failure Modes

### Memory Flooding

The system may store too much, making retrieval noisy and expensive.

Mitigation:

- capture candidates first
- promote selectively
- apply decay
- keep summaries compact
- measure retrieval quality

### False Continuity

The system may appear to remember things that are distorted, outdated, or overgeneralized.

Mitigation:

- store confidence
- preserve source references
- distinguish fact from interpretation
- mark obsolete memories

### Over-Reinforcement

The system may repeatedly retrieve the same memories and become stuck on old themes.

Mitigation:

- avoid reinforcing retrieval alone
- penalize unhelpful retrievals
- track topic diversity
- allow decay despite repeated accidental activation

### Premature Architecture Lock-In

The project may commit too early to a graph, vector store, or schema.

Mitigation:

- keep early storage simple
- document assumptions
- separate concept from implementation
- run retrieval experiments before hardening the design

### Poor Explainability

The system may retrieve memories without a clear reason.

Mitigation:

- log retrieval signals
- store ranking explanations
- expose association paths
- keep retrieval packages inspectable

### Stale Memory

Old observations may become false or irrelevant.

Mitigation:

- use timestamps
- classify freshness sensitivity
- decay time-sensitive memories faster
- mark memories as superseded

## Open Questions

- What is the smallest useful memory item format?
- Should early memory be file-based, SQLite-based, vector-based, or hybrid?
- How should memory usefulness be judged after retrieval?
- How should the system distinguish stable facts from temporary observations?
- How should contradictions be represented?
- How aggressively should weak memories decay?
- Should memory retrieval be deterministic in experiments?
- Should memory associations be created mostly during runtime or during sleep-phase consolidation?
- How much should the live model know about the memory system internals?
- Should user-visible memory and internal simulation memory be separated?
- How should audio interaction dynamics be remembered?
- How should failed or irrelevant retrievals affect future ranking?

## Possible Experiments

### Experiment: Minimal Associative Recall

Test whether simple weighted links improve retrieval relevance compared with keyword search alone.

### Experiment: Decay and Reinforcement

Test whether decay and reinforcement improve long-term memory quality over several sessions.

### Experiment: Sleep Consolidation

Test whether a post-session consolidation pass produces better memory candidates than live capture alone.

### Experiment: Retrieval Explanation

Test whether developers can understand and debug memory retrieval using logged explanations.

### Experiment: Context Budget Reduction

Test whether associative retrieval reduces live prompt size while preserving response quality.

### Experiment: Contradiction Handling

Test whether superseded memory can remain historically available without polluting current behavior.

## Initial Implementation Bias

For the first implementation, prefer:

- simple storage
- explicit records
- inspectable logs
- conservative promotion
- compact summaries
- retrieval explanations
- easy manual review

Avoid early overcommitment to:

- complex graph databases
- hidden memory state
- opaque retrieval systems
- automatic promotion of every interaction
- large always-loaded transcripts

## Current Status

The memory-system architecture is a sketch.

The next useful step is to create a small experiment around memory capture, associative retrieval, and context-budgeted recall. The architecture should evolve based on what that experiment reveals.
