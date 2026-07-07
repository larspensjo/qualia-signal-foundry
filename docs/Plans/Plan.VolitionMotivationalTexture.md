# Plan: Volition Motivational Texture

## Maturity

Candidate. Phases 1–3 are implemented and compacted to summaries below — Phase 1 (goal
coherence under a protected floor), Phase 2 (live goal formation and off-hot-path coherence),
and Phase 3 (emotion-like signals, visualization-first), which is offline-validated and
live-browser verified for coherence-decline signal rows. Phase 4 (conscious/subconscious
visibility) is specified below, including the reduced ambient-injection treatment for
subconscious winners. Phase 5 remains sequenced but not yet specified.

## Purpose

The realtime volition system is fully built and human-tested: tensions, goals, salience,
arbitration, mode bias, opportunity detection, shaping-intensity dial, bounded initiative in
the live loop, cross-session continuity, and a browser volition panel. See
[Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md).

This plan gives volition more **inspectable motivational texture** so the system reads as a
*distinct, motivated agent* — without reopening the evidence-based, anti-anthropomorphic
stance (DecisionLog 2026-05-15, 2026-06-27, 2026-06-30).

The spine of that work is **goal coherence**. The imported brief proposed tagging goals by
owner — user / simulator / shared (§12). That ownership model is **declined**
([DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---adopted-goals-belong-to-the-simulation-coherence-replaces-goal-provenance)):
every goal the simulation adopts belongs to the simulation, whatever its origin. What makes
it read as a separate agent is not a label but its capacity to **own its goals, keep them
mutually consistent, and decline input that would make it incoherent**. Origin survives only
as an optional background memory/association, never a class of goal. The brief's other three
deferred concepts — emotion-like signals (§8), conscious/subconscious visibility (§6), and
multi-turn Plans (§3.5) — follow, each building on the coherent-agent substrate.

## Guardrails (carry into every phase)

- Project vocabulary stays authoritative; nothing is renamed (reconciliation D1).
- No claim of subjective experience; all new state is inspectable and trace-backed (D2).
- "Emotion" is only ever a named, evidence-derived functional signal — never a felt state,
  never used to confabulate narration (D4).
- New goals cannot enter at or below the protected tier floor. Protected goal *definitions
  and core membership* cannot be formed, edited, replaced, or cancelled at runtime (D3); their
  dynamic state (salience, status) still changes through the normal lifecycle. The
  coherence-specific rule: never cancel a protected goal, never admit into the protected floor.
- Contradiction detection is **model judgment isolated in an adapter**; its verdict is
  recorded as a trace artifact and fed back into the pure reducer as events. The model
  *detects*; the pure reducer *resolves* deterministically
  ([DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---goal-coherence-is-model-judged-off-the-hot-path-and-repaired-during-sleep),
  [2026-05-09](../DecisionLog.md#2026-05-09---unidirectional-event-reducer-state-flow)).
- Per [Agents.md](../../Agents.md): any phase whose behavior is explained by traces needs a
  trace-completeness contract (required fields, artifact boundary, artifact-parsing
  verification) defined before implementation.

## Phases (in order)

Ordered by increasing cost and decreasing certainty. The coherence engine came first
because every later concept (an honest conflict signal, subconscious bias, multi-turn plans)
is more legible once goals are a consistent, owned set.

### Phase 1 — Goal coherence under a protected floor (offline engine) — done

The reusable, model-judged coherence engine is built and proven offline: a model *detects*
contradictions (`CoherenceVerdict`), pure functions in
[`qsf_volition::coherence`](../../crates/qsf_volition/src/coherence.rs) *resolve* them
deterministically (`resolve_admission`, `resolve_sweep`, plus the hard tier-floor gate) into
the **existing** goal-lifecycle events — no new event types. Admission judges
`{existing goals + one candidate}`; the sweep judges the whole set in one round-trip. The
`CoherenceJudge` adapter seam (scripted default, model-backed opt-in) validates verdicts
against the queried goal set. Validated by
[Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md);
durable stance in the two 2026-06-30 DecisionLog entries.

**Constraints that carry forward:** the tier-floor gate rejects floor-tier candidates before
any model call; the sweep never cancels a floor goal and flags floor-vs-floor contradictions
for human review. `reducer::effective_tier_from_tension_ids` is the one correct way to tier a
candidate (the old fixture-goals-only lookup mis-tiered candidates as `u8::MAX`).

### Phase 2 — Live goal formation and off-hot-path coherence — implemented

The Phase 1 engine is wired into the realtime loop: one cache-structured model call per
trusted turn, *after* the response (`tokio::task::spawn_blocking`, since
`ModelClient::complete` blocks), does formation + contradiction detection together
([`live_goal_formation.rs`](../../crates/qsf_realtime_server/src/realtime/live_goal_formation.rs));
pure `resolve_admission` decides. A rejection becomes a `DeclinedCandidate` on volition state
(reducer-derived from `GoalCandidateRejected` events carrying a `CoherenceDecline`, capped at
`DECLINED_CANDIDATES_WINDOW`) and is injected as a session-scoped `coherence` context layer
from the next turn onward
([volition_injection.rs](../../crates/qsf_realtime_server/src/realtime/volition_injection.rs)).
The sleep pass does whole-history formation plus the `resolve_sweep`. The model layer
(`ModelClient`, `ModelRole`/`ModelRoleId`, `CoherenceJudge`,
[`LiveGoalFormationJudge`](../../crates/qsf_models/src/live_goal_formation.rs)) was extracted
into the shared [`qsf_models`](../../crates/qsf_models/src/lib.rs) crate with a `ModelInvoker`
trait decoupling callers from observability backends. Rationale recorded in
[DecisionLog 2026-07-01](../DecisionLog.md#2026-07-01---live-goal-formation-and-coherence-detection-run-as-one-cache-structured-model-call-per-turn);
offline validation in
[Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md).

**Lessons and constraints that carry forward:**
- **Prompt caching is application-level here**: neither `openai_provider_kit` nor the raw API
  exposes a `cache_control` breakpoint; caching rides on a byte-stable prefix marked by
  `stable_prefix_message_count` / `stable_prefix_hash` (2026-07-01 DecisionLog addendum). Any
  later phase adding model calls should reuse that seam, not invent a provider field.
- `DeclinedCandidate` records (conflict + rationale + tick) are durable, evidence-backed
  session state — Phase 3's natural `coherence_decline` source. True `tension` remains reserved
  for unresolved current conflict among selected goals.
- A pending candidate is structurally unable to shape turns; only admission promotes it.
- **Open item:** human voice testing (the Human Test Steps in
  [Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md))
  has not yet been run. It does not block Phase 4's offline and panel work, but it should be
  run before conclusions about how the texture *feels* in conversation — and can be combined
  with Phase 4's live introspection check (see Phase 4 verification).

### Phase 3 — Emotion-like signals, visualization-first (brief §8) — implemented

The visualization-first functional-signal slice is built and offline-validated. Reducer
lifecycle facts (`blocked_count`, `last_blocked_tick`, and `last_satisfied_evidence_ref`, all
`#[serde(default)]` for snapshot back-compat) feed a pure derivation module
[`qsf_volition::signals`](../../crates/qsf_volition/src/signals.rs) whose
`derive_signals(state, fixture)` emits four named, evidence-derived signals —
`coherence_decline` (from `declined_candidates`), `frustration` (a goal `Blocked` past
`FRUSTRATION_BLOCKED_COUNT_THRESHOLD` despite activation), `satisfaction` (a recent
`GoalSatisfied` with its `last_satisfied_evidence_ref`), and `boredom` (every non-retired goal
below `BOREDOM_SALIENCE_THRESHOLD`, past a prior-activation / `BOREDOM_MIN_ELAPSED_TICKS`
cold-start guard). Each signal carries structured evidence resolving to recorded state, is
recomputed on demand, and is never stored — there is deliberately no `tension` kind. The offline
`volition-emotion-signals` harness
([volition_emotion_signals.rs](../../crates/qsf_app/src/experiments/volition_emotion_signals.rs))
drives every signal on and off and re-derives each from its own artifacts (the trace contract).
Signals are surfaced to the operator panel only: a top-level `signals` list on
`VolitionInspectionCapture` populated by the capture builder and rendered as a browser
"Functional signals" section ([realtime.ts](../../crates/qsf_realtime_server/ui/src/realtime.ts))
that never shows a bare emotion word without its evidence; nested `VolitionStateInspection` and
the `inspect_volition_state` tool are untouched. The gate is **structural** — the only consumers
are the capture builder and the harness. Durable stance in
[DecisionLog 2026-07-06](../DecisionLog.md#2026-07-06---volition-functional-signals-are-visualization-first-and-operator-panel-only);
offline validation and the trace contract in
[Experiment.VolitionEmotionLikeSignals.md](../Experiments/Experiment.VolitionEmotionLikeSignals.md).

**Lessons and what remains:**
- Every automated criterion passes: unit tests for presence *and* absence of all four signals,
  the reducer field tests (including re-blocking after satisfaction resets the counters), the
  harness artifact re-derivation, and the UI parser/view-model tests; `cargo build` / `clippy` /
  `fmt` and `npm run check` / `fmt` are clean.
- Continuity snapshots predating the new `GoalDynamicState` fields still load (`#[serde(default)]`).
- **Patterns Phase 4 should reuse:** the derive-on-demand, never-stored module shape
  (`signals.rs`), presence-*and*-absence unit-test discipline, `#[serde(default)]` back-compat
  for every new serialized field, and artifact re-derivation as the trace-contract check.
- Deferred, unchanged from the resolved scope: true D4 `tension` (needs unresolved
  current-conflict state), `curiosity` (needs an explicit open-delta record), `attachment`
  (needs cross-session reinforcement semantics), sustained N-tick boredom (needs salience
  history), and any model-visible signal exposure (would edge toward narration input; its own D4
  review). Feeding any signal into arbitration stays out of scope (see Parked questions).
- **Human review closed:** after the 2026-07-06 negative attempt, the live-formation adapter
  pre-extracts explicit goal requests so the existing coherence resolver can reject incoherent
  requests into declined-candidate state. The browser retest showed two `coherence_decline`
  functional-signal rows in Scoring detail, each carrying candidate title, tick, conflicting goal,
  rationale, and intensity. The operator interpretability review passed: the rows read as honest
  instrument readouts, not claimed feelings. Live `satisfaction` remains offline-harness-only
  until ordinary realtime turns emit `GoalSatisfied` lifecycle events.

### Phase 4 — Conscious / subconscious visibility (brief §6) — implemented (offline)

A visibility attribute on goals: a "subconscious" goal biases salience and arbitration exactly
like any other goal but is narrated only on introspection or when its behavior forces an
explanation. Partly latent already in the sideband surfacing gate + anti-nag wiring
([sideband_turn_injection.rs](../../crates/qsf_realtime_server/src/realtime/sideband_turn_injection.rs),
suppression reasons `Intensity` / `ProtectedNoOpportunity` / `AntiNagRepeat` /
`NonRenderableOutput`).

**Adopted resolution (closes the reconciliation's open question):** visibility is an
introspection-*surfacing filter*, not a separate runtime path. A goal's visibility never
changes `select_goals_ranked`, `arbitrate_with_mode`, salience dynamics, the surfacing gate's
decision logic, or the coherence engine — identical inputs must produce identical selection and
arbitration results whatever the visibility mix. Only *presentation* changes: which goals are
narrated, where. Ambient context follows the same policy with one extra constraint: an ordinary
subconscious arbitration winner is reduced in model-visible turn text rather than rendered as a
full `Active goal: {title} — {summary}` line. Full subconscious detail is still available to the
operator panel, traces, explicit `inspect_volition_state` / `select_volition_goals` tool calls,
and forced-surfacing cases with evidence. Recorded in
[DecisionLog 2026-07-06](../DecisionLog.md#2026-07-06---subconscious-volition-goals-use-reduced-ambient-exposure).

#### Design

- New `GoalVisibility` enum (`Conscious` | `Subconscious`) in `qsf_volition::model`, carried
  as a `visibility` field on `Goal` and `ProposedGoalCandidate`, `#[serde(default)]` =
  `Conscious` so existing fixtures, continuity snapshots, and previously captured artifacts
  still deserialize (the Phase 3 back-compat pattern).
- `ProposedGoalCandidate` visibility is defaulted/internal for this slice. Live-formation model
  prompts and `ProposedGoalCandidate::json_schema_hint()` must not invite the model to set
  `Subconscious`; live-formed candidates are conversation-originated and therefore conscious.
  If the deserializer accepts an optional `visibility` field for back-compat or future
  sleep-consolidation artifacts, the schema-hint guard test must document the deliberate
  exclusion instead of failing from silent drift.
- Visibility is part of the goal *definition* (fixture-authored). D3's runtime-immutability of
  definitions therefore already covers it — there is no runtime path that flips visibility.
  A protected-floor goal may be subconscious; visibility and tier are orthogonal.
- Live-formed candidates are always `Conscious`: they originate in conversation, so they are
  introspectable by construction (`LiveGoalFormationJudge` output never sets `Subconscious`).
  A future sleep-consolidation path forming subconscious goals is explicitly out of scope.
- The realtime seed fixture (`realtime_seed_fixture`) marks at least one goal `Subconscious` —
  a background disposition-style goal — so the **default configuration exercises the new code
  path** (Agents.md rule). Choosing which seed goal (or adding a new one) is an implementation
  detail; it must not be a protected-floor goal in the first slice, to keep the experiment's
  conflict scenario free to decline against it.
- A pure, behavior-named derivation module `qsf_volition::visibility` computes — on demand,
  never stored, mirroring [`signals.rs`](../../crates/qsf_volition/src/signals.rs) — which
  subconscious goals are **forced surfaced**, from recorded facts only:
  - *rendered initiative*: the goal has a recorded rendered/surfaced initiative fact, not merely
    `GoalDynamicState.last_initiative_output`. The realtime path records
    `InitiativeExecuted` even when the line is suppressed by intensity,
    protected-no-opportunity, anti-nag, or non-renderable output; those suppressed internal
    initiatives must not count as forced surfacing. Add reducer-backed, `#[serde(default)]`
    initiative evidence such as `last_initiative_tick`, `last_rendered_initiative_tick`, and a
    rendered initiative evidence/artifact reference so a pure derivation can prove the line was
    actually rendered;
  - *forced conflict*: the goal is named as the conflicting goal in a `DeclinedCandidate`
    record (the decline record already names it; hiding it would make the coherence layer
    incoherent).
  The brief's third condition — "the user asks for introspection" — needs no derivation:
  calling `inspect_volition_state` *is* the ask, so the tool always reports subconscious goals,
  but in a separate labeled section (below), never silently merged into the ordinary lists.

#### Steps (each independently implementable and reviewable)

1. **Attribute plumbing.** Add `GoalVisibility` and the `visibility` field on `Goal`,
   `ProposedGoalCandidate`, and the seed fixture (one subconscious seed goal); thread it onto
   `GoalStatusSummary` in
   [`inspection.rs::build_state_inspection`](../../crates/qsf_volition/src/inspection.rs)
   (keep `build_state_inspection` complete and unfiltered — it feeds the operator capture).
   No consumer behavior changes yet. Tests: serde-default back-compat for fixtures, continuity
   snapshots, and capture JSON lacking the field; summaries carry visibility.
2. **Pure surfacing policy.** Add the reducer-backed rendered-initiative evidence above, then
   implement `qsf_volition::visibility` with a function of shape
   `forced_surfaced_goal_ids(state, fixture) -> …` deriving the two forcing conditions above.
   Unit tests for presence *and* absence of each condition (Phase 3 test discipline), including
   a suppressed `InitiativeExecuted` that must **not** force surface; plus a test that the same
   scenario with the goal marked `Conscious` yields identical `select_goals_ranked` /
   `arbitrate_with_mode` results (the no-runtime-effect invariant).
3. **Simulator-facing introspection sectioning.**
   [`volition_tools.rs`](../../crates/qsf_realtime_server/src/realtime/volition_tools.rs):
   `inspect_volition_state` moves subconscious goals out of the per-status lists into a
   `subconscious_goals` section, each entry carrying its status, visibility, and forcing
   condition (if any). `select_volition_goals` must keep the arbitration explanation complete
   while separating presentation: keep `arbitration` truthful, add `winner_visibility`, and put
   subconscious selected goals in `subconscious_goals` entries that carry their selection role
   (`selected_non_winner`, `winner`, `below_threshold`, etc.), status, visibility, forcing
   condition, and match detail. Do not leave a subconscious winner only as
   `arbitration.winner_id` with no section entry, and do not silently merge subconscious goals
   into the ordinary selected-goal list. A tool call is explicit introspection/selection, so full
   detail may be returned there when sectioned and labeled. Update both tool descriptions so the
   model knows the section exists and what it means. Tool-layer tests over the JSON shape,
   including a subconscious selected non-winner and a subconscious winner.
4. **Operator panel.** The operator panel keeps **full** visibility (guardrail D2 — all state
   inspectable): `VolitionInspectionCapture` already carries the unfiltered inspection; the
   browser panel ([realtime.ts](../../crates/qsf_realtime_server/ui/src/realtime.ts)) badges
   subconscious goals and shows their forced-surfacing status, never hiding them from the
   operator. UI parser/view-model tests per the repo's UI testing rules; `npm run check` and
   `npm run fmt` from the crate's `ui/`.
5. **Turn-trace fields.** No change to the surfacing gate's decision logic — a subconscious
   winner that renders an initiative line *is* the forced-surfacing event. Persist the rendered
   fact explicitly when applying `InitiativeExecuted`; `last_initiative_output` alone is
   insufficient evidence because suppressed outputs are recorded too. Add visibility to the turn
   trace per the contract below (winner visibility; per-goal visibility on `selector_output`;
   subconscious selected count; rendered-line flag and suppression reason), so an operator can
   reconstruct exactly which subconscious goals shaped a turn and which ones actually surfaced.
6. **Ambient injection treatment.** When the arbitration winner is subconscious and has no
   forced-surfacing condition, reduce the model-visible rendered packet: do not render the
   winner's title/summary as the ordinary `Active goal` line. Instead render a labeled
   background-guidance line carrying only the minimum shaping contract the response model needs
   (visibility, intensity, safe guidance, and request/artifact reference), while the trace keeps
   the full winner identity and summary. If the same turn has a forced-surfacing condition
   (rendered initiative or coherence conflict), full detail may be included, but it must be
   labeled as a surfaced subconscious/background goal and backed by the forcing evidence. Tests:
   conscious winners still render the current `Active goal` packet; ordinary subconscious
   winners render reduced text; forced-surfaced subconscious winners render labeled full detail;
   trace summaries remain complete in all three cases.

#### Resolved question

- **Q1 — ambient injected text.** Resolved on 2026-07-06: ordinary subconscious arbitration
  winners are reduced, not omitted and not rendered as ordinary full `Active goal` text.
  Explicit introspection/selection tools may return full detail when sectioned and labeled;
  operator panel and traces keep full detail; forced-surfacing cases may expose full detail with
  evidence. This preserves enough model-visible guidance for coherent shaping while making
  "subconscious" behaviorally meaningful in ordinary ambient context.

#### Trace-completeness contract (define fully in the experiment scaffold before implementing)

- **Required fields:** per-goal `visibility` on inspection summaries and on `selector_output`
  entries; arbitration-winner visibility; a forcing-condition record for every surfaced
  subconscious goal (`goal_id`, condition kind, evidence reference, tick); for rendered
  initiative forcing, the recorded rendered/surfaced flag, suppression reason, initiative tick,
  rendered tick, and artifact/request reference; ambient exposure treatment
  (`ordinary`, `reduced_subconscious`, `forced_surfaced_subconscious`) on the turn packet trace;
  the introspection tool's full JSON output captured as a trace artifact.
- **Artifact boundary:** `events.jsonl` keeps chronological lifecycle facts and is unchanged —
  this phase introduces **no new event types**. `InitiativeExecuted` may gain defaulted rendered
  evidence fields, but suppressed initiative executions remain distinguishable from rendered
  initiative lines. Trace records carry the visibility/surfacing chain; the human-readable
  report derives from the structured artifacts.
- **Artifact-parsing verification:** the offline harness parses its own artifacts, asserts the
  required fields exist, and re-derives every surfaced subconscious goal's forcing condition
  from recorded state alone (the Phase 3 re-derivation pattern), including proving that a
  suppressed internal initiative does not force surface.

#### Verification and acceptance criteria

- Write `Experiment.VolitionGoalVisibility.md` (behavior-named — no plan phase numbers) with
  the trace contract *before* implementation. Its offline harness (pattern:
  [volition_emotion_signals.rs](../../crates/qsf_app/src/experiments/volition_emotion_signals.rs))
  drives one subconscious goal through: (a) selected and biasing with no forcing condition —
  absent from simulator-facing status lists, present with badge on the operator capture;
  (b) winning arbitration with a suppressed initiative — not forced surfaced; (c) winning
  arbitration with a rendered initiative line — forced surfaced; (d) named as the conflicting
  goal in a coherence decline — forced surfaced; (e) an `inspect_volition_state` call —
  reported in the `subconscious_goals` section. It also asserts the invariant: identical
  selection and arbitration outcomes when the same goal is marked `Conscious`.
- Automated: unit tests per steps 1–3 and 5 (presence *and* absence); back-compat
  deserialization tests; harness artifact re-derivation; `cargo build`,
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt`; for the UI step, `npm run check`
  and `npm run fmt` in `crates/qsf_realtime_server/ui`.
- **External human testing recommended:** (1) browser operator panel — badges and sectioning
  read clearly and nothing is hidden from the operator; (2) a live-voice introspection ask —
  the sectioned reply reads as an honest instrument readout ("a background tendency I can
  report"), never a claimed hidden feeling (D4). Combine this session with Phase 2's still-open
  human voice test.

### Phase 5 — Multi-turn Plans (brief §3.5, §4.6)

A genuinely new domain structure: a `Plan` sequencing initiatives across turns with
suspend / resume / abandon. The current system is single-turn initiative.

- **Cost note:** largest new structure; most likely to feel mechanical. Deferred last
  deliberately — revisit need after earlier phases add texture, and prove offline before the
  live loop.
- **Verification:** offline Experiment scaffold over the plan lifecycle before any live wiring.

## Parked questions

- **Initiative derivation:** stay rule-based (`execute_initiative`) or add a later model-assisted
  proposer emitting the same `InitiativeOutput` shape. Default: rule-based only. Revisit if the
  rule-based outputs feel mechanical after more personality experimentation (a natural checkpoint
  is the emotion-like-signals work, which adds texture on top of the same outputs).
- **Signals feeding arbitration:** deliberately excluded from the visualization-first slice;
  reopen only with a dedicated decision after the signals have been observed live.

## Documents to update (per ProjectWorkflow.md)

- **Done at Phases 1–2:** coherence stance and cadence decisions are in
  [DecisionLog.md](../DecisionLog.md) (two 2026-06-30 entries; 2026-07-01 entry + caching
  addendum); validation scaffolds are
  [Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md)
  and
  [Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md);
  the coherent-agent stance is in [ProjectVision.md](../ProjectFrame/ProjectVision.md).
- **Done at Phase 3 detailing:** `Experiment.VolitionEmotionLikeSignals.md` with its trace
  contract is written; the DecisionLog entry for the visualization-first signal set is the
  2026-07-06 entry.
- **Done on implementing Phase 3:** the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) now carries
  both the signals slice and Phase 2's additions (shared model crate, live formation +
  off-hot-path admission, declined-candidate injection layer, sleep formation + sweep); the
  experiment Results record the offline validation and completed live browser coherence-decline
  review.
- **At Phase 4 detailing (before implementation):** write `Experiment.VolitionGoalVisibility.md`
  with its trace contract. The DecisionLog entry adopting visibility-as-surfacing-filter and
  reduced ambient exposure for ordinary subconscious winners is done
  ([2026-07-06](../DecisionLog.md#2026-07-06---subconscious-volition-goals-use-reduced-ambient-exposure)).
- **Done on implementing Phase 4:** the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) now carries the
  visibility attribute, the pure `qsf_volition::visibility` surfacing-policy module, the sectioned
  introspection surface, the operator-panel badges, and the reduced ambient injection;
  [Glossary.md](../Glossary.md) has `GoalVisibility`, `Forced surfacing`, and `AmbientExposure`
  rows and the brief-translation rows for conscious/subconscious goals are marked delivered; the
  brief's §6 is annotated **delivered (translated)**; `Experiment.VolitionGoalVisibility.md`
  records the offline validation. Open: the browser + live-voice human review.
- When a later phase is detailed: write its `Experiment.*.md` scaffold and trace contract.
- **Done as brief concepts landed:** the brief's §12 is annotated **not-adopted** (ownership
  declined), §11 **delivered** through coherence, and §8 **delivered (translated)** now that the
  signal slice ships. Delete the brief once nothing in it remains unmerged.
