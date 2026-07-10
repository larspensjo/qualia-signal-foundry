# Handoff — Resume Here

**Updated:** 2026-07-09 — the world corpus ingested the real WPFM output (6,304 articles;
schema v1; no skipped files) and the content-hash ledger fully reused it on the next run. The
17 ms real-corpus query probe needs profiling against the live budget before consultation.

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

**Build the world-perception diagnostic UI in [Plan.WorldPerception](Plans/Plan.WorldPerception.md).**
Why: the live adapter now injects correctly framed fixture facts and records the complete JSONL
causal chain; the next useful slice is making that evidence visible during a session.

## Next — active plan

**Run the live world-consultation experiment in
[Experiment.WorldConsultation](Experiments/Experiment.WorldConsultation.md).**
Why: the adapter’s 5 ms inline/defer policy is automated-tested, but live relevance and
transcript-to-audio impact still need human evidence.

## Horizon — direction

**World-memory consolidation** — add provenance/trust-tier memory fields only after live
consultation has been observed.
Why: durable external facts need evidence-based eligibility, decay, and supersession rules rather
than carrying unreviewed corpus text directly into memory.
