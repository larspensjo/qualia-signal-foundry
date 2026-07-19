# Handoff — Resume Here

**Updated:** 2026-07-19 — a real-corpus voice session confirmed the goal-activation gate
surfaces nothing on natural speech (all candidates `missing_required_anchor`, including an
on-topic article), and a plain "find information about…" turn triggered no consultation
(see [Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)). The
world-perception diagnostic panel landed and its capture carried the full trace. The
synchronous corpus ingest still blocks server readiness (~17 s debug) before the port binds,
motivating a background-load slice.

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
### Relax the goal-activation relevance gate
**Implement the goal-activation anchor relaxation and search-request cue slice in
[Plan.WorldPerception](Plans/Plan.WorldPerception.md).**
Why: two real-corpus sessions show the require-all anchor policy omits even exactly on-topic
articles and the cue lexicon misses plain search requests, so consultations execute but never
surface anything useful.
Alternate: move corpus loading off the server-readiness path (~17 s debug port-bind delay,
proxy 502s during startup) — the recurring papercut for every live probe.

## Next — active plan or plans
### World perception
**After the relevance relaxation, repeat the live real-corpus probe on a covered topic
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)).**
Why: real-corpus surfacing is the plan's still-unobserved central result; the diagnostic
panel's retrieval detail is the verification surface for it.

## Horizon — direction

**World-memory consolidation** — add provenance/trust-tier memory fields only after live
consultation has been observed.
Why: durable external facts need evidence-based eligibility, decay, and supersession rules rather
than carrying unreviewed corpus text directly into memory.
