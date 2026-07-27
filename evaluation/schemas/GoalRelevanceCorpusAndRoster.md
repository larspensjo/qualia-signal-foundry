# Goal relevance corpus and roster schema

Rust types in `crates/qsf_semantic_eval` are authoritative. This document explains their stable
JSON shapes so corpus reviewers can inspect versioned artifacts without reverse-engineering Rust.

## Dataset JSONL

Each nonblank line is one `PairRecord` and carries `schema_version: 2`. Unknown fields, malformed
JSON, empty required identifiers, and any other schema version are errors; there is no silent
migration. A dataset uses one `dataset_version` across all records.

Required pair fields are `roster_snapshot_version`, `utterance_id`, `utterance`, `goal_ref`,
`gold_label`, `slice_tags`, `session_id`, `semantic_cluster_id`, `provenance`, and
`utterance_roster_annotation`. `language` defaults to
`"en"`. Gold labels serialize as `relevant`, `not_relevant`, or `ambiguous`.

`utterance_roster_annotation` is either `has_roster_relevance` or `none_of_roster`. It is copied
onto each pair for an utterance so JSONL remains self-contained, but validation requires it to be
consistent across repeated utterances. `utterance_id` is the stable per-utterance identity shared
by all of that utterance's pairs: one text cannot use multiple IDs, and one ID must keep the same
utterance text, `session_id`, and `semantic_cluster_id`. A `none_of_roster` utterance cannot have a
`relevant` pair.

Provenance records `source` (`teacher` or `real_session`), an optional `generation` block for
teacher data (`generator_model_id`, `generation_run_id`, `generation_output_sha256`,
`prompt_version`, `saw_activation_keywords`), a `labeling` block (`guideline_version` and labelers
with `labeler_id`, `labeling_run_id`, `output_sha256`), and a `review` block
(`review_decisions_sha256`, `review_status`, which is `draft` or `reviewed`). Slice tags are tagged JSON objects: a paraphrase cluster
is `{ "kind": "paraphrase_cluster", "id": "..." }`; the remaining kinds are
`hard_negative`, `explicit_negation`, `implicit_negation`, `quoted_speech`, `hypothetical`,
`subject_confusion`, `punctuation_casing_loss`, `synthetic_asr`, `real_asr`, and
`rare_high_cost`. The `real_asr` slice tag remains part of the schema although it is unused in v1.

## Frozen roster JSON

`RosterSnapshot` has its own `schema_version: 1`, a `roster_snapshot_version`, the complete
serialized `qsf_volition::VolitionFixture`, and explicit
`qsf_volition::VolitionState::from_fixture` default state. It also records `fixture_hash`, the
SHA-256 hash of the serialized fixture, and `goals`, the ordered frozen description index.

Each frozen goal exposes `goal_ref`, its SHA-256 description key. The hash preimage is the JSON
tuple `[roster snapshot version, goal title, goal summary, tension summaries]`, with tension
summaries in the goal's declared tension order. Therefore field boundaries are unambiguous and a
label follows a frozen description rather than a mutable fixture goal ID. Validation recomputes
all keys and the fixture hash.

The sample roster is a serialized `qsf_volition::realtime_seed_fixture()` plus the explicit
default state. Its drift guard compares the entire snapshot to the current fixture and fails with
a re-versioning instruction when they differ.

## Roster re-versioning and label rebinding

A keyword-only seed-goal edit (activation keywords or weight classes only) receives a new roster
snapshot version. Existing labels carry forward through a deterministic rebinding: rewrite every
record's `roster_snapshot_version`, mechanically re-issue every `goal_ref` using the new version,
and re-point the labels without relabeling. Title, summary, or tension-summary edits change the
described goal and require relabeling the affected pairs.

## Generated run artifacts

The runner writes `pair-results.jsonl` as the authoritative per-pair scoring chain. It records
matched terms with production weight classes, numeric strength, and the exact qualification
threshold in force. `metrics.json` is derived from that JSONL. Markdown summaries are derived
human-readable views and must never become the regression-gate input.

## Frozen-set manifest and lineage

The deterministic split command writes `split-summary.json` beside
`validation.dataset.jsonl` and `test.dataset.jsonl`. This is a strict JSON object with numeric
`split_seed`, `assignment_by_component`, and required `assignment_rationale`, whose keys are
connected-component identifiers and whose values are `validation` or `test`. The rationale records
why the seed-to-component binding is authoritative for downstream census and freeze evidence.
Freeze reads this artifact by default; `--seed` may
explicitly override its seed. Before writing frozen artifacts, freeze must rerun the split over the
combined reviewed pool and require byte equality with both supplied split files.

`evaluation/frozen/goal-relevance/freeze-manifest.json` is a strict JSON object with
`dataset_version`, `roster_snapshot_version`, `roster_fixture_hash`, `split_seed`,
`validation_sha256`, `test_sha256`, `per_slice_counts_by_split`,
`generation_output_sha256`, `label_mini_sha256`, `label_fable_sha256`,
`review_decisions_sha256`, and `frozen_at`. The two dataset hashes are SHA-256 hashes of the
canonical JSONL emitted by the deterministic split and freeze transport. Per-slice counts are
distinct utterance counts; `paraphrase_clusters` counts clusters containing at least two
utterances.

The full committed provenance boundary is
`evaluation/frozen/goal-relevance/lineage/<dataset_version>/`: `generation-output.jsonl`,
`labeling-input.jsonl`, `label-mini.jsonl`, `label-fable.jsonl`, `reconciliation.jsonl`,
`reconciliation-summary.json`, `review-decisions.jsonl`, `blind-qa-decisions.jsonl`, and
`reviewed-pool.jsonl`. Blind-QA decisions are measurement evidence only: they never enter the
reviewed-pool fold. `reconciliation-summary.json` records the mini/Fable numerator and denominator
used by the methodology note.

The freeze gate requires the dense utterance×roster cross-product, version-bound and resolving
goal references, reviewed pairs, floor-satisfying validation and test components, no session or
semantic-cluster identifier shared by both splits, valid `none_of_roster` labels, a roster that
round-trips against the realtime seed, and at least 0.80 cold blind-QA agreement in the negation,
quoted-speech, and hypothetical slices.
