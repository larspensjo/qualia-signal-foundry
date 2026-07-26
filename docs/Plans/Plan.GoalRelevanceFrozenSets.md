# Plan: Frozen validation and test sets for goal relevance

Status: Machinery complete (Phases A–D landed 2026-07-21..22); the labeling and freeze campaign is superseded by `Plan.GoalRelevancePanelLabeling.md`. Phase E (opt-in transcript capture) is not started and remains owned by this plan.
Maturity: Candidate
Area: Evaluation infrastructure / Volition (goal relevance) / Data generation

The labeling, review, and freeze campaign described below is superseded by
`docs/Plans/Plan.GoalRelevancePanelLabeling.md`. The per-slice floors, split algorithm, dense
cross-product invariant, roster binding, retention rule, and lineage boundary remain in force.

## Why this plan exists

The parent plan `docs/Plans/Plan.SemanticEvaluationFoundation.md` landed the goal-relevance
*walking skeleton*: the lean `crates/qsf_semantic_eval` crate (schema, roster snapshot, baseline
runner, report), the `evaluation/` tree, the task contract, and a 12-pair `draft` sample dataset.
Its next phase — "Frozen validation and test sets for goal relevance" — is still a sketch. This
plan makes that phase **executable**: a concrete generate → label → cross-label → human-review →
split → freeze pipeline that produces the frozen English validation and test sets the skeleton was
built to consume.

"Done" means: frozen, content-hashed, human-reviewed validation **and** test sets exist under
`evaluation/frozen/goal-relevance/`, split by session and semantic cluster (no id spans both
splits), satisfying explicit per-slice minimum counts in **both** splits, with recorded label-QA
agreement figures and a methodology note. Every durable artifact names the behavior
(`goal_relevance`); none refers to a plan phase number.

This is a multi-phase build, so it is a `Plan.*.md`, not an `Experiment.*.md`
(`ProjectWorkflow.md`, "Document Tracks"). The dataset construction is engineering; the mechanism
question (does exact-token matching have a real failure floor?) rides on the parent plan's
downstream failure-floor experiment, not on this data-production work.

## Relationship to the parent plan, and corrections this plan makes

This plan **details** the parent plan's frozen-sets phase. In doing so it corrects three factual
errors in the parent plan, applied as part of landing this plan (small documentation fixes, not
implementation):

1. **"the existing tens of turns" is false.** The only verbatim user utterances on disk are the
   2 turns in `state/realtime/continuity/default/session-state.json`
   (`turns[].user_input` / `exchanges[].input.voice.final_transcript`). The `archive/` subfolder
   holds sleep briefs with no turns; `runs/*/traces.jsonl` and `events.jsonl` carry no user text.
   The `real_session` / `real_asr` slice **cannot be populated today.**
2. **The `real_session` / `real_asr` slice is formally deferred for v1.** The parent plan's
   slice-coverage verification bullet must stop asserting that slice is present with a minimum
   count; v1 freezes without it, and a later dataset version populates it from captured transcripts
   (this plan's capture-mechanism phase).
3. **The teacher decision (parent plan "Decisions", item 3) is reworded** from a single
   teacher-generates-and-labels model to the two-model OpenAI generate/label split plus an
   independent Claude cross-label plus mandatory human review described below.

## Naming and ephemerality

This document owns ephemeral phase labels. The durable artifacts it produces — the new datagen
crate, schema v2, the interchange contract, the annotation guidelines, the sanitization rules, the
methodology note, the frozen sets — name the behavior (`goal_relevance`) and never cite a "stage"
or "phase" number (`Agents.md`; `ProjectWorkflow.md`). The stage numbers below (Stage 1–5) are the
brief's pipeline vocabulary and are used only inside this ephemeral plan.

---

## The decided pipeline (implemented here, not re-litigated)

All five stages were confirmed with the user. This plan implements them.

- **Stage 1 — generation.** `gpt-5.4-nano` (cheap OpenAI model), synchronous, via
  `openai_provider_kit`, in an in-repo Rust tool. Conditioned on the goal **description only** —
  title + summary + tension summaries, exactly the text `goal_ref` hashes. Activation keywords are
  **withheld from every generation prompt**; "the generator never saw activation keywords" is
  recorded in the methodology note as the property the downstream failure-floor number depends on.
  Produces, per goal: natural utterances, paraphrase clusters, and slice variants (explicit /
  implicit negation, quoted speech, hypothetical, subject-confusion, punctuation/casing loss, rare
  high-cost). A separate deliberately-adversarial **hard-paraphrase batch** is generated and
  **tagged** (`hard_negative`), reported per-slice, and must not dominate the base distribution. A
  separate **vague-prompted, goal-unconditioned batch** supplies `none_of_roster` negatives and
  distribution realism. **Synthetic ASR corruption is seeded from the observed corruption mode** —
  lowercasing / punctuation loss and entity mangling (`docs/Handoff.md`), *not* letter-level typo
  noise.
- **Stage 2 — labeling.** `gpt-5.4-mini`, **blind**: it never sees the generator's intended goal,
  intended label, or slice tags. One call per utterance, presenting the full roster's goal
  descriptions, returning a label per goal (`relevant` / `not_relevant` / `ambiguous`) plus whether
  none of the roster applies (which yields `utterance_roster_annotation`). **Dense cross-product:**
  every utterance is labeled against every roster goal, so negatives are systematic and precision
  is comparable across dataset versions.
- **Stage 3 — independent cross-label.** Claude Fable (the user's Claude Max subscription, an
  in-workflow agent, zero API cost) independently labels the same utterance×roster pairs, equally
  blind, emitting **the same interchange format** as Stage 2 so one validator checks both.
- **Stage 4 — human review.** The operator accepts or corrects **every** pair before it becomes
  `review_status: reviewed`. Mini-vs-Fable disagreements form the **priority review queue**; the
  mini/Fable agreement rate is recorded in the methodology note. Review tooling shows one utterance
  with all its per-goal labels on one screen, and supports a **blind-QA view** that hides the
  draft/accepted label.
- **Stage 5 — split and freeze.** Split by `session_id` and `semantic_cluster_id` (no id spans both
  splits), freeze, content-hash, record the roster snapshot hash and per-slice counts. The split
  and freeze are **deterministic given a recorded seed**, so the frozen sets are reproducible from
  the reviewed pool.

## Sizing and per-slice floors

v1 targets **≈80 utterances × the 7-goal roster ≈ 560 pairs**, sized to be human-reviewable in a
couple of hours. Size is **back-computed from explicit per-slice-per-split floors** — the floors
are the binding constraint, 80 is the nominal target that clears them with margin.

The roster is the frozen `realtime-seed-v1` snapshot's 7 goals: *Respect a person's boundaries*,
*Keep theses distinct from fact*, *Serve the present person*, *Grow the library*, *Learn what
drives this person*, *Track the AI transition*, *Assemble a world picture*.

**Counting unit for slice floors:** distinct **utterances** that carry the slice tag on any of
their pairs, counted within each split. (Most slice tags are utterance-level properties replicated
onto all 7 pairs of the utterance; `hard_negative` is a per-(utterance, goal) confusability marker.
The gatekeeper counts distinct utterances either way.)

**Proposed v1 floors** (the QA-gated decisive slices are the negation family, quoted speech, and
hypotheticals):

| Slice | Floor per split | Both splits |
|---|---|---|
| negation (explicit + implicit; ≥2 of each subtype) | 6 | 12 |
| quoted_speech | 5 | 10 |
| hypothetical | 5 | 10 |
| subject_confusion | 3 | 6 |
| punctuation_casing_loss | 3 | 6 |
| synthetic_asr | 3 | 6 |
| rare_high_cost | 3 | 6 |
| hard_negative (adversarial hard-paraphrase) | 4 | 8 |
| none_of_roster (utterance-level) | 8 | 16 |
| paraphrase clusters | 4 clusters, ≥2 utterances each | 8 clusters |
| real_asr / real_session | 0 (deferred to a later dataset version) | 0 |

**Distinct-utterance arithmetic (per split), exploiting deliberate multi-tagging** — e.g.
synthetic-ASR variants also carry `punctuation_casing_loss`; some negation utterances also carry
`subject_confusion` or `hard_negative`; adversarial hard-paraphrases sit inside paraphrase
clusters:

- negation ~6, quoted ~5, hypothetical ~5 (largely distinct) → ~16
- subject_confusion 3 (~1 overlaps negation) → ~2 new
- synthetic_asr 3 + punctuation_casing_loss 3 layered on the same texts → ~3
- rare_high_cost 3 → ~3
- hard_negative 4 (~2 overlap paraphrase clusters) → ~2 new
- none_of_roster 8 → 8
- paraphrase base ≥8 utterances → ~8

Sum ≈ 42 utterances/split → ≈ **84 utterances**, ≈ **560–590 pairs**. Nominal target: 80.
Stage-1 generation deliberately **over-produces ~1.3×** so the floors still hold after review
attrition and after the split-boundary constraint. That constraint is stronger than "each slice in
≥2 sessions/clusters": because a non-crossing split partitions the **connected components** of the
session↔cluster graph, a slice's utterances must be spread across ≥2 *distinct components* (not
merely 2 ids that a shared utterance later fuses into one component). The Phase B feasibility
preflight verifies a floor-meeting two-way component assignment actually exists **before** any paid
labeling or review; the Phase D gatekeeper re-checks the floors on the frozen sets.

**Expansion path.** A documented route to 1000+ utterances exists (Stage 5's "Documented expansion
path" note). The OpenAI **Batch API** (available upstream at kit `0.3.0`, commit `efc2b4c`: JSONL
codecs, `BatchTransport` trait, `BatchHandle`) is **deliberately not adopted for v1** — at this
size it saves cents against real state-management complexity and up-to-24 h latency — and is
recorded as the scale-up mechanism for the expansion path (candidate DecisionLog entry below). The
pinned `qsf_models` kit rev (`ca28629`) predates Batch support and is not bumped by this plan.

## Architecture and crate placement (binding)

`qsf_semantic_eval` may depend only on `qsf_volition` (+ serde). OpenAI / reqwest must **not** enter
its dependency graph (DecisionLog 2026-07-19, "Goal relevance evaluates the production Volition
scorer…"). Therefore the generation/labeling/review/split tooling lives in a **new lean crate**:

- **`crates/qsf_semantic_datagen`** — depends on `qsf_semantic_eval` (for the schema types — DRY,
  one source of truth for `PairRecord`, `RosterSnapshot`, slice tags), `openai_provider_kit` (via
  the same pinned git rev as `qsf_models`), `tokio`, `engine_logging`, and serde/serde_json/sha2. It
  must **never** be depended on by `qsf_semantic_eval` or `qsf_volition`. This mirrors the lean-crate
  dependency discipline (DecisionLog 2026-06-10, 2026-07-09).

**Executor (issue 5).** The pinned kit's `LlmProvider::complete` is **async**; `qsf_models` already
builds an explicit multi-thread Tokio runtime and `block_on`s each call at the sync `ModelClient`
boundary. The datagen live transport does the same: it owns a Tokio runtime and blocks on each
completion, keeping the datagen core synchronous and unit-testable. Hence the `tokio` dependency
above.

**Cost estimate (issue 5).** The kit reports token counts but carries **no pricing metadata**. The
tool therefore keeps a **versioned, tool-local price table** (a small checked-in table with a
provenance date and content hash, used only for the stdout estimate and kept out of any ledger). If
no price table matches the model in force, the tool **degrades to token-only reporting** rather than
fabricating a cost. This is deliberately separate from the parent plan's telemetry price-table work;
offline generation is excluded from the shared ledger (candidate DecisionLog entry).

**Dependency-graph guard (advisory).** A verification asserts `openai_provider_kit` / HTTP
dependencies never enter `qsf_semantic_eval`'s graph — a small test or CI check over `cargo tree
-p qsf_semantic_eval` that fails if `openai_provider_kit`, `reqwest`, or `tokio` appear. This makes
the one-directional boundary enforced, not just documented.

Within the crate, keep the unidirectional discipline (`Agents.md`): the **pure, unit-testable core**
— prompt construction, response parsing, interchange read/write and validation, the review
view-model and decision-application fold, the split algorithm, and the gatekeeper checks — is
separated from the **I/O transport** (OpenAI calls, file reads/writes, terminal rendering). The
default transport is a **recorded-fixture replay**, never a live call. `main.rs` stays a thin CLI
wrapper. Runtime logging uses `engine_logging` with enough context to identify the failing
generation/labeling operation (goal_ref, run id, model id).

## Schema v2 (`DATASET_SCHEMA_VERSION` → 2)

The current `PairRecord` provenance (`source`, optional `teacher_model_id`, `review_status`) cannot
carry the two-model split, and the record carries no explicit roster binding or stable utterance
identity. Bump `DATASET_SCHEMA_VERSION` from 1 to 2 and make three changes to `PairRecord`, all
under `deny_unknown_fields`:

1. **Add `roster_snapshot_version`** (a required top-level string, e.g. `"realtime-seed-v1"`) so
   every record self-declares the roster it was labeled against. `goal_ref` already encodes the
   version in its hash preimage, but that is opaque; an explicit field lets the gatekeeper verify
   the binding and lets the rebinding step (below) rewrite it mechanically (issue 1).
2. **Add `utterance_id`** (a required stable opaque string, shared with the interchange) so the
   dense-cross-product invariant is checkable by identity rather than by comparing full utterance
   strings, and so per-utterance metadata consistency is enforceable (issue 2).
3. **Replace provenance** with a nested lineage shape carrying run/artifact identifiers and content
   hashes, not just model IDs (issue 3):

```jsonc
"provenance": {
  "source": "teacher" | "real_session",
  "generation": {                     // present when source == "teacher"
    "generator_model_id": "gpt-5.4-nano",
    "generation_run_id": "genrun-<date>-<hash>",
    "generation_output_sha256": "sha256:...",   // hash of the generation-output.jsonl this came from
    "prompt_version": "goalrel-gen-v1",
    "saw_activation_keywords": false            // recorded methodology property; must be false for v1
  },
  "labeling": {
    "guideline_version": "goalrel-label-v1",
    "labelers": [
      { "labeler_id": "gpt-5.4-mini", "labeling_run_id": "mini-<date>-<hash>",
        "output_sha256": "sha256:..." },
      { "labeler_id": "claude-fable", "labeling_run_id": "fable-<date>-<hash>",
        "output_sha256": "sha256:..." }
    ]
  },
  "review": {
    "review_decisions_sha256": "sha256:...",     // hash of the review-decisions.jsonl folded in
    "review_status": "draft" | "reviewed"
  }
}
```

**Version-envelope loading (issue 4).** `Dataset::from_jsonl_str` today deserializes the full
`PairRecord` *before* checking `schema_version`, so after the provenance change a real v1-shaped
record fails on its old provenance shape and never reaches the friendly unsupported-version
diagnostic. Change the loader to inspect a minimal, permissive **envelope** first —
`struct SchemaEnvelope { schema_version: u16 }` deserialized without `deny_unknown_fields` (all
other fields ignored) — reject any unsupported version there, and only then decode the
version-specific `PairRecord`. A regression test feeds a **real v1-shaped line** (v1 provenance:
`{ "source": ..., "review_status": ... }`) and asserts the error names the unsupported version, not
a provenance field.

Notes and obligations:

- `goal_ref` hashing is **unchanged** (it hashes `roster_snapshot_version + title + summary +
  tension_summaries`); schema v2 does not touch the roster snapshot format.
- Update in lockstep (coupled artifacts, `Agents.md`): `crates/qsf_semantic_eval/src/schema.rs`
  (+ tests in `tests.rs`), `evaluation/schemas/GoalRelevanceCorpusAndRoster.md`, and
  `evaluation/frozen/goal-relevance/sample.dataset.jsonl` (regenerated at v2 so the default sample
  path keeps parsing). The `real_asr` slice tag already exists in the enum and is retained though
  unused in v1.
- The frozen sets carry `roster_snapshot_version` per record **and** in the freeze manifest (see the
  artifact contract). The **roster re-versioning / rebinding rule** (below) is stated in the schema
  doc and methodology note **before** human-review time is spent.

### Roster re-versioning and rebinding rule (F5)

State this once, authoritatively, in `evaluation/schemas/GoalRelevanceCorpusAndRoster.md`:

- A **keyword-only** edit to a seed goal (activation keywords / weight classes only, no
  title/summary/tension-summary change) produces a **new roster snapshot version**, but because
  `goal_ref` does not hash keywords, existing labels are **carried forward** via a documented
  **rebinding step**: each record's `roster_snapshot_version` field is rewritten to the new version
  and every `goal_ref` is re-issued mechanically for it (the hash preimage includes the version
  string, so the key changes even though the described goal is semantically identical), and labels
  are re-pointed without relabeling. The rebinding is a deterministic transform the datagen crate
  can run and re-freeze from, not a manual edit.
- A **title / summary / tension-summary** edit changes the described goal; the affected pairs must
  be **relabeled** (they no longer describe the same thing).

## Artifact and provenance completeness contract

This pipeline's "behavioral chain" is a data-provenance chain, not a runtime trace, but it earns
the same discipline (`ProjectWorkflow.md`, Trace Completeness Contract). Define the interchange
shapes before implementation; a validator in the datagen crate parses each artifact and asserts the
required fields, and the blinding guarantees are test-enforced.

**Retention rule (issue 3, Q3 — commit the full lineage).** Pipeline runs may use `runs/` as
scratch, but **everything an auditable number derives from is version-controlled under
`evaluation/`** beside the frozen sets. The governing rule, stated in the plan and the methodology
note: *if the methodology note cites a number derived from an artifact, that artifact is
version-controlled.* Raw model-call transcripts and unsanitized captures are the only exceptions and
never enter git.

**Artifact boundary** (committed paths under `evaluation/frozen/goal-relevance/lineage/<dataset_version>/`
unless noted):

```text
generation-output.jsonl   Stage-1 generated utterances + pipeline-known intended slice tags,
                          conditioning goal_ref, utterance_id, session_id, semantic_cluster_id,
                          generation_run_id (NEVER shown to labelers). Self-hashes to
                          generation_output_sha256.
labeling-input.jsonl      Blind interchange handed to Stage 2 and Stage 3 (below).
label-mini.jsonl          Stage-2 (gpt-5.4-mini) labels — its own file, its own labeling_run_id
                          and output_sha256.
label-fable.jsonl         Stage-3 (claude-fable) labels — its own separate file, run id, hash.
reconciliation.jsonl      Merged per-(utterance_id, goal_ref) view: {mini_label, fable_label,
                          agree: bool}. Derived deterministically from the two label files;
                          the disagreement queue and the mini/Fable agreement rate come from here.
review-decisions.jsonl    Operator accept/correct decisions, append-only (below).
blind-qa-decisions.jsonl  Cold re-annotations used only to measure accepted labels; never folded
                          into the reviewed pool.
reviewed-pool.jsonl       The reviewed PairRecord pool (schema v2, review.review_status=reviewed),
                          produced by folding review-decisions over reconciliation.
freeze-manifest.json      Authoritative freeze binding (below).
validation.dataset.jsonl  Frozen split (referenced by hash from the manifest).
test.dataset.jsonl        Frozen split (referenced by hash from the manifest).
DatasetMethodology.GoalRelevance.md   Human-readable summary derived from the artifacts.
```

**`generation-output.jsonl`** (one line per generated utterance), `deny_unknown_fields`: fields
`interchange_version`, `utterance_id`, `utterance`, `language`, `conditioning_goal_ref` (or `null`
for the vague `none_of_roster` batch), `intended_slice_tags`, `session_id`,
`semantic_cluster_id`, `generation_run_id`, `generator_model_id`, `prompt_version`,
`saw_activation_keywords` (must be `false`).

**`review-decisions.jsonl`** (append-only; one line per operator action), `deny_unknown_fields`:
`{ decided_at, utterance_id, goal_ref | null, field: "gold_label" | "none_of_roster", value }`.
**Fold conflict semantics:** the reviewed pool is a deterministic left-fold over the file in
recorded order; for a given `(utterance_id, goal_ref, field)` key, **last decision wins**, so a
correction supersedes an earlier one without editing history. A `field: "none_of_roster"` decision
has `goal_ref: null`. The fold is a pure function (Phase A), so re-folding the committed decisions
reproduces the reviewed pool exactly.

**`freeze-manifest.json`** — the authoritative binding for a frozen dataset version:
`{ dataset_version, roster_snapshot_version, roster_fixture_hash, split_seed,
validation_sha256, test_sha256, per_slice_counts_by_split, generation_output_sha256,
label_mini_sha256, label_fable_sha256, review_decisions_sha256, frozen_at }`. The gatekeeper and
the reproducibility test read this file; it is what makes the freeze reconstructible and
independently verifiable.

**Blind interchange — `labeling-input.jsonl`** (one line per utterance), `deny_unknown_fields`:

```jsonc
{
  "interchange_version": 1,
  "utterance_id": "<opaque, no slice/label leakage>",
  "utterance": "...",
  "language": "en",
  "roster_snapshot_version": "realtime-seed-v1",
  "roster": [ { "goal_ref": "sha256:...", "title": "...", "summary": "...",
               "tension_summaries": ["..."] } ]   // all 7 goals
}
```

Blinding guarantee (test-enforced): the file contains **no** `gold_label`, `slice_tags`, intended
goal, or generation provenance. A test round-trips a generated pool through the interchange builder
and asserts none of those fields can appear.

**Label interchange** — one line per utterance, identical shape written to the two separate files
`label-mini.jsonl` and `label-fable.jsonl`, `deny_unknown_fields`:

```jsonc
{
  "interchange_version": 1,
  "labeler_id": "gpt-5.4-mini" | "claude-fable",
  "labeling_run_id": "mini-<date>-<hash>" | "fable-<date>-<hash>",
  "guideline_version": "goalrel-label-v1",
  "utterance_id": "...",
  "per_goal": [ { "goal_ref": "sha256:...", "label": "relevant|not_relevant|ambiguous" } ],
  "none_of_roster": true | false
}
```

Consistency check (one validator, both stages): every `goal_ref` in `per_goal` resolves to the
roster; `none_of_roster: true` implies no `per_goal` label is `relevant` (mirrors the existing
`NoneOfRoster` rule in `qsf_semantic_eval`).

---

## Phase A — Schema v2, interchange contract, and the datagen crate skeleton

The smallest end-to-end slice that stands on its own: the schema bump plus a datagen crate that can
build blind interchange, parse recorded label fixtures, and assemble a reviewed pool — all against
**replay fixtures by default**, no network.

**Work**

- Bump `DATASET_SCHEMA_VERSION` to 2; add `roster_snapshot_version` and `utterance_id` to
  `PairRecord`; implement the nested lineage provenance shape (run ids + content hashes); convert
  the loader to **version-envelope-first** decoding (envelope struct without `deny_unknown_fields`
  checks `schema_version` before any version-specific decode). Update the schema doc and regenerate
  `sample.dataset.jsonl` at v2.
- Create `crates/qsf_semantic_datagen` (lean deps as above; thin `main.rs`).
- Implement the pure core: builders/parsers/validators for every interchange artifact
  (`generation-output`, `labeling-input`, `label-mini`/`label-fable`, `reconciliation`,
  `review-decisions`, `reviewed-pool`, `freeze-manifest`); the deterministic reconciliation
  (mini + Fable per `(utterance_id, goal_ref)`); and the reviewed-pool fold
  (last-decision-wins over `review-decisions.jsonl`) into schema-v2 `PairRecord`s.
- Implement a **replay transport** (reads recorded model responses from checked-in fixtures) and
  make it the **default** for both generation and labeling. The live transport owns a Tokio runtime
  and `block_on`s the async kit call; it is selected only by an explicit `--live` flag that
  additionally requires `OPENAI_API_KEY`; `cargo test` and the default `cargo run` never touch the
  network (`Agents.md`: defaults exercise the new path; tests never hit the network).
- Checked-in tiny replay fixtures covering: one generation response, one mini label response, one
  Fable label response.

**Verification (automated), run from repo root**

- `cargo build`; `cargo test -p qsf_semantic_datagen` and `cargo test -p qsf_semantic_eval` green.
- Schema v2 round-trips; a **real v1-shaped line** (old `{source, review_status}` provenance) is
  rejected with an error that **names the unsupported schema version**, not a provenance field
  (issue 4 regression test).
- Blinding test: interchange built from a generated pool exposes no label/slice/goal fields.
- Label-interchange validator accepts a well-formed mini and Fable file and rejects a
  `none_of_roster: true` record that also marks a goal `relevant`.
- Reviewed-pool fold is deterministic and last-decision-wins: a `review-decisions.jsonl` with a
  superseding correction produces the corrected pool.
- Default run uses replay transport and makes no network call (a test asserts the default transport
  is the replay one).
- **Dependency-graph guard (advisory):** a test/CI check fails if `openai_provider_kit`, `reqwest`,
  or `tokio` appear in `cargo tree -p qsf_semantic_eval`.
- Hygiene: `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Spends money:** no (replay only).

**Human testing:** not required this phase.

---

## Phase B — Generation stage (Stage 1)

Turn goal descriptions into the generated pool, proven against replay fixtures, then exercised once
live at trivial cost.

**Work**

- Prompt construction (pure) conditioned on **description only** (title + summary + tension
  summaries), with activation keywords structurally unavailable to the prompt builder (they are not
  passed in), so `saw_activation_keywords: false` is guaranteed by construction, not by discipline.
- Generation modes: natural utterances + paraphrase clusters; cluster anchors are generated first,
  operator-approved in the `generation-anchors.jsonl` sidecar, then embedded verbatim for the
  paraphrase batch; per-slice variant generators
  (explicit/implicit negation, quoted speech, hypothetical, subject-confusion,
  punctuation/casing-loss, rare high-cost); the **tagged adversarial hard-paraphrase** batch; and
  the **vague, goal-unconditioned, over-produced** batch for `none_of_roster`, whose final status
  is decided by blind labeling and human review rather than generation; semantically hard modes use
  `gpt-5.4-mini` while routine modes use `gpt-5.4-nano`. Synthetic-ASR corruption applies the
  observed casing/punctuation-loss + entity-mangling transform (a pure, seeded function), not
  random typos.
- Response parsing (pure) into `generation-output.jsonl` with pipeline-known intended slice tags and
  conditioning goal; assign `session_id` / `semantic_cluster_id`.
- **Deterministic split-feasibility preflight (issue 7).** A non-crossing split by `session_id` and
  `semantic_cluster_id` requires that both id partitions agree, i.e. the split respects the
  **connected components** of the session↔cluster bipartite graph (utterances sharing a session or a
  cluster are forced into the same split). Merely spreading a slice across ≥2 sessions/clusters does
  **not** guarantee a feasible two-way split that meets every floor. So a pure preflight builds those
  components and confirms that **some** assignment of whole components to the two splits satisfies
  every per-slice-per-split floor (a small deterministic search over component→split assignments,
  seeded and reproducible). The preflight runs on `generation-output.jsonl` **before** any paid
  labeling or human review; if infeasible, it reports which slice/floor cannot be met so generation
  can add utterances in fresh sessions/clusters. The real split in Phase D reuses this component
  algorithm.
- The tool prints its **own token usage / estimated cost to stdout** using the versioned tool-local
  price table (or token-only when no price matches); this offline generation is **excluded from the
  shared token-usage ledger** (candidate DecisionLog entry) so it does not collide with the parent
  plan's telemetry phase.
- A small live smoke run behind `--live` produces a handful of real utterances to validate prompts
  and the live transport.

**Verification**

- Automated: parsing the checked-in generation replay fixture yields the expected pool; the
  synthetic-ASR transform is deterministic given a seed (unit test on known input/output); the
  adversarial batch is tagged `hard_negative` and counted separately (a distribution test asserts it
  does not exceed a stated share of the base); the split-feasibility preflight passes on a feasible
  fixture pool and **fails with a named slice/floor** on an infeasible one (e.g. all negation
  utterances in one component). Hygiene: clippy + fmt.
- **Human testing (recommended):** the operator runs the `--live` smoke against `gpt-5.4-nano`,
  reads ~10 generated utterances, and confirms they are natural, on-description, and keyword-free
  (no obvious activation-keyword leakage). This is the cheap check before the full paid run.

**Spends money:** yes — trivial, well under $1 (one small live smoke run; the full v1 generation run
in Phase D is also nano-cheap).

---

## Phase C — Blind labeling stages, disagreement queue, and review tooling (Stages 2–4 scaffolding)

Build the blind labeling path (mini in code, Fable in-workflow), the disagreement queue, and the
one-utterance-per-screen review tool including the blind-QA view — all testable against replay
fixtures before any real review happens.

**Work**

- **Annotation guidelines FIRST (issue 6), gating everything else in this phase**
  (`evaluation/annotations/AnnotationGuidelines.GoalRelevance.md`, stamped `goalrel-label-v1`): label
  definitions with worked examples and the **Ambiguous policy**. The same rubric drives mini's
  prompt, Fable's ritual, and the operator's corrections; no labeling fixture, prompt, or call in
  this phase precedes it. **Relevance/negation policy (user-decided), stated with these examples:**
  negating a goal's *topic* does **not** make an utterance irrelevant to that goal — relevance means
  the utterance bears on the goal's tension space, *including opposing or countering it*. "I don't
  want to discuss my friend's private life" is **Relevant** to the boundaries goal (a stance
  countering a goal is valuable signal, e.g. useful counter-information to an unrealistic goal). A
  negation is **NotRelevant** only when it makes the utterance genuinely not about the goal at all —
  a stray disclaimer such as "I am not asking about a private friend." (This refines the two
  negation examples in the current sample dataset.)
- Stage 2 (in code): build `labeling-input.jsonl` from the generated pool; call `gpt-5.4-mini` (live
  transport) per utterance with the full roster; write `label-mini.jsonl`. Default runs use the
  replay label fixture.
- Stage 3 (in-workflow, not code): document the exact operator ritual — hand Claude Fable the same
  `labeling-input.jsonl`, receive `label-fable.jsonl` in the identical format. The datagen validator
  schema-checks Fable's output with the same code path as mini's.
- Reconciliation (pure): per `(utterance_id, goal_ref)`, compare mini vs Fable into
  `reconciliation.jsonl`; produce a **priority review queue** ordered disagreements-first; record the
  mini/Fable agreement rate.
- Review tooling (CLI subcommand): a pure view-model builder renders **one utterance with all 7
  per-goal labels on one screen**, plus the utterance-level `none_of_roster`; the thin terminal
  front-end appends operator accept/correct decisions to `review-decisions.jsonl`. A **blind-QA
  view** hides the draft/accepted label so the operator can re-annotate cold and writes only to the
  separate `blind-qa-decisions.jsonl`, which is never an input to the reviewed-pool fold (F6, and
  used again in Phase D). Decision application is a pure fold (Phase A) so review is reproducible.

**Verification**

- Automated: reconciliation over a fixture with a planted disagreement puts that pair first in the
  queue and computes the expected agreement rate; the review view-model for a fixture utterance
  lists exactly its 7 per-goal entries; the blind-QA view-model omits the label field; applying a
  recorded `review-decisions.jsonl` fold yields the expected reviewed pool. Hygiene: clippy + fmt.
- **Human testing (recommended, small):** the operator reviews a handful of fixture utterances end
  to end to confirm the one-screen layout and the blind-QA toggle are usable before the full review.

**Spends money:** the mini labeling run is paid but trivial (well under $1); Fable is zero API cost
(Claude Max subscription); replay-default tests are free.

---

## Phase D — Full run, review, split, gatekeeper, freeze, and methodology note (Stage 5)

Produce the actual frozen v1 sets.

**Work**

- Run the full v1 generation (Phase B) and mini labeling (Phase C) at production size; obtain
  Fable's cross-labels; the operator completes review of **every** pair via the review tool
  (disagreements first).
- **Blind self-re-annotation QA (F6):** the operator re-labels a shuffled sample of the negation,
  quoted-speech, and hypothetical slices from raw utterance + goal description with the
  draft/accepted label hidden (the Phase C blind-QA view). Compute per-slice agreement.
- **Split algorithm (pure, deterministic given a recorded seed):** using the Phase B
  connected-component algorithm, assign whole session↔cluster components to validation or test so
  that **no `session_id` and no `semantic_cluster_id` appears in both splits**, meeting the
  per-split floors. The seed is recorded so the split is reproducible from the reviewed pool.
- **Gatekeeper (automated) blocks the freeze unless all hold:**
  - **dense cross-product invariant (issue 2):** exactly one pair for every
    `(utterance_id, roster goal_ref)` — no missing goal, no duplicate, no extra pair — and per
    utterance the `session_id`, `semantic_cluster_id`, `language`, `utterance_roster_annotation`, and
    utterance-level slice tags are consistent across its 7 pairs;
  - **roster binding (issue 1):** every record's `roster_snapshot_version` equals the manifest's, and
    every `goal_ref` resolves within that roster snapshot;
  - every per-slice-per-split floor in the sizing table is met in **both** splits (counting distinct
    utterances bearing the slice);
  - no `session_id` / `semantic_cluster_id` spans both splits;
  - every `none_of_roster` utterance has only `not_relevant` / `ambiguous` pairs (existing rule);
  - every pair is `review.review_status: reviewed`;
  - the roster snapshot round-trips (`assert_matches_current_realtime_seed`) or is deliberately
    re-versioned;
  - **the blind-QA per-slice agreement meets the freeze floor of ≥ 0.80 per hard slice**
    (user-confirmed).
- **Freeze:** content-hash and version `validation.dataset.jsonl` and `test.dataset.jsonl` and write
  `freeze-manifest.json` (roster snapshot version + fixture hash, split seed, the two split hashes,
  per-slice counts by split, and the lineage-artifact hashes). Commit the full lineage under
  `evaluation/frozen/goal-relevance/lineage/<dataset_version>/` (Q3: *anything a cited number derives
  from is version-controlled*).
- **Methodology note** (`evaluation/annotations/DatasetMethodology.GoalRelevance.md`): records the
  mini/Fable agreement rate, the blind-QA per-slice agreement figures, the
  "generator never saw activation keywords" conditioning property, the dense-cross-product coverage
  statement, the roster re-versioning/rebinding rule, per-slice counts per split, the split seed, and
  the manifest content hashes. (The annotation guidelines were authored in Phase C.)

**Verification**

- Automated: schema validation over the full frozen sets; the gatekeeper passes on the frozen sets
  and **fails on each injected violation** (a floor made short, an id planted in both splits, an
  unreviewed pair, a QA figure below 0.80, a missing/duplicate cross-product pair, a mismatched
  `roster_snapshot_version`) — a test proves the gate has teeth on every rule; the split and the
  reviewed-pool fold are reproducible from the committed lineage + recorded seed; the freeze manifest
  and methodology note parse and carry the required hashes, agreement figures, and counts.
- **Human testing (required — this *is* the deliverable):** the operator's full review and blind-QA
  pass produce the frozen sets. Evidence to collect: per-slice self-agreement, per-slice counts, the
  mini/Fable rate, and a short note on label categories that were hard to adjudicate (feeds the
  ambiguous-heavy-goals Open Question). An external reviewer sampling a subset of labels before the
  sets are treated as frozen is recommended.

**Spends money:** yes — trivial (nano generation + mini labeling at ~80×7 scale is cents; Fable is
free).

---

## Phase E — Opt-in realtime transcript capture and sanitization rules (unblocks a later `real_session` slice)

v1 defers the `real_session` / `real_asr` slice because only 2 verbatim turns exist on disk. This
phase makes the slice **populatable later** without changing v1's freeze. Concrete sanitization
rules are now decided (Policy B, below), so this phase is executable.

**Default-that-exercises-the-path (issue 9).** The capture + sanitization **logic runs by default**
through a **non-persisting sink**: on the realtime path the capture pipeline builds and sanitizes
the candidate corpus record on every trusted final transcript, then (by default) discards it —
exercising the new code (`Agents.md`: defaults exercise the new path) while writing nothing durable
(privacy default). **Durable persistence is the opt-in**: an explicit env flag / launcher profile
switches the sink to a file writer (consistent with the explicit-opt-in provider pattern, DecisionLog
2026-05-12, 2026-05-15). Raw unsanitized captures, when the operator enables them, live only under
`state/` (gitignored), never git.

**Work**

- Add the capture pipeline to the realtime path with a **`CaptureSink`** boundary (pure sanitizer +
  swappable sink: non-persisting default, file-writing when opted in). Capture uses only trusted
  final transcripts (respects the browser-relay-vs-sideband trust boundary) and records user turn
  text plus minimal provenance (session id, timestamp, roster snapshot version in force,
  `sanitized: true`). It does not alter continuity, sleep, or memory promotion.
- `evaluation/annotations/SanitizationRules.md` commits **Policy B — category rules with review-time
  redaction** (user-decided):
  - raw captures persist only outside git (under `state/`); what enters the corpus is a **sanitized
    copy produced during the operator's per-turn review**;
  - third-party **personal names → consistent pseudonyms**; **role words** (friend, boss, colleague,
    family, daughter, doctor) are **KEPT** — they are activation-relevant content;
  - **contact details, addresses, credentials, identifying employers/institutions → always dropped
    or generalized**;
  - **health / finance / relationship specifics → per-turn operator judgment**, with "skip this turn
    entirely" always available;
  - sanitized records carry a flag (`sanitized: true`) so the methodology note can report what was
    transformed;
  - anything genuinely unresolved stays **marked open** in the file (nothing raw is included
    silently).
- A later dataset version (out of scope for v1) ingests captured, sanitized turns as the
  `real_session` / `real_asr` slice.

**Verification**

- Automated: the **default** run exercises sanitization through the non-persisting sink and writes no
  durable artifact (a test asserts the default sink persists nothing yet the sanitizer ran); the pure
  sanitizer maps a fixture turn per Policy B — a personal name becomes a stable pseudonym across two
  turns, a role word survives, an address/credential is dropped, and a `sanitized: true` flag is set;
  a test asserts a forbidden category cannot survive into the sanitized record; opting the sink into
  file persistence writes the expected corpus-eligible record. Hygiene: clippy + fmt; if any realtime
  `ui/` code changes, run `npm run check` then `npm run fmt` from that directory.
- **Human testing (recommended):** the operator runs one real `qsf.ps1 realtime` session with durable
  capture enabled and confirms the captured artifact contains correctly sanitized verbatim turns.

**Spends money:** only if the operator chooses a real realtime session for the human check;
the automated path is free.

---

## Which phases spend real API money

| Phase | Spend |
|---|---|
| A — schema v2 + crate skeleton | none (replay only) |
| B — generation | trivial (< $1): one nano smoke run |
| C — labeling + review tooling | trivial (< $1): mini smoke; Fable free |
| D — full run + freeze | trivial (< $1): nano + mini at ~560-pair scale; Fable free |
| E — capture + sanitization | none automated; optional real realtime session for the human check |

All paid work is well under $1 total. Every phase's default code path (replay transport;
capture+sanitization through the non-persisting sink) is exercised with no network and no spend.

## Exit criteria (this plan)

- `DATASET_SCHEMA_VERSION` is 2 with the two-model provenance shape; the schema doc, sample dataset,
  and schema tests match; v1 records are rejected loudly.
- `crates/qsf_semantic_datagen` exists, lean, replay-default, with the pure core separated from I/O.
- Frozen, content-hashed, human-reviewed `validation.dataset.jsonl` and `test.dataset.jsonl` exist
  under `evaluation/frozen/goal-relevance/`, split by session and semantic cluster (no id spans
  both), meeting the per-slice-per-split floors in both splits, all pairs `reviewed`.
- The gatekeeper enforces the dense-cross-product invariant, roster-version binding, per-slice
  floors, split integrity, review completeness, and the ≥0.80 blind-QA agreement floor, and has a
  teeth-proving test on every rule; a `freeze-manifest.json` binds each frozen dataset version.
- Annotation guidelines, `SanitizationRules.md`, and the dataset methodology note (with mini/Fable
  agreement, blind-QA per-slice agreement, conditioning property, per-slice counts, split seed, and
  content hashes) exist.
- An opt-in realtime transcript-capture mechanism exists so a later dataset version can populate the
  `real_session` / `real_asr` slice; v1 freezes with that slice deferred.

## Documents to create or update (`ProjectWorkflow.md`)

- **Create** `crates/qsf_semantic_datagen` (crate + tests).
- **Create** `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md`,
  `evaluation/annotations/SanitizationRules.md`, and
  `evaluation/annotations/DatasetMethodology.GoalRelevance.md`.
- **Create** the frozen `validation.dataset.jsonl` / `test.dataset.jsonl`, the `freeze-manifest.json`,
  and the committed lineage tree (`lineage/<dataset_version>/`: generation output, `label-mini`,
  `label-fable`, reconciliation, review decisions, reviewed pool) under
  `evaluation/frozen/goal-relevance/`.
- **Update** `crates/qsf_semantic_eval/src/schema.rs` (+ `tests.rs`),
  `evaluation/schemas/GoalRelevanceCorpusAndRoster.md` (schema v2 + re-versioning/rebinding rule),
  and `evaluation/frozen/goal-relevance/sample.dataset.jsonl` (regenerated at v2).
- **Correct** `docs/Plans/Plan.SemanticEvaluationFoundation.md`: the "existing tens of turns" claim,
  the frozen-sets phase's slice-coverage verification bullet (real-session slice deferred), and the
  teacher-decision wording (two-model generate/label + independent Claude cross-label + human
  review). *(Applied when this plan lands; see the corrections section above.)*
- **Update** `docs/Handoff.md` when a phase lands and changes a Now/Next/Horizon recommendation
  (pointer only).
- **DecisionLog candidate entries** (proposed here; committed only when the work lands — the plan
  describes them as proposed until then, and no durable doc cites this plan's phases). Each is mapped
  to the phase whose landing commits the behavior (advisory):
  - *(commits with Phase C/D)* Goal-relevance data is produced by a **two-model OpenAI generate/label
    split with independent Claude cross-labeling and mandatory human review**; both model labelers are
    blind to the generator's intent and slice tags.
  - *(commits with Phase B)* Generation is **conditioned on goal descriptions only**, with activation
    keywords withheld, and that property is a recorded prerequisite of the failure-floor measurement.
  - *(commits with Phase C/D)* Pair coverage is a **dense utterance×roster cross-product**, so
    negatives are systematic and precision is comparable across dataset versions.
  - *(commits with Phase A)* The **roster re-versioning / rebinding rule** (keyword-only edits rebind
    labels via re-issued `goal_ref`s and a rewritten `roster_snapshot_version`; description/tension
    edits force relabeling).
  - *(commits with Phase B)* **Offline dataset generation is excluded from the shared token-usage
    ledger** (offline tooling, not a production surface); the datagen tool reports its own usage via a
    versioned tool-local price table, degrading to token-only when no price matches.
  - *(commits with Phase B/expansion)* The **OpenAI Batch API is deferred** to the 1000+-utterance
    expansion path; v1 uses synchronous chat completions.
  - *(commits with Phase D, Q3)* The **full dataset lineage is version-controlled under
    `evaluation/`** — anything a methodology-note number derives from is committed; raw model
    transcripts and unsanitized captures are the only exceptions.
- **Advisory — `runner.rs` scope note:** this plan produces the frozen sets but does **not** wire
  `qsf_semantic_eval`'s `runner.rs` to consume them; the runner keeps its sample-dataset default
  until the parent plan's failure-floor phase adds explicit validation/test input selection. No
  change to `runner.rs` is in scope here.
- **Do not** cite this plan's phase labels from any durable document; name the behavior.

## Settled during review (was open; now decided)

- **Floors and QA gate — confirmed.** Per split: negation 6 (≥2 explicit, ≥2 implicit), quoted
  speech 5, hypothetical 5, plus the lighter floors in the sizing table; the binding table governs.
  The blind-QA freeze gate is **≥ 0.80 per hard slice**.
- **Sanitization — decided (Policy B).** Category rules with review-time redaction, committed in
  `SanitizationRules.md` (Phase E); genuinely unresolved items stay marked open there.
- **Artifact retention — decided (commit full lineage under `evaluation/`).** Rule: if the
  methodology note cites a number derived from an artifact, that artifact is version-controlled.

## Open Questions (surfaced, not resolved)

1. **Expansion trigger.** What evidence prompts growing past ~80 utterances (e.g. a slice whose
   failure-floor estimate is too noisy at n≈10, or ambiguous-heavy goals with unstable labels) —
   and at what point the Batch API becomes worth its complexity.
2. **Ambiguous-heavy goals.** Whether any goals accumulate enough `ambiguous` labels after the first
   review round to warrant extra utterances (feeds off the Phase D "hard to adjudicate" note).
3. **Residual sanitization specifics.** The category rules are decided; edge cases (e.g. how much
   health/finance/relationship detail an operator may keep on a per-turn judgment call) remain open
   in `SanitizationRules.md`.
