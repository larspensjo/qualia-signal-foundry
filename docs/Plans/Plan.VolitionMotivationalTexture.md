# Plan: Volition Motivational Texture

## Maturity

Candidate. **Detail level: Phase 1 detailed** — the remaining phases are sequenced and
scoped but not yet specified. Phase 1 (goal coherence under a protected floor) has a full
spec below plus an `Experiment.*.md` scaffold and trace contract. Detailing the next phase is
the step after Phase 1 ships.

## Purpose

The realtime volition system is fully built and human-tested: tensions, goals, salience,
arbitration, mode bias, opportunity detection, shaping-intensity dial, bounded initiative in
the live loop, cross-session continuity, and a browser volition panel. See
[Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) and
[Handoff.Volition.md](../Handoff.Volition.md).

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

Ordered by increasing cost and decreasing certainty. The coherence engine comes first
because every later concept (an honest conflict signal, subconscious bias, multi-turn plans)
is more legible once goals are a consistent, owned set.

### Phase 1 — Goal coherence under a protected floor (offline engine)

Build the reusable, model-judged coherence engine and prove it offline before any live
wiring. The engine decides whether a proposed goal may be **admitted** (it does not
contradict a more fundamental goal), and periodically **re-checks** the whole goal set for
contradictions that have accumulated, **cancelling** the less-fundamental goal of a
contradicting pair. This is the substrate the brief's "separate agent" and truthful
goal-conflict (§11, §12) rest on.

**Why offline first:** the model judge, the pure resolution rules, and the admission /
rejection / cancellation lifecycle are the novel, hard parts and are identical regardless of
where candidates come from. Proving them over the existing candidate source, deterministically,
de-risks the live wiring (Phase 2). Consistent with the plan's "prove offline before the live
loop" discipline.

**Detect vs resolve (the core split).** Contradiction is semantic, so a **model** judges it —
non-determinism is expected and acceptable. Detection lives in an adapter (a `CoherenceJudge`
seam backed by a `ModelRole`, mock-by-default per the 2026-05-11 model-provider decision, real
model opt-in). The judge returns a `CoherenceVerdict` that is recorded as a trace artifact and
then handed to **pure** resolution functions in `qsf_volition` that emit reducer events. The
model never mutates state; the reducer never calls the model.

**Two triggers, one primitive:**
- **Admission:** run the judge over `{existing goals + one candidate}` and ask whether the
  candidate contradicts any existing goal.
- **Sweep:** run the judge once over the whole goal set (all tiers, one round-trip) to catch
  drift between goals that have come to contradict each other.

**Admission rule (per candidate):**
1. **Hard tier-floor gate (deterministic, model-independent):** a candidate whose effective
   tier is at or below the protected floor is rejected outright, before any model call. Nothing
   new enters the core. This pre-judge rejection records a distinct trace shape with no verdict
   (`judge_ref: null`, `contradictions: []`, `resolution: reject_protected_floor`).
2. **Coherence gate (model verdict → pure resolution):** effective tier is computed from a
   goal's or candidate's `tension_ids` against `fixture.tensions` (never by id lookup in
   `fixture.goals`, which mis-tiers accepted/proposed candidates as `u8::MAX`). For each
   contradiction the verdict pairs between the candidate and an existing goal `X`:
   - candidate equal-or-less fundamental than `X` (candidate tier number ≥ `X`) → the candidate
     is **rejected** (`X` joins `rejected_by`);
   - candidate strictly more fundamental than `X` (lower tier number) → `X` is a **cancellation
     target** (`X` is above the floor by construction).
   Reject dominates: if the candidate is rejected by any contradiction it is not admitted, and
   its cancellation targets are recorded as `suppressed_cancellations_due_to_rejection` (nothing
   is cancelled on a rejected admission).

Resolution is a multi-conflict record — `{ admitted, rejected_by, cancelled_goal_ids,
suppressed_cancellations_due_to_rejection }` — mapped to the **existing** lifecycle events (no
new event types): admit → `GoalCandidateAccepted`; reject → `GoalCandidateRejected { reason }`
(reason carries the conflicting goal id); cancel → `GoalRetired`. Deterministic event order:
emit all `GoalRetired` cancellations first, then the `GoalCandidateAccepted`.

**Sweep rule (whole set):** the sweep snapshot merges `fixture.goals` with
`state.accepted_candidates` so every current goal is evaluated. For each contradicting pair,
cancel (`GoalRetired`) the higher-tier-number (less fundamental) goal; never cancel a goal at
or below the protected floor. Equal-tier tie-break (fully deterministic): treat a missing
`last_activated_tick` as tick 0 (the existing convention), cancel the goal with the greater
activation tick, and on a tick tie cancel the lexicographically greater goal id. A
contradiction between two floor goals is a fixture-curation error: it is **flagged for human
review** as a trace-only record (no state change, so not a reducer event), not auto-resolved.

- **Open question — resolved (candidate source):** Phase 1 admits candidates from the existing
  reflection path (`propose_goal_candidates()` over open questions), which already produces
  evidence-bearing `ProposedGoalCandidate`s. *Live discussion-formed candidates are Phase 2*,
  wired into this same engine. (Recommended default; open to change during implementation.)
- **Open question — resolved (sweep tie-break):** on an equal-tier contradiction, treat a
  missing `last_activated_tick` as 0, cancel the greater activation tick, and break a tick tie
  by cancelling the lexicographically greater goal id. Evidence-strength comparison is a
  possible later refinement. (Recommended default; open to change.)
- **Retirement reason:** cancellation reuses the existing `GoalRetired` event / `Retired`
  status; because `GoalRetired` carries no reason field, the incompatibility reason (and the
  conflicting goal id) lives in the coherence trace record, not in a new status variant.

**New pure surface (`qsf_volition`, new `coherence` module):**
- Types: `Contradiction { goal_a, goal_b, rationale }`, `CoherenceVerdict { contradictions,
  judge_ref }`, a multi-conflict `AdmissionResolution { admitted, rejected_by,
  cancelled_goal_ids, suppressed_cancellations_due_to_rejection }`, and a `SweepResolution`.
- A generalized effective-tier helper computing from a `Goal`/`ProposedGoalCandidate`'s
  `tension_ids` against `fixture.tensions` — replacing the fixture-goals-only lookup in
  `reducer.rs` that returns `u8::MAX` for anything outside `fixture.goals` (which would
  mis-tier accepted and proposed candidates).
- Pure resolution: `resolve_admission(candidate, verdict, state, fixture) -> Vec<VolitionEvent>`
  and `resolve_sweep(verdict, state, fixture) -> Vec<VolitionEvent>`, both total and
  unit-testable with synthetic verdicts (no model). They emit **existing** events only —
  `GoalCandidateAccepted`, `GoalCandidateRejected`, `GoalRetired` — so the coherence engine
  reuses the goal lifecycle rather than duplicating it. The floor-vs-floor flag and the model
  `judge_ref` are recorded in the trace record, not in reducer events.

**Adapter surface (offline, `qsf_app`):**
- A `CoherenceJudge` trait with a deterministic mock impl (scripted verdict for the
  contradiction fixture) and a model-backed impl over the `ModelRole` boundary. Mock is the
  default so tests stay deterministic; the real model is opt-in via the existing provider
  selection.
- An offline experiment harness (`qsf_app`, `RunContext`) that loads a fixture with a
  known-contradictory candidate, runs admission and a sweep, applies the emitted events through
  `apply()`, and records each check as a `TraceRecord` (operation `goal-coherence-check`) in
  `traces.jsonl` linked to `events.jsonl` — the offline artifacts, not the realtime
  `DiagnosticRecord` stream.

**Attach points:**
- [crates/qsf_volition/src/coherence.rs](../../crates/qsf_volition/src/coherence.rs) (new) —
  types + pure resolution.
- [crates/qsf_volition/src/reducer.rs](../../crates/qsf_volition/src/reducer.rs) — reuse the
  existing `GoalCandidateAccepted` / `GoalCandidateRejected` / `GoalRetired` events; generalize
  the private `goal_effective_tier` helper so it also tiers candidates by `tension_ids`.
- [crates/qsf_volition/src/candidate.rs](../../crates/qsf_volition/src/candidate.rs) /
  `propose_goal_candidates()` — the Phase 1 candidate source.
- `crates/qsf_app/src/experiments/` — the offline coherence harness and `CoherenceJudge`
  model-backed impl.

**Verification:** offline `Experiment.GoalCoherenceUnderProtectedFloor` (scaffold written with
this phase) — see its trace-completeness contract. Automated: pure-resolution unit tests over
synthetic verdicts (admit, reject-by-tier, cancel-less-fundamental, equal-tier tie-break,
floor-vs-floor flag, hard tier-floor gate, a protected-tier proposed candidate, and an accepted
candidate not present in `fixture.goals`); the harness parses `traces.jsonl` and asserts each
decision is reconstructable from the trace alone. Human testing is **not** required for Phase 1
(offline, deterministic); it belongs to Phase 2's live behavior.

### Phase 2 — Live goal formation and off-hot-path coherence

Wire the Phase 1 engine into the realtime loop. A live proposer may **form** a candidate goal
from trusted discussion; admission judging runs **off the hot path** (after the turn, per
[DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---goal-coherence-is-model-judged-off-the-hot-path-and-repaired-during-sleep)),
so turn latency is unaffected; an incompatible candidate is retired before it can shape later
turns, with the rejection surfacing on a later turn or under introspection. The whole-set
coherence sweep runs in the sleep/consolidation pass. This is where "the agent forms its own
goals and can decline the user" becomes a felt, human-testable behavior.

- **Attach points:** the realtime sideband (`events_for_trusted_transcript` / a sibling
  proposer) for live formation and off-path admission; the sleep pass for the sweep.
- **Open questions:** what evidence in a trusted turn justifies forming a durable candidate;
  where the off-path admission runs (post-turn promotion vs. queued to sleep); how a past
  rejection is surfaced without nagging.
- **Verification:** live Experiment + human voice testing (recommended) that the agent can
  form a goal, keep it when consistent, and decline it when it contradicts the core.

### Phase 3 — Emotion-like signals, visualization-first (brief §8)

Derive named functional signals from existing goal/delta state per reconciliation D4
(frustration = repeatedly `Blocked`; satisfaction = `GoalSatisfied` + `EvidenceRef`; tension =
unresolved conflict; etc.). Pure derivations over recorded state — no new mutable emotion
object.

- **Scope discipline:** **visualization only at first** — no arbitration feedback. Gated.
- **Natural source of the `tension` signal:** the coherence engine's detected contradictions,
  rejections, and cancellations (Phases 1–2) — an evidence-backed conflict signal rather than a
  narrated one.
- **Attach point:** derived at the salience/initiative layer; surfaced in the existing browser
  volition panel / brain-state surface
  ([Design.LiveActivationDashboard.md](Design.LiveActivationDashboard.md)).
- **Open question:** which signals earn a place first; whether any later feed arbitration.
- **Verification:** Experiment scaffold asserting each signal derives only from recorded state.

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

## Documents to update (per ProjectWorkflow.md)

- **Done at Phase 1 detailing:** the coherence commitment (single ownership + belief-coherence
  invariant; model-judged detection off the hot path + sleep sweep) is recorded in
  [DecisionLog.md](../DecisionLog.md) (two 2026-06-30 entries), and the coherent-agent stance is
  added to [ProjectVision.md](../ProjectFrame/ProjectVision.md).
- When a phase is detailed: write its `Experiment.*.md` scaffold and trace contract. Phase 1's
  is [Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md).
- On implementing a phase: refresh the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) (Phase 1 adds
  the coherence module, the new events, and the `CoherenceJudge` adapter seam).
- As brief concepts land in project docs, retire the corresponding brief sections: §12 is
  retired as **not-adopted** (ownership declined); §11 is **delivered** through coherence. Delete
  the brief once nothing in it remains unmerged.
