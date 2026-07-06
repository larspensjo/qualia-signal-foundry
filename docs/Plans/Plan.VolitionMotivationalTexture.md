# Plan: Volition Motivational Texture

## Maturity

Candidate. **Detail level: Phase 3 detailed** — Phase 1 (goal coherence under a protected
floor) and Phase 2 (live goal formation and off-hot-path coherence) are implemented and
compacted to summaries below. Phase 3 (emotion-like signals, visualization-first) is now
expanded into implementation steps. Phases 4–5 remain sequenced but not yet specified.

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
  has not yet been run. It does not block Phase 3's offline and panel work, but it should be
  run before any Phase 3 conclusions about how the texture *feels* in conversation.

### Phase 3 — Emotion-like signals, visualization-first (brief §8)

Derive named functional signals from existing goal/delta state per reconciliation D4
([Design.VolitionBriefReconciliation.md](Design.VolitionBriefReconciliation.md)): frustration
= repeatedly `Blocked` despite activation; satisfaction = `GoalSatisfied` + `EvidenceRef`;
coherence decline = a coherence-engine rejection recorded in `declined_candidates`; boredom = low
selected-goal salience across recent ticks. Pure derivations over recorded state — **no new mutable emotion object**:
a signal is a value recomputed from `VolitionState` on demand, never stored, never an input to
anything but display.

**Scope discipline (what "gated" means here):** visualization only. The gate is *structural*,
not a runtime flag — signal derivation has no code path into arbitration, salience, selection,
initiative, or context injection. There is nothing to toggle, so no config flag is needed and
the default build exercises the new path (per Agents.md). Feeding any signal back into
arbitration is a separate future decision (see Parked questions), out of scope.

**First signal set** — the four whose evidence already exists or needs only a small
reducer-derived counter: `coherence_decline`, `frustration`, `satisfaction`, `boredom`.
Deferred: true `tension` (reserved for an unresolved current conflict among selected goals,
per reconciliation D4), `curiosity` (needs an explicit open-delta representation the state does
not yet carry), and `attachment` (needs settled cross-session reinforcement semantics on top of
the continuity snapshot).

**Steps (each independently implementable and reviewable):**

1. **Detailing prerequisites (docs first).** Write
   `docs/Experiments/Experiment.VolitionEmotionLikeSignals.md` from the template, including
   the trace-completeness contract below, and add a DecisionLog entry recording the
   visualization-first stance, the chosen signal set, and each signal's functional
   definition. Resolve the open questions below with the reviewer before coding.

2. **Signal substrate in the reducer (Rust, pure, smallest slice).** `frustration` needs
   "repeatedly Blocked despite activation", but
   [`GoalDynamicState`](../../crates/qsf_volition/src/reducer.rs) has no repetition record.
   Add reducer-maintained bookkeeping — `blocked_count: u32` and
   `last_blocked_tick: Option<u64>`, both `#[serde(default)]` so existing continuity
   snapshots still deserialize — updated by the existing `GoalBlocked` arm of `apply`.
   `satisfaction` needs exact event evidence, but `progress_evidence_refs` merges
   `GoalProgressObserved` and `GoalSatisfied`; add
   `last_satisfied_evidence_ref: Option<EvidenceRef>` with `#[serde(default)]`, set only by
   `GoalSatisfied`, pairing with the existing `last_satisfied_tick`. `blocked_count` is
   since-last-satisfaction lifecycle state: increment it on `GoalBlocked`, set
   `last_blocked_tick` on `GoalBlocked`, and reset both `blocked_count` and `last_blocked_tick`
   on `GoalSatisfied` so a later single block cannot re-trigger frustration from stale
   history. These are lifecycle facts, not emotion state; the reducer stays pure. Unit tests
   on the new fields, including re-blocking after satisfaction; no other behavior changes.

3. **Pure derivation module** `crates/qsf_volition/src/signals.rs` (`lib.rs` stays a thin
   re-export). A `FunctionalSignal { kind, intensity, evidence }` where `evidence` names the
   recorded state justifying it (goal ids, ticks, `EvidenceRef`s, declined-candidate
   conflict + rationale), and `derive_signals(state: &VolitionState, fixture: &VolitionFixture)
   -> Vec<FunctionalSignal>`:
   - `coherence_decline` — from `state.declined_candidates`: evidence is the rejected
     candidate title, conflict (`DeclineReason`), rationale, and tick. Do not label this
     `tension`; true tension remains reserved for an unresolved current conflict among selected
     goals and needs a separate persisted substrate or replay input.
   - `frustration` — goals with `status == Blocked`, `blocked_count` at or above a named
     threshold constant, and `last_activated_tick` present ("despite activation");
   - `satisfaction` — goals with a recent `last_satisfied_tick` and
     `last_satisfied_evidence_ref`;
   - `boredom` — every non-retired goal's salience below a named threshold for the current
     tick, with a prior-activity guard so a fresh session whose salience values all start at
     zero does not count. The guard can be satisfied by at least one prior goal activation or
     by the state passing a named minimum elapsed-tick threshold.
   Thresholds are named constants next to the existing salience constants, with defaults
   chosen so scripted fixtures exercise every signal. Exhaustive unit tests: each signal
   appears exactly when its evidence exists, and every emitted signal's evidence fields are
   non-empty and resolve to state that is actually present.

4. **Offline experiment harness** `volition-emotion-signals` in
   `crates/qsf_app/src/experiments/` (pattern:
   [live_goal_formation_and_coherence.rs](../../crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs)).
   Wire it through the experiment registry (`ExperimentName` enum, experiment id/description
   mapping, and `experiment_for` dispatch) so `scripts/qsf.ps1 app -Experiment
   volition-emotion-signals` works, with a small availability/dispatch test.
   Scripted event sequences drive each signal on and off (e.g. repeated `GoalBlocked` raises
   frustration; `GoalSatisfied` with evidence produces satisfaction and clears frustration for
   that goal; a coherence-engine rejection produces `coherence_decline`). Each derivation is recorded as an
   `emotion-signal-derivation` trace record; the harness parses its own artifacts and asserts
   the trace contract.

5. **Realtime surfacing (Rust).** Extend
   [`VolitionInspectionCapture`](../../crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs)
   with a top-level `signals` list populated via `derive_signals` in
   `build_volition_inspection_capture`, flowing through the existing `volition_state`
   websocket message (`push_volition_inspection` in
   [routes.rs](../../crates/qsf_realtime_server/src/realtime/routes.rs)) — no new transport.
   Leave nested `VolitionStateInspection` and the `inspect_volition_state` tool unchanged:
   signals are operator-panel only in this phase, not model-visible introspection input.

6. **Volition panel section (TypeScript).** In `crates/qsf_realtime_server/ui/`: extend the
   `volition_state` message parser and top-level `VolitionInspectionCapture` types, and add a
   "Functional signals" section to the `VolitionPanelModel` view-model
   ([realtime.ts](../../crates/qsf_realtime_server/ui/src/realtime.ts)) — one row per signal
   with its evidence text (e.g. *coherence decline: declined "X", conflicts with goal Y —
   rationale*), so the display never shows a bare emotion word without its evidence. Keep derivation of
   rows in the pure view-model, components render only. Tests at the parser/reducer/view-model
   level per project UI testing rules; `npm run check` + `npm run fmt` from the `ui/`
   directory.

**Trace-completeness contract** (finalized in the experiment spec at step 1):
- required fields per `emotion-signal-derivation` record: `tick`, `signal_kind`,
  `intensity`, `evidence` (goal ids / declined-candidate ids / evidence refs / threshold
  values used), and a `dynamic_state_snapshot` reference sufficient to recompute the signal;
- artifact boundary: `traces.jsonl` is the lifecycle-fact and derivation boundary for this
  experiment. Each `emotion-signal-derivation` trace record includes the applied lifecycle
  `VolitionEvent`s needed to reconstruct the relevant state slice, the
  `dynamic_state_snapshot`, and the emitted signal evidence. `events.jsonl` keeps the existing
  experiment pattern: generic `TraceRecorded` entries that link to trace ids, not a new
  lifecycle-event log shape. The report summarizes per-signal outcomes from the trace records.
- verification: the harness parses `traces.jsonl`, replays the included lifecycle
  `VolitionEvent`s or checks the included dynamic snapshot as appropriate, re-derives each
  signal, and asserts it matches the trace record — proving signals derive **only** from
  recorded state.

**Acceptance criteria:**
- `derive_signals` is pure, deterministic, and covered by unit tests for presence *and*
  absence of every signal in the first set.
- Every emitted signal carries non-empty evidence resolving to recorded state; the harness
  asserts this from parsed artifacts alone (trace contract satisfied).
- No arbitration, selection, initiative, or context-injection code path reads signals
  (reviewable by inspection: the only consumers are the capture builder and the harness).
- Continuity snapshots from before the new `GoalDynamicState` fields still load.
- The browser volition panel shows the signal section with evidence during a live session.
- `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt` clean; `npm run
  check` + `npm run fmt` clean in `crates/qsf_realtime_server/ui/`.

**Verification guidance:** steps 2–4 are fully offline (unit tests + harness artifacts).
Steps 5–6 need a running realtime server with the browser panel open — a short session where
the operator provokes a coherence decline and a satisfied goal, then confirms the panel rows
match the capture. **Human review is recommended** at the end, for interpretability rather
than correctness: do the displayed signals read as honest instrument readouts, not as claimed
feelings? (Same review lens as the activation dashboard,
[Design.LiveActivationDashboard.md](Design.LiveActivationDashboard.md).)

**Resolved Phase 3 decisions:**
- **First set:** `{coherence_decline, frustration, satisfaction, boredom}`. True D4
  `tension` needs unresolved current-conflict state this phase should not build; the other two
  D4 signals also need substrate this phase should not build (curiosity: an explicit
  open-delta record; attachment: cross-session reinforcement semantics).
- **Should the model see its own signals?** Decision for this phase: no. Signals are attached
  only to the top-level realtime `VolitionInspectionCapture` consumed by the operator panel.
  Extending `VolitionStateInspection` would expose signals through the `inspect_volition_state`
  tool, letting the model self-report them — which edges from visualization toward narration
  input and deserves its own D4 review. Tool exposure is a separate later decision.
- **Boredom window semantics:** Decision for this phase: current-tick low salience plus a
  prior-activity guard. Sustained N-tick boredom is deferred until the reducer records salience
  history or a replay-based derivation is deliberately introduced. The experiment spec should
  name the salience threshold and the prior-activity guard constants, and include cold-start
  absence coverage.

### Phase 4 — Conscious / subconscious visibility (brief §6)

A visibility attribute on goal selection: a "subconscious" goal biases salience/arbitration but
surfaces only on introspection or forced conflict. Partly latent already in the sideband
surfacing gate + anti-nag wiring.

- **Resolution leaning:** treat as an introspection-*surfacing filter*, not a separate runtime
  path (the reconciliation's open question).
- **Attach point:** the selection/inspection layer (`build_state_inspection`) + surfacing gate.
- **Verification:** Experiment scaffold over what surfaces vs what only biases.

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
- **At Phase 3 detailing (step 1 of the phase):** write
  `Experiment.VolitionEmotionLikeSignals.md` with its trace contract; add the DecisionLog
  entry for the visualization-first signal set.
- **On implementing Phase 3:** refresh the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) — it should
  also still gain Phase 2's additions (shared model crate, live formation + off-hot-path
  admission, declined-candidate injection layer, sleep formation + sweep) if not yet folded in.
- When a later phase is detailed: write its `Experiment.*.md` scaffold and trace contract.
- As brief concepts land in project docs, retire the corresponding brief sections: §12 is
  retired as **not-adopted** (ownership declined); §11 is **delivered** through coherence; §8
  is retired as **delivered (translated)** once the signal slice ships. Delete the brief once
  nothing in it remains unmerged.
