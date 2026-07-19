# Handoff — Resume Here

**Updated:** 2026-07-19 (evening) — every phase of
[Plan.WorldPerception](Plans/Plan.WorldPerception.md) is implemented, but a second real-corpus
live session requested no consultation at all: the observed blockers moved from the candidate
gate to the trigger layer. Voice transcripts lowercase topic terms, defeating the explicit
path's capitalization-based entity check, and winner-takes-the-turn arbitration let
`serve-the-present-person` crowd out a weak world-goal match — details in
[Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md) Results. The
relaxed relevance gate therefore remains unexercised live. The synchronous corpus ingest still
blocks server readiness (~17 s debug) before the port binds, motivating a background-load
slice.

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

## Horizon — direction

**Corpus background loading** — move corpus ingest off the server-readiness path with a
status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
