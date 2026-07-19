# Handoff — Resume Here

**Updated:** 2026-07-19 — every phase of
[Plan.WorldPerception](Plans/Plan.WorldPerception.md) is now implemented: the anchor
relaxation and search-request cues (motivated by the morning's real-corpus session), the
provenance/trust-tier memory substrate, and sleep-phase world-memory consolidation with a
provisional eligibility rule, cap-deferral, and degraded-corpus no-promotion policy. What
remains is evidence, not code: the live real-corpus consultation probe and a first real sleep
consolidation run. The synchronous corpus ingest still blocks server readiness (~17 s debug)
before the port binds, motivating a background-load slice.

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
### Run the live real-corpus consultation probe
**Repeat the live real-corpus probe on a covered topic with the relaxed relevance gate
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)).**
Why: real-corpus surfacing is the plan's still-unobserved central result; the majority-based
anchor gate and search-request cues now exist precisely to make it observable, and the
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

## Horizon — direction

**Corpus background loading** — move corpus ingest off the server-readiness path with a
status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
