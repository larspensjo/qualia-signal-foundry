# Architecture: Model Roles

## Maturity

Candidate

## Summary

Model roles define how Qualia Signal Foundry may use multiple AI functions as parts of a larger simulated mind.

The project should not assume that one model invocation does everything. Different tasks may benefit from different model roles, context shapes, latency budgets, cost budgets, and reasoning styles.

Model roles are not necessarily separate personalities. They are functional responsibilities inside the system.

## Purpose

The purpose of defining model roles is to make the system more modular, observable, and cost-aware.

Different model roles may be used for:

- real-time interaction
- speech transcription
- speech synthesis
- memory extraction
- associative memory updates
- context assembly
- tool-use planning
- sleep-phase consolidation
- research question extraction
- critique and review
- experiment analysis
- safety and permission checks

The goal is not to create a committee of chatbots. The goal is to split cognitive work into understandable and controllable functions.

## Design Principle

The core principle is:

```text
Use the right model role for the right cognitive task.
```

A live conversation step, a memory consolidation pass, and an architecture review do not have the same requirements.

For example:

```text
Live interaction:
  low latency, small context, responsive behavior.

Sleep consolidation:
  slower, broader context, memory-oriented reasoning.

Research review:
  deeper reasoning, larger context, stronger critique.

Speech transcription:
  audio-specific, streaming-oriented, not general reasoning.
```

This separation allows the project to explore consciousness-like behavior without requiring every process to use the same model, prompt, or context budget.

## Role Versus Model

A model role is not the same thing as a specific provider or model name.

A role describes a responsibility.

A model is one possible implementation of that responsibility.

Example:

```text
Role:
  Memory Extractor

Possible implementations:
  small language model
  larger reasoning model
  rules-based extractor
  local model
  remote API model
```

This distinction helps keep the architecture stable even as available models change.

## Candidate Role Map

A possible early role map:

```text
Realtime Interaction Role
  Handles live conversation and immediate response.

Audio Transcription Role
  Converts microphone audio into text or speech events.

Speech Synthesis Role
  Converts response text or intent into spoken audio.

Context Assembly Role
  Selects and compresses relevant context.

Memory Extraction Role
  Converts events and conversations into memory candidates.

Memory Retrieval Role
  Retrieves candidate memories for the current situation.

Association Builder Role
  Updates links between memories and concepts.

Sleep Consolidation Role
  Performs offline review and memory maintenance.

Tool Selection Role
  Decides whether a tool is needed and why.

Safety and Permission Role
  Checks whether proposed tool use or behavior is allowed.

Research Planner Role
  Turns open questions into experiment proposals.

Critic or Reviewer Role
  Reviews plans, architecture, assumptions, and experiment results.
```

Not all roles need to exist in the MVP.

## Initial MVP Roles

A minimal first implementation could use only a few roles:

```text
Realtime Interaction Role
  Primary live interaction loop.

Audio Transcription Role
  Speech-to-text input.

Speech Synthesis Role
  Text-to-speech output.

Memory Extraction Role
  Creates simple memory candidates after a session.

Sleep Consolidation Role
  Produces a session summary and open questions.
```

Other roles can be added as the project becomes more complex.

## Realtime Interaction Role

The realtime interaction role handles the main live exchange with the user.

Responsibilities:

- respond to current user input
- maintain conversational flow
- use selected memories from context
- respect low-latency constraints
- handle interruption and turn-taking signals
- request tools when needed
- emit response intent or text
- preserve the feeling of presence

This role should usually receive small, focused context.

Possible inputs:

- current user input
- recent dialogue
- current runtime state
- selected memory fragments
- relevant tool observations
- timing or interruption state

Possible outputs:

- response text
- speech output request
- tool request proposal
- memory-worthy event marker
- uncertainty marker
- follow-up intent

Key constraint:

```text
This role should be fast enough to preserve presence.
```

## Audio Transcription Role

The audio transcription role converts audio into text or structured speech events.

Responsibilities:

- process microphone input
- support streaming or chunked transcription
- produce text hypotheses
- detect confidence or uncertainty
- handle partial speech
- detect silence or speech boundaries
- optionally identify interruption events

Possible outputs:

```text
SpeechStarted
SpeechPartialText
SpeechFinalText
SpeechEnded
TranscriptionUncertain
AudioNoiseDetected
UserInterrupted
```

This role should not need access to general memory or project context.

## Speech Synthesis Role

The speech synthesis role converts system output into spoken audio.

Responsibilities:

- generate or play spoken response
- support interruption or cancellation
- expose speaking state to the runtime loop
- possibly support voice style, pacing, and emphasis
- report synthesis latency and playback state

Possible outputs:

```text
SpeechPlaybackStarted
SpeechPlaybackCompleted
SpeechPlaybackInterrupted
SpeechSynthesisFailed
```

In later experiments, speech synthesis may become part of simulated presence rather than a simple output layer.

## Context Assembly Role

The context assembly role helps decide what information should enter a model invocation.

Responsibilities:

- gather candidate context fragments
- score relevance
- respect token, cost, and latency budgets
- compress long material
- avoid context pollution
- produce a context trace

This role may be partly algorithmic and partly model-assisted.

Possible inputs:

- current event
- runtime state
- memory candidates
- tool results
- active role definition
- context budget

Possible outputs:

- assembled context package
- omitted context list
- compression summary
- context trace

This role connects directly to `Architecture.ContextManagement.md`.

## Memory Retrieval Role

The memory retrieval role selects candidate memories from the memory system.

Responsibilities:

- search by semantic similarity
- follow associative links
- consider recency, reinforcement, and importance
- return candidates with scores and explanations
- avoid retrieving too much material

This role may be implemented with embeddings, graph traversal, ranking rules, model calls, or a hybrid approach.

Possible outputs:

```text
MemoryCandidate
  memory_id
  summary
  relevance_score
  association_path
  reason_for_retrieval
```

The context manager should still decide which retrieved memories enter active context.

## Memory Extraction Role

The memory extraction role converts raw events into candidate memories.

Responsibilities:

- inspect recent interaction logs
- detect durable user/project/system information
- extract important events
- identify recurring themes
- avoid saving trivial or noisy details
- propose memory type and importance
- preserve source traceability

Possible memory types:

```text
Episodic memory
Semantic memory
Project memory
User preference
Open question
Decision candidate
Experiment observation
Tool observation
```

This role should be conservative early.

## Association Builder Role

The association builder role updates links between memories, concepts, and events.

Responsibilities:

- detect related memories
- create new associations
- strengthen repeated associations
- weaken stale associations
- merge duplicate concepts
- link memories to research questions or decisions
- record association update reasons

Possible association signals:

- co-occurrence
- repeated retrieval
- explicit user correction
- shared concept
- decision dependency
- experiment result
- tool observation
- temporal proximity

This role is closely related to associative memory and sleep-phase consolidation.

## Sleep Consolidation Role

The sleep consolidation role performs offline or between-session processing.

Responsibilities:

- summarize recent sessions
- extract memory candidates
- update associations
- apply decay and reinforcement
- extract open questions
- identify decision candidates
- prepare future context hints
- write sleep reports

This role can use more context and more time than the realtime role.

It should be heavily logged and should not silently make accepted decisions.

## Tool Selection Role

The tool selection role decides whether a tool is needed.

Responsibilities:

- identify when model memory is insufficient
- select appropriate read-only or computational tool
- state the purpose of tool use
- respect tool permissions
- avoid unnecessary tool calls
- estimate latency impact
- produce structured tool requests

Possible outputs:

```text
ToolRequestProposal
  tool_id
  purpose
  urgency
  expected_result_type
  fallback_if_unavailable
```

This role may be integrated into the realtime interaction role at first.

## Safety and Permission Role

The safety and permission role checks proposed tool use, external actions, and boundary-sensitive behavior.

Responsibilities:

- enforce read-only-first policy
- block disallowed tools
- require human approval for write-capable actions
- check side-effect levels
- detect unsafe local execution
- ensure tool use is logged
- preserve project non-goals

Early versions may implement this mostly through deterministic policy code rather than a model.

## Research Planner Role

The research planner role helps convert open questions into experiments.

Responsibilities:

- inspect unresolved questions
- propose small experiments
- define hypotheses
- identify measurements
- suggest success criteria
- connect experiments to concepts and architecture

This role is useful for the project manager and researcher.

It should not directly change architecture. It should produce proposals.

## Critic or Reviewer Role

The critic or reviewer role evaluates proposals, decisions, and experiment results.

Responsibilities:

- find weak assumptions
- identify premature commitments
- compare alternatives
- detect missing safety boundaries
- review architecture consistency
- evaluate experiment evidence
- recommend changes

This role is useful because the project is exploratory and should avoid locking down ideas too early.

## Role Coordination

Model roles need coordination.

A candidate coordination flow:

```text
Runtime event
  -> role router identifies needed role
  -> context manager assembles role-specific context
  -> selected model is invoked
  -> output is normalized
  -> runtime state is updated
  -> trace is recorded
```

For sleep-phase processing:

```text
Sleep trigger
  -> session summarizer
  -> memory extractor
  -> association builder
  -> question extractor
  -> context hint builder
  -> sleep report
```

The orchestration should be explicit and inspectable.

## Role Router

A role router decides which role should handle a task.

Candidate inputs:

- event type
- current runtime mode
- latency budget
- context budget
- tool availability
- user interaction state
- sleep-phase plan
- experiment configuration

Candidate outputs:

- selected role
- selected model
- context budget
- timeout
- fallback behavior
- trace id

Early implementation may use simple rules.

## Context Per Role

Each role should have its own context strategy.

Examples:

```text
Realtime Interaction Role:
  recent turn, current input, selected memories, small runtime state.

Memory Extraction Role:
  recent event log, memory extraction rules, source references.

Association Builder Role:
  memory candidates, nearby memory graph, association rules.

Critic Role:
  proposal, related decisions, project non-goals, evaluation criteria.

Research Planner Role:
  open questions, concept documents, experiment history.
```

This avoids overloading every model call with the same global context.

## Cost and Latency

Roles should have explicit cost and latency expectations.

Examples:

```text
Low latency:
  realtime interaction
  transcription
  speech synthesis
  interruption handling

Medium latency:
  tool selection
  memory retrieval
  context assembly

High latency acceptable:
  sleep consolidation
  research planning
  architecture review
  experiment analysis
```

The system should be able to route expensive work away from the live loop when possible.

## Fallback Behavior

Each role should define fallback behavior.

Examples:

```text
If transcription confidence is low:
  ask for clarification or mark uncertainty.

If memory retrieval fails:
  continue with recent session context only.

If tool selection fails:
  avoid tool use unless required.

If sleep consolidation fails:
  preserve raw event log and report failure.

If speech synthesis fails:
  fall back to text output.
```

Fallback behavior is important for real-time robustness.

## Observability

Model role use should be logged.

The system should record:

- role invoked
- model used
- input context summary
- selected context fragments
- omitted context fragments
- prompt or prompt reference
- output
- latency
- cost
- errors
- fallback behavior
- downstream effects
- trace id

For research purposes, the system should make it possible to inspect which role influenced which behavior.

## Role Output Normalization

Role outputs should be structured when possible.

Examples:

```text
RealtimeInteractionOutput
  response_text
  tool_request
  memory_marker
  uncertainty
  speech_directive

MemoryExtractionOutput
  memory_candidates
  rejected_items
  source_references
  confidence

SleepConsolidationOutput
  session_summary
  memory_updates
  open_questions
  decision_candidates
  context_hints

CriticOutput
  concerns
  alternatives
  recommendation
  confidence
```

Structured outputs make the system easier to test and debug.

## Human Review Points

Some role outputs should require human review before becoming durable project state.

Examples:

- accepted decisions
- major architecture changes
- external write permissions
- deletion policies
- identity/self-model changes
- experiment conclusions
- safety boundary changes

A model role may propose these, but should not silently commit them.

## Candidate Implementation Shape

A possible implementation could include:

```text
ModelRole
  Defines responsibility, allowed inputs, allowed tools, and expected output.

ModelProfile
  Defines provider/model choice, cost, latency, and capabilities.

RoleRouter
  Selects the role and model for a task.

RoleContextBuilder
  Assembles context for a specific role.

RoleInvoker
  Calls the model or local implementation.

RoleOutputParser
  Normalizes output into structured data.

RoleTrace
  Records role invocation and effects.
```

This is a candidate implementation shape, not a final requirement.

## Role Configuration

Each role may be configured with:

```text
role_id
description
allowed_tools
default_model_profile
fallback_model_profile
context_budget
latency_budget
output_schema
logging_level
human_review_required
safety_policy
```

Configuration should be inspectable and versioned.

## Relationship to Other Documents

This document connects to:

- `Concept.MultiModelMind.md`
- `Concept.RealtimePresence.md`
- `Concept.AssociativeMemory.md`
- `Concept.SleepPhase.md`
- `Architecture.RuntimeLoop.md`
- `Architecture.AudioLoop.md`
- `Architecture.ContextManagement.md`
- `Architecture.MemorySystem.md`
- `Architecture.ToolSystem.md`
- `Architecture.SleepPhase.md`
- `Architecture.StateAndObservability.md`

## Risks and Failure Modes

### Role Explosion

Too many roles may make the system complex, expensive, and hard to reason about.

### Incoherent Mind

Separate roles may produce inconsistent behavior if coordination is weak.

### Hidden Authority

A background role may influence behavior without clear traceability.

### Cost Growth

Specialized roles may increase the number of model calls.

### Latency Problems

Role chaining may damage real-time presence.

### Over-Specialization

Roles may become too narrow and require constant orchestration.

### Premature Architecture

The project may lock into a multi-role architecture before simpler alternatives have been tested.

### Conflicting Outputs

Different roles may disagree about memory importance, tool use, or interpretation.

### Model Drift

Changing underlying model providers may alter role behavior.

## Safety Boundaries

Early safety boundaries:

- write-capable tools should not be available to most roles
- accepted decisions require human review
- sleep-phase roles should not perform external communication
- safety checks should not depend only on model judgment
- role traces should be preserved
- role outputs should be treated as proposals unless explicitly committed
- model provider choices should remain replaceable

## Open Questions

### RQ-ModelRoles-MinimalSet

What is the smallest useful set of model roles for the MVP?

### RQ-ModelRoles-LiveVsOffline

Which cognitive tasks must happen in the live loop, and which can be moved to sleep or reflection?

### RQ-ModelRoles-RoleGranularity

How specialized should roles be?

### RQ-ModelRoles-Coherence

How can separate roles produce behavior that feels like one continuous mind?

### RQ-ModelRoles-CostTradeoff

When does specialization reduce total cost, and when does it increase overhead?

### RQ-ModelRoles-Traceability

How much role-level tracing is needed to understand system behavior?

### RQ-ModelRoles-Fallback

What fallback behavior is acceptable when a role fails?

### RQ-ModelRoles-ProviderIndependence

How can roles be defined so they are not tied to one model provider?

## Possible Experiments

### Experiment: Single Model Baseline

Use one model role for all text reasoning and compare simplicity, latency, cost, and coherence.

### Experiment: Split Live and Sleep Roles

Use one role for live interaction and another for sleep consolidation. Compare continuity and cost.

### Experiment: Memory Extraction Role

Add a dedicated memory extraction role and measure whether memory quality improves.

### Experiment: Critic Role Review

Use a critic role to review architecture or experiment plans and compare against human review.

### Experiment: Tool Selection Role

Compare tool use when selected directly by the live interaction role versus a dedicated tool selection role.

### Experiment: Role Trace Inspection

Review role traces after a session to determine whether role separation makes behavior easier to understand.

## Current Status

Model roles are considered a promising architectural direction, but the project should avoid adding too many roles too early.

The current working assumption is that the MVP should distinguish at least between live interaction, audio transcription, speech synthesis, memory extraction, and sleep consolidation. Additional roles should be introduced only when they solve a clear problem or support a specific experiment.
