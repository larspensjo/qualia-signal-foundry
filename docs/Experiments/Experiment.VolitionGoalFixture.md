# Experiment: Volition Goal Fixture

## Experiment ID

`Experiment.VolitionGoalFixture`

## Status

Planned.

This is the first build slice of the volition/goal system: the static tension and goal
fixture. The overall sequencing lives in
[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md); the rationale,
terminology, and candidate state shapes live in
[Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md). The concept is
already captured and recorded as a research surface in
[DecisionLog.md](../DecisionLog.md) (2026-05-15 "Volition is an explicit research
surface").

## Summary

This experiment introduces a small, deterministic, read-only fixture of tensions and
goals and tests whether the system can select a relevant subset of goals for a given
input and explain, through traces, which goal would influence behavior.

The experiment does not let goals cause any effect. It only demonstrates that the
three-layer terminology — **tension → goal → initiative** — can be represented as
inspectable state, that goal selection is deterministic and budget-bounded, and that
changing the fixture changes which initiatives are proposed in a predictable way.

This is the volition analogue of the memory toy model: just as
`Experiment.AssociativeMemoryToyModel` seeds a static `phase_four_fixture()` and runs
deterministic retrieval against it, this experiment seeds a static goal fixture and
runs deterministic goal selection against scripted inputs.

## Motivation

The volition idea is documented but has no code. Before goals can ever influence
attention, reflection, or proposals, the project needs the smallest inspectable
representation of goals that:

- establishes the tension/goal/initiative distinction in code from the start, so
  later work does not conflate persistent pressures with concrete objectives or with
  behavioral effects;
- proves that goal selection can be deterministic, budget-bounded, and replayable;
- produces traces that connect an input to the goal(s) it activated and the
  initiative(s) those goals would propose;
- does all of this without any model call and without any external agency.

This experiment reduces uncertainty around:

- what the minimal `Tension`, `Goal`, and `InitiativeProposal` state shapes should be;
- how goal relevance should be scored against an input;
- how goals should compete for a small context budget (reusing `ContextBudget` and
  `assemble_context`);
- what a "candidate initiative" looks like before any initiative can be executed;
- whether perturbing the fixture changes selection predictably enough to be a useful
  research surface.

If the experiment succeeds, it gives the later volition slices (salience/satisfaction,
arbitration, reflection-generated goals, bounded initiatives) a trustworthy, testable
foundation. If it fails — for example, if deterministic relevance scoring is too
brittle to be useful — that is valuable negative evidence that arrives before any
behavioral coupling exists.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Plans/Idea.SelfReflectionProjectIntrospection.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
ProjectFrame/NonGoals.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

## Hypothesis

A small static fixture of tensions and goals can be represented as pure, inspectable
state and can deterministically select a budget-bounded, input-relevant subset of
goals, each producing a candidate initiative proposal, such that changing the fixture
changes the proposed initiatives in a predictable, trace-explained way — without any
model call and without executing any effect.

## Scope

### In Scope

- Pure data types: `Tension`, `Goal`, `GoalStatus`, `GoalScope`, `AllowedEffect`,
  `InitiativeProposal`.
- A static, hand-authored fixture of a few tensions and goals derived from them.
- A pure, deterministic goal selector: input + goal pool + budget → selected goals
  (with relevance rationale) and omitted goals (with skip reason).
- Candidate `InitiativeProposal` generation for each selected goal (proposed only,
  never executed).
- Reuse of the existing `ContextBudget` / `assemble_context` budgeting pattern for
  goal selection.
- Event + trace emission linking input → active goal(s) → candidate initiative(s),
  including omitted goals.
- A small written report artifact.
- A "perturbation" run that changes the fixture and shows selection changes
  predictably.
- Unit tests on the pure selector (relevance ordering, budget enforcement,
  determinism, fixture-perturbation behavior).

### Out of Scope

- Any execution of an initiative (no attention update, no memory retrieval request,
  no reflection queueing, no response shaping).
- Wiring into `AttentionState` or any live loop (that state does not exist yet).
- Event-driven salience, satisfaction, blocking, cooldown, decay, or retirement
  (a later slice).
- Multi-goal arbitration / conflict resolution (a later slice).
- Reflection- or sleep-generated goal candidates (a later slice).
- Any model call or model-assisted goal evaluation.
- World-model delta detection (a later refinement, out of scope here).
- Durable persistence of goals or goal-as-memory storage.
- Mood/personality bias state.

## Setup

- OS/runtime: existing Rust workspace, run via the experiment registry
  (`cargo run -p qsf_app -- experiment volition-goal-fixture` once implemented).
- New pure module (candidate name `volition`) under `crates/qsf_app/src/`, mirroring
  how `memory` exposes a fixture plus pure operations.
- No model provider, no network, no audio.
- Deterministic fixture analogous to `phase_four_fixture()` in
  [memory](../../crates/qsf_app/src/memory/).

Implementation note: goal candidates should reuse `assemble_context` by mapping each
goal to an experiment-local `ContextFragment` with `ContextSourceKind::RuntimeState`.
This keeps the first slice inside the existing serialized context schema; adding a
goal-specific source kind is deferred until goal context proves useful beyond the
fixture experiment.

Candidate fixture shape (illustrative, not final):

```text
Tensions:
  research-curiosity        (priority_bias: medium)
  coherence-maintenance     (priority_bias: high)
  continuity-preservation   (priority_bias: high)
  boundary-preservation     (priority_bias: highest)

Goals (each derived from one or more tensions, status: Accepted):
  clarify-weak-evidence-topic      <- research-curiosity
  avoid-overstating-impl-status    <- coherence-maintenance, boundary-preservation
  resurface-open-thread            <- continuity-preservation
  propose-followup-experiment      <- research-curiosity

Each goal carries:
  base_priority, scope, allowed_effects, activation_keywords,
  satisfaction_condition_summary, evidence_refs
```

Candidate scripted inputs (illustrative):

```text
Input A: "Is the goal system implemented yet?"
  -> expected to activate avoid-overstating-impl-status (coherence/boundary)
Input B: "We never settled how voice memory affects continuity."
  -> expected to activate resurface-open-thread, clarify-weak-evidence-topic
Input C: "Give me the build command."
  -> expected to activate no goals (direct task; nothing relevant in budget)
```

## Procedure

```text
1. Load the static tension/goal fixture; write a fixture snapshot artifact.
2. For each scripted input:
   a. Record an InputReceived-style event.
   b. Run the pure selector against the goal pool under a small goal budget.
   c. Record a selection trace: relevance scores, selected goals, omitted goals
      with skip reasons.
   d. For each selected goal, build a candidate InitiativeProposal (not executed)
      and record it in the trace.
3. Run one perturbation pass: change the fixture (e.g., remove a tension or alter a
   goal's activation keywords) and re-run input B to show selection changes.
4. Write a comparison/summary report.
```

Reducers, if any state transitions are modeled, must stay pure `(State, Event) →
State`; selection itself is a pure selector/view-model, consistent with the repo
convention of keeping view-derivation in pure selectors rather than inline.

## Baseline

`Input C` (a direct task request with no relevant goal) is the standing baseline: it
should activate **no** goals and propose **no** initiatives within budget. This guards
against the failure mode where goals fire on everything. The perturbation run is an
internal baseline-vs-variant comparison for predictability.

## Measurements

### Quantitative Measurements

- number of goals in the pool vs. selected vs. omitted per input
- relevance score per goal per input
- goal budget used vs. available
- selector latency (expected negligible; recorded for the trace model's sake)
- number of candidate initiatives proposed per input

### Qualitative Observations

- Are selected goals actually relevant to the input?
- Does the baseline input correctly select nothing?
- Does the trace clearly connect input → goal → candidate initiative?
- Are omitted goals legible (clear skip reasons)?
- Does the perturbation change selection in the expected direction?
- Is the tension/goal/initiative distinction clear and non-redundant in practice?

## Success Criteria

The experiment is successful if it shows that a static tension/goal fixture can be
represented as inspectable state and that deterministic, budget-bounded goal selection
(a) chooses input-relevant goals, (b) leaves the baseline input with no selected
goals, (c) emits traces that connect input → active goal → candidate initiative
including omitted goals with reasons, and (d) changes selection predictably when the
fixture is perturbed — all without a model call and without executing any effect.

Useful negative results also count as success: e.g., evidence that simple keyword/
priority relevance is too brittle, or that the tension layer adds no value over goals
alone at this scale.

## Failure Criteria

- Selection is non-deterministic or not replayable.
- The baseline input spuriously activates goals.
- Traces cannot explain why a goal was selected or omitted.
- The fixture is so artificial that selection behavior teaches nothing.
- Implementation complexity overwhelms the concept (e.g., the selector grows toward
  arbitration or world-model logic that belongs to later phases).

## Required Observability

This slice should use the existing event surface: record `InputReceived` events for
scripted inputs and manually record `TraceRecorded` events linked to selection traces.
Do not add goal-specific `EventType` variants in this first fixture unless the
implementation shows that trace details cannot make selection legible.

- input events
- the loaded goal/tension fixture snapshot
- per-input relevance scores
- selected goals (with rationale)
- omitted goals (with skip reason)
- candidate initiative proposals (effect kind + source goal + rationale)
- selector latency
- a written report artifact

## Risks and Confounders

- **Overfitting the fixture:** hand-authored goals and keywords may be tuned to make
  selection look good. Mitigation: include the no-match baseline and a perturbation
  run; record this risk in results.
- **Premature mechanism creep:** the selector may drift toward salience, arbitration,
  or world-model deltas. Mitigation: keep those explicitly out of scope; they are
  later slices in the build plan.
- **Anthropomorphic language:** "goal" and "tension" can imply real desire.
  Mitigation: treat all of it as simulated, inspectable state; prefer the terms
  tension, goal, initiative, priority, relevance.
- **Tension layer redundancy:** at this small scale, tensions may seem to add nothing
  over goals. That observation is itself a useful result to record, not a bug to hide.

## Expected Output

- a new `volition-goal-fixture` registered experiment and run directory
- event log
- selection traces
- a goal/tension fixture snapshot artifact
- a comparison/summary report (baseline + perturbation)
- follow-up questions and decision candidates

## Workflow & Documents To Update

Per [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md), this is Stage 5 (a
planned experiment). The following are expected to change as it progresses:

- **Now (planning):** this experiment doc; a backlog entry in
  [Experiment.Backlog.md](Experiment.Backlog.md); the matching slice in
  [Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md).
- **On implementation:** the experiment registry and a new `volition` module (code
  only — no architecture promotion yet).
- **After running:** fill in Results / Interpretation / Follow-Up below.
- **Only if results warrant it:** add an *Implementation Status* note to
  [Architecture.RuntimeLoop.md](../Architecture/Architecture.RuntimeLoop.md) (and/or
  a new volition architecture doc) describing the candidate goal-selection structure,
  and record any promoted decision in [DecisionLog.md](../DecisionLog.md). Do not
  promote architecture or decisions before the experiment produces evidence.

## Results

_To be filled in after running._

### What Happened

### Measurements

### Observations

### Surprises

### Failure Modes

## Interpretation

_To be filled in after running. Separate Observed / Interpreted / Uncertain._

## Follow-Up Questions

- Is deterministic keyword/priority relevance enough, or is a richer match needed?
- Does the tension layer earn its place at this scale, or only once goals are derived
  dynamically?
- What is the smallest useful set of `AllowedEffect` variants to keep before any
  effect is executed?
- Which goals should eventually be durable vs. experiment-local fixtures?

## Follow-Up Experiments

```text
Experiment.VolitionTraceBackedInitiative   (pre-initiative traces)
Experiment.VolitionSalienceAndSatisfaction (event-driven goal state)
Experiment.VolitionArbitrationConflict     (multi-goal conflict order)
```

## Decision Candidates

- Candidate: adopt `tension → goal → initiative` as the standing volition vocabulary
  in code and docs.
- Candidate: reuse the existing `ContextBudget` / `assemble_context` budgeting for
  goal selection rather than building a parallel budgeter.
- Candidate: require a selection trace (selected + omitted + rationale) for every
  volition experiment, mirroring the memory-experiment retrieval-trace convention.
