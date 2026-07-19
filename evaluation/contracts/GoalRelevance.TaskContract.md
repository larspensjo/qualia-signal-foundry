# Goal relevance task contract

Contract format version: 1  
Status: active baseline contract

## Purpose and input unit

`goal_relevance` asks whether one current-turn utterance bears on one frozen goal description.
The input unit is the persona-independent pair `(utterance, goal_ref)`, where `goal_ref` is the
content-addressed description key in a named frozen roster snapshot. It is not a live fixture
goal ID.

The action boundary is current-turn volition shaping only: a score may inform selection and
arbitration, but it is not itself a durable write, memory promotion, or external effect.

## Labels and predictions

Gold pair labels are `Relevant`, `NotRelevant`, and `Ambiguous`. `Ambiguous` is excluded from
binary precision/recall/F1 and reported as a counted slice. `NoneOfRoster` is an utterance-level
annotation; every pair for such an utterance must be `NotRelevant` or `Ambiguous`, and its
non-ambiguous pairs contribute negatives.

Prediction states are relevant/not-relevant at a chosen score threshold, plus reserved
`Abstain` for a future learned scorer. The deterministic lexical baseline does not emit
`Abstain`: every pair receives a numeric score.

## Baseline prediction contract

The baseline grades a pair by production `match_strength`, the sum of production matched-keyword
weights. Reports sweep binary P/R/F1 over match-strength thresholds. The fixture's qualification
threshold is an arbitration gate rather than a relevance boundary; nevertheless the production
threshold of **4** is reported as one marked operating point. The baseline never abstains.

## Costs, availability, and latency

A false positive can misdirect present-turn framing or crowd out a more pertinent goal. A false
negative can miss a person-relevant concern or fail to surface an important tension. Cost-weighted
error is deferred until a learned model and an asymmetric action policy exist.

The evaluator must be deterministic and available offline once its dataset and roster snapshot
are present. It fails loudly for unreadable artifacts, unsupported schemas, malformed records,
unknown goal references, or roster drift.

The future learned-scorer latency target remains a hypothesis of **5–30 ms** on the live path,
not a measured promise. A later measurement records scorer-only per-utterance p50/p95/p99 after
warm-up, excluding dataset I/O, alongside machine and build profile. This baseline does not
fabricate latency percentiles.

## Explanation and trace contract

`pair-results.jsonl` is the authoritative structured causal chain. Each record contains
`dataset_version`, `roster_snapshot_hash`, `utterance`, `goal_ref`, `gold_label`, `slice_tags`,
`matched_terms` (including weight classes), `match_strength`,
`qualification_threshold_in_force`, `qualifies_at_threshold_in_force`, and `scorer_source`.
`metrics.json` is the derived structured metric artifact; the Markdown metric summary and error
analysis are derived views. Automated tests parse the JSONL and reconcile a sample record with
the production API.

## Dataset slices and metrics

Slices include paraphrase clusters, hard negatives, explicit/implicit negation, quoted speech,
hypotheticals, subject confusion, punctuation/casing loss, synthetic/real ASR, and rare high-cost
examples. Required v1 metrics are threshold-sweep pair P/R/F1, the threshold-4 operating point,
paraphrase-cluster recall, and slice breakdowns for negation, quoted speech, hypotheticals, and
ASR corruption. Ambiguous pairs are counted separately. Latency percentiles are deferred.

## Promotion and rollback

A candidate replacement may be promoted only after a versioned frozen-set report meets the
agreed quality, availability, and future measured latency gates with complete trace artifacts.
The deterministic production scorer and its frozen roster remain the rollback path. Any
regression, missing trace field, unavailable scorer, or unreviewed threshold change blocks
promotion and restores the deterministic baseline.
