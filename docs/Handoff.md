# Handoff — Resume Here

**Updated:** 2026-07-26 — the goal_relevance pipeline machinery and v1 lineage rescue are complete;
the eighteen replay/evidence artifacts are committed under
`evaluation/frozen/goal-relevance/lineage/`, and the remaining work is the panel labeling and
freeze campaign; see
[Plan.GoalRelevancePanelLabeling](Plans/Plan.GoalRelevancePanelLabeling.md). World perception is
unchanged since 2026-07-22: all of
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
### Semantic evaluation (goal relevance)
**Execute the weighted panel labeling, audit, and freeze campaign in
[Plan.GoalRelevancePanelLabeling](Plans/Plan.GoalRelevancePanelLabeling.md).**
Why: the machinery, availability smoke, and committed v1 lineage are ready; the remaining
work is operator-driven labeling and its gated replay.
Alternate: close [Plan.WorldPerception](Plans/Plan.WorldPerception.md) once both live probes
have evidence, then delete the plan per workflow.

## Horizon — direction

### Semantic shared infrastructure

See docs\Plans\Plan.SharedSemanticInfrastructure.md, a continuation from docs\Research\TechBrief.QSF_Local_Semantic_Classification.md.

### Corpus background loading
Move corpus ingest off the server-readiness path with a status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
