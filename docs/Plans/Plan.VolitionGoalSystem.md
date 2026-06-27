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
**Phase 8 (optional inspectable mode/bias state) is complete**; its design is captured in
[`Design.VolitionModeBias.md`](Design.VolitionModeBias.md) and its validation scaffold is
[`Experiment.VolitionModeBias.md`](../Experiments/Experiment.VolitionModeBias.md) with
status Running (automated tests pass; awaiting human review).

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
| 8 | Optional inspectable mode/bias state (arbitration bias) — **complete** | Yes | Yes | `Experiment.VolitionModeBias` |

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

### Phase 7 — Bounded internal initiative execution (complete)

Wired accepted goal candidates into selection and added bounded, side-effect-free
initiative execution, in two additive sub-slices over Phase 6. **Selector wiring:**
accepted candidates now compete alongside fixture goals in `select_goals_with_salience`
and arbitration. Following **Option A**, `ProposedGoalCandidate` gained an
`activation_keywords: Vec<String>` field that `propose_goal_candidates` derives from
matched tension id parts (e.g. `continuity-preservation` → `["continuity",
"preservation"]`, deduplicated) and passes through `try_new` and `into_goal`; the
`GoalCandidateAccepted` reducer branch inserts an initial `GoalDynamicState` into
`state.goals` so accepted goals receive salience, decay, and cooldown via the same
reducer branches as fixture goals (no parallel code path), and the selector gains a
second pass over `state.accepted_candidates` using the shared fixture tensions for tier
and priority. **Initiative execution:** a pure, deterministic `execute_initiative(&
InitiativeProposal, &Goal) -> InitiativeOutput` maps each `AllowedEffect` to one
variant of a new serializable `InitiativeOutput` enum (`ReflectionRequested`,
`ContextRetrievalRequested`, `ExperimentProposed`, `OpenThreadSurfaced`). A new
`VolitionEvent::InitiativeExecuted { goal_id, effect, output, rationale, tick }`
reducer sets the goal `Active`, records `last_activated_tick`, and stores the output in
the new `GoalDynamicState::last_initiative_output`, no-op on unknown goal id. No
write-capable external action was added: all output is a structural record of what a
runtime *would* do, and `executed_effects = 0` on every experiment turn. The
`volition-bounded-initiative-execution` experiment replays the scripted propose →
accept → arbitrate → execute → outcome sequence and traces the full chain goal → delta
→ arbitration → execution → output. Status: Running (automated tests pass; awaiting
human review). Validation scaffold:
[`Experiment.VolitionBoundedInitiativeExecution.md`](../Experiments/Experiment.VolitionBoundedInitiativeExecution.md).

### Phase 8 — Optional inspectable mode/bias state (complete)

Introduce an inspectable **mode**: a named, declared bias over arbitration ordering that can
deterministically shift which goal wins a conflict — *without* being able to override the
safety/boundary floor. A mode's meaning *is* its declared bias vector; the label ("Focused",
"Exploratory") is only a handle, so no free-form mood drives behavior. This slice biases
**arbitration only**; salience/selection scoring and proposal-threshold behavior are explicit
follow-ups. Full design, rationale, and alternatives:
[`Design.VolitionModeBias.md`](Design.VolitionModeBias.md).

**Build (pure, additive over the arbitration slice):**

- A `Mode` enum (`Neutral`, `Focused`, `Exploratory`; `Default` = `Neutral`) with a declared
  `bias_vector() -> BTreeMap<String, i8>` keyed by tension id (negative promotes, positive
  demotes; empty for `Neutral`) — the source of truth for the bias.
- `PROTECTED_TIER_FLOOR: u8 = 3`. A **protected floor** (effective tier 1–3: safety/boundary,
  explicit user intent, task completion) is immune to bias; the **biasable band** (effective
  tier ≥ 4) is reorderable. A band goal's biased tier is clamped to a lower bound of
  `PROTECTED_TIER_FLOOR + 1`, so a band goal can **never enter the floor** — the safety invariant
  holds by construction. The bias is added in a widened signed integer and then clamped, so the
  `u8` tier and `i8` bias cannot overflow or wrap (a `u8::MAX` no-tension goal stays at `u8::MAX`).
- A pure `arbitrate_with_mode(selections, fixture, mode) -> Option<ModeArbitrationResult>`
  (sort key `(biased_tier asc, base_priority desc, goal_id asc)`); `arbitrate` is refactored to
  delegate to `arbitrate_with_mode(.., Mode::Neutral)` and map the neutral result back into
  `ArbitrationResult`, so there is one sort implementation and `arbitrate`'s
  `Option<ArbitrationResult>` signature and behavior are unchanged. New `BiasOutcome`,
  `ModeArbitrationLoser`, `ModeArbitrationResult` types
  record each goal's pre-bias tier, the bias applied, the post-bias tier, and whether it was
  protected.
- `VolitionState` gains `#[serde(default)] mode: Mode` (default `Neutral`; `serde(default)` keeps
  prior run artifacts deserializable); a new `VolitionEvent::ModeChanged { mode, tick }` sets it
  via the pure reducer.
- A registered `volition-mode-bias` experiment scripting the turns below.

**Verify (automated):** `arbitrate_with_mode(.., Neutral)` matches `arbitrate`; a biasing mode
flips the winner among band goals; a present tier-1 goal wins under every mode (floor immunity);
no band goal's biased tier drops below `PROTECTED_TIER_FLOOR + 1`; bias is attributed to each
goal's effective tension; results are deterministic and replayable; `executed_effects = 0` on
every turn. The runner parses the generated trace records and asserts the flip/floor outcomes
(trace contract in the experiment scaffold).

**Verify (human test):** a researcher reads the per-turn traces and judges that the mode and its
vector are legible, that the flip and non-flip outcomes are sensible consequences of the declared
vectors (not arbitrary), that floor immunity is convincing, and that the label is clearly a handle
over an explicit vector — no free-form mood doing hidden work.

**Default-exercises-new-path:** runtime default mode is `Neutral`, but the experiment scripts a
biasing mode by default, so `arbitrate_with_mode` runs on every experiment run.

**Scripted turns:** (1) Neutral band-only conflict baseline; (2) same input under `Exploratory`
(`ModeChanged`) → winner flips from the continuity goal to the curiosity goal; (3) conflict that
also activates a tier-1 boundary goal under a biasing mode → winner stays the floor goal; (4)
`Focused` → winner stays the continuity goal and the curiosity goal is demoted further. Full
scope, inputs, and success/failure criteria live in
[`Experiment.VolitionModeBias.md`](../Experiments/Experiment.VolitionModeBias.md).

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
- **Phase 8:** *(Decisions made with the user; see
  [`Design.VolitionModeBias.md`](Design.VolitionModeBias.md).)* (1) A mode biases **arbitration
  only** this slice; salience/selection and proposal-threshold bias are follow-ups. (2) Bias
  reorders only within the **biasable band** (effective tier ≥ 4); the **protected floor** (tiers
  1–3) is immune, and a band goal's biased tier is clamped away from the floor so the safety
  invariant holds by construction. (3) Mode is event-driven `VolitionState` set by
  `ModeChanged`; a mode's meaning is its declared bias vector (no free-form mood label). (4)
  `arbitrate` delegates to `arbitrate_with_mode(.., Neutral)` (one sort implementation,
  behavior-preserving). Open follow-ups: whether mode should also bias salience/selection and the
  proposal threshold; how a mode is chosen outside a script; cross-session mode persistence.
- **Cross-cutting:** Should goals be live state, memory records, or both? Which fields
  belong only in live state vs. durable memory? (Leaning: both, carefully — live state
  for runtime reducer behavior, memory records for cross-session continuity. Confirm
  when Phase 7 needs it.)

## Documents To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md):

- **This plan** as phases start, complete, or change shape.
- **Per-phase `Design.*.md`** for a slice with non-trivial design decisions (Phase 5 has
  [`Design.VolitionArbitration.md`](Design.VolitionArbitration.md); Phase 8 has
  [`Design.VolitionModeBias.md`](Design.VolitionModeBias.md)).
- **Per-phase experiment specs** under `docs/Experiments/` (validation scaffolds);
  fill in their Results/Interpretation after each run. Phase 7's scaffold
  (`Experiment.VolitionBoundedInitiativeExecution.md`) and Phase 8's scaffold
  (`Experiment.VolitionModeBias.md`) are already created.
- **`Experiment.Backlog.md`** when a future phase's experiment is promoted from idea
  to planned. Update each entry through Planned → Running → Completed as the phase
  progresses.
- **Architecture docs** (e.g.
  [`Architecture.RuntimeLoop.md`](../Architecture/Architecture.RuntimeLoop.md), or a
  new volition architecture doc) only once a phase produces evidence worth promoting —
  via an *Implementation Status* section, not speculative description.
- **`DecisionLog.md`** when a phase outcome is promoted into an accepted rule (for Phase 8,
  the rule that mode bias may reorder only within the biasable band — protected tiers are
  immune).

This plan is ephemeral: when the volition system is built and reflected in architecture
and the decision log, archive this plan rather than citing its phases from durable
documents.
