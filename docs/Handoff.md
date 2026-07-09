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

**Profile the real-corpus lexical query in [Plan.WorldPerception](Plans/Plan.WorldPerception.md)**
before the live adapter is built.
Why: ingestion correctness is confirmed, but the 17 ms query probe is above the intended
single-digit-ms live-path target.

## Next — active plan

**Build the live world-consultation experiment in
[Plan.WorldPerception](Plans/Plan.WorldPerception.md)** after query latency is characterized.
Why: the corpus boundary is deterministic and inspectable; the remaining question is whether
curiosity-driven consultation can inject a correctly attributed fact within the live latency budget.

## Horizon — direction

**World-memory consolidation** — add provenance/trust-tier memory fields only after live
consultation has been observed.
Why: durable external facts need evidence-based eligibility, decay, and supersession rules rather
than carrying unreviewed corpus text directly into memory.
