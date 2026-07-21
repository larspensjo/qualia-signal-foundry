# Handoff — Resume Here

**Updated:** 2026-07-21 — the goal_relevance frozen-sets pipeline now has its foundation:
dataset schema v2 (stable per-utterance identity, two-labeler lineage provenance,
envelope-first version rejection) and the replay-default `qsf_semantic_datagen` crate
(interchange contracts, mini/Fable reconciliation, review fold, dependency guard) landed;
see [Plan.GoalRelevanceFrozenSets](Plans/Plan.GoalRelevanceFrozenSets.md). World
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
**Produce the generated pool, perform the documented independent Claude Fable cross-label, and
use the goal-relevance review queue to reach reviewed validation/test candidates (parent:
[Plan.SemanticEvaluationFoundation](Plans/Plan.SemanticEvaluationFoundation.md)).**
Why: the replay-default tooling now constructs blind mini labels, validates the shared interchange,
prioritizes disagreements, and supports one-utterance human review; the remaining work is the
human-controlled corpus run and freeze gates.
Run the shipped stages from the repository root (omit `--live` for replay fixtures):

```text
cargo run -p qsf_semantic_datagen -- generate --live evaluation/frozen/goal-relevance/realtime-seed.roster.json generation-output.jsonl
cargo run -p qsf_semantic_datagen -- label --live generation-output.jsonl evaluation/frozen/goal-relevance/realtime-seed.roster.json goalrel-labeling goalrel-mini-live
cargo run -p qsf_semantic_datagen -- validate-labels evaluation/frozen/goal-relevance/realtime-seed.roster.json goalrel-labeling/label-fable.jsonl
cargo run -p qsf_semantic_datagen -- reconcile evaluation/frozen/goal-relevance/realtime-seed.roster.json goalrel-labeling/label-mini.jsonl goalrel-labeling/label-fable.jsonl goalrel-labeling/reconciliation.jsonl
cargo run -p qsf_semantic_datagen -- review evaluation/frozen/goal-relevance/realtime-seed.roster.json goalrel-labeling/labeling-input.jsonl goalrel-labeling/label-mini.jsonl goalrel-labeling/label-fable.jsonl goalrel-labeling/review-decisions.jsonl
```

`--live` requires `OPENAI_API_KEY`. For a cold QA pass, add `--blind-qa` after `review`; the CLI
writes `goalrel-labeling/blind-qa-decisions.jsonl` and leaves authoritative review decisions alone.
Alternate: the remote-usage telemetry baseline (token-ledger extraction) is independent and can
proceed in parallel; small pending item — the operator label review that flips the 12 sample
records from `draft`.

## Horizon — direction

### Semantic shared infrastructure

See docs\Plans\Plan.SharedSemanticInfrastructure.md, a continuation from docs\Research\TechBrief.QSF_Local_Semantic_Classification.md.

### Corpus background loading
Move corpus ingest off the server-readiness path with a status chip in the debug UI.
Why: the ~17 s synchronous load delays port bind and 502s the UI on every live probe.
Alternate: the `world-curiosity` open-delta substrate investigation — letting volition
represent what it actually does not know (deferred `curiosity` signal, 2026-07-06).
