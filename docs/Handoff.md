# Handoff — Resume Here

**Updated:** 2026-07-04 — weighted goal activation implemented (weight classes + qualification
threshold gate arbitration wins); all automated verification passes; awaiting the voice retest.

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

**Run the voice protocol in
[Experiment.WeightedGoalActivation.md](Experiments/Experiment.WeightedGoalActivation.md)**
(~two minutes: natural step-2 AI-transition probe should let `track-the-ai-transition` win and
fire `ProposeExperiment`; "for what it's worth, thanks" should record a
`below_qualification_threshold` suppression with no initiative; injection latency unchanged at
0 ms).
Why: the weighted-activation mechanics and all automated checks are in; the retest is the
remaining gate and decides whether the threshold default of 4 survives.
Alternate: re-run the formation voice test
([Experiment.LiveGoalFormationAndCoherence.md](Experiments/Experiment.LiveGoalFormationAndCoherence.md))
in the same session, since selection mechanics have now changed under it.

## Next — active plan

**Record the voice retest results in
[Experiment.WeightedGoalActivation.md](Experiments/Experiment.WeightedGoalActivation.md)** and,
if the threshold default holds, promote it in the decision log; the implementation is
code-complete and the plan retires once the gate passes.
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
