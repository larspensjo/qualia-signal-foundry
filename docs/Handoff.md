# Handoff — Resume Here

**Updated:** 2026-07-04 — weighted goal activation implemented and voice-retested; the AI-transition
probe wins on-topic, stopword turns stay quiet, threshold 4 confirmed. Gate passed.

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

**Resume persona/goal experimentation on the live volition system** — the weighted-activation
gate is closed, so the fixture can now be tuned and felt-tested without stopwords hijacking the
turn.
Why: the deterministic activation layer is validated end-to-end
([Experiment.WeightedGoalActivation.md](Experiments/Experiment.WeightedGoalActivation.md), Useful
Result); the remaining value is in what the fixture expresses.
Alternate: re-run the formation voice test
([Experiment.LiveGoalFormationAndCoherence.md](Experiments/Experiment.LiveGoalFormationAndCoherence.md)),
since selection mechanics changed under it.

## Next — active plan

**Elaborate a persona experiment** (e.g. `Experiment.PersonaTensionVariations` in
[Experiment.Backlog.md](Experiments/Experiment.Backlog.md)) that exercises the now-validated
weighted activation — new tensions/keyword-weight mixes and their felt behavior.
Why: with qualification gating in place, fixture-data changes are the cheapest lever on persona
behavior and no longer masked by stopword wins.
Alternate: [Plan.RealtimeVoiceConversation.md](Plans/Plan.RealtimeVoiceConversation.md) Phase 5
(live memory extraction), independent of the volition work.

## Horizon — direction

**Personality/goal experimentation** on the live volition system — new tensions, modes, and felt
behavior changes.
Why: the volition build is complete; the value now is in what the fixture can express.
Candidates: `Experiment.PersonaTensionVariations` in
[Experiment.Backlog.md](Experiments/Experiment.Backlog.md); guidance in
[Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md) (Persona And Fixture
Experimentation).
Alternate: elaborate another `docs/Plans/Idea.*.md` into a plan.
