# Plan: Volition Motivational Texture

## Maturity

Candidate. **Detail level: skeleton** — phases are sequenced and scoped, but per-phase
specs, verification contracts, and `Experiment.*.md` scaffolds are not yet written. Detailing
each phase (starting with the first) is the next step.

## Purpose

The realtime volition system is fully built and human-tested: tensions, goals, salience,
arbitration, mode bias, opportunity detection, shaping-intensity dial, bounded initiative in
the live loop, cross-session continuity, and a browser volition panel. See
[Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) and
[Handoff.Volition.md](../Handoff.Volition.md).

Four concepts from the external brief remain genuinely new — classified as "Defer — new" in
[Design.VolitionBriefReconciliation.md](Design.VolitionBriefReconciliation.md) and parked in the
handoff. This plan sequences them. Together they give volition more **inspectable motivational
texture**: goals that are owned by someone, derived affect signals, a conscious/subconscious
surfacing distinction, and multi-turn structure. The unifying outcome is that the system reads
as a *distinct, motivated agent* — without reopening the evidence-based, anti-anthropomorphic
stance (DecisionLog 2026-05-15, 2026-06-27).

## Guardrails (carry into every phase)

- Project vocabulary stays authoritative; nothing is renamed (reconciliation D1).
- No claim of subjective experience; all new state is inspectable and trace-backed (D2).
- "Emotion" is only ever a named, evidence-derived functional signal — never a felt state,
  never used to confabulate narration (D4).
- New goals/signals cannot enter at or below the protected tier floor.
- Per [Agents.md](../../Agents.md): any phase whose behavior is explained by traces needs a
  trace-completeness contract (required fields, artifact boundary, artifact-parsing
  verification) defined before implementation.

## Phases (in order)

Ordered by increasing cost and decreasing certainty. Earlier phases make later ones more
legible (e.g. provenance makes the conflict/tension signal honest).

### Phase 1 — Goal provenance tag (brief §12)

Add ownership to goals: user / simulator / shared. Today every goal is implicitly a simulator
goal. Cheapest change, highest leverage — it is what makes the system "feel like a separate
agent rather than an extension of the user," and it makes goal-conflict explanation (§11)
*truthful* rather than rhetorical ("I wanted to stay with this, but noticed you moving on").

- **Attach point:** a provenance field on `Goal`
  ([crates/qsf_volition/src/model.rs](../../crates/qsf_volition/src/model.rs)); tagging happens
  in the realtime sideband that maps trusted transcripts to volition events.
- **Open question to resolve in detailing:** do user-originated goals share the full
  `GoalStatus` lifecycle or only a subset?
- **Verification:** Experiment scaffold over provenance tagging + conflict-explanation trace.

### Phase 2 — Emotion-like signals, visualization-first (brief §8)

Derive named functional signals from existing goal/delta state per the reconciliation D4 table
(frustration = repeatedly `Blocked`; satisfaction = `GoalSatisfied` + `EvidenceRef`; tension =
unresolved arbitration conflict; etc.). Pure derivations over state already tracked — no new
mutable emotion object.

- **Scope discipline:** **visualization only at first** — no arbitration feedback. Gated. The
  user/sim conflict from Phase 1 is the natural source of the `tension` signal.
- **Attach point:** derived at the salience/initiative layer; surfaced in the existing browser
  volition panel / brain-state surface
  ([Design.LiveActivationDashboard.md](Design.LiveActivationDashboard.md)).
- **Open question:** which signals earn a place in the first slice; whether any later feed
  arbitration.
- **Verification:** Experiment scaffold asserting each signal derives only from recorded state.

### Phase 3 — Conscious / subconscious visibility (brief §6)

A visibility attribute on goal selection: a "subconscious" goal biases salience/arbitration but
surfaces only on introspection or forced conflict. Partly latent already in the sideband
surfacing gate + anti-nag wiring.

- **Resolution leaning:** treat as an introspection-*surfacing filter*, not a separate runtime
  path (the reconciliation's open question).
- **Attach point:** the selection/inspection layer (`build_state_inspection`) + surfacing gate.
- **Verification:** Experiment scaffold over what surfaces vs what only biases.

### Phase 4 — Multi-turn Plans (brief §3.5, §4.6)

A genuinely new domain structure: a `Plan` sequencing initiatives across turns with
suspend / resume / abandon. The current system is single-turn initiative.

- **Cost note:** largest new structure; most likely to feel mechanical. Deferred last
  deliberately — revisit need after Phases 1–3 add texture, and prove offline before the live
  loop (consistent with the reconciliation's attach note).
- **Verification:** offline Experiment scaffold over the plan lifecycle before any live wiring.

## Documents to update (per ProjectWorkflow.md)

- When a phase is detailed: write its `Experiment.*.md` scaffold and trace contract.
- On implementing a phase: refresh the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md).
- Record the commitment (sequence order + emotion-gating guardrail) in
  [DecisionLog.md](../DecisionLog.md) once Phase 1 is detailed, not at skeleton stage.
- As brief concepts land in project docs, retire the corresponding brief sections; delete the
  brief once nothing in it remains unmerged.
