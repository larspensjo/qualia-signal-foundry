# Handoff — Resume Here

**Updated:** 2026-07-04 — weighted-goal-activation design approved (arbitration was won by
stopwords in the live session); implementation plan created.

<!--
Rules (see ProjectWorkflow.md, "Handoff Discipline"):
- Three levels: Now (immediate action) / Next (active plan) / Horizon (direction).
- One primary recommendation per level, plus at most 1-2 one-line alternates.
- Pointer, not content: recommendation + one-line rationale + link. Details live in the
  linked plan/experiment/architecture doc — write them there first, then link.
- Update only when an event changes a recommendation at some level; rewrite in place.
- Keep the whole file readable in a couple of minutes (about one screen).
-->

## Now — immediate action

**Implement the first phase (weighted keyword schema + pure scoring) of
[Plan.WeightedGoalActivation.md](Plans/Plan.WeightedGoalActivation.md)**.
Why: the 2026-07-04 session showed stopwords winning arbitration
([Design.WeightedGoalActivation.md](Plans/Design.WeightedGoalActivation.md)); the pending voice
retests are phrase-engineering exercises until match strength gates the win.
Alternate: re-run the formation voice test first
([Experiment.LiveGoalFormationAndCoherence.md](Experiments/Experiment.LiveGoalFormationAndCoherence.md))
if a live session is wanted before selection mechanics change under it.

## Next — active plan

**Complete [Plan.WeightedGoalActivation.md](Plans/Plan.WeightedGoalActivation.md)** through its
arbitration/traces phase, then run the
[Experiment.WeightedGoalActivation.md](Experiments/Experiment.WeightedGoalActivation.md) voice
gate (created by the plan's first task).
Why: it unblocks honest persona retests, including the curiosity-persona step-2 gate.
Alternate: [Plan.RealtimeVoiceConversation.md](Plans/Plan.RealtimeVoiceConversation.md) Phase 5
(live memory extraction), independent of the volition gate.

## Horizon — direction

**Personality/goal experimentation** on the live volition system — new tensions, modes, and felt
behavior changes.
Why: the volition build is complete; the value now is in what the fixture can express.
Candidates: `Experiment.PersonaTensionVariations` in
[Experiment.Backlog.md](Experiments/Experiment.Backlog.md); guidance in
[Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md) (Persona And Fixture
Experimentation).
Alternate: elaborate another `docs/Plans/Idea.*.md` into a plan.
