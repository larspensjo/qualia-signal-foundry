# Handoff — Resume Here

**Updated:** 2026-07-18 — the live explicit-topic probe (Grok 4.5) verified the trigger,
anchor relevance, inline injection, and honest external attribution end-to-end against the
bundled fixture corpus. A real-corpus probe on a covered topic remains open; separately, the
synchronous corpus ingest was found to block server readiness (~17 s debug) before the port
binds, motivating a background-load slice.

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
### Move corpus loading
**Move corpus loading off the server-readiness path, with a corpus status indicator in the
realtime debug UI ([Plan.WorldPerception](Plans/Plan.WorldPerception.md)).**
Why: the synchronous ingest delays port bind (~17 s debug, growing with the corpus), which
breaks the UI with proxy 502s during startup; background load plus a ledger-backed faster
refresh removes the recurring papercut before diagnostic-panel human testing begins.
Alternate: repeat the live probe against the real WPFM corpus on a covered topic
([Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md)) — real-corpus
relevance is still unobserved.

## Next — active plan or plans
### World perception
**Human-verify the realtime world-perception diagnostic panel in [Plan.WorldPerception](Plans/Plan.WorldPerception.md).**
Why: the implemented observation surface now distinguishes no consultation from nothing relevant
and exposes the exact untrusted injection plus anchor/omission evidence; a live browser pass is
the remaining confidence check.

## Horizon — direction

**World-memory consolidation** — add provenance/trust-tier memory fields only after live
consultation has been observed.
Why: durable external facts need evidence-based eligibility, decay, and supersession rules rather
than carrying unreviewed corpus text directly into memory.
