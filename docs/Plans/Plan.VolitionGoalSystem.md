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
its validation scaffold is
[`Experiment.VolitionSalienceAndSatisfaction.md`](../Experiments/Experiment.VolitionSalienceAndSatisfaction.md).
**Phase 5 (arbitration and conflict resolution) is complete**; its design is captured
in [`Design.VolitionArbitration.md`](Design.VolitionArbitration.md) and its
validation scaffold is
[`Experiment.VolitionArbitrationConflict.md`](../Experiments/Experiment.VolitionArbitrationConflict.md).
**Phase 6 (reflection-generated goal candidates) is complete**; its design is captured
in this document and its validation scaffold is
[`Experiment.VolitionReflectionGoalCandidates.md`](../Experiments/Experiment.VolitionReflectionGoalCandidates.md).
**Phase 7 (bounded internal initiative execution) is complete**; its validation scaffold
is
[`Experiment.VolitionBoundedInitiativeExecution.md`](../Experiments/Experiment.VolitionBoundedInitiativeExecution.md)
and its status is Running (automated tests pass; awaiting human review).
Phase 8 remains sketched at a high level.

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
| 6 | Reflection-generated goal candidates (proposed, not auto-accepted) — **complete** | Yes | Yes | `Experiment.VolitionReflectionGoalCandidates` |
| 7 | Bounded internal initiative execution — selector wiring + `InitiativeExecuted` — **complete** | Yes | Yes | `Experiment.VolitionBoundedInitiativeExecution` |
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

### Phase 5 — Arbitration and conflict resolution (complete)

Added deterministic cross-goal arbitration as a pure, additive layer over Phase 4's
selector. `arbitrate(selections, fixture) -> Option<ArbitrationResult>` resolves
conflicts by tension tier: a goal's effective tier is the minimum `arbitration_tier`
among its parent tensions (default `u8::MAX`); tiebreakers are `base_priority`
descending then `goal_id` ascending. `ArbitrationLoser` records each losing goal's
structured tension provenance and a rendered reason string. A per-turn
`arbitration_status` field (`no_selection | single_selection | conflict_resolved`)
makes absent output distinguishable from silent failure. `arbitration_tier: u8` was
added to `Tension`; existing selectors and reducers were untouched. A
`VolitionEvent::TickAdvanced` variant was added to guarantee monotonic tick advances
even when no lifecycle events are emitted. The `volition-arbitration-conflict`
experiment confirmed that `boundary-preservation` (tier 1) consistently outranks
`continuity-preservation` (tier 5) and `research-curiosity` (tier 7). All 54 unit
tests pass. Design decisions: [`Design.VolitionArbitration.md`](Design.VolitionArbitration.md).
Validation scaffold: [`Experiment.VolitionArbitrationConflict.md`](../Experiments/Experiment.VolitionArbitrationConflict.md).

### Phase 6 — Reflection-generated goal candidates (complete)

Added `propose_goal_candidates`, a pure, model-free function that maps open questions
to `ProposedGoalCandidate` values by keyword-matching against tension ids and
summaries. `ProposedGoalCandidate` enforces non-empty `proposal_evidence` via
`try_new` (and through a custom `Deserialize` impl so the invariant holds through
serde). Three new `VolitionEvent` variants (`GoalCandidateAdded`,
`GoalCandidateAccepted`, `GoalCandidateRejected`) manage the candidate lifecycle;
the reducer keeps candidates in two separate, clearly-named collections —
`VolitionState::pending_candidates` and `VolitionState::accepted_candidates` — both
distinct from the fixture-seeded `goals` map. Accepted candidates store a full `Goal`
data record keyed by goal id; their dynamic state (`GoalDynamicState`) and selector
wiring are deferred to Phase 7. A `volition-reflection-goal-candidates` experiment
replays a scripted 4-turn sequence (propose → accept → reject → inert) and writes
per-turn snapshots with `executed_effects = 0` on every turn. All prior tests pass;
new unit tests cover the full candidate lifecycle. Validation scaffold:
[`Experiment.VolitionReflectionGoalCandidates.md`](../Experiments/Experiment.VolitionReflectionGoalCandidates.md).

### Phase 7 — Bounded internal initiative execution

Two sub-slices in sequence:

1. **Selector wiring** — wire accepted candidates into `select_goals_with_salience`
   so they compete alongside fixture goals in selection and arbitration.
2. **Initiative execution** — translate the arbitration winner into a bounded
   `InitiativeOutput`, apply it as a `VolitionEvent::InitiativeExecuted`, and trace
   the full chain: goal → delta → arbitration → execution → output.

No write-capable external action is added. All execution output is a purely structural
record describing what a runtime system *would* do. `executed_effects = 0` on every
experiment turn; `InitiativeExecuted` records the internal output only.

#### Open question to resolve before building

**How should accepted candidates get `activation_keywords` and enter the selector?**

*Context:* `ProposedGoalCandidate::into_goal()` currently sets
`activation_keywords: vec![]`. Without keywords the selector's keyword-match gate
rejects them immediately, so selector wiring requires extending the candidate type.

**Option A — Derive keywords at proposal time from matched tension id parts
(recommended).** `propose_goal_candidates` splits each matched tension id on `-` and
uses the parts as activation keywords (e.g. `continuity-preservation` →
`["continuity", "preservation"]`). `ProposedGoalCandidate` gains an
`activation_keywords: Vec<String>` field; `into_goal()` passes the keywords through.
The selector loop gains a second pass over `state.accepted_candidates` after
`fixture.goals`, reusing `state.goals` for dynamic state with an initial
`GoalDynamicState` entry inserted by the `GoalCandidateAccepted` reducer branch.
Deterministic and pure; no API change to events.

**Option B — Supply keywords at acceptance time.** `GoalCandidateAccepted` carries
`activation_keywords: Vec<String>`. The caller specifies which keywords activate the
accepted goal. Flexible but requires callers to know activation terms that are already
implicit in the matched tensions.

**Option C — Skip keyword matching for accepted candidates.** The selector normalizes
the accepted goal's `title` and `summary` and matches against those directly.
No event or type changes needed, but creates inconsistent matching behaviour between
fixture goals (explicit keywords) and accepted goals (content-based).

The plan proceeds with **Option A**. Verify this choice with the user before building.

#### Data model additions

- `ProposedGoalCandidate` gains `activation_keywords: Vec<String>` (set by
  `propose_goal_candidates` from matched tension id parts; passed through `try_new`
  and `into_goal`).
- `GoalCandidateAccepted` reducer branch inserts `GoalDynamicState::initial()` into
  `state.goals` for the accepted goal id, so the accepted goal receives salience,
  decay, and cooldown management immediately on acceptance using the same reducer
  branches as fixture goals.
- `select_goals_with_salience` gains a second goal source: after iterating
  `fixture.goals` it iterates `state.accepted_candidates.values()`, using
  `state.goals` for dynamic state lookup (identical logic). Tier and priority
  resolution uses the shared fixture tensions.
- New `InitiativeOutput` enum (pure, serializable, one variant per `AllowedEffect`):

  ```rust
  pub enum InitiativeOutput {
      ReflectionRequested { proposed_question: String },
      ContextRetrievalRequested { query_terms: Vec<String> },
      ExperimentProposed { hypothesis: String, scope: GoalScope },
      OpenThreadSurfaced { thread_summary: String },
  }
  ```

- `GoalDynamicState` gains `last_initiative_output: Option<InitiativeOutput>`.

#### New VolitionEvent variant

```rust
InitiativeExecuted {
    goal_id: String,
    effect: AllowedEffect,
    output: InitiativeOutput,
    rationale: String,
    tick: u64,
},
```

Reducer: sets the goal's status to `Active`, records `last_activated_tick`, stores
`output` in `GoalDynamicState::last_initiative_output`. Does not panic on unknown
goal id (same no-op pattern as other goal lifecycle events).

#### New pure function

```rust
pub fn execute_initiative(
    initiative: &InitiativeProposal,
    goal: &Goal,
) -> InitiativeOutput
```

Deterministic, no model call. Maps `AllowedEffect` → `InitiativeOutput` using goal
`summary`, `title`, and `initiative.matched_terms`:
- `Reflect` → `ReflectionRequested { proposed_question }`
- `RetrieveContext` → `ContextRetrievalRequested { query_terms: matched_terms }`
- `ProposeExperiment` → `ExperimentProposed { hypothesis, scope: goal.scope }`
- `SurfaceOpenThread` → `OpenThreadSurfaced { thread_summary: goal.summary }`

#### Experiment

Register a `volition-bounded-initiative-execution` experiment (spec:
`Experiment.VolitionBoundedInitiativeExecution.md`) with a scripted sequence:

1. **Proposal turn** — `propose_goal_candidates` on one matched question; apply
   `GoalCandidateAdded`. Verify candidate carries non-empty `activation_keywords`
   derived from its matched tension id parts.
2. **Accept turn** — apply `GoalCandidateAccepted`. Verify:
   - A `GoalDynamicState` entry now exists in `state.goals` for the accepted goal id.
   - `select_goals_with_salience` returns the accepted goal when input matches its
     derived keywords (selector wiring verified here).
3. **Arbitration turn** — input that matches both one fixture goal and the accepted
   goal. Run `select_goals_with_salience` + `arbitrate`. Verify tier ordering is
   respected; the accepted goal's effective tier is derived from its `tension_ids`
   in the shared fixture.
4. **Execution turn** — call `execute_initiative` on the arbitration winner; apply
   `InitiativeExecuted`. Verify:
   - `GoalDynamicState::last_initiative_output` is set with the correct variant.
   - The event records the output, rationale, and tick.
   - `executed_effects = 0` (no external side effect); `InitiativeExecuted` records
     the structural output only.
5. **Outcome turn** — apply `GoalProgressObserved` or `GoalSatisfied`. Verify goal
   lifecycle advances correctly and the evidence ref is preserved.

Each turn records: input, events applied, `select_goals_with_salience` output,
arbitration result (turns 3–5), `InitiativeExecuted` output (turn 4), and
`GoalDynamicState` snapshot. Replay must produce identical output.

#### Verify (automated)

- Accepted candidate appears in `select_goals_with_salience` output after acceptance
  when input matches its derived activation keywords.
- Accepted candidate competes in arbitration alongside fixture goals; tier ordering
  is respected using the shared fixture tensions.
- `execute_initiative` is deterministic: same input → same `InitiativeOutput`.
- `InitiativeExecuted` stores the output in `GoalDynamicState::last_initiative_output`.
- The accepted goal's salience, cooldown, and lifecycle are managed by the same
  reducer branches as fixture goals (no parallel code path).
- All prior tests pass; existing selector and reducer behaviour is unchanged.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.

#### Verify (human)

- Read the selection output and confirm the accepted candidate is visually distinct
  from fixture goals (source is identifiable from `source_reference` and
  `evidence_refs`, not from position alone).
- The `InitiativeOutput` for each `AllowedEffect` variant reads as a plausible
  internal action — a reflection question, a retrieval query, an experiment
  hypothesis, or a thread summary.
- The trace chain — goal → tension provenance → delta → arbitration →
  `execute_initiative` output — answers "why did this initiative execute?" without
  requiring external context.
- Nothing in the output implies a real external write-capable action occurred.

Full scope and success/failure criteria live in
[`Experiment.VolitionBoundedInitiativeExecution.md`](../Experiments/Experiment.VolitionBoundedInitiativeExecution.md).

### Phase 8 — Optional mode/bias experiments

Introduce inspectable mode/bias state that shifts arbitration weights deterministically.

- **Verify:** mode is explicit, inspectable, and traceable; effects are deterministic;
  no free-form mood labels drive the bias vector.

## Open Questions To Resolve Before The Affected Phase

These are carried from the idea doc and should be answered when the relevant phase is
expanded, not silently resolved:

- **Phase 2:** Is deterministic keyword/priority relevance enough, or is a richer
  match needed? Does the tension layer earn its place at this small scale?
  *(Resolved in practice: keyword match was sufficient for fixture-scale experiments.)*
- **Phase 4:** What evidence is strong enough to mark a goal progressed or satisfied?
  Should satisfaction be auto-accepted when evidence is structured, or reviewed?
  *(Resolved: `EvidenceRef` newtype enforces non-empty evidence; auto-accepted when
  caller provides a valid ref.)*
- **Phase 5:** Probabilistic arbitration is deferred — Phase 5 is deterministic only.
  If introduced later it must be gated behind an explicit experiment mode flag and
  recorded in traces. *(Confirmed resolved.)*
- **Phase 6:** *(Resolved.)* (1) `pending_candidates` and `accepted_candidates` are
  separate collections, distinct from fixture-seeded `goals`. (2) Selector wiring is
  deferred to Phase 7. (3) `EvidenceRef` newtype enforces non-empty evidence for
  proposals. (4) No hard cap in Phase 6; the experiment scripts ≤4 questions.
- **Phase 7:** *(Decision made for this plan; confirm with user before building.)*
  Accepted candidates wire into `select_goals_with_salience` via Option A: keywords
  derived from matched tension id parts at proposal time; `GoalDynamicState` entry
  inserted into `state.goals` at acceptance time; selector gains a second pass over
  `state.accepted_candidates` using the same dynamic state map. See Phase 7 section
  for full rationale and alternatives.
- **Cross-cutting:** Should goals be live state, memory records, or both? Which fields
  belong only in live state vs. durable memory? (Leaning: both, carefully — live state
  for runtime reducer behavior, memory records for cross-session continuity. Confirm
  when Phase 7 needs it.)

## Documents To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md):

- **This plan** as phases start, complete, or change shape.
- **Per-phase experiment specs** under `docs/Experiments/` (validation scaffolds);
  fill in their Results/Interpretation after each run. Phase 7's validation scaffold
  (`Experiment.VolitionBoundedInitiativeExecution.md`) is already created.
- **`Experiment.Backlog.md`** when a future phase's experiment is promoted from idea
  to planned. Update Phase 7's entry from Planned → Running → Completed as the phase
  progresses.
- **Architecture docs** (e.g.
  [`Architecture.RuntimeLoop.md`](../Architecture/Architecture.RuntimeLoop.md), or a
  new volition architecture doc) only once a phase produces evidence worth promoting —
  via an *Implementation Status* section, not speculative description.
- **`DecisionLog.md`** when a phase outcome is promoted into an accepted rule.

This plan is ephemeral: when the volition system is built and reflected in architecture
and the decision log, archive this plan rather than citing its phases from durable
documents.
