# Architecture: Volition System

## Maturity

Candidate

## Implementation Status

Last reviewed: 2026-07-02

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
- Goal coherence (offline) in `qsf_volition::coherence`: `Contradiction`,
  `CoherenceJudgeRef`, `CoherenceVerdict`, `AdmissionResolution`, `SweepResolution`,
  and the pure resolution functions `resolve_admission`, `resolve_sweep`,
  `candidate_hard_tier_floor_rejected`, and `resolve_protected_floor_rejection`. The
  model only *detects* contradictions (recorded as a `CoherenceVerdict`); resolution
  is pure and reuses the **existing** goal-lifecycle events — no new `VolitionEvent`
  variants. `reducer::effective_tier_from_tension_ids` (public) tiers any
  `tension_ids` slice against `fixture.tensions`, replacing the old fixture-goals-only
  lookup so accepted and proposed candidates tier correctly instead of defaulting to
  `u8::MAX`. The `CoherenceJudge` adapter seam
  ([crates/qsf_models/src/coherence_judge.rs](../../crates/qsf_models/src/coherence_judge.rs))
  has a deterministic `ScriptedCoherenceJudge` (default) and a `ModelBackedCoherenceJudge`
  over a new `ModelRoleId::CoherenceJudge` role (real-model opt-in via the existing
  provider selection). The offline harness experiment `volition-goal-coherence`
  ([crates/qsf_app/src/experiments/volition_goal_coherence.rs](../../crates/qsf_app/src/experiments/volition_goal_coherence.rs))
  exercises admit / reject / admit-and-cancel / hard-tier-floor-gate / reject-dominates
  admission and a whole-set sweep (cancel, activation-tick tie-break, goal-id tie-break,
  floor-vs-floor flag), recording each decision as a `goal-coherence-check` trace record
  per the contract in
  [Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md).
- Live goal formation and off-hot-path coherence in the new shared
  [`qsf_models`](../../crates/qsf_models/src/lib.rs) crate and the `qsf_realtime_server`
  adapter. `qsf_models` lifts `ModelClient`, `ModelRequest`/`ModelMessage` (with a
  `stable_prefix_message_count` / `stable_prefix_hash` cache-boundary seam — an
  application-level marker, since neither `openai_provider_kit` nor the raw OpenAI Chat
  Completions API expose a request-side cache-breakpoint field; OpenAI's own prompt
  caching is automatic over a byte-stable prefix), `ModelRole`/`ModelRoleId`,
  `CoherenceJudge`, and the new `LiveGoalFormationJudge`
  ([crates/qsf_models/src/live_goal_formation.rs](../../crates/qsf_models/src/live_goal_formation.rs) —
  `ScriptedLiveGoalFormationJudge` default, `ModelBackedLiveGoalFormationJudge` real-model
  opt-in) out of `qsf_app`, so both `qsf_app` and `qsf_realtime_server` can invoke a model
  without either depending on the other. Cache-eligibility for the formation prompt is
  tracked through a single mechanism — `live_goal_formation_stable_prefix_hash`, derived from
  `ModelRequest::stable_prefix_hash()` over the exact prefix sent — so the tracked hash and the
  request bytes cannot diverge. A `ModelInvoker` trait decouples model callers
  from any one observability backend: `qsf_app`'s `RunContext` implements it (reusing the
  existing `invoke_model_role` event/trace recording unchanged), while the realtime loop
  uses `DirectModelInvoker` and records its own `DiagnosticRecord::LiveGoalFormationPerformed`
  (or `LiveGoalFormationFailed` on error) around the whole formation call. In the realtime
  server
  ([crates/qsf_realtime_server/src/realtime/live_goal_formation.rs](../../crates/qsf_realtime_server/src/realtime/live_goal_formation.rs)),
  a post-response hook fires once per trusted turn, after `response.create` is dispatched
  (via `tokio::task::spawn_blocking`, since `ModelClient::complete` is a blocking call) so
  turn latency is unaffected. It is gated on a completed, promotable, non-degraded, non-empty
  assistant turn, serialized per session (a second turn skips rather than races the first), and
  discards its outcome if the goal set changed during the model call. It proposes a candidate
  (or nothing) and detects contradictions in one call, then resolves the verdict through the
  **same** pure resolvers the offline engine uses — a single shared `resolve_formed_candidate`
  (hard tier-floor gate + `resolve_admission`) called from both the live hook and the offline
  harness. A rejection is carried as reducer-derived `DeclinedCandidate` state in
  `qsf_volition::VolitionState::declined_candidates` (populated when `apply` folds a
  `GoalCandidateRejected` event that carries a `CoherenceDecline`, deduplicated by title and
  windowed), injected into the turn context as a `coherence` layer
  ([crates/qsf_realtime_server/src/realtime/volition_injection.rs](../../crates/qsf_realtime_server/src/realtime/volition_injection.rs))
  from the next turn onward — the rejection turn's own context already predates admission — and
  emitted even on a turn with no selected goal (a coherence-only packet). Layers are modeled as
  data so the injection trace never declares a layer the text lacks. The real
  sleep/consolidation pass runs the same engine over the whole session
  ([`run_sleep_volition_goal_maintenance`](../../crates/qsf_app/src/experiments/volition_continuity.rs)):
  one whole-history formation call and one whole-set `resolve_sweep`, applied to the persisted
  volition snapshot during `commit_cross_session_sleep`. The offline harness
  `live-goal-formation-and-coherence`
  ([crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs](../../crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs))
  exercises admit / reject-with-decline / no-goal-formed, the declined-candidate
  injection-ordering invariant, the pending-candidate-not-selectable invariant, sleep
  whole-history formation, and the sleep sweep, recording each decision as a
  `live-goal-formation` trace record per the contract in
  [Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md).

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

`qsf_models` (introduced with live goal formation) sits below both adapters for the same reason:
it depends on `qsf_volition` and `qsf_context` (for `CoherenceJudgeGoalRef` and
`ContextBudget`) plus the networked model dependencies (`openai_provider_kit`,
`reqwest`), but neither `qsf_app` nor `qsf_realtime_server` depend on each other to
reach the model layer. `qsf_app` re-exports the pieces it needs
(`crates/qsf_app/src/models/mod.rs`) and adds its own `RunContext`-tied
`invoke_model_role`; `qsf_realtime_server` depends on `qsf_models` directly.

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
