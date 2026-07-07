# Architecture: Volition System

## Maturity

Candidate

## Implementation Status

Last reviewed: 2026-07-06

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
  `VolitionEvent`, and `apply()` — the only place lifecycle status changes. Per-goal
  bookkeeping includes block-repetition counters (`blocked_count`, `last_blocked_tick`, set and
  incremented by the `GoalBlocked` arm and reset by `GoalSatisfied`) and exact satisfaction
  evidence (`last_satisfied_evidence_ref`, set only by `GoalSatisfied`), all `#[serde(default)]`
  so continuity snapshots predating them still load. These are lifecycle facts, not emotion
  state.
- Tick-driven lifecycle: salience decay, cooldown elapse, and inactivity retirement via
  `tick_events()`.
- Context-neutral selection record `GoalSelection` (goal, relevance score, matched keywords
  with weight classes, `match_strength`, proposed initiative) and the deterministic
  arbitration functions `arbitrate()` and `arbitrate_with_mode()`.
- Weighted goal activation: every activation keyword carries a coarse `KeywordWeightClass`
  (`Weak = 1`, `Normal = 4`, `Strong = 8`), and a selection's `match_strength` is the summed
  weight of its matched keywords. `match_strength` is the single scoring quantity — the ranked
  relevance bonus (`match_strength × RELEVANCE_PER_STRENGTH_POINT`) and the arbitration
  qualification gate both derive from it, so ranked display and eligibility can never disagree.
  Weight classes are fixture data, so tuning a persona is a data diff, not a code change.
- Qualification gating in arbitration: a fixture-level
  `arbitration_qualification_threshold` (default 4) partitions selections before the tier sort.
  Only selections at or above the threshold may *win*; sub-threshold selections still activate,
  bump salience, and appear in ranked selection, but are recorded as below-threshold candidates
  that never enter the sort. Among qualified goals the tier ordering is unchanged, and there is
  no exemption for protected tiers — protection still governs cancellation, not speaking, so a
  protected goal can no longer win the turn on a stopword while a multi-term on-topic match
  loses. When no selection qualifies, the turn is a no-winner turn: volition stays quiet and the
  outcome records a dedicated `below_qualification_threshold` suppression instead of promoting a
  weak winner or falling back to a default goal. The rich-match effect gate
  (`ProposeExperiment`) requires `match_strength ≥ 8` **and** at least two distinct non-Weak
  matched terms. `ModeArbitrationOutcome` / `ArbitrationOutcome` wrap the qualification
  partition (`qualified`, `below_threshold`, `qualification_threshold`) around the existing
  sorted result. Per-tier thresholds, corpus-derived weights, stemming, and phrase matching are
  deferred; the long-term semantic direction lives in
  [Idea.SemanticGoalActivation.md](../Plans/Idea.SemanticGoalActivation.md), and this
  deterministic lexical layer doubles as its no-GPU fallback and evaluation harness.
- Mode-aware arbitration: `Mode` (`Neutral` / `Focused` / `Exploratory`) reads its bias
  per goal via `Mode::tension_delta`, sourced from each tension's own `focused_bias` /
  `exploratory_bias` fixture data rather than a hardcoded vector — a persona swap is a
  fixture-data change, not a code change. A `PROTECTED_TIER_FLOOR` makes safety/boundary
  tiers immune to bias, and per-goal `BiasOutcome` records carry the applied delta.
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
  `matched_keywords` (returns weighted `ActivationKeyword`s), `match_strength`,
  `compute_relevance`, `compute_relevance_with_salience`, `select_effect_for_goal`,
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
- Display-only functional signals in `qsf_volition::signals`
  ([crates/qsf_volition/src/signals.rs](../../crates/qsf_volition/src/signals.rs)): a pure,
  deterministic `derive_signals(state, fixture) -> Vec<FunctionalSignal>` that reads recorded
  `VolitionState` and emits named, evidence-derived readouts — `coherence_decline` (one per
  entry in `declined_candidates`: candidate title, conflict, rationale, tick), `frustration`
  (a goal `Blocked` at least `FRUSTRATION_BLOCKED_COUNT_THRESHOLD` times despite a prior
  activation), `satisfaction` (a `GoalSatisfied` within `SATISFACTION_RECENCY_WINDOW_TICKS`
  carrying its `last_satisfied_evidence_ref`), and `boredom` (every non-retired goal below
  `BOREDOM_SALIENCE_THRESHOLD`, past a prior-activation / `BOREDOM_MIN_ELAPSED_TICKS`
  cold-start guard). Each `FunctionalSignal { kind, intensity, evidence }` carries structured
  `evidence` naming the exact recorded state that justifies it; there is deliberately no
  `tension` kind (true tension remains reserved for an unresolved current conflict among
  selected goals). Signals are recomputed on demand, never stored on state, and never a felt
  claim. The gate is **structural**: the only consumers are the offline harness and the
  realtime capture builder — no code path into arbitration, salience, selection, initiative,
  context injection, or the model-visible `inspect_volition_state` tool. Offline-validated by
  the `volition-emotion-signals` experiment
  ([crates/qsf_app/src/experiments/volition_emotion_signals.rs](../../crates/qsf_app/src/experiments/volition_emotion_signals.rs)),
  which re-derives every recorded signal from its own artifacts per the contract in
  [Experiment.VolitionEmotionLikeSignals.md](../Experiments/Experiment.VolitionEmotionLikeSignals.md).
  Surfaced to the operator panel only: a top-level `signals` list on `VolitionInspectionCapture`
  ([crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs](../../crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs)),
  populated by `derive_signals` in the capture builder and riding the existing `volition_state`
  websocket message; nested `VolitionStateInspection` and the `inspect_volition_state` tool are
  unchanged. Visualization-first stance recorded in the 2026-07-06 DecisionLog entry
  "Volition functional signals are visualization-first and operator-panel only".
- Conscious/subconscious goal visibility as an introspection-surfacing filter. `GoalVisibility`
  (`Conscious` | `Subconscious`, `#[serde(default) = Conscious]`) is part of the goal *definition*
  on `Goal` (fixture-authored; D3 runtime-immutability already covers it) and on
  `ProposedGoalCandidate` (defaulted/internal — live-formed candidates stay `Conscious`, and
  `json_schema_hint` deliberately omits it). Visibility never changes `select_goals_ranked`,
  `arbitrate_with_mode`, salience, the surfacing gate, or coherence — only *presentation*. A pure,
  never-stored `qsf_volition::visibility`
  ([crates/qsf_volition/src/visibility.rs](../../crates/qsf_volition/src/visibility.rs)) derives —
  from recorded facts only — which subconscious goals are **forced surfaced**:
  `forced_surfaced_goals(state, fixture)` returns a `RenderedInitiative` condition (a *rendered*
  initiative line, proven by reducer-backed `last_rendered_initiative_tick` /
  `last_rendered_initiative_ref`, distinct from a suppressed internal `last_initiative_tick`) or a
  `CoherenceConflict` condition (the goal named in a `DeclinedCandidate`). The realtime layer
  applies this as three surfaces: `inspect_volition_state` / `select_volition_goals` section
  subconscious goals into a labeled `subconscious_goals` block with their forcing condition and
  selection role (never merged into the ordinary lists), the operator panel keeps **full** detail
  and badges them (`VolitionInspectionCapture` gains `forced_surfaced`; the decision summary gains
  `winner_visibility` / `ambient_exposure`), and ambient turn injection reduces an ordinary
  subconscious winner to a labeled background-guidance line (withholding title/summary/id) via
  `AmbientExposure` (`ordinary` / `reduced_subconscious` / `forced_surfaced_subconscious`) while
  the trace keeps the full winner identity. Offline-validated by the `volition-goal-visibility`
  experiment
  ([crates/qsf_app/src/experiments/volition_goal_visibility.rs](../../crates/qsf_app/src/experiments/volition_goal_visibility.rs)),
  which re-derives every forcing condition and the visibility-flip invariant from recorded state
  per [Experiment.VolitionGoalVisibility.md](../Experiments/Experiment.VolitionGoalVisibility.md).
  Stance recorded in the 2026-07-06 DecisionLog entry "Subconscious volition goals use reduced
  ambient exposure".

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

## Persona And Fixture Experimentation

A persona is fixture data (DecisionLog 2026-07-03): the tensions and goals in
[crates/qsf_volition/src/fixture.rs](../../crates/qsf_volition/src/fixture.rs) shape live behavior,
and adding a new "personality" means adding tensions and goals with the character you want, at the
right tier. The fixture source is authoritative for the current roster; the curiosity-observer
roster is also named in the 2026-07-03 decision entry.

**Immediate (current session only):** edit `static_fixture()` or `realtime_seed_fixture()`, add a
`Tension` + `Goal`, rebuild, and run `qsf.ps1 realtime`. After `cargo build`, the browser volition
panel shows mode, tick, winning goal, tier/protection status, shaping intensity, and initiative
outcome live on every trusted turn — so a new goal can be watched winning or losing arbitration in
real time without inspecting JSONL.

Tier placement guidance:

- Tier 4–6: biasable band — mode bias can reorder these relative to each other.
- Tier 7+: lowest priority, easily outranked (but still fires when nothing else matches).
- Tier ≤ 3: protected floor — use only for genuine safety/user-intent constraints; immune to bias
  and never cancelled.

**Cross-session persistence:** write a `volition-seed.reviewed.json` (the reviewed-seed format from
`qsf_volition::continuity`) and run `accept-reviewed-volition-seed` to merge it into future
sessions. New goals in the reviewed seed cannot be admitted at tier ≤ 3 (the `apply_reviewed_seed`
invariant enforces this). The `volition-continuity` experiment runs the consolidation pass.

## Related Documents

- [Architecture.ContextManagement.md](Architecture.ContextManagement.md) — context
  assembly that adapters layer on top of volition selections.
- [Architecture.StateAndObservability.md](Architecture.StateAndObservability.md) — how
  volition state and traces are observed.
- `docs/DecisionLog.md` — crate-boundary and bounded-initiative decisions.
