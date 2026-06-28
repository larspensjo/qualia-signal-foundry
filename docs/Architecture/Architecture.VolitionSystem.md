# Architecture: Volition System

## Maturity

Candidate

## Implementation Status

The volition domain is extracted into a standalone `qsf_volition` crate
([crates/qsf_volition/src/lib.rs](../../crates/qsf_volition/src/lib.rs)). It holds
the pure, deterministic core: goal/tension fixtures, durable-within-a-run state, a pure
reducer, context-neutral selection and arbitration records, mode-aware bias, and bounded
internal initiative output. Context assembly and report shapes live in caller adapters
(`qsf_app`, and—going forward—`qsf_realtime_server`), not in the crate.

**Implemented today:**

- Fixture domain: `Tension`, `Goal`, `VolitionFixture`, and `static_fixture()` with
  arbitration tiers and tension priority bias.
- Durable-within-a-run state and a pure reducer: `VolitionState`, `GoalDynamicState`,
  `VolitionEvent`, and `apply()` — the only place lifecycle status changes.
- Tick-driven lifecycle: salience decay, cooldown elapse, and inactivity retirement via
  `tick_events()`.
- Context-neutral selection record `GoalSelection` (goal, relevance score, matched terms,
  proposed initiative) and the deterministic arbitration functions `arbitrate()` and
  `arbitrate_with_mode()`.
- Mode-aware arbitration: `Mode` with a declared `bias_vector()`, a `PROTECTED_TIER_FLOOR`
  that makes safety/boundary tiers immune to bias, and per-goal `BiasOutcome` records.
- Bounded internal initiative: `InitiativeProposal`, `InitiativeOutput`, and
  `execute_initiative()` — structural records only; no external write-capable effect.
- Goal-candidate proposal from open questions: `propose_goal_candidates()`,
  `ProposedGoalCandidate` (non-empty evidence invariant), and `EvidenceRef`.

**Not in this crate (by design):**

- `ContextFragment`, `ContextBudget`, `ContextAssembly`, and context assembly itself —
  these stay in the shared `qsf_context` crate.
- Context-attached selection results (`GoalSelectionResult`,
  `SalienceGoalSelectionResult`), the salience-aware selector, pre-initiative trace
  assembly, and experiment/report shapes — these stay in the `qsf_app` adapter
  ([crates/qsf_app/src/volition.rs](../../crates/qsf_app/src/volition.rs)).

## Crate Boundary And Dependency Direction

`qsf_volition` depends only on `serde`. It does **not** depend on `qsf_context`,
`qsf_app`, or any caller. This keeps arbitration a pure volition-domain operation: it
sorts on tension tiers, base priority, and goal id, and is structurally incapable of
reading context-assembly data.

Adapters depend on `qsf_volition`, never the reverse:

- `qsf_app` re-exports the crate (`pub use qsf_volition::*`), then turns selected goals
  into `ContextFragment`s, assembles context, and builds traces/reports.
- The realtime server may depend on `qsf_volition` directly for read-only inspection and
  context packets without importing `qsf_app` experiment/report code.

A `GoalSelection` is associated with its assembled `ContextFragment` by the adapter via
the caller's result shape (which carries the full `ContextAssembly`), joinable by
`fragment_id`. The selection record itself stays context-free, so the same arbitration
core serves text, sleep, and realtime callers without dragging a context dependency into
the domain. See the 2026-06-27 DecisionLog entry "Realtime volition extraction keeps
context assembly outside `qsf_volition`".

## Data Flow

Volition participates in the unidirectional `input -> action -> reducer -> state ->
render` flow:

1. An adapter selector (in `qsf_app`) matches input against goal activation keywords,
   scores relevance, and assembles context — producing context-attached results.
2. The selector projects those into context-neutral `GoalSelection` records.
3. `arbitrate()` / `arbitrate_with_mode()` resolve cross-goal conflict deterministically
   and return a winner plus structured losers.
4. Lifecycle transitions are expressed as `VolitionEvent`s and folded into
   `VolitionState` by the pure `apply()` reducer; selectors never mutate lifecycle.
5. Adapters render traces, reports, or realtime context packets from the results.

## Related Documents

- [Architecture.ContextManagement.md](Architecture.ContextManagement.md) — context
  assembly that adapters layer on top of volition selections.
- [Architecture.StateAndObservability.md](Architecture.StateAndObservability.md) — how
  volition state and traces are observed.
- `docs/DecisionLog.md` — crate-boundary and bounded-initiative decisions.
