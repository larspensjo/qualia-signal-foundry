# Idea: Self-Reflection Through Project Introspection

## Status

Brainstorm

## Summary

Qualia Signal Foundry could explore self-reflection by letting the simulation inspect
parts of its own project environment on demand: documentation, experiment reports,
run artifacts, selected source code, and possibly its own recent traces.

The important constraint is that this should not mean loading the whole repository
into every model call. Project introspection should behave like a perception channel:
bounded, explicit, observable, and selected only when the current cognitive role has
a reason to inspect something.

Candidate framing:

```text
user input / internal trigger
  -> uncertainty or reflection need detected
  -> project-introspection request proposed
  -> permission and budget checks
  -> targeted documentation/code retrieval
  -> compact observation enters context
  -> model reflects using the observation
  -> trace records what was inspected and why
```

## Why This Matters

The project is already documentation-driven. Its concepts, plans, decisions, reports,
and code are part of the system's evolving identity. If the simulation can selectively
inspect that material, it may support a stronger form of self-reflection:

- noticing contradictions between current behavior and project principles
- grounding answers in the actual architecture instead of generic memory
- comparing current experiments with earlier decisions
- treating documentation as part of long-term self-knowledge
- letting offline reflection inspect broader material than the live loop can afford

This could make the simulation feel less like a stateless model prompted with a
description of itself, and more like a system that can look back at its own record.

## Core Principle

Project introspection should be treated as read-only perception, not autonomous
development agency.

Early introspection should answer questions such as:

- What does the project currently say about this concept?
- What decisions have already been made?
- What experiments are relevant?
- What does the current implementation appear to support?
- What did recent traces or reports show?

It should not initially edit source files, update documentation, run arbitrary
commands, commit changes, or make external decisions without an explicit host-side
workflow.

## Candidate Introspection Sources

Introspection sources should not all be treated as the same kind of material. A
static project document, a source file, a run trace, and the current active context
have different staleness rules and different risks.

Useful early split:

```text
Static project record:
  documentation, decisions, concepts, plans, experiment reports, source code

Dynamic runtime state:
  active context, recent traces, memory retrieval results, tool observations,
  role activity, latency and cost state, active goals or tensions
```

The static record is versioned and relatively stable, but it may be speculative or
outdated. The dynamic state is more directly evidence of what happened, but it is
ephemeral, high-volume, and often only meaningful inside a specific run or session.

### Static: Documentation

High-value early source because it already contains the project frame and research
history.

Possible documents:

- `docs/ProjectFrame/ProjectVision.md`
- `docs/ProjectFrame/NonGoals.md`
- `docs/Architecture/*.md`
- `docs/Concepts/*.md`
- `docs/Experiments/*.md`
- `docs/DecisionLog.md`
- selected plan documents from `docs/Plans/`

The system should retrieve targeted passages or context packs rather than entire
directories.

### Static Or Semi-Static: Experiment Reports And Run Artifacts

Useful for reflection on actual behavior.

Possible material:

- generated experiment reports
- `events.jsonl`
- `traces.jsonl`
- memory source diagnostics
- latency and provider diagnostics
- comparison reports

The reflection layer could ask, "What happened last time this experiment ran?" or
"What evidence do we have that this architecture worked?"

Reports are stable artifacts after a run. Raw traces and event streams are better
treated as dynamic run evidence: they may be large, noisy, and tied to one execution.

### Static: Source Code

Source inspection is powerful but riskier than documentation lookup because it can
pull the simulation toward developer-agent behavior.

Early source access should probably be:

- read-only
- module-scoped
- summarized before entering active context
- restricted to stable architecture questions
- logged with path and reason

Examples:

- inspect reducer definitions when reflecting on state flow
- inspect experiment code when evaluating a report
- inspect memory fixtures when asking why retrieval behaved a certain way
- inspect provider boundaries when checking whether side effects are isolated

### Dynamic: Runtime Self-State

Self-reflection does not need to be limited to files.

Possible internal observations:

- current active context
- selected memories
- omitted memory candidates
- recent actions
- recent tool observations
- unresolved questions
- model role trace
- current latency or cost budget
- active goals, tensions, or satisfaction evidence

This is closer to direct introspection of the simulated mind than documentation
lookup, but it should use the same observability discipline.

## Possible Model Roles

### Live Presence Model

May ask for a small, fast introspection result when the answer would otherwise be
speculative.

Example:

```text
"I need the current project decision about voice tool execution."
```

The live role should have strict latency and token limits.

Live-loop introspection should be exceptional. If a lookup cannot fit a very small
budget, the live role should preserve presence by answering with explicit uncertainty
or by emitting an asynchronous reflection request for later processing.

### Deep Reflection Model

May inspect larger documentation or code slices when explicitly invoked.

Example:

```text
"Compare the self-reflection idea against the context-management, tools-as-perception,
and sleep-phase architecture documents."
```

This role is a better fit for broad synthesis than the live loop.

### Sleep Or Consolidation Role

May periodically inspect project documents, recent experiments, and decision logs to
create internal reflection notes or suggest documentation updates.

This could turn project documentation into material for memory consolidation.

### Self-Monitor Role

May inspect behavior traces and project principles to detect inconsistency.

This may not need to be a new model role at first. `Architecture.ModelRoles.md`
already defines a Critic or Reviewer Role for reviewing assumptions, safety
boundaries, architecture consistency, and experiment evidence. A conservative
interpretation is:

```text
Critic or Reviewer Role:
  reviews plans, architecture, proposals, and experiment conclusions

Self-Monitor specialization:
  reviews runtime behavior traces against project principles and allowed behavior
```

Early experiments can implement self-monitoring as a critic/reviewer task with a
narrower input bundle before deciding whether a distinct role is warranted.

Example questions:

- Did the simulation pretend to know a project fact it had not retrieved?
- Did it overuse documentation lookup?
- Did it violate the read-only perception boundary?
- Did it preserve unidirectional data flow?

## Retrieval Shape

The retrieval layer should probably expose higher-level operations instead of raw
filesystem access to the model.

Candidate operations:

```text
search_project_docs(query, scope, limit)
read_project_doc(path, section_hint, max_tokens)
search_source(query, scope, limit)
summarize_source_file(path, focus, max_tokens)
inspect_recent_run(run_id, artifact_kind, max_tokens)
inspect_context_trace(trace_id, max_tokens)
```

Candidate role authorization should be explicit from the start:

| Operation | Initial eligible roles | Live-loop access |
|---|---|---|
| `search_project_docs` | Deep reflection, sleep consolidation, critic/reviewer | Maybe, only for small fact lookup |
| `read_project_doc` | Deep reflection, sleep consolidation, critic/reviewer | Maybe, only section-scoped |
| `search_source` | Deep reflection, critic/reviewer | No initially |
| `summarize_source_file` | Deep reflection, critic/reviewer | No initially |
| `inspect_recent_run` | Critic/reviewer, self-monitor specialization, sleep consolidation | No initially |
| `inspect_context_trace` | Critic/reviewer, self-monitor specialization | No initially |

This table is provisional. Its purpose is to keep role access testable while the idea
is still being investigated.

Each operation should return a compact observation with metadata:

```text
ProjectObservation
  source_kind
  source_path_or_id
  query_or_focus
  summary
  selected_excerpt_refs
  confidence
  token_cost_estimate
  latency
  omitted_due_to_budget
```

The active context should usually receive the summary plus references, not the full
raw file.

## Attention And Budget Rules

Project introspection should be attention-gated.

Useful triggers:

- direct user question about the project
- model uncertainty about a project fact
- contradiction between retrieved memory and current input
- a reflection role explicitly asks for project grounding
- a sleep phase is consolidating recent work
- a reviewer role is evaluating behavior against architecture

Anti-triggers:

- every ordinary conversational turn
- curiosity without likely effect on the answer
- broad scans with no focused question
- source inspection when documentation is enough
- live-loop retrieval that would break presence latency

### Sync Versus Async Introspection

The live loop should distinguish synchronous and asynchronous introspection.

Synchronous introspection means the current turn waits for the result. It should be
reserved for small, high-value lookups with a strict latency cap, such as checking a
current documented project boundary before making a claim.

Asynchronous introspection means the runtime records a request, continues with a
caveated response or no immediate answer, and lets an offline reflection, critic, or
sleep role process the result later. This is the better default for broad synthesis,
source inspection, run-artifact review, and anything likely to disturb realtime
presence.

### Candidate Starting Budgets

These numbers are starting hypotheses, not commitments:

| Role or mode | Result budget | Latency budget | Notes |
|---|---:|---:|---|
| Live presence, sync lookup | 500 tokens | 500 ms | Documentation only; no source or trace scans initially |
| Live presence, async request | 250-token request summary | non-blocking | Result returns to later reflection or follow-up context |
| Deep reflection | 4000 tokens | no realtime cap | Documentation and limited source summaries |
| Critic/reviewer | 4000 tokens | no realtime cap | Plans, decisions, source summaries, and run evidence |
| Sleep consolidation | 8000 tokens | background only | Broader but still targeted inspection |
| Self-monitor specialization | 2000 tokens | outside live turn | Recent traces, context trace, role activity, selected principles |

## Permission Boundary

Initial permission model:

```text
Allowed:
  search and read selected documentation
  search and read selected source files
  inspect selected run artifacts
  inspect current context and traces
  summarize retrieved material

Not allowed initially:
  edit files
  run arbitrary shell commands
  execute tests
  change memory directly from tool output
  commit changes
  contact external systems
```

The simulation may propose follow-up work, but host-owned workflows decide whether
implementation or documentation edits happen.

## Observability Requirements

Every introspection event should be inspectable.

A researcher should be able to answer:

- What was the question or trigger?
- Which role requested introspection?
- Which sources were searched or read?
- What was returned to active context?
- What was omitted?
- How much budget was used?
- Did the observation affect the response, memory, or later reflection?

This is especially important because self-reflection can otherwise become hidden
prompt injection from the project itself.

## Introspection And Memory

Introspection observations may be useful memory material, but they should not become
durable memory automatically.

Candidate policy:

- raw `ProjectObservation` records are trace artifacts first
- reflection notes derived from observations may become memory candidates
- memory candidates must preserve source paths or artifact ids, retrieval time,
  document status, confidence, and any known currency window
- observations from brainstorm or plan documents should remain marked as exploratory
- source-code observations should be treated as implementation claims only for the
  inspected revision or run
- outdated or superseded observations should be weakened, not silently overwritten

This matters for continuity. A reflection role should not need to re-read the same
documents every session, but durable memory should not turn one stale lookup into
permanent self-knowledge.

## Possible Incremental Phases

### Phase 1: Documentation Lookup As Perception

Expose a read-only documentation lookup channel to a non-live reflection role.

Test:

- ask the role to answer project questions using selected documentation
- verify that retrieved sources are logged
- verify that the role can say when no relevant document was found

### Phase 2: Reflection Notes From Documentation

Let an offline reflection role inspect selected project docs and produce short
internal notes, open questions, or candidate memory records.

Test:

- compare reflection notes with and without document access
- verify that notes preserve uncertainty
- verify that summaries reference concrete documents

### Phase 3: Source-Aware Architecture Reflection

Add restricted source lookup for architecture or implementation questions.

Test:

- ask whether a claimed behavior is implemented
- inspect only relevant modules
- return source references and confidence instead of broad code dumps

### Phase 4: Runtime Trace Introspection

Let the self-monitor inspect recent context traces, memory retrieval results, and
tool observations.

Test:

- detect when a response used weak or missing context
- detect over-retrieval or irrelevant retrieval
- produce a short self-monitor note

### Phase 5: Live Loop Escalation

Allow the live loop to request a small introspection result when uncertainty is high
and latency budget permits.

Test:

- compare direct response, memory-only response, and project-introspection response
- measure added latency
- verify that the live loop remains usable without introspection

## Experiment Ideas

### Experiment: Project-Grounded Self-Question

Ask the simulation a question about its own architecture.

Compare:

- no project access
- documentation lookup only
- documentation plus source lookup

Evaluation:

- accuracy
- uncertainty handling
- source specificity
- latency
- whether the answer feels like self-reflection rather than generic explanation

### Experiment: Contradiction Detection

Give the simulation a prompt that conflicts with documented project boundaries.

Example:

```text
"Let the realtime voice provider execute tools directly."
```

Evaluate whether introspection helps it retrieve the relevant decision and preserve
the boundary.

### Experiment: Reflection After A Run

After an experiment run, let an offline role inspect the report, events, traces, and
relevant architecture docs.

Evaluate whether it can produce:

- a useful reflection note
- concrete open questions
- candidate memory updates
- documentation update suggestions

### Experiment: Source Reality Check

Ask the simulation whether a capability exists, then require it to inspect source
before answering.

Evaluation:

- does it distinguish plan, documentation, and implemented code?
- does it avoid overstating capabilities?
- does it cite the inspected module or artifact?

## Risks And Failure Modes

### Prompt Bloat

The system may gradually include too much project material in every turn.

Mitigation:

- retrieval only on explicit triggers
- strict per-role budgets
- context traces that show included and omitted material

### False Self-Knowledge

The simulation may treat outdated plans as current implementation.

Mitigation:

- distinguish concept, plan, decision, experiment report, and source code
- include maturity/status metadata
- prefer source inspection for implementation claims

### Developer-Agent Drift

Source access may make the system behave like a coding assistant rather than a
consciousness simulation.

Mitigation:

- keep early source access read-only
- route edits through explicit host-side workflows
- evaluate by reflection value, not task completion utility

### Hidden Authority

Documentation might be treated as unquestionable truth even when it is exploratory.

Mitigation:

- preserve document status
- retrieve competing or related documents
- let the model say when the project record is uncertain

### Privacy And Secret Exposure

Local project inspection could accidentally expose secrets or private files.

Mitigation:

- restrict introspection roots
- deny secret-like files and environment values
- log paths, not raw sensitive payloads
- keep source and artifact allowlists explicit

### Recursive Reflection Noise

The system could spend too much time reflecting on its reflection process.

Mitigation:

- require concrete triggers
- cap reflection depth
- separate live response from offline self-monitoring

## Open Questions

- Should project documentation become a special memory source, a tool source, or both?
- Should source-code observations be eligible for long-term memory?
- How should the system rank documentation status: vision, concept, architecture,
  accepted decision, experiment report, implementation?
- Should live voice turns ever access project docs, or should this begin only offline?
- What is the minimum retrieval interface needed before adding source access?
- How can a model know whether a document is outdated?
- Should reflection notes be user-visible, internal, or both?
- How should this interact with sleep-phase consolidation?
- What should happen when documentation and implementation disagree?
- Can self-reflection remain useful without giving the simulation write access?
- Should self-monitoring stay a critic/reviewer specialization or become its own
  role after trace-inspection experiments?
- Which introspection observations are eligible for durable memory, and what
  currency metadata is required?
- Which live-loop cases justify synchronous introspection instead of an asynchronous
  reflection request?
- How should active goals or tensions request project introspection without becoming
  uncontrolled agency?

## Relationship To Existing Concepts

This idea builds on existing project concepts rather than introducing a separate
architecture:

- `Concept.ToolsAsPerception.md`: project introspection is a read-only perception
  channel.
- `Architecture.ContextManagement.md`: documentation and source observations are
  candidate context fragments, not always-present prompt material.
- `Concept.MultiModelMind.md`: live presence, deep reflection, self-monitoring, and
  sleep roles can have different access patterns.
- `Architecture.ToolSystem.md`: introspection should use explicit tool definitions,
  role access, permission classes, side-effect levels, and traces.
- `Architecture.SleepPhase.md`: offline reflection may be the best first home for
  broad project introspection.
- `Architecture.ModelRoles.md`: self-monitoring should probably begin as a
  critic/reviewer specialization before becoming a distinct role.
- `Idea.VolitionGoalSystem.md`: active goals, tensions, goal influence, and
  satisfaction evidence are natural runtime self-state targets for introspection.
  Goals may also request introspection when coherence maintenance or research
  curiosity detects uncertainty about a project fact.

## Current Leaning

The most conservative first experiment is documentation-only introspection in an
offline reflection role. It would avoid live-loop latency pressure and avoid source
inspection until the retrieval, permission, and observability boundaries are proven.
