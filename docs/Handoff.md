# Handoff — Resume Here

**Updated:** 2026-09-22 — Transcript CLI implementation is complete through the launcher and awaits
its live-session acceptance run. The goal_relevance pipeline machinery and v1 lineage rescue are complete;
the eighteen replay/evidence artifacts are committed under
`evaluation/frozen/goal-relevance/lineage/`, while the current active plan is the isolated per-turn
relevance-judgment foundation. World perception is
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
### Run the Transcript CLI live-session acceptance
**Hold a live conversation of four or more turns (`.\scripts\qsf.ps1 realtime -RandomSessionId`,
one turn expected to fire a goal, one expected to fire none), emit
`.\scripts\qsf.ps1 transcript -Out turns.jsonl`, and check the artifact: `source.complete` is
`true`, one `turn` line per trusted turn, no non-empty `undecodable`; then cross-check two or
three turns' `user`/`assistant`/`volition.fired` against the realtime debug UI.** Usage is in the
README's transcript section (the ephemeral plan has been deleted).
Why: the implementation and automated gates are complete; this human acceptance run is the remaining
evidence for the tool.
Pointer: the capitalized-entity world-trigger corpus is now producible headlessly with
`.\scripts\qsf.ps1 probe`; the remaining live-session need is specifically the speech/STT path
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)).
Alternate: repeat the live real-corpus probe using turns the current triggers can catch — a capitalized
proper-name entity plus a search cue ("Can you find the latest information about Nvidia?") and
a turn that names "AI" explicitly
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)) — this separates
surfacing from the newly observed trigger gaps.
Alternate: run a first real-corpus sleep consolidation and a recall probe
([Experiment.WorldMemoryConsolidation](Experiments/Experiment.WorldMemoryConsolidation.md)) —
tunes the provisional eligibility rule and 7-day half-life with real evidence.

## Next — active plan or plans
### Per-turn relevance judgment
**Continue the isolated relevance-judge foundation with deterministic retrieval and domain adapters in
[Plan.PerTurnRelevanceJudgment](Plans/Plan.PerTurnRelevanceJudgment.md).**
Why: the pair-scoring contract and backends now exist, while live-path behavior remains deliberately
unwired.
Alternate: continue the panel labeling and freeze campaign
([Plan.GoalRelevancePanelLabeling](Plans/Plan.GoalRelevancePanelLabeling.md)).

## Horizon — direction

### Semantic shared infrastructure

See docs\Plans\Plan.SharedSemanticInfrastructure.md, a continuation from docs\Research\TechBrief.QSF_Local_Semantic_Classification.md.

### Corpus background loading
Move corpus ingest off the server-readiness path with a status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
