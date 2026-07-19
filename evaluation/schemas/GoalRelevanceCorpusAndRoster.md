# Goal relevance corpus and roster schema

Rust types in `crates/qsf_semantic_eval` are authoritative. This document explains their stable
JSON shapes so corpus reviewers can inspect versioned artifacts without reverse-engineering Rust.

## Dataset JSONL

Each nonblank line is one `PairRecord` and carries `schema_version: 1`. Unknown fields, malformed
JSON, empty required identifiers, and any other schema version are errors; there is no silent
migration. A dataset uses one `dataset_version` across all records.

Required pair fields are `utterance`, `goal_ref`, `gold_label`, `slice_tags`, `session_id`,
`semantic_cluster_id`, `provenance`, and `utterance_roster_annotation`. `language` defaults to
`"en"`. Gold labels serialize as `relevant`, `not_relevant`, or `ambiguous`.

`utterance_roster_annotation` is either `has_roster_relevance` or `none_of_roster`. It is copied
onto each pair for an utterance so JSONL remains self-contained, but validation requires it to be
consistent across repeated utterances. A `none_of_roster` utterance cannot have a `relevant` pair.

Provenance records `source` (`teacher` or `real_session`), optional `teacher_model_id`, and
`review_status` (`draft` or `reviewed`). Slice tags are tagged JSON objects: a paraphrase cluster
is `{ "kind": "paraphrase_cluster", "id": "..." }`; the remaining kinds are
`hard_negative`, `explicit_negation`, `implicit_negation`, `quoted_speech`, `hypothetical`,
`subject_confusion`, `punctuation_casing_loss`, `synthetic_asr`, `real_asr`, and
`rare_high_cost`.

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

## Generated run artifacts

The runner writes `pair-results.jsonl` as the authoritative per-pair scoring chain. It records
matched terms with production weight classes, numeric strength, and the exact qualification
threshold in force. `metrics.json` is derived from that JSONL. Markdown summaries are derived
human-readable views and must never become the regression-gate input.
