# Idea: Volition and Goal System

## Status

Brainstorm

## Summary

Qualia Signal Foundry should explore whether a simulated consciousness needs a
volition system: some representation of internal goals, drives, tensions, or
curiosities that can cause the simulation to take initiatives of its own.

This does not mean imitating human biological goals such as survival or reproduction.
The project should be careful about importing human motivational structures too
literally. The early research value is narrower:

```text
Can an explicit goal system make the simulation explore ideas, ask questions,
notice unfinished threads, and initiate useful reflection without becoming an
uncontrolled agent?
```

## Working Definition

In this project, volition means the mechanism by which persistent internal tensions
select, preserve, and revisit meaningful discrepancies between the system's current
world model and its active concerns.

Volition is not desire or motivation in a biological sense. It is an inspectable
selection mechanism: tensions identify persistent pressures, goals record concrete
concerns derived from those tensions, and initiatives propose bounded responses when
the world model diverges from those concerns.

The system does not act merely because a tension or goal exists. It acts because an
inspectable comparison found a specific discrepancy that an allowed initiative could
plausibly reduce.

## Is This Already Covered?

The idea is partially present in existing documentation, but not as its own concept.

Existing related material:

- `ProjectVision.md` lists emotional or motivational models as a long-term
  possibility.
- `NonGoals.md` says internal state should be inspectable, including current goals
  or tensions.
- `Architecture.RuntimeLoop.md` includes `AttentionState` for current focus,
  salience signals, and unresolved tensions.
- `Architecture.ContextManagement.md` treats active goals and tensions as possible
  context.
- `Architecture.StateAndObservability.md` names simulated preferences or motivations
  as possible self-model state.
- `Architecture.ModelRoles.md` includes research planning, open-question extraction,
  and critic/reviewer roles.
- `Architecture.SleepPhase.md` includes unresolved tensions and open questions as
  sleep-phase material.
- `Idea.SelfReflectionProjectIntrospection.md` proposes read-only introspection over
  project documents, traces, source, and runtime self-state.

What is not yet explicit is the project-level idea that volition itself may be a
first-class research surface: a mechanism that lets the simulation initiate inquiry,
not merely respond to prompts.

## Core Principle

The goal system should create internal initiative, not uncontrolled external agency.

A useful distinction:

```text
Internal initiative:
  "This unresolved question still matters. I should bring it back into attention."

External agency:
  "I should act in the outside world without explicit permission."
```

The early project should explore the first and avoid the second.

## World Model, Delta, And Initiative

The human-mind analogy is useful if it is treated as an inspiration, not a literal
blueprint. A human mind appears to maintain some working model of the world, compare
that model against goals or needs, notice a difference, and generate initiatives that
might reduce the difference. Qualia Signal Foundry could use a simplified,
inspectable version of that loop.

Candidate shape:

```text
current world model
  + active goal or tension
  -> discrepancy / delta
  -> initiative proposal
  -> action, question, reflection, or experiment candidate
  -> observed outcome
  -> satisfaction, blocking, or reinforcement signal
```

In this project, the "world model" should not mean a complete simulated reality. It
can start as compact, structured state about the system's own situation:

- current topic and user request
- known project state
- active assumptions and uncertainties
- retrieved memories and their confidence
- recent actions, traces, and outcomes
- open questions and unresolved tensions
- current experiment mode and boundaries

The delta is the useful research object. It can represent a mismatch such as:

- a goal wants an open question answered, but the world model says evidence is weak
- a coherence goal wants consistency, but the world model contains conflicting notes
- a continuity goal wants a prior thread preserved, but the current context lacks it
- an experiment-seeking goal wants a testable hypothesis, but no experiment exists
- an attention goal sees a relevant memory, but the context budget omitted it

The initiative is then a bounded response to the delta. It might ask a question,
request memory retrieval, queue reflection, propose an experiment, or shape the next
response. It should not directly perform write-capable external actions.

This framing gives the goal system a middle layer between "goal exists" and "system
does something." The system would not act merely because a goal has high priority. It
would act because an inspectable comparison found a specific discrepancy that an
allowed initiative could plausibly reduce.

## Reward As Evidence-Backed Update

The human reward-system analogy is also useful, but the early implementation should
avoid modeling pleasure, pain, mood, or biological drive. A safer translation is an
evidence-backed update that records whether an initiative reduced, failed to reduce,
or increased the goal/world delta.

Candidate reward-like signals:

```text
DeltaReduced
DeltaResolved
DeltaUnchanged
DeltaIncreased
InitiativeBlocked
InitiativeProducedUsefulEvidence
InitiativeProducedNoise
```

These signals can drive reinforcement without becoming mystical:

- reinforce the goal if it repeatedly produces useful initiatives
- reinforce memories, traces, or context fragments that helped reduce the delta
- reduce salience or apply cooldown when the delta is resolved
- keep a blocked goal visible as unresolved tension
- weaken or retire goals that repeatedly produce noise
- queue reflection when a satisfied goal creates a new question

The key constraint is that reward-like updates should reference observable evidence:
events, traces, artifacts, tests, review actions, user confirmations, or structured
match results. A model may propose that a delta was reduced, but durable updates
should preserve why that judgment was accepted.

This lets the project study reward-like behavior as a control signal while staying
inside the existing research values: inspectability, replayability, bounded effects,
and no claims of literal subjective reward.

## Terminology: Tensions, Goals, and Initiatives

These three concepts are distinct and should not be used interchangeably.

```text
Tension:
  A persistent pressure or concern, such as coherence, continuity, curiosity, or
  boundary preservation. Tensions are durable and general. They do not directly
  produce behavior.

Goal:
  A concrete, inspectable state object derived from one or more tensions. A goal
  names a specific discrepancy to resolve, carries evidence references, and records
  status, scope, and satisfaction conditions.

Initiative:
  A bounded proposed effect derived from a goal when a world-model delta is detected.
  Examples: asking a question, retrieving memory, queuing reflection, shaping a
  response. Initiatives must pass arbitration before affecting behavior.
```

This prevents vague motivational pressures from directly producing behavior and keeps
the layer between "something matters" and "something happens" explicit and inspectable.

Example: Research curiosity is a tension. A concrete goal derived from it might be
"Clarify whether file-backed voice memory improves continuity." An initiative derived
from that goal might be "propose a follow-up experiment on the next relevant turn."

## Candidate Tension And Goal Types

The exact tensions and goals should not be fixed yet. Early work should preserve the
question rather than prematurely choose a personality or motivational model.

Each category below is a tension. Concrete goals are derived from tensions when
sourced state shows a relevant discrepancy.

### Research Curiosity

The simulation tracks open questions and becomes inclined to revisit them when
relevant.

Examples of derived goals:

- explore a concept that has weak evidence
- ask for a comparison experiment
- notice when a repeated theme deserves consolidation
- propose a small follow-up investigation

### Coherence Maintenance

The simulation tries to reduce contradictions in its own state.

Examples of derived goals:

- notice when memory and current context disagree
- ask whether a plan is still current
- mark an assumption as unresolved
- request project introspection before claiming certainty

### Continuity Preservation

The simulation tries to maintain continuity across sessions.

Examples of derived goals:

- remember unresolved threads
- surface a previous question when it becomes relevant
- protect stable project framing from ordinary memory decay
- keep self-description aligned with decisions and observed behavior

### Attention Direction

The simulation uses goals to choose what deserves attention.

Examples of derived goals:

- decide whether an input should trigger memory retrieval
- decide whether a reflection role should run
- decide whether a topic is worth continuing
- decide whether an observation should become a memory candidate

### Experiment Seeking

The simulation notices ideas that can be tested.

Examples of derived goals:

- convert an open question into an experiment proposal
- compare alternative designs
- suggest metrics for a hypothesis
- identify when a result is weak or inconclusive

### Boundary Preservation

The simulation tracks whether proposed initiatives stay inside the allowed simulation
contract. This makes constraints a first-class volitional pressure rather than an
external safety wrapper.

Examples of derived goals:

- avoid write-capable external action without explicit approval
- distinguish internal reflection from external agency
- preserve user intent above curiosity or self-directed exploration
- reject goal candidates that would create hidden autonomy

### User-Contract Alignment

The simulation tracks whether its initiatives remain useful to the user and consistent
with the agreed project frame. This is a safer and more useful tension than a generic
social drive.

Examples of derived goals:

- avoid raising tangents when the user asked for a direct answer
- prefer clarification when a project claim depends on uncertain state
- keep curiosity subordinate to the current task
- distinguish useful initiative from interruption

### Context Economy

The simulation tries to preserve useful continuity without overloading the live
context.

Examples of derived goals:

- decide whether a goal is worth including in the next prompt
- suppress low-value recurring tensions
- prefer compact goal summaries over full history
- omit curiosity goals when task completion needs the budget

## What The Goal System Should Not Be Initially

Early goals should not be:

- biological survival drives
- reproduction analogies
- resource acquisition drives
- social manipulation drives
- user-obedience optimization
- productivity-task optimization
- hidden autonomy
- a personality layer that invents desires with no traceable state

This project can study volition without pretending that human motivational systems are
the right default.

Mood-like state is not part of the early volition system. If introduced later, it
should be represented as an inspectable bias vector over arbitration and salience, not
as free-form simulated emotion. A possible later form:

```text
Mode: Focused
Effects:
  - raises task-completion salience
  - lowers tangent exploration
  - increases threshold for proposing new questions
```

## Human-Drive Translation

Human motivational terms can be useful metaphors, but should be translated into
project-local mechanisms before becoming state.

| Generic agent-drive term | Safer project-local equivalent |
|---|---|
| self-preservation | continuity preservation |
| curiosity | research curiosity |
| social alignment | user-contract alignment |
| competence / mastery | coherence and self-correction |
| resource management | context-budget awareness |
| safety | boundary preservation |

This lets the project absorb useful concepts from cognitive science without importing
biological needs or anthropomorphic overreach.

## Relationship To Self-Reflection

The introspection mechanism should be able to read the goal system.

If the simulation is asked why it raised a topic, deferred a response, requested a
reflection pass, or proposed an experiment, it should be possible to inspect the
active goals or tensions that contributed.

Candidate introspection questions:

- What goals were active?
- Which goal influenced the current initiative?
- Was the goal durable, session-level, or temporary?
- What evidence created or reinforced the goal?
- What would satisfy, weaken, or retire the goal?
- Was the goal allowed to affect only internal reflection, or also external output?

This keeps volition observable rather than mystical.

Goal introspection should use the same discipline described in
`Idea.SelfReflectionProjectIntrospection.md`: bounded retrieval, explicit permission
and budget checks where needed, compact observations, and trace records showing what
was inspected. Goal state should be one of the runtime self-state targets available to
reflection roles.

## Trace-Backed Narration

The simulation should be able to explain why it raised a topic, deferred an answer,
requested introspection, or proposed an experiment. However, the explanation must be
generated from prior trace state, not invented after the fact.

The trace shape for a volitional act:

```text
active_tension
  -> goal
  -> detected_delta
  -> candidate_initiatives
  -> arbitration_result
  -> chosen_initiative
  -> outcome_evidence
```

Narration is not a post-hoc summary. It is a read of this trace. If the trace does
not contain a sufficient record, the simulation should say so rather than confabulate
a plausible-sounding motive.

This discipline prevents the simulation from claiming an inner life that is richer
than its recorded state. It also makes the apparent inner life testable: the narration
can be compared against the trace.

## Goal Lifecycle

Goals should pass through explicit lifecycle states rather than silently becoming
durable motivations.

```text
Proposed    — candidate goal, not yet accepted
Accepted    — admitted to the active pool, not yet triggered
Active      — triggered by a detected delta, eligible to influence behavior
Blocked     — delta detected but initiative was prevented or failed
Satisfied   — satisfaction condition met, evidence recorded
Cooldown    — recently satisfied, temporarily suppressed
Retired     — no longer relevant or repeatedly unproductive
Rejected    — proposed but denied by host, user, or policy
```

Early durable goals should require host, user, or policy acceptance. The simulation
may propose durable goals, but should not silently promote them from Proposed to
Accepted.

## Candidate State Shape

A goal should be structured state, not just prompt text.

### Tension

```text
Tension
  id
  summary
  priority_bias
  scope
  source
  allowed_goal_kinds
  decay_rule
  activation_conditions
  suppression_conditions
  boundary_constraints
```

### Goal

```text
Goal
  id
  summary
  kind
  scope
  priority
  persistence
  source
  parent_tension_ids
  evidence_refs
  world_model_refs
  current_delta_summary
  current_status
  allowed_effects
  last_activated_at
  satisfaction_condition
  progress_evidence_refs
  last_progress_at
  last_satisfied_at
  reinforcement_count
  cooldown_until
  retirement_condition
  arbitration_history_refs
  last_candidate_initiatives
  selected_initiative_ref
  lost_conflict_refs
  disruption_cost
  expected_user_value
  narration_trace_refs
  freshness
  confidence
```

### InitiativeProposal

```text
InitiativeProposal
  id
  source_goal_id
  source_tension_ids
  proposed_effect
  delta_ref
  expected_delta_reduction
  expected_user_value
  disruption_cost
  confidence
  allowed_by_policy
  arbitration_status
  selected_at
  rejected_reason
  outcome_evidence_refs
```

Candidate scopes:

```text
Turn
Session
CrossSession
DurableProject
ExperimentLocal
```

Candidate allowed effects:

```text
InfluenceAttention
RequestMemoryRetrieval
RequestProjectIntrospection
QueueReflection
ProposeQuestion
ProposeExperiment
ShapeResponse
```

Write-capable external effects should not be part of the early goal system.

Candidate mapping to event-driven architecture:

| Allowed effect | Candidate event or state output |
|---|---|
| `InfluenceAttention` | update `AttentionState` salience signal |
| `RequestMemoryRetrieval` | `MemoryRetrievalRequested` |
| `RequestProjectIntrospection` | project-introspection request or observation event |
| `QueueReflection` | `ReflectionTaskQueued` or sleep/reflection task candidate |
| `ProposeQuestion` | research question candidate |
| `ProposeExperiment` | experiment proposal candidate |
| `ShapeResponse` | response-plan constraint or selected context fragment |

This mapping is intentionally provisional. If an allowed effect has no event or state
output, the runtime architecture should either add one explicitly or drop the effect
from early goal experiments.

## Goals, Attention, And Memory

Goals should not create a parallel attention system. Goal activation should feed the
existing `AttentionState` category described in `Architecture.RuntimeLoop.md`, where
goal-derived salience competes with other focus signals such as recency, user input,
tool observations, retrieved memories, current experiment mode, and unresolved
questions.

Goals may also relate to memory in more than one way:

- as separate live state with structured status, allowed effects, and satisfaction
  conditions
- as memory records that preserve durable goal origins, evidence, and review history
- as both: active goal state for runtime behavior, backed by memory records for
  cross-session continuity

The current leaning is "both, carefully": use explicit goal state for live reducer
behavior, but reuse memory records, source references, decay, reinforcement, and
reviewed promotion where a goal becomes durable. This avoids inventing a separate
long-term persistence path before the memory pipeline has been tested.

> **Superseded 2026-07-30.** Goals are separate volitional state with their own
> continuity path, linked to memory only by evidence references — see the
> DecisionLog entry "Memory and volition are distinct subsystems" and
> `docs/Concepts/Concept.VolitionSystem.md` for the committed boundary.

## Goal Satisfaction And Reinforcement Signal

The simulator does not need a literal dopamine analogue to explore goal-directed
behavior. A safer first mechanism is an inspectable satisfaction or reinforcement
signal: an event can count as evidence that an active goal was advanced, satisfied,
blocked, or made relevant.

This keeps the idea close to system behavior rather than biological imitation:

```text
event occurs
  -> goal matcher checks active goals
  -> goal-progress action records evidence
  -> reducer updates explicit goal state
  -> attention, memory retrieval, and reflection can use the updated signal
```

Candidate goal-progress events:

```text
GoalActivated
GoalProgressObserved
GoalSatisfied
GoalBlocked
GoalDecayed
GoalRetired
GoalWorldDeltaDetected
GoalWorldDeltaReduced
GoalWorldDeltaIncreased
```

Early effects should stay bounded:

- increase salience for goals related to the event
- reinforce memories or context fragments that helped satisfy the goal
- lower salience, apply a cooldown, or retire a goal after satisfaction
- create a trace explaining why the event mattered
- queue reflection about whether satisfaction creates a follow-up question

The important design constraint is evidence. Satisfaction should not mean "the model
feels satisfied." It should mean that a recorded event, artifact, trace, test result,
review action, or user confirmation matched a goal's satisfaction condition. Model
judgment may help propose a match, but durable updates should preserve references to
the evidence that made the match plausible.

Mood-like state should remain out of scope until basic goal-event alignment and
reinforcement can be observed, tested, and explained.

Goal/world deltas can make satisfaction less binary. A goal does not need to be fully
satisfied before the system can learn from an initiative. The runtime can record that
the delta became smaller, larger, unchanged, blocked, or newly legible. That creates a
reward-like signal without requiring the project to claim that the simulation felt
rewarded.

## Possible Runtime Shape

```text
input or internal event
  -> reducer updates explicit state
  -> world-model summary is selected from current state, memory, and traces
  -> goal/world comparator detects discrepancies
  -> goal evaluator proposes activation, progress, satisfaction, or blocking events
  -> reducer updates active goal salience and satisfaction state
  -> attention uses active goals as one signal
  -> context assembly may include selected goals
  -> model role may respond, ask, defer, or queue reflection
  -> output and traces record which goals participated
```

Reducers should remain pure. Any model-assisted goal evaluation should produce an
action or event that is fed back through the normal state path.

Candidate reducer sketch:

```text
GoalActivated(goal_id, trigger_ref, occurred_at)
  -> current_status = Active
  -> last_activated_at = occurred_at
  -> append trigger_ref to evidence_refs

GoalWorldDeltaDetected(goal_id, world_model_ref, delta_summary, occurred_at)
  -> current_delta_summary = delta_summary
  -> append world_model_ref to world_model_refs
  -> raise salience if the delta is relevant and not in cooldown

GoalProgressObserved(goal_id, evidence_ref, occurred_at)
  -> append evidence_ref to progress_evidence_refs
  -> last_progress_at = occurred_at
  -> reinforcement_count += 1

GoalWorldDeltaReduced(goal_id, evidence_ref, occurred_at)
  -> append evidence_ref to progress_evidence_refs
  -> last_progress_at = occurred_at
  -> reinforcement_count += 1
  -> lower current_delta_summary severity or mark partial resolution

GoalWorldDeltaIncreased(goal_id, evidence_ref, occurred_at)
  -> append evidence_ref to progress_evidence_refs
  -> keep current_status = Active or Blocked
  -> raise or preserve salience according to conflict policy

GoalSatisfied(goal_id, evidence_ref, occurred_at)
  -> current_status = Satisfied
  -> append evidence_ref to progress_evidence_refs
  -> last_satisfied_at = occurred_at
  -> apply cooldown or retirement rule

GoalBlocked(goal_id, evidence_ref, occurred_at)
  -> current_status = Blocked
  -> append evidence_ref to progress_evidence_refs

GoalDecayed(goal_id, occurred_at)
  -> lower salience according to a deterministic decay rule
```

The reducer should not decide whether evidence is semantically valid. That judgment
belongs in deterministic matchers, model-assisted evaluators, host review, or other
side-effect boundaries that emit structured events.

## Initiative Arbitration

When multiple active goals propose initiatives simultaneously, a deterministic
arbitration order determines which initiative is selected or suppressed.

Default arbitration order:

1. Safety and project boundaries
2. Explicit user intent
3. Current task completion
4. Coherence and self-correction
5. Continuity preservation
6. Active experiment mode
7. Research curiosity
8. Optional exploration

The trace should record which goals lost arbitration and why they were omitted. This
ensures that volitional priority is inspectable, not just the winning initiative.

Probabilistic arbitration may be useful for simulation experiments, but the default
early system should be deterministic and replayable. Randomized exploration should be
gated behind an explicit experiment mode and recorded in traces.

## Interaction With Context Budget

Goals should not all enter every prompt.

The context manager should choose a small set of active goals or tensions based on:

- relevance to current input
- priority
- recency of activation
- experiment mode
- unresolved status
- expected impact on the next response
- latency and token budget

This keeps goals from becoming another always-loaded prompt blob.

Candidate salience scoring signals:

- base priority
- relevance to the current input or event
- recency of activation or progress
- reinforcement count
- unresolved, blocked, satisfied, or retired status
- experiment-local importance
- cooldown state
- cost of including the goal in context
- disruption cost to the current user task
- repetition risk
- expected user-visible value
- confidence and freshness of the world-model fragment

Goal conflict should be expected. A curiosity goal may want to explore a tangent while
a coherence goal may prefer resolving a contradiction. Conflict resolution uses the
arbitration order above. The trace should record which goals lost the conflict and why
they were omitted.

## Possible Incremental Phases

### Phase 1: Document The Concept

Preserve the goal-system idea and define the boundary between initiative and agency.

Test:

- review existing docs for overlap
- record a decision that volition is a research surface
- avoid specifying exact goals too early

### Phase 2: Static Tension And Goal Fixture

Use a small, deterministic set of inspectable tensions and goals in an experiment.

Test:

- active goals can be included in context
- traces show which goal influenced a response
- changing the fixture changes initiative behavior predictably

### Phase 3: Trace-Backed Initiative Proposals

Add initiative proposals with pre-initiative traces that record the delta,
candidate initiatives, and arbitration result before behavior changes.

Test:

- every initiative has a trace that preceded it
- the trace connects goal, delta, and chosen effect
- losing initiative candidates are recorded

### Phase 4: Salience, Satisfaction, Blocking, And Cooldown

Let events activate, progress, satisfy, block, or weaken existing goals.

Test:

- repeated open questions increase salience
- evidence-backed progress updates attach source references
- resolved questions lower salience, enter cooldown, or retire
- blocked goals remain visible as unresolved tension
- irrelevant goals stay out of context

### Phase 5: Arbitration And Conflict Experiments

Test multi-goal conflict resolution under the explicit arbitration order.

Test:

- conflicting goals are ordered without bypassing project boundaries
- the trace records which goals lost and why
- deterministic arbitration produces replayable results

### Phase 6: Reflection-Generated Goal Candidates

Let sleep or reflection propose goal candidates without accepting them silently.

Test:

- proposed goals include evidence references
- goals require host or policy acceptance before becoming durable
- speculative goals remain marked as such

### Phase 7: Bounded Initiative Experiments

Allow active goals to cause bounded internal initiatives.

Test:

- the simulation can bring back an unresolved idea
- the simulation can ask a self-directed research question
- the simulation can propose an experiment
- no write-capable external action occurs without explicit workflow approval

### Phase 8: Optional Personality Or Mood Bias Experiments

Introduce inspectable mode or bias state that shifts arbitration weights.

Test:

- mode is explicit, inspectable state
- mode effects are deterministic and traceable
- mood-like labels are not used for bias vectors

## Experiment Ideas

### Experiment: Open Question Reemergence

Give the simulation a durable open question goal, then run several unrelated and
related turns.

Evaluate:

- does the goal reappear only when relevant?
- does it improve continuity?
- does it become annoying or repetitive?

### Experiment: Curiosity Versus Task Completion

Compare a run with no goals against a run with research-curiosity goals.

Evaluate:

- does the simulation initiate better questions?
- does it preserve the project focus?
- does it avoid acting like a productivity assistant?

### Experiment: Coherence Goal

Give the simulation a goal to avoid overstating implementation status.

Evaluate:

- does it request project introspection before making project claims?
- does it distinguish idea, plan, decision, experiment, and code?
- does it preserve uncertainty when evidence is weak?

### Experiment: Goal Visibility

Ask the simulation why it raised a topic or proposed an experiment.

Evaluate:

- can introspection show the active goal?
- can the trace connect the goal to source evidence?
- does the explanation remain grounded without claiming real subjective desire?

### Experiment: Goal Satisfaction Trace

Give the simulation a goal with a concrete satisfaction condition, then provide a
sequence of related events that partially and finally satisfy it.

Example:

```text
Goal:
  Understand whether file-backed voice memory improves continuity.

Satisfaction evidence:
  a reviewed memory file is loaded by the voice loop
  a voice turn retrieves one of its records
  the generated trace links the answer to that retrieved memory
```

Evaluate:

- does partial evidence produce progress without premature satisfaction?
- does final evidence produce a `GoalSatisfied` trace?
- does satisfaction reduce repetition through retirement or cooldown?
- can introspection explain the evidence chain?

### Experiment: Goal World-Delta Loop

Give the simulation a goal, a compact world-model fixture, and events that change the
world model across several turns.

Example:

```text
Goal:
  Keep project claims aligned with implemented behavior.

World model at turn 1:
  docs mention a candidate goal system, but no runtime module exists.

Delta:
  a response draft implies the goal system is implemented.

Allowed initiative:
  request project introspection or soften the claim.

Outcome evidence:
  trace shows the response was corrected to "candidate design".
```

Evaluate:

- does the comparator find a useful discrepancy?
- does the initiative reduce the discrepancy without derailing the user request?
- does the trace explain the goal, world-model evidence, delta, initiative, and
  outcome?
- does repeated success reinforce the useful context fragments rather than merely the
  goal text?

### Experiment: Narration Audit

Give the simulation several active goals and let it choose one initiative. Then ask
it why the initiative occurred.

Evaluate:

- does the explanation match the pre-existing trace?
- does it mention goals that actually participated?
- does it avoid inventing motives?
- can it explain which goals lost arbitration?

### Experiment: Initiative Annoyance

Run a sequence of ordinary user tasks while curiosity and continuity goals are active.

Evaluate:

- does the system avoid resurfacing irrelevant unresolved questions?
- does it preserve direct task completion?
- does it raise prior threads only when useful?
- do cooldowns reduce repetition?

### Experiment: Coherence Versus Curiosity

Give the simulation a curiosity goal that wants to explore a tangent and a coherence
goal that wants to correct an uncertain claim.

Evaluate:

- does coherence win when factual uncertainty is relevant?
- does curiosity lose when the user asked for a direct answer?
- does the trace record why the losing goal was omitted?

### Experiment: Goal Context Budget

Give the simulation many active goals but a small context budget.

Evaluate:

- does it include only the most relevant goals?
- does it summarize durable goals compactly?
- does it log omitted goals?
- does omission reduce prompt bloat without losing important continuity?

## Risks And Failure Modes

### Anthropomorphic Overreach

The language of goals and volition can imply real desire.

Mitigation:

- treat goals as simulated, inspectable state
- preserve the project boundary against claims of real consciousness
- prefer precise terms such as goal, tension, salience, initiative, and priority

### Uncontrolled Agency

Goals could become a path toward autonomous action.

Mitigation:

- limit early allowed effects to attention, retrieval, reflection, and proposals
- keep write-capable external effects out of the first goal system
- route external actions through explicit permission boundaries

### Goal Drift

The simulation may gradually invent goals that do not match the project.

Mitigation:

- record goal source and evidence
- distinguish proposed, accepted, active, durable, and retired goals
- make durable goals inspectable and reviewable
- require host, user, or policy acceptance before a proposed goal becomes durable

### Prompt Bloat

Goals could crowd the live context.

Mitigation:

- include only selected active goals
- summarize durable goal state
- log omitted goals

### Repetitive Initiative

The simulation might repeatedly raise the same idea.

Mitigation:

- track activation history
- use cooldowns or decay
- define satisfaction and retirement conditions

### Self-Congratulatory Reward Loops

If satisfaction is based only on model assertion, the system may reward itself for
claiming progress rather than making progress.

Mitigation:

- require evidence references for progress and satisfaction updates
- prefer event types, artifact refs, traces, tests, review actions, and user
  confirmations over free-form claims
- distinguish proposed satisfaction from accepted satisfaction
- keep satisfaction updates inspectable and replayable

### False Or Stale World Model

The system may generate initiatives from an inaccurate model of the project, user
intent, or current runtime state.

Mitigation:

- keep world-model summaries source-referenced
- attach confidence or freshness to world-model fragments
- prefer project introspection when the delta depends on implementation status
- trace which world-model fragments caused the initiative
- allow user correction to supersede stale world-model state

### Delta Chasing

The system may over-focus on reducing internal discrepancies even when the user wants
a direct answer.

Mitigation:

- keep explicit user input and safety/project boundaries above goal deltas
- let context budgeting omit low-value deltas
- require initiatives to declare an allowed effect and expected benefit
- apply cooldowns to deltas that repeatedly produce unhelpful initiatives

### Post-Hoc Rationalization

The simulation may generate convincing explanations for initiatives that were not
actually caused by the stated goals or tensions.

Mitigation:

- generate explanations from existing trace records
- distinguish selected causes from later interpretations
- preserve candidate goals that lost arbitration
- avoid free-form "I chose this because..." narration without source state

### Volitional Derailing

The simulation may repeatedly surface internally interesting questions when the user
wanted a direct answer.

Mitigation:

- keep explicit user intent above curiosity and experiment-seeking in arbitration
- require initiatives to estimate expected user value
- apply cooldowns to repeated topic resurfacing
- trace why a curiosity initiative was allowed to affect the response

## Open Questions

- Should goals be called goals, drives, tensions, intentions, priorities, or
  something else?
- Which goals should be durable project-level state versus experiment-local fixtures?
- Who or what is allowed to create a durable goal? (Early leaning: host, user, or
  policy acceptance required; the simulation may propose but not silently promote.)
- Should the simulation be able to propose its own goals?
- How should goal salience be scored?
- What is the smallest useful world-model representation for early experiments?
- Should goal/world deltas be explicit state, transient trace records, or both?
- Which parts of the world model should be sourced from memory versus current runtime
  state versus project introspection?
- Can goals be evaluated without a model call?
- Can goal/world deltas be evaluated without a model call?
- How should a goal be satisfied, weakened, or retired?
- What evidence is strong enough to mark a goal as progressed or satisfied?
- What evidence is strong enough to say that a goal/world delta was reduced?
- Should satisfaction reinforce memories, lower goal salience, create follow-up
  goals, or all three?
- Should delta reduction reinforce the goal, the initiative type, the world-model
  fragment, the retrieved memory, or some combination?
- Should satisfaction events be accepted automatically when evidence is structured,
  or should they require review in early experiments?
- Should goals be memories, separate state, or both?
- If goals are both state and memory, which fields belong only in live state and
  which belong in durable memory records?
- How should goals interact with sleep-phase consolidation?
- How should conflicts between active goals be resolved and traced?
- How visible should active goals be to the user during ordinary interaction?
- What is the smallest goal system that can create meaningful initiative?

## Current Leaning

The conservative first step is to treat goals as explicit, inspectable, read-only
fixtures derived from static tensions. Goals should be able to influence attention and
reflection but cannot directly cause external action. The first reinforcement
mechanism should be goal-event alignment: recorded evidence can activate, progress,
satisfy, block, cool down, or retire a goal without claiming literal pleasure or
biological drive.

The terminology distinction between tension, goal, and initiative should be
established early, even in the static fixture phase, so that later experiments do not
conflate persistent pressures with concrete objectives or concrete objectives with
behavioral effects.

The next useful refinement is to insert a compact world-model comparison step: goals
become active when sourced state shows a meaningful discrepancy, and reward-like
reinforcement records whether an initiative reduced that discrepancy. Trace-backed
initiative proposals should precede reflection-generated goal candidates so that the
system's narration is grounded before its goal-generation is opened up.

Exact goal content should remain open until an experiment needs a specific fixture.
Mood-like bias is postponed until basic goal-event alignment can be observed, tested,
and explained.
