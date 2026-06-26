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
**Phase 6 (reflection-generated goal candidates) is being expanded** — its detail is
in this document and its validation scaffold will be
`Experiment.VolitionReflectionGoalCandidates.md`. Phases 7–8 remain sketched at a
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
| 4 | Event-driven salience, satisfaction, blocking, cooldown — **complete** | Yes | Yes | `Experiment.VolitionSalienceAndSatisfaction` |
| 5 | Arbitration and multi-goal conflict resolution — **complete** | Yes | Yes | `Experiment.VolitionArbitrationConflict` |
| 6 | Reflection-generated goal candidates (proposed, not auto-accepted) — **expanding** | Yes | Yes | `Experiment.VolitionReflectionGoalCandidates` |
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

### Phase 6 — Reflection-generated goal candidates

Let a reflection/sleep step propose goal candidates with evidence references. Proposals
stay in `Proposed` status until an explicit accept or reject event moves them; nothing
is silently promoted. This is a pure, model-free slice — the proposer function maps
scripted open questions to candidates deterministically, without any LLM call.

#### Open questions to resolve before building

1. **Where do proposed goals live in `VolitionState`?** Leaning: a separate
   `pending_candidates` collection distinct from the `goals` map, so fixture-seeded
   goals and proposed candidates never share the same map. On acceptance the candidate
   moves into a new `accepted_candidates` map keyed by goal id, providing a clear
   boundary between the two sources.
2. **Should accepted candidates feed into the selector in this phase?** Leaning: no —
   Phase 6 tracks accepted candidates in state but does not wire them into
   `select_goals_with_salience`. Wiring is deferred to Phase 7, which introduces
   bounded initiative execution that needs the selector to see accepted candidates.
3. **What is the minimal evidence ref for a proposal?** Leaning: a non-empty
   `EvidenceRef` naming the source document or open question that motivated the
   candidate (e.g. `"open-question: Is continuity preserved across sessions?"`). The
   same structural rule as progress and satisfaction evidence from Phase 4.
4. **Should there be a cap on `pending_candidates` length?** Leaning: no hard cap in
   Phase 6; the experiment scripts a small fixed number (≤5) so overflow is not
   exercised. Add a cap in a later phase if the experiment reveals accumulation risks.

#### Data model additions

- Add `ProposedGoalCandidate` with private fields and a `try_new(...)` constructor:

  ```rust
  pub struct ProposedGoalCandidate { /* private fields */ }

  impl ProposedGoalCandidate {
      pub fn try_new(
          id: String,
          title: String,
          summary: String,
          tension_ids: Vec<String>,
          scope: GoalScope,
          base_priority: u8,
          allowed_effects: Vec<AllowedEffect>,
          satisfaction_condition_summary: String,
          proposal_evidence: Vec<EvidenceRef>, // must be non-empty
          source_description: String,
      ) -> Result<Self, &'static str>
  }
  ```

  `try_new` returns `Err` if `proposal_evidence` is empty; the error is
  structural rather than convention — a candidate with no evidence refs cannot
  be constructed at all.

- Extend `VolitionState` with two new collections:
  - `pending_candidates: Vec<ProposedGoalCandidate>` — awaiting review; presence
    in this collection is the "pending review" state (no separate status field)
  - `accepted_candidates: BTreeMap<String, Goal>` — accepted data records keyed
    by goal id; not yet wired into any selector. Lifecycle state
    (`GoalDynamicState`: salience, cooldown, blocking, satisfaction) is deferred
    to Phase 7 when selector integration is designed.

#### New VolitionEvent variants

```rust
GoalCandidateAdded {
    candidate: ProposedGoalCandidate,
    tick: u64,
},
GoalCandidateAccepted {
    goal_id: String,
    acceptance_evidence: EvidenceRef,
    tick: u64,
},
GoalCandidateRejected {
    goal_id: String,
    reason: String,
    tick: u64,
},
```

The reducer handles each:
- `GoalCandidateAdded`: appends to `pending_candidates`.
- `GoalCandidateAccepted`: requires a non-empty `acceptance_evidence`; moves the
  candidate from `pending_candidates` into `accepted_candidates` (with status
  `Accepted`), recording the evidence ref in the candidate's initial `evidence_refs`.
  If no candidate with the given `goal_id` exists in pending, the event is a no-op
  (reducer remains pure; no panic).
- `GoalCandidateRejected`: removes the candidate from `pending_candidates`. The
  rejection reason is recorded in the event log. No durable state for rejected
  candidates is needed in Phase 6; the event log is the audit trail.

Existing event semantics and selector behavior remain unchanged; Phase 6 adds
candidate-specific event variants and reducer branches only.

#### New pure function

```rust
pub struct GoalCandidateProposalResult {
    pub candidates: Vec<ProposedGoalCandidate>,
    pub unmatched_questions: Vec<String>,
}

pub fn propose_goal_candidates(
    open_questions: &[String],
    fixture: &VolitionFixture,
) -> GoalCandidateProposalResult
```

Deterministic, pure, no model call. For each open question string, attempts to match
keywords against fixture tension summaries and ids to assign `tension_ids`. Each
question that matches at least one tension becomes a `ProposedGoalCandidate` in
`candidates`, with `proposal_evidence` containing an `EvidenceRef` naming the source
question (trimmed, non-empty). Questions that match no tension are collected in
`unmatched_questions` so callers can inspect what was dropped without inferring it
from input/output count differences.

#### Experiment

Register a `volition-reflection-goal-candidates` experiment (spec:
`Experiment.VolitionReflectionGoalCandidates.md`) with a scripted sequence:

1. **Propose turn** — call `propose_goal_candidates` with 3–4 scripted open questions
   drawn from different tension categories (e.g. one coherence question, one continuity
   question, one curiosity question, one that matches no tension). Apply resulting
   `GoalCandidateAdded` events. Verify all matched candidates are present in
   `pending_candidates`; verify the unmatched question appears in
   `GoalCandidateProposalResult.unmatched_questions` and produces no candidate.
2. **Accept turn** — apply `GoalCandidateAccepted` for one candidate with an
   `EvidenceRef`. Verify it moves to `accepted_candidates` and is absent from
   `pending_candidates`.
3. **Reject turn** — apply `GoalCandidateRejected` for one candidate with a reason.
   Verify it is removed from `pending_candidates`.
4. **Inert turn** — apply no review events. Verify the remaining candidate stays in
   `pending_candidates` unchanged.

Each turn records: input, events applied, `pending_candidates` snapshot,
`accepted_candidates` snapshot, and an explicit no-execution marker. Replay must
produce identical output.

#### Verify (automated)

- `ProposedGoalCandidate` cannot be constructed with an empty `proposal_evidence` list.
- `GoalCandidateAdded` appends to `pending_candidates`; does not auto-accept.
- `GoalCandidateAccepted` without a prior `GoalCandidateAdded` for the same id is a
  no-op (reducer does not panic).
- `GoalCandidateAccepted` with a valid `EvidenceRef` moves the candidate to
  `accepted_candidates`.
- `GoalCandidateRejected` removes the candidate from `pending_candidates`.
- A remaining (neither accepted nor rejected) candidate stays in `pending_candidates`
  across ticks.
- `accepted_candidates` is keyed by goal id; goals in it are distinct from
  fixture-seeded goals in `VolitionState.goals`.
- `propose_goal_candidates` is deterministic: same input produces identical output.
- Existing reducer branches and selector outputs are unchanged; all prior event-handling unit tests still pass.
- No effect is executed; `accepted_candidates` map is not fed into any selector in this
  phase.
- Replay produces identical state and event logs.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.

#### Verify (human)

Read the per-turn candidate snapshot and confirm:
- Proposed candidates are clearly distinct from fixture-seeded accepted goals.
- The accept/reject trace answers "why was this accepted or rejected?" from the
  evidence ref and reason fields alone.
- Nothing in the output implies a candidate was active or influenced behavior before
  acceptance.

- Full scope and success/failure criteria live in
  `Experiment.VolitionReflectionGoalCandidates.md` (to be created when Phase 6 build
  begins).

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
  *(Resolved in practice: keyword match was sufficient for fixture-scale experiments.)*
- **Phase 4:** What evidence is strong enough to mark a goal progressed or satisfied?
  Should satisfaction be auto-accepted when evidence is structured, or reviewed?
  *(Resolved: `EvidenceRef` newtype enforces non-empty evidence; auto-accepted when
  caller provides a valid ref.)*
- **Phase 5:** Probabilistic arbitration is deferred — Phase 5 is deterministic only.
  If introduced later it must be gated behind an explicit experiment mode flag and
  recorded in traces. *(Confirmed resolved.)*
- **Phase 6:** The four open questions from the Phase 6 detail above must be confirmed
  before implementation: (1) pending vs. accepted storage boundary, (2) selector
  wiring deferral, (3) minimal evidence ref shape, (4) pending-candidate cap.
- **Phase 7:** How should accepted candidates from Phase 6 wire into the selector and
  initiative pipeline? Should they merge into the fixture-backed `goals` map or remain
  in a separate `accepted_candidates` collection?
- **Cross-cutting:** Should goals be live state, memory records, or both? Which fields
  belong only in live state vs. durable memory? (Leaning: both, carefully — live state
  for runtime reducer behavior, memory records for cross-session continuity. Confirm
  when Phase 7 needs it.)

## Documents To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md):

- **This plan** as phases start, complete, or change shape.
- **Per-phase experiment specs** under `docs/Experiments/` (validation scaffolds);
  fill in their Results/Interpretation after each run. Phase 6 requires creating
  `Experiment.VolitionReflectionGoalCandidates.md` before implementation begins.
- **`Experiment.Backlog.md`** when a future phase's experiment is promoted from idea
  to planned. Update Phase 6's entry from Planned → Running → Completed as the phase
  progresses.
- **Architecture docs** (e.g.
  [`Architecture.RuntimeLoop.md`](../Architecture/Architecture.RuntimeLoop.md), or a
  new volition architecture doc) only once a phase produces evidence worth promoting —
  via an *Implementation Status* section, not speculative description.
- **`DecisionLog.md`** when a phase outcome is promoted into an accepted rule.

This plan is ephemeral: when the volition system is built and reflected in architecture
and the decision log, archive this plan rather than citing its phases from durable
documents.
