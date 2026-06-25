# Plan: Volition and Goal System

## Status

Active build plan. Phase 1 (document the concept) is **complete**: the idea is
captured in [`Idea.VolitionGoalSystem.md`](Idea.VolitionGoalSystem.md) and the
decision "Volition is an explicit research surface" is recorded in
[`DecisionLog.md`](../DecisionLog.md) (2026-05-15). **Phase 2 (static tension and
goal fixture) is complete**; its validation scaffold is
[`Experiment.VolitionGoalFixture.md`](../Experiments/Experiment.VolitionGoalFixture.md).
Phase 3 is the next slice to expand immediately before it is executed. Phases 4–8
remain sketched at a high level until they are ready.

> Companion to the idea note
> [`Idea.VolitionGoalSystem.md`](Idea.VolitionGoalSystem.md), which is authoritative
> for the rationale, terminology, candidate state shapes, risks, and open questions.
> This document is the **phased build plan**: it sequences the work into independently
> testable slices and marks where external human verification is recommended.
>
> **Intentionally high-level for future phases.** Each unstarted phase is a
> self-contained slice, not a task-by-task script. Expand a phase into detailed steps
> (file paths, fixtures, tests) immediately before executing it, surfacing that
> phase's open questions first (per `Agents.md`).
>
> Per-phase experiment specs under `docs/Experiments/` are **validation scaffolds**
> for the slices below — they measure a phase, they are not the plan itself.

## Goal

Build a small, inspectable volition/goal mechanism that can create *internal
initiative* (revisiting open questions, requesting reflection, proposing experiments)
without becoming *external agency*, growing it one testable slice at a time so that
behavioral coupling is only added after the inspectable state and traces it depends on
are proven.

The end state is not a personality layer. It is an inspectable selection mechanism in
which **tensions** name persistent pressures, **goals** record concrete concerns, and
**initiatives** are bounded proposed effects that must pass arbitration before
influencing behavior.

## Phasing Principles

- Each phase builds, passes `cargo test`, and is green under
  `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.
- Reducers stay pure and unit-tested; goal/initiative selection lives in pure
  selectors/view-models. Side effects stay at the edge and feed back as events
  (`input -> action -> reducer -> state -> render`).
- A phase that adds a flag or threshold must default to exercising the new path.
- Early phases are read-only: goals may influence attention, retrieval, reflection,
  and proposals, but no write-capable external effect is added until Phase 7, and only
  behind explicit workflow approval.
- "Human test" marks slices where a researcher should manually judge whether the
  behavior is useful, not annoying, and grounded — automated tests cannot cover that.
- Runtime modules are named after stable behavior (`volition`, goal selection,
  arbitration), never after a phase number.

## Phase Overview

| Phase | Slice | Code? | Human test? | Validation scaffold |
|-------|-------|-------|-------------|---------------------|
| 1 | Document the concept; record the research-surface decision — **complete** | No | No | — |
| 2 | Static tension/goal fixture + deterministic, budget-bounded selection — **complete** | Yes | Light | `Experiment.VolitionGoalFixture` |
| 3 | Trace-backed initiative proposals (pre-initiative traces) | Yes | Light | `Experiment.VolitionTraceBackedInitiative` (future) |
| 4 | Event-driven salience, satisfaction, blocking, cooldown | Yes | Yes | `Experiment.VolitionSalienceAndSatisfaction` (future) |
| 5 | Arbitration and multi-goal conflict resolution | Yes | Yes | `Experiment.VolitionArbitrationConflict` (future) |
| 6 | Reflection-generated goal candidates (proposed, not auto-accepted) | Yes | Yes | future |
| 7 | Bounded internal initiative execution | Yes | Yes | future |
| 8 | Optional inspectable mode/bias state | Yes | Yes | future |

## Phase Details

### Phase 1 — Document the concept (complete)

Captured the idea and fixed the boundary between internal initiative and external
agency. Recorded as a research surface in the decision log (2026-05-15). No code.

### Phase 2 — Static tension and goal fixture (complete)

Introduce a pure `volition` module and a hand-authored, read-only fixture of tensions
and goals, plus a deterministic selector that picks a budget-bounded, input-relevant
subset of goals and emits a candidate initiative per selected goal — without executing
any effect and without a model call.

- **Build:** pure `Tension`, `Goal`, `GoalStatus`, `GoalScope`, `AllowedEffect`,
  `InitiativeProposal` types; a static `fixture()`; a pure goal selector reusing the
  existing `ContextBudget` / `assemble_context` budgeting; a `volition-goal-fixture`
  experiment in the registry.
- **Verify (automated):** selector unit tests for relevance ordering, budget
  enforcement, determinism/replayability, and predictable change under fixture
  perturbation; a direct-task baseline input selects no goals.
- **Verify (human, light):** read the selection traces and confirm input → active
  goal → candidate initiative is legible and that omitted goals carry clear reasons.
- **Default-exercises-new-path:** the experiment runs the selector on every scripted
  input by default.
- Full scope, fixture, inputs, and success/failure criteria live in
  [`Experiment.VolitionGoalFixture.md`](../Experiments/Experiment.VolitionGoalFixture.md).

### Phase 3 — Trace-backed initiative proposals

Add a pre-initiative trace recorded *before* any behavior could change, capturing the
active tension, goal, detected delta, candidate initiatives, and arbitration result.
Still no effect execution.

- **Verify:** every proposed initiative has a preceding trace that connects goal →
  delta → chosen effect; losing candidates are recorded.

### Phase 4 — Salience, satisfaction, blocking, cooldown

Let events activate, progress, satisfy, block, or weaken existing goals through pure
reducers. Satisfaction and progress must reference observable evidence, not model
assertion.

- **Verify:** repeated relevant inputs raise salience; evidence-backed progress
  attaches source refs; resolved goals enter cooldown or retire; blocked goals stay
  visible; irrelevant goals stay out of context.

### Phase 5 — Arbitration and conflict resolution

Resolve simultaneous initiative proposals under the deterministic arbitration order
from the idea doc (safety/boundaries → user intent → task → coherence → continuity →
experiment mode → curiosity → optional exploration).

- **Verify:** conflicts are ordered without bypassing project boundaries; the trace
  records which goals lost and why; arbitration is replayable.

### Phase 6 — Reflection-generated goal candidates

Let sleep/reflection propose goal candidates with evidence references, requiring host
or policy acceptance before any goal becomes durable.

- **Verify:** proposed goals carry evidence; nothing is silently promoted from
  Proposed to Accepted; speculative goals stay marked.

### Phase 7 — Bounded internal initiative execution

Allow active goals to cause bounded *internal* effects (bring back an unresolved
thread, ask a self-directed research question, propose an experiment).

- **Verify:** internal initiatives occur and are traced; no write-capable external
  action occurs without explicit workflow approval.

### Phase 8 — Optional mode/bias experiments

Introduce inspectable mode/bias state that shifts arbitration weights deterministically.

- **Verify:** mode is explicit, inspectable, and traceable; effects are deterministic;
  no free-form mood labels drive the bias vector.

## Open Questions To Resolve Before The Affected Phase

These are carried from the idea doc and should be answered when the relevant phase is
expanded, not silently resolved:

- **Phase 2:** Is deterministic keyword/priority relevance enough, or is a richer
  match needed? Does the tension layer earn its place at this small scale?
- **Phase 4:** What evidence is strong enough to mark a goal progressed or satisfied?
  Should satisfaction be auto-accepted when evidence is structured, or reviewed?
- **Phase 5:** Should arbitration ever be probabilistic, and only behind an explicit
  experiment mode?
- **Phase 6:** Who may create a durable goal? (Leaning: host, user, or policy
  acceptance required; the simulation may propose but not silently promote.)
- **Cross-cutting:** Should goals be live state, memory records, or both? Which fields
  belong only in live state vs. durable memory?

## Documents To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md):

- **This plan** as phases start, complete, or change shape.
- **Per-phase experiment specs** under `docs/Experiments/` (validation scaffolds);
  fill in their Results/Interpretation after each run.
- **`Experiment.Backlog.md`** when a future phase's experiment is promoted from idea
  to planned.
- **Architecture docs** (e.g.
  [`Architecture.RuntimeLoop.md`](../Architecture/Architecture.RuntimeLoop.md), or a
  new volition architecture doc) only once a phase produces evidence worth promoting —
  via an *Implementation Status* section, not speculative description.
- **`DecisionLog.md`** when a phase outcome is promoted into an accepted rule.

This plan is ephemeral: when the volition system is built and reflected in architecture
and the decision log, archive this plan rather than citing its phases from durable
documents.
