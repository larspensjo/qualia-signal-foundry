# Handoff — Resume Here

**Updated:** 2026-07-22 — the goal_relevance generation path is cleared for the full
production run after five operator-reviewed smokes: two-step anchor approval, per-mode
nano/mini generation, mode validators with per-batch retry, and over-produced
`none_of_roster` negatives (labels are the authority) all landed on top of the schema-v2 /
datagen foundation; see [Plan.GoalRelevanceFrozenSets](Plans/Plan.GoalRelevanceFrozenSets.md). World
perception is unchanged since the morning: all of
[Plan.WorldPerception](Plans/Plan.WorldPerception.md) is implemented, but the second
real-corpus live session requested no consultation — blockers moved to the trigger layer
(lowercased voice transcripts defeat the capitalization-based entity check; arbitration lets
`serve-the-present-person` crowd out weak world-goal matches), so the relaxed relevance gate
remains unexercised live — details in
[Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md) Results. The
synchronous corpus ingest still blocks server readiness (~17 s debug), motivating a
background-load slice.

<!--
Rules (see ProjectWorkflow.md, "Handoff Discipline"):
- Three levels: Now (immediate action) / Next (active plan) / Horizon (direction).
- One primary recommendation per level, plus at most 1-2 one-line alternates.
- Pointer, not content: recommendation + one-line rationale + link. Details live in the
  linked plan/experiment/architecture doc — write them there first, then link.
- Update only when an event changes a recommendation at some level; rewrite in place.
- Keep the whole file readable in a couple of minutes (about one screen).
-->

## Now — immediate few actions
### Re-run the live probe with trigger-compatible phrasing
**Repeat the live real-corpus probe using turns the current triggers can catch — a capitalized
proper-name entity plus a search cue ("Can you find the latest information about Nvidia?") and
a turn that names "AI" explicitly
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)).**
Why: this separates "surfacing works once a consultation is requested" from the newly observed
trigger gaps; real-corpus surfacing is still the plan's unobserved central result, and the
panel's retrieval detail is the verification surface.
Alternate: run a first real-corpus sleep consolidation and a recall probe
([Experiment.WorldMemoryConsolidation](Experiments/Experiment.WorldMemoryConsolidation.md)) —
tunes the provisional eligibility rule and 7-day half-life with real evidence.

## Next — active plan or plans
### World perception
**Close [Plan.WorldPerception](Plans/Plan.WorldPerception.md) once both probes have evidence,
then delete the plan per workflow.**
Why: all phases are implemented; the experiments own the remaining open questions
(eligibility rule, news half-life, anti-repeat window, deferred-path latency).
Alternate: if the rephrased probe surfaces facts but natural phrasing still cannot reach the
trigger, scope a trigger-robustness slice from the experiment's new follow-ups (STT-aware
entity detection; arbitration standing for ConsultWorld-capable goals under
current-information cues).

### Semantic evaluation (goal relevance)
**Run the full-production slice of
[Plan.GoalRelevanceFrozenSets](Plans/Plan.GoalRelevanceFrozenSets.md): a multi-goal
generation schedule meeting the per-slice floors across the 7-goal roster, then mini
labeling, the independent Claude Fable cross-label, full human review, split, gatekeeper,
and freeze.**
Why: the generation path (two-step anchor approval via `generate --anchors-only` then
`generate --anchors <approved>`, per-mode models, mode validators) is smoke-cleared; the
current schedule is still single-goal smoke-scale, and the gatekeeper/freeze machinery is
the remaining implementation before the human-controlled corpus run.
Alternate: the remote-usage telemetry baseline (token-ledger extraction) is independent and
can proceed in parallel; small pending item — the operator label review that flips the 12
sample records from `draft`.

## Horizon — direction

### Semantic shared infrastructure

See docs\Plans\Plan.SharedSemanticInfrastructure.md, a continuation from docs\Research\TechBrief.QSF_Local_Semantic_Classification.md.

### Corpus background loading
Move corpus ingest off the server-readiness path with a status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
