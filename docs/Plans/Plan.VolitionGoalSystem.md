# Plan: Volition and Goal System

## Status

Active build plan. Phase 1 (document the concept) is **complete**: the idea is
captured in [`Idea.VolitionGoalSystem.md`](Idea.VolitionGoalSystem.md) and the
decision "Volition is an explicit research surface" is recorded in
[`DecisionLog.md`](../DecisionLog.md) (2026-05-15). **Phase 2 (static tension and
goal fixture) is complete**; its validation scaffold is
[`Experiment.VolitionGoalFixture.md`](../Experiments/Experiment.VolitionGoalFixture.md).
**Phase 3 (trace-backed initiative proposals) is complete**; its validation scaffold
is
[`Experiment.VolitionTraceBackedInitiative.md`](../Experiments/Experiment.VolitionTraceBackedInitiative.md).
**Phase 4 (event-driven salience, satisfaction, blocking, cooldown) is now expanded
for implementation** — see its detail below; its validation scaffold
[`Experiment.VolitionSalienceAndSatisfaction.md`](../Experiments/Experiment.VolitionSalienceAndSatisfaction.md)
is a planned skeleton, ready to fill in after a run. Phases 5–8 remain sketched at a
high level until they are ready.

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
| 3 | Trace-backed initiative proposals (pre-initiative traces) — **complete** | Yes | Light | `Experiment.VolitionTraceBackedInitiative` |
| 4 | Event-driven salience, satisfaction, blocking, cooldown — **expanded, ready to build** | Yes | Yes | `Experiment.VolitionSalienceAndSatisfaction` (planned) |
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

### Phase 3 — Trace-backed initiative proposals (complete)

Add a pre-initiative trace recorded *before* any behavior could change, capturing the
active tension, goal, detected delta, candidate initiatives, and local candidate-choice
result. This is not full arbitration; that remains a later slice. Still no effect
execution.

- **Built:** a pure additive trace layer over the Phase 2 selector
  (`build_pre_initiative_traces`) plus `PreInitiativeTrace`, `DeltaAssessment`,
  `DetectedDelta`, `TensionProvenance`, `InitiativeChoice`, and `LosingCandidate`
  types in the `volition` module; a registered `volition-trace-backed-initiative`
  experiment that records one trace per selected goal (and a single explicit no-delta
  trace for the baseline) without changing selection behavior.
- **Resolved open questions:** losing-candidate reasons are deterministic and
  precedence-based (first allowed effect wins; semantic/structured reasons deferred to
  arbitration); delta vs. baseline is modeled as a `DeltaAssessment` enum so the
  no-delta case is type-enforced; tension priority is recorded as provenance only with
  an explicit note that it did not drive selection.
- **Verify:** every proposed initiative has a preceding trace that connects goal →
  delta → chosen effect; losing candidates are recorded; no trace executes an effect.
- Full scope and success/failure criteria live in
  [`Experiment.VolitionTraceBackedInitiative.md`](../Experiments/Experiment.VolitionTraceBackedInitiative.md).

### Phase 4 — Salience, satisfaction, blocking, cooldown (expanded for implementation)

Phases 2–3 are stateless: each scripted input is selected against an immutable fixture,
and the pre-initiative trace is recomputed from scratch. Phase 4 adds the first
*durable-within-a-run* volition state — a pure, replayable layer in which events raise,
lower, satisfy, block, or retire goals across a sequence of turns, so that selection on
a later turn depends on what happened on earlier turns. No effect is executed and no
write-capable external action is added; the only new coupling is that salience and
lifecycle status feed goal *selection* ordering and visibility.

This is the project's first test of the idea doc's reward-as-evidence-backed-update
discipline: satisfaction and progress must reference observable evidence (a source
reference to an event, artifact, or trace), never a model assertion. The reducer stays
pure; the matcher that turns evidence into events stays at the side-effect boundary.

**Build (incremental sub-slices; each builds, tests green, clippy clean, then `fmt`):**

1. *Lifecycle + live state.* Extend `GoalStatus` with the runtime states the idea doc
   names but the fixture does not yet use — `Active`, `Blocked`, `Satisfied` — keeping
   `Proposed`/`Accepted`/`Cooldown`/`Retired` (and update its `Display`). Introduce a
   pure `VolitionState` holding per-goal dynamic state *separate from* the read-only
   fixture: runtime `status`, `salience`, `reinforcement_count`,
   `progress_evidence_refs`, `last_activated_tick`, `last_satisfied_tick`, and
   `cooldown_until_tick`. The fixture stays immutable and is the seed for `Accepted`
   goals. Add a deterministic logical `tick` (a monotonic counter advanced per
   processed event) so decay and cooldown are replayable without wall-clock time.
2. *Volition events + pure reducer.* Define a `VolitionEvent` enum with one variant per
   transition so every lifecycle advance is explicit and replayable: `GoalActivated`,
   `GoalProgressObserved`, `GoalSatisfied`, `GoalBlocked`, `GoalDecayed` (salience-only,
   never a status change), `GoalCooldownElapsed` (`Cooldown -> Accepted`), and
   `GoalRetired` (`-> Retired`). A pure `apply(state, event) -> state` is the only place
   status changes; the selector never mutates lifecycle and there is no hidden tick
   logic. The tick-driven events (`GoalDecayed`, `GoalCooldownElapsed`, `GoalRetired`)
   are emitted deterministically from the tick at the boundary (or by a pure helper
   that, given state plus a new tick, returns the events to apply), never silently
   inside another event. Progress- and satisfaction-bearing events carry an
   `EvidenceRef` — a validated newtype around a non-empty, non-whitespace string with a
   fallible constructor (`EvidenceRef::try_new` / `TryFrom<String>`) used in
   `VolitionEvent`, in `progress_evidence_refs`, and in tests — so an evidence-free
   progress or satisfaction cannot be constructed; a regression test asserts that
   empty/whitespace evidence is rejected. The reducer records evidence and updates
   salience/status; it never judges whether evidence is *semantically* valid (that
   stays in the deterministic matcher at the side-effect boundary).
3. *Salience, decay, cooldown, retirement rules.* Activation and evidence-backed
   progress raise salience; `GoalDecayed` lowers it by a deterministic per-tick rule
   without changing status. `GoalSatisfied` records satisfaction evidence, sets the
   observable snapshot status `Satisfied` at that tick, resets salience, and opens a
   cooldown window (`cooldown_until_tick = tick + span`); the goal then reads as
   `Cooldown` until `GoalCooldownElapsed` returns it to `Accepted`. `GoalBlocked` sets
   `Blocked` but preserves salience so the goal stays a visible unresolved tension. A
   goal that stays unproductive past the retirement threshold receives `GoalRetired`.
   Thresholds must default to values a single standard scripted run actually crosses
   (at least one decay, one cooldown elapse, one retirement).
4. *Salience-aware selection.* Add a pure `select_goals_with_salience(input, fixture,
   &state, budget)` that reuses the Phase 2 relevance scoring, adds a salience term,
   suppresses goals in `Cooldown`, and keeps `Blocked` goals visible (surfaced with a
   distinct blocked reason rather than silently dropped). Keep the existing stateless
   `select_goals` and `build_pre_initiative_traces` untouched so earlier runs stay
   byte-stable; the new selector is the salience entry point.
5. *Validation scaffold.* Register a `volition-salience-and-satisfaction` experiment
   (new `ExperimentName` variant + runner under `crates/qsf_app/src/experiments/`) that
   replays a scripted multi-turn sequence of inputs and events against the fixture,
   snapshots `VolitionState` after each turn, and writes the salience/lifecycle trace.
   Full scope lives in
   [`Experiment.VolitionSalienceAndSatisfaction.md`](../Experiments/Experiment.VolitionSalienceAndSatisfaction.md)
   (a planned skeleton; fill in Results after a run).

- **Verify (automated):** repeated relevant inputs raise a goal's salience
  monotonically before decay, while an irrelevant input leaves salience at zero;
  per-tick decay lowers salience by the deterministic rule; evidence-backed progress
  appends the source ref and increments `reinforcement_count`, and no code path
  satisfies a goal without an evidence ref; a satisfied goal enters `Cooldown`, is
  suppressed from selection for the cooldown span, then becomes selectable again, and
  an unproductive goal retires; a blocked goal keeps `Blocked` status and stays visible
  in the selection output; the same event sequence yields identical `VolitionState`
  snapshots and identical selection output (replay determinism).
- **Verify (human):** read the per-turn salience and lifecycle trace and judge whether
  rising/falling salience, cooldown suppression, and persistent blocked-goal visibility
  feel useful and grounded rather than noisy or nagging — automated tests cannot judge
  "annoying."
- **Default-exercises-new-path:** the scripted experiment drives activation, progress,
  satisfaction, blocking, decay, cooldown, and retirement in one standard run, and the
  decay/cooldown/retirement thresholds default to values that run crosses.

**Open questions to resolve while expanding (leanings noted; confirm before/while
building, per `Agents.md`):**

- *Evidence strength* (carried from the cross-cutting list): is any structured
  `evidence_ref` enough to record progress, and is a deterministic match of the goal's
  satisfaction condition enough to mark `Satisfied`? Leaning: yes for this slice —
  structured evidence auto-records and a deterministic matcher (no model judgment)
  marks satisfaction; host review of satisfaction is deferred to the
  reflection-acceptance slice.
- *Salience representation:* integer points vs float `[0, 1]`. Leaning: integer points,
  to keep decay and replay exact.
- *Decay shape:* linear per-tick decrement vs multiplicative. Leaning: linear,
  integer-friendly.
- *Clock:* per-event tick vs per-input tick. Leaning: per processed event, monotonic,
  no wall-clock.
- *Blocked-goal visibility:* a visible omitted entry tagged blocked vs a dedicated
  carried-tension list. Leaning: a visible omitted entry with a blocked reason, so it
  stays inspectable without consuming response budget.
- *AttentionState wiring:* the idea doc wants goal salience to feed the existing
  `AttentionState`, not a parallel system. Leaning: keep salience inside
  `VolitionState` and feed only selection ordering for now; defer `AttentionState`
  wiring to the bounded-initiative slice so no behavioral coupling is added early.

Runtime names follow stable behavior (`volition` events, salience, cooldown), never a
phase number.

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
  *(Now expanded with proposed leanings in the Phase 4 detail above — plus salience
  representation, decay shape, the logical clock, blocked-goal visibility, and
  AttentionState wiring. Confirm these before building.)*
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
