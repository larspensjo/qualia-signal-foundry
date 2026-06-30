# Architecture: Volition System

## Maturity

Candidate

## Implementation Status

The volition domain is extracted into a standalone `qsf_volition` crate
([crates/qsf_volition/src/lib.rs](../../crates/qsf_volition/src/lib.rs)). It holds
the pure, deterministic core: goal/tension fixtures, durable-within-a-run state, a pure
reducer, context-neutral selection and arbitration records, mode-aware bias, and bounded
internal initiative output. Continuity snapshot and consolidation helpers also live in
this crate so the realtime server and sleep pass can share a pure persistence and
analysis model. Context assembly and report shapes live in caller adapters (`qsf_app`,
and—going forward—`qsf_realtime_server`), not in the crate.

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
- Continuity and consolidation helpers: `VolitionContinuitySnapshot`,
  `ReviewedVolitionSeed`, `VolitionSuppressionReason`, `VolitionTurnOutcome`,
  `persist_volition_continuity_snapshot()`, `load_reviewed_volition_seed()`,
  `apply_reviewed_seed()`, and `build_volition_consolidation_report()`.
- Goal-candidate proposal from open questions: `propose_goal_candidates()`,
  `ProposedGoalCandidate` (non-empty evidence invariant), and `EvidenceRef`.
- Grounded-term and stance helpers in `qsf_volition::terms` and
  `qsf_volition::stance`: `GroundedTerm`, `GroundingRef`, `grounded_terms_from_text()`,
  `render_volition_stance()`, and the stable-baseline hashing helper used by the
  realtime adapter. The realtime/project trust-boundary preamble is owned by the
  `qsf_realtime_server` adapter, not by the pure volition crate.
- Opportunity detection and shaping guidance in `qsf_volition::opportunity` and
  `qsf_volition::shaping`: `detect_opportunities()`,
  `OpportunitySignalKind`, `OpportunitySignal`, `ShapingIntensity`,
  `ReceptivenessHint`, and `choose_shaping_intensity()`.
- Context-neutral goal-selection helpers in `qsf_volition::selection`:
  `matched_keywords`, `compute_relevance`, `compute_relevance_with_salience`,
  `initiative_for_goal`, `initiative_for_effect`, and `select_goals_ranked`.
  Re-exported via `pub use selection::*` so both `qsf_app` and
  `qsf_realtime_server` can call them without importing `qsf_app`. The
  `RankedSelectionResult` type groups selected, omitted, suppressed-cooldown, and
  visible-blocked goals without any context-assembly dependency.
- State inspection in `qsf_volition::inspection`: `build_state_inspection` returns
  a `VolitionStateInspection` grouping goals by status with id, title, salience,
  cooldown tick, and last-activated tick, plus `InitiativeSummary` records for
  recent initiative outputs. Consumed by `inspect_volition_state` in the realtime
  server.

**Not in this crate (by design):**

- `ContextFragment`, `ContextBudget`, `ContextAssembly`, and context assembly itself —
  these stay in the shared `qsf_context` crate.
- Context-attached selection results (`GoalSelectionResult`,
  `SalienceGoalSelectionResult`), pre-initiative trace assembly, and
  experiment/report shapes — these stay in the `qsf_app` adapter
  ([crates/qsf_app/src/volition.rs](../../crates/qsf_app/src/volition.rs)).
  `qsf_app::volition` now calls `select_goals_ranked` from this crate and wraps
  the result with context assembly.
- Realtime bounded-initiative surfacing and the realtime-specific initiative
  trace live in `qsf_realtime_server` adapter code
  ([crates/qsf_realtime_server/src/realtime/volition_initiative.rs](../../crates/qsf_realtime_server/src/realtime/volition_initiative.rs),
  [crates/qsf_realtime_server/src/realtime/sideband.rs](../../crates/qsf_realtime_server/src/realtime/sideband.rs)).

## Crate Boundary And Dependency Direction

`qsf_volition` depends only on `serde`, `serde_json`, `anyhow`, and `tempfile`. It
does **not** depend on `qsf_context`, `qsf_app`, or any caller. This keeps selection
and arbitration pure volition-domain operations: they sort on tension tiers, base
priority, and goal id, and are structurally incapable of reading context-assembly data.

Adapters depend on `qsf_volition`, never the reverse:

- `qsf_app` re-exports the crate (`pub use qsf_volition::*`), calls
  `select_goals_ranked` from `qsf_volition::selection`, then turns the ranked result
  into `ContextFragment`s via `build_fragment`, assembles context, and builds
  traces/reports.
- `qsf_realtime_server` depends on `qsf_volition` directly and calls
  `select_goals_ranked`, `build_state_inspection`, `arbitrate_with_mode`,
  `detect_opportunities`, and `choose_shaping_intensity` from the volition
  adapters without importing `qsf_app` experiment/report code.

The pure reviewed-seed merge is intentionally conservative: fixture protected goals
must remain present at their original tiers/effects, reviewed additions cannot overwrite
fixture ids, reviewed goals cannot enter at or below the protected tier floor, and the
merge order is deterministic because the reviewed seed is stored as a BTreeMap.

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
