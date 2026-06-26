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
**Phase 4 (event-driven salience, satisfaction, blocking, cooldown) is complete**;
its validation scaffold
[`Experiment.VolitionSalienceAndSatisfaction.md`](../Experiments/Experiment.VolitionSalienceAndSatisfaction.md)
is implemented and ready to run. **Phase 5 (arbitration and conflict resolution) is
complete**; its design is captured in
[`docs/Plans/Design.VolitionArbitration.md`](Design.VolitionArbitration.md)
and its validation scaffold is
[`Experiment.VolitionArbitrationConflict.md`](../Experiments/Experiment.VolitionArbitrationConflict.md).
Phases 6–8 remain sketched at a high level until they are ready.

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
| 4 | Event-driven salience, satisfaction, blocking, cooldown — **complete** | Yes | Yes | `Experiment.VolitionSalienceAndSatisfaction` |
| 5 | Arbitration and multi-goal conflict resolution — **complete** | Yes | Yes | `Experiment.VolitionArbitrationConflict` |
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

### Phase 4 — Salience, satisfaction, blocking, cooldown (complete)

Added the first durable-within-a-run volition state: a pure, replayable `VolitionState`
holding per-goal `status`, `salience`, `reinforcement_count`, `progress_evidence_refs`,
and cooldown/tick fields, seeded from the immutable fixture. A `VolitionEvent` enum
drives all lifecycle transitions via a pure `apply(state, event) -> state` reducer;
progress and satisfaction events require an `EvidenceRef` (a validated newtype) so
evidence-free updates are structurally impossible. Salience rises on activation and
evidence-backed progress, decays linearly per tick, resets on satisfaction, and is
preserved under blocking. Cooldown suppresses a satisfied goal from selection until
`GoalCooldownElapsed` returns it to `Accepted`; an unproductive goal receives
`GoalRetired`. A `select_goals_with_salience` selector adds the salience term while
keeping blocked goals visible with a distinct reason, and the earlier stateless
selectors are untouched. The `volition-salience-and-satisfaction` experiment replays a
scripted multi-turn sequence and snapshots state after each turn.

- Full scope, inputs, and success/failure criteria live in
  [`Experiment.VolitionSalienceAndSatisfaction.md`](../Experiments/Experiment.VolitionSalienceAndSatisfaction.md).

### Phase 5 — Arbitration and conflict resolution

Add deterministic cross-goal arbitration as a pure, additive layer over Phase 4's
salience-aware selection. When `select_goals_with_salience` returns multiple selected
goals simultaneously, a new `arbitrate()` function picks the winning initiative and
records every losing goal with a tier-based reason. Still no effect execution.

The design decisions for this phase are captured in
[`docs/Plans/Design.VolitionArbitration.md`](Design.VolitionArbitration.md).

#### Data model changes

- Add `arbitration_tier: u8` to `Tension`. Lower tier wins arbitration. The existing
  `priority_bias` field and `TENSION_PRIORITY_NOTE` remain unchanged — arbitration tier
  is a distinct concept from selection weight. Fixture mapping:

  | Tension | `arbitration_tier` |
  |---|---|
  | `boundary-preservation` | 1 |
  | `coherence-maintenance` | 4 |
  | `continuity-preservation` | 5 |
  | `research-curiosity` | 7 |

  Tiers 2 (user intent), 3 (task completion), 6 (experiment mode), and 8 (optional
  exploration) are not yet covered by any fixture tension. Document this as an explicit
  extension point in a `Tension` doc comment and in the experiment spec. Future tensions
  must be assigned their correct tier when added.

- Add two new types:
  - `ArbitrationLoser { selection: GoalSelection, effective_tier: u8,
    effective_tension_id: String, effective_tension_title: String, reason: String }`
    where the structured fields name the tension that placed this goal at its effective
    tier, and `reason` is a rendered convenience string (e.g. "tier 7 lost to winner at
    tier 1 (boundary-preservation)"). Tests must assert the structured fields.
  - `ArbitrationResult { winner: GoalSelection, winner_effective_tier: u8,
    winner_effective_tension_id: String, winner_effective_tension_title: String,
    losers: Vec<ArbitrationLoser> }`

#### Build

A pure `arbitrate(selections: Vec<GoalSelection>, fixture: &VolitionFixture) -> Option<ArbitrationResult>`
function. Returns `None` for empty input. A goal's effective tier equals the minimum
`arbitration_tier` among its parent tensions (default `u8::MAX` if no tensions in the
fixture). Winner = goal with the lowest effective tier. Tiebreaker within the same tier:
higher `base_priority` wins; still tied: lower `goal_id` lexicographically. All existing
selectors and reducers are untouched.

#### Experiment

Register a `volition-arbitration-conflict` experiment (spec:
[`Experiment.VolitionArbitrationConflict.md`](../Experiments/Experiment.VolitionArbitrationConflict.md))
with a scripted multi-turn sequence covering `no_selection`, `single_selection`, and
`conflict_resolved` turns. For each turn, the experiment runner calls
`select_goals_with_salience` → `arbitrate`, records the full `ArbitrationResult`
alongside the selection result, a per-turn `arbitration_status`, and an explicit
no-execution marker. The scripted sequence must include at least one conflict turn that
produces a non-empty `losers` list.

#### Verify (automated)

- Single selection passes through as winner with an empty losers list.
- Two goals at different tiers → lower tier wins.
- Two goals at the same tier → higher `base_priority` wins; still tied → lower `goal_id`
  wins.
- A goal backed by multiple tensions uses the minimum tier (best wins).
- Multiple parent tensions at the same minimum tier → lexicographic `tension_id` picks
  the effective tension.
- Structured provenance fields (`winner_effective_tension_id`, each loser's
  `effective_tension_id`) are asserted directly; tests do not parse `reason`.
- Losers are ordered: effective tier ascending, `base_priority` descending, `goal_id`
  ascending.
- `ArbitrationResult` is deterministic: same input produces identical output across runs.
- No effect is executed.

#### Verify (human)

Read the per-turn arbitration trace and confirm the winning goal's dominance is legible —
the trace should answer "why did X lose to Y?" without external explanation. Confirm that
boundary-preservation goals consistently outrank curiosity and continuity goals when they
conflict.

- Full scope and success/failure criteria live in
  [`Experiment.VolitionArbitrationConflict.md`](../Experiments/Experiment.VolitionArbitrationConflict.md).

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
- **Phase 5:** Probabilistic arbitration is deferred — Phase 5 is deterministic only.
  If introduced later it must be gated behind an explicit experiment mode flag and
  recorded in traces.
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
