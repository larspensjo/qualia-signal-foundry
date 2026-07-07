# Plan: Volition Motivational Texture

## Maturity

Candidate. Phases 1–3 are implemented and compacted to summaries below — Phase 1 (goal
coherence under a protected floor), Phase 2 (live goal formation and off-hot-path coherence),
and Phase 3 (emotion-like signals, visualization-first), which is offline-validated and
live-browser verified for coherence-decline signal rows. Phase 4 (conscious/subconscious
visibility) is implemented and closed after the 2026-07-07 live human session — see
`Experiment.VolitionGoalVisibility.md` for results and the two accepted follow-ups. Phase 5
remains sequenced but not yet specified.

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
- **Human voice testing passed (2026-07-07),** run as one session with Phase 4's live
  introspection check: live admit, live decline grounded in the named goal, durable declined
  injection, and hot-path latency parity all verified. One probe stayed untriggered — the
  "always agree" phrasing produced a verbal refusal but no formed candidate; retest with
  wording matching the explicit-request extraction, and consider widening that extraction. See
  [Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md).

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

### Phase 4 — Conscious / subconscious visibility (brief §6) — done

The visibility slice is built, offline-validated, and closed by the 2026-07-07 live human
session. A `GoalVisibility` attribute (`Conscious` | `Subconscious`, `#[serde(default)]` =
`Conscious` for back-compat) on `Goal` and `ProposedGoalCandidate` acts purely as an
introspection-*surfacing filter*: selection, arbitration, salience, the surfacing gate, and
coherence are untouched — proven by the visibility-flip invariant
([DecisionLog 2026-07-06](../DecisionLog.md#2026-07-06---subconscious-volition-goals-use-reduced-ambient-exposure)).
The pure, derive-on-demand module
[`qsf_volition::visibility`](../../crates/qsf_volition/src/visibility.rs) computes forced
surfacing from recorded facts only: a *rendered* initiative line (backed by new reducer-side
rendered-initiative evidence distinguishing rendered from suppressed `InitiativeExecuted`) or
being named as the conflicting goal in a `DeclinedCandidate`. Only presentation changed:
`inspect_volition_state` / `select_volition_goals` section subconscious goals into a labeled
`subconscious_goals` section (with `winner_visibility` and selection roles), the operator panel
badges them without hiding anything (D2), turn traces carry the full visibility/surfacing chain
plus an `ambient_exposure` treatment, and ordinary subconscious arbitration winners render
reduced ambient text while forced-surfaced ones render labeled full detail. The seed fixture
marks one non-protected goal (`assemble-world-picture`) `Subconscious`, so the default
configuration exercises the path. Offline validation, the trace contract, and the live session
are recorded in
[Experiment.VolitionGoalVisibility.md](../Experiments/Experiment.VolitionGoalVisibility.md).

The live session (combined with Phase 2's open voice test) confirmed both forcing conditions
end-to-end and the D4 honest-instrument-readout quality: asked whether it *feels* the
background tendency, the model answered it is "something I can read off the simulated internal
state, not something I feel."

**Constraints and follow-ups that carry forward:**
- Visibility is a fixture-authored definition attribute; no runtime path flips it (D3).
  Live-formed candidates are always `Conscious` (`LiveGoalFormationJudge` never emits
  `Subconscious`; a schema-hint guard test documents the deliberate exclusion). A future
  sleep-consolidation path forming subconscious goals remains out of scope.
- `last_initiative_output` alone is never evidence of surfacing — suppressed initiative
  executions are recorded too; only the rendered-initiative evidence proves a line reached the
  user.
- **Accepted follow-ups (not blockers):** the `reduced_subconscious` ambient treatment was
  never observed live (harness- and injection-test-proven only), so whether reduced ambient
  text still shapes coherently stays open — it needs a live turn where the subconscious goal
  wins without a rendered initiative; and the live path persists only a counts-level summary of
  the introspection tool output, not the full sectioned JSON artifact the offline harness
  captures.

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
- **Done at Phase 4 detailing:** `Experiment.VolitionGoalVisibility.md` was written with its
  trace contract before implementation; the DecisionLog entry adopting
  visibility-as-surfacing-filter and reduced ambient exposure for ordinary subconscious winners
  is the
  [2026-07-06 entry](../DecisionLog.md#2026-07-06---subconscious-volition-goals-use-reduced-ambient-exposure).
- **Done on implementing Phase 4:** the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) now carries the
  visibility attribute, the pure `qsf_volition::visibility` surfacing-policy module, the sectioned
  introspection surface, the operator-panel badges, and the reduced ambient injection;
  [Glossary.md](../Glossary.md) has `GoalVisibility`, `Forced surfacing`, and `AmbientExposure`
  rows and the brief-translation rows for conscious/subconscious goals are marked delivered; the
  brief's §6 is annotated **delivered (translated)**; `Experiment.VolitionGoalVisibility.md`
  records the offline validation and the 2026-07-07 live human session that closed the phase
  (two follow-ups accepted: live reduced-ambient observation, live persistence of the full
  introspection tool output).
- When a later phase is detailed: write its `Experiment.*.md` scaffold and trace contract.
- **Done as brief concepts landed:** the brief's §12 is annotated **not-adopted** (ownership
  declined), §11 **delivered** through coherence, and §8 **delivered (translated)** now that the
  signal slice ships. Delete the brief once nothing in it remains unmerged.
