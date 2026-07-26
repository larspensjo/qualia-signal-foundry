# Plan: Panel labeling for the frozen goal-relevance sets

Status: Proposed — not started
Maturity: Candidate
Area: Evaluation infrastructure / Volition (goal relevance) / Data generation
Implements: `docs/Plans/Design.GoalRelevancePanelLabeling.md` (authoritative for the design; this
plan sequences it, and the census corrections below have been carried back into it)
Evidence: `docs/Reviews/Review.GoalRelevanceGateFeasibility.md`
Parent: `docs/Plans/Plan.GoalRelevanceFrozenSets.md`

## Why this plan exists

The goal-relevance pipeline machinery is complete and reviewed — generate, label, reconcile,
validate-labels, review, split, gatekeep, freeze, methodology, with gatekeeper teeth-proving tests
and a byte-reproducible freeze. Nothing is blocked on missing infrastructure. What failed was the
*human review stage*: per-pair adjudication against a capable model is not a task an operator can
perform reliably, and the review CLI pre-fills mini's answer, so the ten utterances that were
reviewed are anchored and unusable.

`Design.GoalRelevancePanelLabeling.md` replaces human per-pair review with a weighted seven-model
panel plus an independent auditor. This plan makes that design executable. It also carries the
consequences of the gate-feasibility census run on 2026-07-25, which invalidated the design's gate
shape on structural grounds.

Two facts about the current state shape everything below:

- The production pool (114 utterances × 7 goals = 798 pairs, labeled by GPT-5.4-mini and Claude
  Fable 5 under `goalrel-label-v1`) and its replay/evidence lineage are committed under
  `evaluation/frozen/goal-relevance/lineage/pools/goalrel-generation-live/`. The 591/798 agreement
  figure and the 207-disagreement corpus that the census, rubric-sensitivity check, and rubric's
  worked examples rest on are therefore version-controlled rather than local-only.
- No frozen test set exists. The only frozen artifact is
  `evaluation/frozen/goal-relevance/sample.dataset.jsonl` — 12 records, utterance ids
  `sample-1`..`sample-11`, schema v2 — which is also the runner's default input.

This is a multi-phase effort, so it is a `Plan.*.md` and not an `Experiment.*.md`: it is
data-production methodology, not a consciousness-simulation mechanism under question
(`ProjectWorkflow.md`, *Document Tracks*; the design says the same).

## Parent-plan reconciliation

`Plan.GoalRelevanceFrozenSets.md` still reads *"Status: Proposed — not started"* while its Phases A
through D are implemented and its Phase E is not. That status line is corrected as the first
documentation act of this plan (Phase A), to:

> Status: Machinery complete (Phases A–D landed 2026-07-21..22); the labeling and freeze campaign is
> superseded by `Plan.GoalRelevancePanelLabeling.md`. Phase E (opt-in transcript capture) is not
> started and remains owned by this plan.

The parent plan's per-slice floors, split algorithm, dense cross-product invariant, roster binding,
retention rule, and lineage boundary all stand. This plan changes only what the design changes: the
labeling stage, the review stage, the gate, and the manifest.

## Naming and ephemerality

This document owns its phase labels (A–L). No durable artifact — the schema, the guidelines, the
audit policy, the manifest, the methodology note, the decision log, the code — may cite them; each
names the behavior instead (`Agents.md`; `ProjectWorkflow.md`). Runtime modules are named after
behavior (`ledger`, `aggregation`, `audit`, `census`), never after a phase.

### Vocabulary: a member pass is not a labeling run

Chunking is mandatory (`labeling-input.jsonl` is 403 KB / ~100k tokens), and snapshot selection
admits a set of runs per member, so these two must not be conflated anywhere in the plan, the code,
or the manifest:

- **Panel member pass** — one panel member's *complete* coverage of the pool under one guideline
  version. There are exactly **eight** passes in v1: seven panel members plus the auditor. A pass is
  not an identifier; it is the `(panel_member_id, guideline_version)` grouping that the snapshot
  selection expresses.
- **Labeling run** — one `labeling_run_id`, one ledger entry, one content-hashed artifact, covering
  a *subset* of the pool. A pass is composed of one or more runs whose coverage is pairwise disjoint
  and whose union is the pool. At a 10-utterance chunk size the ledger will hold on the order of
  **12 runs per hand-driven pass**, so roughly 60–70 runs for eight passes.

Wherever a count is stated, it says which unit it counts. The manifest groups `selected_runs[]` by
`panel_member_id` and records `runs_per_member`, so a reader sees eight passes composed of N runs
rather than an undifferentiated list.

---

## What the census settled

`docs/Reviews/Review.GoalRelevanceGateFeasibility.md` is the evidence artifact; it is not restated
here. Its four load-bearing findings:

1. **The designed gate is structurally unreachable.** Each hard slice was generated conditioned on
   exactly two roster goals, so `min_evaluated_goals ≥ 4` per slice cannot be met by any amount of
   labeling. This is a generation-conditioning defect, not a sample-size problem.
2. **The split is forced.** Exactly two `session_id`s and two `semantic_cluster_id`s (pool-a,
   pool-b, 57 utterances each), so pool-a/pool-b is the only non-crossing split. There is no
   freedom to rebalance thin cells. Which pool becomes validation and which becomes test is decided
   only by the recorded split seed (DecisionLog 2026-07-22), which Phase B fixes and records.
3. **The disagreement is one-directional.** 199 of 207 disagreements are mini `relevant` against
   Fable `not_relevant`. Raw positives: mini 312/798 (39.1%), Fable 118/798 (14.8%) — a 2.6×
   spread. This is a relevance-threshold disagreement, not a two-goal confusion.
4. **The conservative breadth policy already exists** in
   `AnnotationGuidelines.GoalRelevance.md`, names four of the five worst-disagreeing goals, carries
   worked examples, and landed on 2026-07-21 *before both labeling runs*. Both labelers had
   it and still split 199 times in one direction. So `goalrel-label-v2` is a **determinacy**
   problem, not a missing-stance problem.

Measured per `(goal × split)` over the whole split, every one of the 14 cells clears
`min_relevant_support = 3` under both the strict and optimistic bounds. The parent plan's per-slice
floors are met (negation 8/8, quoted 6/6, hypothetical 6/6 per split against 6/5/5).

## The gate (defined by the design)

The authoritative gate definition, threshold classification, retirement of
`min_evaluated_goals`, and rationale for moving macro recall to the split level are in the design's
*The gate* section. This plan sequences that design. The census evidence also makes the hard-slice
conditioning defect a recorded v1 known limitation rather than a v1 fix: hard slices must condition
on all seven goals in a future dataset version. Regeneration stays a live option at the pool-size
checkpoint.

### Gold `ambiguous`: modelled minimally, measured fully

- `relevant_false_positive_rate` excludes pairs whose `aggregation_status == tied` — **not**
  gold-`ambiguous` pairs broadly. A deadlocked pair should leave the FP denominator: penalising the
  auditor for picking a side the panel could not call is indefensible. A **consensus**-`ambiguous`
  pair is different — the panel agreed the pair is genuinely undecidable, so an auditor asserting
  `relevant` there *is* over-calling relevance, which is exactly what the metric exists to catch.
  A fixture test pins both halves.
- Gold-`ambiguous` share per slice and per `(goal × split)` is **computed and recorded** in the
  audit evidence, and **not gated**. `report.rs:129-133` already computes `ambiguous_pair_count`
  per slice plus a top-level count, so hollowing is already visible; what is missing is
  **attribution** — deadlock versus rubric-ambiguity.
- `aggregation_status` therefore reaches `PairResult` in the **same** schema bump that adds
  `panel_vote`. The design states a consumer distinguishes the two "by reading the status field";
  that is currently false for the primary consumer, which carries only `gold_label`. This is not
  deferred to a second bump.
- **Ties are observed, never projected.** Tie behaviour is measured at the dry run and reported as
  **partitions** — where the ties fell across goals, splits and hard slices — never as a rate
  multiplied through a per-cell support projection. The erosion that a tie subtraction would have
  modelled is already absorbed by the checkpoint's `min_relevant_support + 2` margin (Phase J), and
  a pre-registered **tie tripwire** (Phase E) turns an unexpectedly active tie mechanism into an
  explicit operator decision rather than a silent numeric adjustment.

---

## Artifact and provenance completeness contract

The parent plan's contract (`Plan.GoalRelevanceFrozenSets.md`, *Artifact and provenance
completeness contract*) still governs; this section extends it for the panel chain, per
`ProjectWorkflow.md`'s trace-completeness discipline.

**Three standing rules apply to every artifact below**, and every phase's verification enforces them
on the *generated* artifact, not on prose:

1. **Named required fields.** Every artifact has a `deny_unknown_fields` type with an explicit
   required-field list and a parser in the datagen crate.
2. **No floating point.** Every rate, share, margin and probability in a persisted artifact is an
   integer in **basis points** (`_bp`, 0–10000) or an explicit integer numerator/denominator pair.
   This is the same constraint the manifest canonicalization already imposes, applied to every
   replay artifact so hashes stay stable and re-derivation is exact. (`AgreementEvidence.rate: f64`
   in `frozen.rs:31-35` disappears with the metric it belongs to.)
3. **Artifact-parsing verification.** Each phase that emits an artifact has a test that parses the
   real generated file and asserts every required field is present and internally consistent —
   not merely that the command exited zero.

### Artifact boundary and retention, split by replay-necessity

The parent plan's governing rule stands: *if the methodology note cites a number derived from an
artifact, that artifact is version-controlled*, with raw model-call transcripts and unsanitized
captures the only exceptions (`Plan.GoalRelevanceFrozenSets.md:284-285`). This plan adds one
distinction.

- **Replay inputs are committed.** The ledger, every per-run label artifact, `labeling-input.jsonl`,
  the weights file, the audit policy, the calibration fixtures, the raw and derived `none_of_roster`
  votes, and the audit evidence are needed to re-derive gold labels. "Replay rebuilds rather than
  trusts" is false unless they are in git. Sizes are trivial: generation output 82 KB, one label
  file ~110 KB, eight passes ~880 KB, the whole current pool 1.1 MB.
- **Provenance evidence is not committed.** Session transcripts are never read by replay and fall
  under the existing exception. `transcript_sha256` is **kept**: dropping it would not drop the
  transcript, it would drop the *binding*, so a transcript could be swapped later with no trace —
  which matters precisely where it defends an attestation.

Committed layout (single home per artifact; no copies to drift):

```text
evaluation/frozen/goal-relevance/lineage/
  ledger.jsonl                                  append-only run index (goalrel-label-v2 runs only)
  policy/
    panel-weights.<weights_version>.json        integer units, lineage totals, declared cap
    audit-policy.<policy_version>.json          metrics, thresholds, failure model, floor table
    calibration/<policy_version>/               planted-drift fixtures + sweep evidence
  pools/<generation_run_id>/
    generation-output.jsonl                     replay input (never shown to labelers)
    generation-anchors.jsonl                    the approved anchor sidecar
    labeling-input.jsonl                        replay input; the artifact every run was handed
    split-summary.json                          the fixed split seed and pool -> split binding
    label-runs/<labeling_run_id>.jsonl          one LabelInterchange file per ledger run
    evidence/
      operator-gates/                           anchors-proposed.jsonl, anchors-approved.jsonl,
                                                anchors-readable.md, AnchorReviewBrief.md,
                                                AnchorReview.Results.md, readable-pool.md,
                                                PoolReviewBrief.md, PoolReview.Results.md
      goalrel-label-v1/                         label-mini.jsonl, label-fable.jsonl,
                                                reconciliation.jsonl, reconciliation-summary.json,
                                                review-decisions.jsonl, disagreements-readable.md,
                                                DisagreementPreSortBrief.md
      census/<date>-<mode>.json                 committed census / re-census evidence
      rubric-v2/                                reader trial outputs (never ledger, never gold)
  <dataset_version>/
    snapshot-selection.json                     the selection this version aggregates
    none-of-roster-raw.jsonl                    the weighted utterance-level vote, as cast
    none-of-roster-derived.jsonl                the derived value and the override record
    audit-evidence.json                         full per-cell audit evidence
    validation.dataset.jsonl  test.dataset.jsonl
    freeze-manifest.json
    DatasetMethodology.GoalRelevance.md
```

`blind-qa-decisions.jsonl` is **not** in this list and is not expected anywhere: it was never
produced. The eighteen files rescued from `runs/goalrel-production/` are exactly those named above;
there is no blind-QA file, so any check that demands one is wrong. The `operator-gates/` group is committed
rather than discarded because DecisionLog 2026-07-22 makes the anchor approval and the pre-labeling
pool review *required campaign steps* — their artifacts are the evidence those gates were passed.

Three deliberate deviations from the design's literal wording, all carried into the amendment:

- **The policy files live under `lineage/policy/`, not under `lineage/<dataset_version>/`.** They
  are version-stamped in their own filenames and pinned by hash in every manifest that uses them, so
  one home keeps them DRY across dataset versions while remaining fully committed and fully
  reproducible. Cross-version reuse is therefore settled by the policy layout and manifest hashes.
- **The ledger indexes runs; the run artifacts hold the verdicts.** The design says the ledger
  "holds every verdict"; duplicating 6,384 verdicts into a second file would create a second source
  of truth. The ledger holds one content-hashed entry per run with its utterance coverage, and
  selection resolves member → runs → artifact → verdicts. Append-only, never rewritten, hash-pinned:
  every property the rule exists for is preserved.
- **The `goalrel-label-v1` mini and Fable runs stay *outside* the ledger.** See Phase C.

### Ledger run entry (`ledger.jsonl`, append-only, `deny_unknown_fields`)

Every field is required; `harness` is required when `build_attestation` is `operator_attested`;
`transcript_sha256` is nullable only for `run_mode: api` runs, where no session transcript exists.

```jsonc
{
  "ledger_version": 1,
  "labeling_run_id": "opus-20260801-part1",     // one chunk-level run, not a member pass
  "panel_member_id": "opus-5",
  "role": "panel" | "auditor",
  "model_id": "claude-opus-5",
  "model_build": "<provider string, or the displayed model name verbatim>",
  "build_attestation": "provider_reported" | "operator_attested",
  "harness": "<IDE or client name and version>",
  "run_mode": "api" | "manual",
  "tools_available": [],
  "workspace_isolation_attested": true,
  "workspace_listing_sha256": "sha256:...",
  "guideline_version": "goalrel-label-v2",
  "guideline_sha256": "sha256:...",
  "labeling_prompt_sha256": "sha256:...",
  "labeling_input_sha256": "sha256:...",
  "pool_id": "<generation_run_id>",
  "run_artifact_path": "lineage/pools/<pool>/label-runs/opus-20260801-part1.jsonl",
  "run_sha256": "sha256:...",
  "transcript_sha256": "sha256:..." | null,
  "utterance_ids": ["..."],
  "appended_at": "2026-08-01T09:14:00Z"
}
```

### Snapshot selection (`<dataset_version>/snapshot-selection.json`, `deny_unknown_fields`)

All fields required:

```jsonc
{
  "selection_version": 1,
  "snapshot_id": "goalrel-v1",
  "pool_id": "<generation_run_id>",
  "weights_version": "panel-weights-v1",
  "guideline_version": "goalrel-label-v2",
  "guideline_sha256": "sha256:...",
  "expected_utterance_ids_sha256": "sha256:...",   // over the pool's sorted utterance id list
  "expected_goal_refs_sha256": "sha256:...",       // over the frozen roster's sorted goal refs
  "panel": [
    { "panel_member_id": "opus-5", "labeling_run_ids": ["opus-20260801-part1", "..."] }
  ],
  "auditor": { "panel_member_id": "kimi-k3", "labeling_run_ids": ["..."] }
}
```

Validation rules, each with a rejection test:

- every `labeling_run_id` resolves to exactly one ledger entry, and its `pool_id` matches;
- within a member, the selected runs' `utterance_ids` are **pairwise disjoint** and their union is
  **exactly** the pool's utterance set — an overlap is a hard error, a gap is a hard error;
- the `panel[]` members are exactly the members carrying weight in `weights_version`, no more and
  no fewer;
- the auditor's `panel_member_id` carries no weight in `weights_version`;
- every selected run carries the same `guideline_version` and `guideline_sha256` as the selection;
- exactly one verdict per `(snapshot, panel_member_id, utterance_id, goal_ref)` and per
  `(snapshot, panel_member_id, utterance_id)` `none_of_roster` vote;
- "latest" is never consulted — a test asserts no ordering or timestamp field influences selection.

Selection is **relaxed** from the design's "exactly one `labeling_run_id` per `panel_member_id`" to
the set form above. Every property the original rule exists for survives. Its **primary**
justification is mid-run resumability, not pool growth: a hand-driven session abandoned at utterance
60 is either one run of a pass, or 60 utterances of wasted operator time. That makes it a
precondition for the labeling phase being survivable at all, which is why its test is a phase-exit
condition of Phase C rather than an extra. Its secondary justification is that without it, growing
the pool later forces every member to re-label the whole pool and writes off the v1 spend —
deferral would cost roughly 170% instead of 75%.

### The `none_of_roster` vote, raw and derived

The manifest requires separate `none_of_roster_raw_sha256` and `none_of_roster_derived_sha256`
hashes, and the schema carries only the *derived* `UtteranceRosterAnnotation`. Two artifacts
therefore exist, both replay inputs:

`none-of-roster-raw.jsonl` — one line per utterance, `deny_unknown_fields`:

```jsonc
{
  "aggregation_version": "goalrel-aggregation-v1",
  "weights_version": "panel-weights-v1",
  "utterance_id": "...",
  "total_units": 540,
  "true_units": 310,
  "false_units": 230,
  "aggregation_status": "consensus" | "plurality" | "tied",
  "winner_share_bp": 5740,
  "margin_bp": 1481,
  "raw_value": true
}
```

**Tie rule for the boolean vote** (a design detail the design leaves open; carried into the
amendment): on `tied`, `raw_value` is `false`. An utterance is not declared outside the roster on a
deadlock, which is the conservative direction and the one consistent with the pair-level derivation
override.

`none-of-roster-derived.jsonl` — one line per utterance, `deny_unknown_fields`:

```jsonc
{
  "utterance_id": "...",
  "raw_value": true,
  "derived_value": false,
  "override_applied": true,
  "overriding_goal_refs": ["sha256:..."]
}
```

The derivation is the design's rule: if any pair for the utterance wins `relevant`, the derived
value is `false` regardless of the vote. `none_of_roster_override_count` in the manifest is the
count of `override_applied: true`, so the methodology note can report how often it fired.

### Audit evidence (`<dataset_version>/audit-evidence.json`)

All rates in basis points. Records, per split: `macro_relevant_recall_bp`; per goal the support
count, `relevant_recall_bp`, the **effective floor at that denominator**, and pass/fail. Per
`(slice × split)`: `relevant_false_positive_rate_bp` with its integer denominator and the count of
`tied` pairs excluded, `abstain_rate_bp`, `utterance_relevant_set_match_bp`. Plus the
gold-`ambiguous` share per slice and per `(goal × split)` with deadlock/rubric attribution, the
`aggregation_status` distribution, and the overall verdict.

---

## Phase 0 — Gate feasibility census (DONE, 2026-07-25)

Complete. `docs/Reviews/Review.GoalRelevanceGateFeasibility.md` is the artifact. No code changed;
nothing was spent. Its findings are inputs to Phases A, B, E, H and J.

The census itself was produced by a throwaway script in a temp directory. A money-releasing
checkpoint cannot rest on that, so Phase B replaces it with a committed, deterministic,
fixture-tested subcommand whose first job is to reproduce these numbers exactly.

---

## Phase A — Design amendment, lineage rescue, parent-plan reconciliation, model availability

Zero code except the availability smoke. Everything here removes a risk that compounds if deferred.

**Work**

1. **Amend `docs/Plans/Design.GoalRelevancePanelLabeling.md` — DONE.** Everything else cites the
   design, which previously described the pre-census gate. The net change was substantial and had to
   be carried back rather than left as a plan-only correction:
   - the gate shape above (per `(slice × split)`, per `(goal × split)`, per `(split)`);
   - `min_evaluated_goals` retired; macro recall moved from per-slice to per-split over all seven
     goals; the insufficient-evidence rule re-attached to `(goal × split)`;
   - `panel_vote` is `Option` at the type level and **mandatory at the gate**;
   - `aggregation_status` reaches `PairResult` in the same schema bump, and is `Option` there too;
   - `Provenance.review` is **deleted**, not made optional, in the same bump;
   - snapshot selection relaxed to a disjoint, complete set of runs per member, with the member
     pass / labeling run vocabulary;
   - `labeling_input_sha256` joins the manifest;
   - the ledger indexes hash-pinned run artifacts rather than duplicating verdicts, and the
     `goalrel-label-v1` mini/Fable runs stay outside it (the earlier design said they remained in the
     ledger unselected; that was written before it was known their provenance was never captured);
   - the tie rule for the boolean `none_of_roster` vote (`tied` → `false`);
   - **the audit policy's contents gain a tie tripwire.** The earlier design enumerated only
     thresholds; it now carries a third kind of pre-registered value — a mechanism-assumption
     tripwire on the dry run's tie **counts**, frozen like the performance thresholds but triggering
     a recorded operator decision rather than a gate failure.
     The design's definition of `tied`, its precedence, and the `tied` → gold `ambiguous` rule are
     **unchanged**; the projection arithmetic that once consumed a tie rate was a plan-level
     construct and never a design claim, so nothing else in the design moves for this;
   - the dry run becomes a **full-panel** sample, not a single-model IDE mechanics check, and one of
     its stated purposes is checking the rarity assumption the aggregation rests on;
   - the hard-slice conditioning defect recorded as a v1 known limitation;
   - the *Documents to update* list is **extended, not replaced** — say so in the design, so a reader
     of both documents does not conclude one is stale.
2. **Commit the v1 lineage now — DONE.** The eighteen replay and evidence artifacts are tracked
   under `evaluation/frozen/goal-relevance/lineage/` at the layout above. This was not
   gated on the labeling phase and is not satisfied by a backup: a copy on another disk satisfies
   neither the project's own version-control rule nor the reproducibility argument that a
   money-releasing checkpoint must not rest on non-reproducible inputs. The 591/798 figure is
   already a methodology number, so under the committed rule these artifacts belong in git.
   Nothing in the pool falls under the transcript exception — it holds no transcripts and no raw
   captures.
3. **Reconcile the parent plan's status line** (above) and mark its labeling/review/freeze campaign
   as superseded by this plan.
4. **Model-availability smoke — DONE, and it sizes the dry run.** The operator ran the live
   single-call tests for `gpt-5.6-sol` and `gpt-5.6-terra`: both were reachable, echoed the exact
   requested id in `model`, returned `finish_reason: stop`, and accepted `max_completion_tokens`,
   confirming the `prefers_max_completion_tokens` branch. `gpt-5.4-mini` was already proven by the
   v1 production run. Opus 5, Fable 5, Sonnet 5, Gemini Flash 3.6, and Kimi K3 were confirmed
   reachable by the operator. Open Question 8 did not fire; no member is unreachable, so panel
   composition and the 5000 bp lineage cap stand exactly as designed, with Anthropic at 270 of 540
   units on the cap, not over it.
   Antigravity is now CLI-drivable through `agy.exe --print`; the chosen Gemini member is
   `gemini-3.6-flash-high`. There are four automatable members (Sol, Terra, mini, Gemini) and four
   hand-driven members (Opus, Fable, Sonnet, Kimi as auditor), one fewer hand-driven pass than the
   earlier sizing assumed in both the dry run and campaign. The lower hand-driven load lowers the
   session-capacity estimate feeding the money-releasing checkpoint. The `run_mode` enum remains
   `api | manual`; its CLI-driven representation is a new open question, not resolved here.
   Antigravity's `claude-sonnet-4-6` and `claude-opus-4-6-thinking` are 4.6 models, not Sonnet 5
   or Opus 5, and must not substitute for the Anthropic panel members because doing so changes the
   member identity over which the 5000 bp cap is computed.
5. **Update `docs/Handoff.md`.** Its *Next* currently recommends running the operator campaign that
   this plan replaces, and its alternate names "the operator label review that flips the 12 sample
   records from `draft`" — an item that ceases to exist when review provenance is deleted.

**Verification** (from repo root)

- `cargo clippy --all-targets -- -D warnings` then `cargo fmt` (no code change expected; run anyway
  so the phase leaves a clean tree).
- `git ls-files` accounts for **all eighteen** files that were in `runs/goalrel-production/`, each at
  the path the layout above assigns it — nothing dropped, nothing invented. **No
  `blind-qa-decisions.jsonl` is expected or required**; it was never produced.
- Manual: re-read the amended design end to end and confirm no sentence still describes the retired
  gate.
- Manual: the availability smoke prints a successful completion for every API-reachable model id,
  with the exact id string recorded for the weights file, and the automatable-member count recorded
  for Phase I's sizing.

**Spends money:** yes — tier 1, two live calls.

**Human review:** the amended design is worth a document handoff (design + this plan's gate section)
before Phase C spends implementation effort on the weights file.

---

## Phase B — Threshold-direction commitment, the split seed, and the rubric-sensitivity measurement

Independent of every aggregation phase: it needs only current-schema `PairRecord`s and
`run_baseline`. It lands early because it sizes a disclosure, generates worked examples for the
rubric, and builds the tool the checkpoint depends on.

**The internal ordering is the point, and is verifiable in git history.** Work item 1 is committed
*before* work item 4 runs.

**Work**

1. **Write the threshold-direction rationale, before the number is known.**
   Determinacy is a quality bar and cannot select the threshold's *direction*: two determinate
   rubrics, one liberal and one conservative, are equally determinate and produce different systems.
   The direction is anchored to a third thing — **what should make a goal activate in the realtime
   system**. That is a product question with existing evidence: the task contract names both costs
   (`GoalRelevance.TaskContract.md:41` — a false positive "can misdirect present-turn framing or
   crowd out a more pertinent goal"; a false negative "can miss a person-relevant concern"), and
   `docs/Handoff.md` lines 12-13 record a live symptom on the false-positive side ("arbitration lets
   `serve-the-present-person` crowd out weak world-goal matches"). Writing the rationale down
   *before* the sensitivity number exists makes the choice auditable and stops it being
   reconstructed later as convenient. This is stronger protection than declining to look at the
   number. The conservative direction may well be right on the merits — and if it is, the failure
   floor improves **legitimately**, because the eval's target was corrected, not flattered.
   Mechanically this commit opens `goalrel-label-v2`: the guideline's own *Consistent use* rule says
   a change requires a new guideline version, so the rationale lands as the first section of the v2
   document and Phase H fills in the executable test beneath it. No label set claims v2 until
   Phase K, so nothing is invalidated by the early bump.
2. **Fix and record the split seed and the pool → split binding.** Which of pool-a / pool-b is
   validation is decided solely by the recorded split seed, and every `(goal × split)` number in the
   census, the re-census, the audit evidence and the freeze must mean the same thing. Run the
   existing component split at a chosen seed over the committed pool and commit
   `pools/<pool>/split-summary.json` (seed plus `assignment_by_component`). Every later census,
   re-census and freeze reads that file; the freeze re-derives the split from the same seed and
   refuses to write a manifest unless it matches.
3. **Build one committed datagen subcommand serving four consumers** — `census`, in a new
   behavior-named `census` module:
   - `census support` — the `(goal × split)` and `(slice × split)` support cells under the strict
     (both labelers `relevant`) and optimistic (either `relevant`) bounds, using the recorded seed;
   - `census rubric-sensitivity` — builds strict and optimistic proxy datasets from the existing 798
     pairs and runs them through `run_baseline`;
   - `census sample-contested` — the deterministic, **stratified** sample of the 199 one-directional
     disagreements that Phase H reads (algorithm in Phase H);
   - `census recensus` — projected post-v2 support per `(goal × split)` (algorithm in Phase J), and
     the tie **partition** tables the checkpoint brief reports.
   Deterministic, fixture-tested, output written as committed JSON evidence under
   `pools/<pool>/evidence/census/`. The subcommand recomputes mini/Fable agreement from the two
   committed label artifacts via `parse_label_interchange` and **asserts it reproduces 591/798** —
   which doubles as an integrity check that the committed lineage is the data the census measured.
   `reconciliation.jsonl` stays committed evidence and is not an input to any tool, so retiring
   `ReconciliationRecord` in Phase G does not strand it.
   Written so the Phase D schema bump does not strand it: proxy records are constructed through the
   crate's own record builder, and `panel_vote` being `Option` means proxy datasets keep parsing at
   schema v3 without fabricating a vote.
4. **Measure the recall gap.** The frozen set feeds the exact-token failure floor, and recall's
   denominator is the set of gold-`relevant` pairs, so the rubric's direction moves the headline
   number: conservative relevance requires "specific content", specific content skews topical,
   topical skews toward what a keyword list matches — so tightening preferentially discards the
   low-lexical-overlap pairs that *are* the failure floor. The measurement must use the **real**
   scorer (`qsf_volition::normalize_terms` → `matched_keywords` → `match_strength`, via
   `run_baseline` at `crates/qsf_semantic_eval/src/runner.rs:33`, offline and pure). A Python
   reimplementation would measure a different scorer, and fidelity is the entire point.
5. **Record the legitimate uses of the number, and the illegitimate one.** It feeds the methodology
   note and the choice of worked examples, and **never** the direction of the threshold. Its two
   legitimate uses: (a) it sizes a disclosure — if the gap is 5 points one sentence covers it; if 30
   the failure floor is barely meaningful without naming its rubric; (b) it is a determinacy signal —
   the pairs driving the difference are exactly the ones v2 must be determinate about, a free
   worked-example generator that says *which* cases to write examples for without saying which way
   to answer them. Regardless of the measured value, the methodology note will state that the
   failure floor is conditioned on `goalrel-label-v2`'s relevance threshold and is not comparable
   across guideline versions.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo test -p qsf_semantic_eval`;
  `cargo clippy --all-targets -- -D warnings`; then `cargo fmt`.
- Fixture tests: `census support` on a small synthetic pool produces hand-computable cell counts;
  the strict/optimistic proxy builders are pure and deterministic; `census sample-contested` is
  reproducible from its seed and satisfies its stratification guarantees; `census recensus` reports
  a cell projecting to 4 as **not** clearing a floor of 3, and its tie partition tables are counts
  with no derived rate anywhere in the output.
- Reproduction test: `census support` over the committed v1 pool reproduces 591/798 agreement and
  207 disagreements, and reproduces the census document's `(goal × split)` table exactly.
- Artifact-parsing test: `split-summary.json` and each census evidence file are parsed back and
  every required field asserted; all rates are `_bp` integers.
- Manual: confirm in `git log` that the direction-rationale commit precedes the rubric-sensitivity
  evidence commit.

**Spends money:** no — `run_baseline` is offline and pure; the census reads committed artifacts.

**Human review:** the direction rationale is a product judgment; a short document handoff (the
rationale text plus the task-contract and handoff citations) before the rubric phase consumes it.

---

## Phase C — Panel weights, the ledger, and snapshot selection

Design increments 1–2. Pure, fixture-tested, no network.

**Work**

- **Weights file and validator** (`lineage/policy/panel-weights.v1.json`): integer weight units
  (1 unit = 0.01), `weights_version`, per-lineage totals and shares **computed by the validator and
  recorded**, never asserted in prose, and the declared 5000 bp lineage cap enforced. Anthropic sits
  exactly on the cap at 270 of 540 units, so the boundary case is the default case.
- **Ledger** (`ledger.jsonl`): the append-only run index above, with the three identities
  (`panel_member_id`, model build identity, `labeling_run_id`) and every run-provenance field
  **required**. Append is the only write; a rewrite is a hard error. Ingest validates the run
  artifact through the existing `parse_label_interchange` path against the frozen roster — the
  identical path `label-mini.jsonl` takes.
- **The `goalrel-label-v1` mini and Fable runs stay outside the ledger.** They were cast under a
  retired rubric and can never be selected, and their provenance was never captured: no
  `labeling_prompt_sha256` (the prompt file does not exist until Phase I), no
  `workspace_listing_sha256`, no isolation attestation, no harness or build string. The three
  available options are fabricate, add a third `unknown` attestation state, or keep them out.
  **This plan keeps them out**, and says so where a reader will look:
  - fabricating hashes and attestations would put fiction into an append-only, hash-pinned structure,
    which is precisely what `build_attestation: provider_reported | operator_attested` exists to
    prevent — the design added that distinction rather than invent a version string;
  - a third `unknown` state would be a permanent hole in the ledger's guarantee, carried forever in
    the type and in every reader's reasoning, for the sake of two rows nothing can ever select;
  - keeping them out costs nothing: they remain committed under
    `pools/<pool>/evidence/goalrel-label-v1/`, the census reads them from there, and the methodology
    note cites them as the prior-rubric evidence they are.
  The ledger therefore starts empty and contains only runs made under the hand-driven ritual with
  complete provenance. A rejection test proves the ledger refuses an entry with a missing or
  placeholder provenance field.
- **Snapshot selection and selector** with the relaxation and every validation rule listed in the
  contract section above.
- **`ledger verify`**: recomputes each recorded run's artifact hash from disk and fails on any
  mismatch or missing file. Used by the per-run durable-landing gate in Phase K.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo clippy --all-targets -- -D warnings`;
  then `cargo fmt`.
- Weights validator: at-cap (Anthropic 270/540) **passes**; one unit over **fails**; a dropped GPT
  or Google member **fails** the cap; computed lineage totals match the recorded ones or the file is
  rejected.
- Ledger rejection tests: rewrite attempt; entry with an empty or placeholder `run_sha256`,
  `labeling_prompt_sha256`, `workspace_listing_sha256`, `model_build` or `harness`; entry whose
  artifact does not parse against the roster.
- Selector rejection tests: missing verdict, duplicate verdict, unknown `labeling_run_id`, mismatched
  `pool_id`, a member absent from the weights file, an auditor carrying weight, a run whose
  `guideline_version` or guideline hash differs from the selection, and a selection that would
  require inferring "latest".
- **Phase-exit condition (not an extra):** a member pass assembled from **two partial runs covering
  disjoint utterance sets is selectable as complete**; an overlap is a hard error; a gap is a hard
  error. Without this, Phase K is not survivable.
- Artifact-parsing test: a generated `snapshot-selection.json` is parsed back and every required
  field asserted.
- Manual: read the generated weights file and confirm the per-lineage share table matches the
  design's 50.00 / 38.89 / 11.11.

**Spends money:** no.

---

## Phase D — Weighted aggregation, schema v3, and the pipeline switchover

Design increments 3, 4 and 7, landing as **one coupled-artifact event**. This is deliberate.

### Why one schema bump, not two

A v3 that carries `panel_vote` **and** a required `Provenance.review` models a combination that will
never exist in data — the panel design retires review entirely, so nothing could legitimately be
stored in that shape. And the phases could not land far apart even if it did: removing
`Provenance.review` is coupled to retiring review-completeness gating in the pool fold, and that
must land before v1 can be frozen at all. There is no scenario where the schema change ships and the
switchover waits.

There is no back-compatibility gain either. `DATASET_SCHEMA_VERSION` is an **exact-match** check
(`crates/qsf_semantic_eval/src/schema.rs:178-185` errors with
`unsupported schema_version N; supported version is M`); there is no range check and no
multi-version reader, so a v3 artifact becomes unloadable the moment a v4 lands. The intermediate
"real, loadable version" would be loadable only until the next phase.

Nothing is stranded: there are no frozen datasets, and the only `PairRecord` artifact in existence is
the 12-record `sample.dataset.jsonl`, which this plan controls. The committed v1 lineage is label
files, generation output and reconciliation records — no `PairRecord`s carrying review provenance.

The real risk being reduced is enumeration error. Listing coupled artifacts is error-prone — the
audit behind this plan caught four the design's list missed — and each version bump is one of those
enumeration events (constant, sample, envelope-regression sibling, schema doc, plus whatever keys off
the changed fields). Halving the number of times that must be got right is worth more than smaller
diffs.

**`Provenance.review` is deleted, not made optional.** The symmetric move to `panel_vote` is
tempting and wrong: `panel_vote` is optional-at-type because it must *always* be present in frozen
data and the gatekeeper enforces that, whereas a permanently-optional `review` that must always be
*absent* is dead weight with no enforcement story.

**Work**

- **N-labeler weighted aggregation** as a pure integer function: scores, ranking precedence
  (`relevant` < `not_relevant` < `ambiguous`, used only to break equal scores), the consensus quorum
  `2 * S(winner) > W`, `aggregation_status`, `winner_share_bp`, `margin_bp`. No floating point
  anywhere, so `PairRecord` keeps `derive(Eq)`.
- **`none_of_roster`** by the same weighted vote over a boolean, with the `tied` → `false` rule, the
  relevant-pair derivation override, and **both persisted artifacts** (`none-of-roster-raw.jsonl`,
  `none-of-roster-derived.jsonl`) in the shapes given above, plus the override counter.
- **Schema v3, as one lockstep change:**
  - `DATASET_SCHEMA_VERSION` 2 → 3;
  - `panel_vote` added to `PairRecord` as `Option`. Option is accurate modelling, not a hole: the
    schema already models "this record did not go through that stage" the same way
    (`Provenance.generation` is `Option<GenerationLineage>`);
  - `Provenance.review` and `ReviewLineage` **deleted**;
  - `aggregation_status` added to `PairResult` (`runner.rs:19`) as an **explicitly optional** field
    (`Option<AggregationStatus>`, `#[serde(default, skip_serializing_if = "Option::is_none")]`),
    because the Phase-D sample carries no `panel_vote` and `run_baseline` must still produce results
    for it;
  - `tied_pair_count` added beside the existing `ambiguous_pair_count` in the report's slice
    breakdowns — the attribution that makes the design's "read the status field" true for the
    primary consumer;
  - `evaluation/frozen/goal-relevance/sample.dataset.jsonl` regenerated at v3 **with the review
    block removed and no `panel_vote`**. Fabricating a vote would be undetectable fabrication: the
    sample's existing placeholders (`sha256:sample-generation`, `sha256:sample-mini`,
    `sha256:sample-review`) are self-evidently fake because they are not 64 hex characters, whereas a
    fabricated vote with plausible integer weights would be structurally indistinguishable from real
    panel output. Populating it from real panel output is a Phase I follow-up — it needs the schema
    and the aggregation function to exist and something to have voted, and the sample's
    `sample-1`..`sample-11` ids have zero overlap with the production pool, so the existing
    mini/Fable labels cannot supply it. The sample's content barely moves, which is another reason
    not to pay for two migrations;
  - `evaluation/schemas/GoalRelevanceCorpusAndRoster.md` updated for schema v3, `panel_vote`, the
    removal of review provenance, and the lineage layout;
  - **the v2 envelope regression line is captured from the sample *as it exists before* the bump**
    and stored under `crates/qsf_semantic_eval/tests/legacy-envelopes/sample.v2.line.json`. It must
    not be re-derived from a later v3 artifact — that would not be an authentic v2 envelope, and the
    guarantee would drift from the artifact it protects. The existing hand-written v1 line
    (`tests.rs:64-71`) moves into the same directory so there is one home for retired-envelope
    fixtures. Phase I's sample regeneration touches the v3 sample only and **must not** regenerate
    this file.
- **Pipeline switchover** (design increment 7, pulled forward because the schema change forces it):
  - `fold_reviewed_pool` takes gold from the aggregated panel vote instead of `item.mini_label`
    (`artifacts.rs:376`), and is **renamed** — retaining "reviewed" after review is deleted obscures
    the new contract. The rename set: `fold_reviewed_pool` → `fold_panel_pool`; `ReviewedPoolRequest`
    → `PanelPoolRequest`; `split_reviewed_pool` → `split_panel_pool`; `ReviewedPoolSplit` →
    `PanelPoolSplit`; `canonical_reviewed_pool` → `canonical_panel_pool`; the artifact
    `reviewed-pool.jsonl` → `panel-pool.jsonl`, with the CLI usage string updated;
  - review-completeness gating (`validate_reviewed`) is replaced by **dense panel coverage**: every
    pair carries a selected verdict from every selected panel member, and a missing verdict fails the
    freeze rather than counting as an abstention;
  - **`panel_vote` becomes mandatory at the gate**: no record in a frozen dataset may lack one,
    enforced by the gatekeeper with a teeth-proving test like every other rule.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo test -p qsf_semantic_eval`;
  `cargo clippy --all-targets -- -D warnings`; then `cargo fmt`.
- Aggregation fixtures for all three declared status rows plus the boundary cases the design names:
  the Anthropic-unanimous `plurality` at exactly 270/540 (an off-by-one in the quorum flips it to
  `consensus`), the Anthropic-unanimous tie against a unified rest-of-panel, and equal-score
  runner-up precedence.
- `none_of_roster`: a fixture where the vote says `true` but a pair wins `relevant` produces
  `raw_value: true`, `derived_value: false`, `override_applied: true`, and increments the counter; a
  tied boolean vote yields `raw_value: false`.
- Artifact-parsing tests over generated `none-of-roster-raw.jsonl` and `none-of-roster-derived.jsonl`:
  every required field present, all rates `_bp` integers, and the two files agree on `raw_value` for
  every utterance.
- A test asserts `PairRecord` still satisfies `Eq`.
- Envelope tests: the stored v1 line and the stored v2 line each produce
  `unsupported schema_version <n>` and neither error names a provenance field. A test asserts the
  stored v2 line still deserializes as a *v2-shaped* record when its version check is bypassed, so a
  later regeneration cannot quietly replace it with a v3 shape.
- Gatekeeper teeth: a record with no `panel_vote` fails; a pair missing one panel member's verdict
  fails.
- `cargo run -p qsf_semantic_eval` over the default sample parses at v3, produces a report with
  `tied_pair_count` present, and omits `aggregation_status` on results whose record has no
  `panel_vote`.

**Spends money:** no.

---

## Phase E — Audit policy, the audit metric, and threshold calibration

Design increment 5, rebuilt around the census-corrected gate. Pure functions and fixtures; the
gatekeeper wiring is Phase G.

**Work**

- **Audit policy file** (`lineage/policy/audit-policy.v1.json`): every metric definition, all six
  thresholds (as `_bp` integers), the declared failure model, the metric-to-corruption map, the
  joint-verification result, the calibration fixture hash, the effective-floor table, and the **tie
  tripwire** (below). Content-hashed and committed **before Kimi K3 produces any verdict that gates
  v1**, and — for the tripwire specifically — before the dry run exists. The stale
  `0.80` constant (`frozen.rs:149`) is removed with the metric it belonged to; it was calibrated for
  human self-agreement and carries no meaning for a model auditor.
- **The metric as a pure function** over frozen gold labels plus auditor verdicts, producing the
  audit-evidence artifact: per-goal `relevant_recall` (an auditor `ambiguous` against a gold
  `relevant` is a **miss**, not partial credit), `relevant_false_positive_rate` with `tied` pairs
  excluded from the denominator, `abstain_rate`, and `utterance_relevant_set_match` — which the
  census identifies as the load-bearing per-slice condition, with a denominator of 6–8 utterances per
  slice per split: thin, but not divided by seven goals and unsatisfiable by negatives.

### Threshold derivation: corruption model, metric mapping, and a joint decision rule

*Corruption model.* Three independent axes applied to known-good labels, deterministic and seeded:

- `x` — flip `x%` of gold `relevant` to `not_relevant` (false negatives);
- `y` — flip `y%` of gold `not_relevant` to `relevant` (false positives);
- `z` — replace `z%` of verdicts with `ambiguous` (abstention).

*Sweep.* `x ∈ {0, 5, 10, 15, 20, 30, 50}`, `y ∈ {0, 2, 5, 10, 20}`, `z ∈ {0, 5, 10, 20, 40}`, 200
seeds per combination, at **every denominator in range** (see representability below), so each cell
yields a distribution rather than a point.

*Failure model*, stated in the policy: **an auditor whose relevant labels are wrong at ≥ 15% must
fail.** A clean auditor (`x = y = z = 0`) must pass.

*Metric-to-corruption map.* The five performance thresholds do not all watch the same failure, so
each is derived against the axis it is responsible for, with the other axes held at zero:

| Threshold | Responsible axis | Also affected by |
|---|---|---|
| `R_floor` (min) | `x` | `z` (an abstention against gold `relevant` is a miss) |
| `R_min` (min) | `x` | `z` |
| `F_max` (max) | `y` | — |
| `A_max` (max) | `z` | — |
| `M_min` (min) | `x` and `y` jointly (set equality breaks on either) | `z` |

*Per-metric selection rule.* For a **minimum** metric (`R_min`, `R_floor`, `M_min`): among the values
on the metric's achievable ladder, take those where the clean auditor passes in ≥ 99% of seeds at
every denominator in range **and** the declared failure model fails in ≥ 95% of seeds; choose the
**largest** (tightest). For a **maximum** metric (`F_max`, `A_max`): the same two conditions, choose
the **smallest** (tightest). `M_min` is derived against `x` and `y` jointly at the declared failure
rate on each, since utterance-set equality breaks on either error direction.

*Tie-break*, applied in order, so the sweep yields exactly one value:

1. prefer the value with the largest margin between the clean-pass rate and the failure-detect rate;
2. then the value exactly representable at the largest number of denominators in range (fewest
   silent promotions in the effective-floor table);
3. then the more conservative value (higher minimum, lower maximum).

*Joint verification pass.* With all five fixed, re-run the sweep with all three axes varying
together and record, per combination, **which metric fires first**. The policy publishes that map. A
combination at or above the declared failure model that **no** metric catches is a calibration
failure: tighten the metric whose axis dominates that combination by one ladder step and re-run,
until the failure model is caught in ≥ 95% of seeds across the joint sweep, or the policy records an
explicit uncovered region with its measured detection probability. Guessing is not an option the
procedure permits.

*No real auditor output is involved*, so nothing is tuned on the evidence it gates.

### Representability is a hard requirement, and it varies per cell

The ladder differs by denominator: at n = 4 recall can only be 0, 0.25, 0.5, 0.75, 1.0; at n = 5 it
is 0, 0.2, 0.4, 0.6, 0.8, 1.0. So a floor of 0.75 is exactly representable in a 4-cell and
**silently becomes 0.8** in a 5-cell — one `R_floor` is a different effective floor in different
cells. The policy therefore publishes an **effective-floor table**: for every denominator `n` from
`min_relevant_support` to the pool's maximum observed support, the smallest achievable value ≥
`R_floor` (and ≤ the maximum metrics). Because real cell sizes are known only after labeling,
calibration runs across the whole denominator range the census shows the pool can produce (4–34),
which pre-registers the effective floor for whatever size a cell turns out to have — and avoids
calibrating on the evidence being gated.

The same requirement extends to **`R_min`**: a macro across seven goals whose denominators run 4–23
inherits the quantization of its thinnest members, so its achievable values are sums of coarse
per-goal steps and its effective resolution is set by the 4-cells. `R_min` is swept at real cell
sizes too, not only `R_floor`.

### The tie tripwire — a pre-registered count, not a rate

The aggregation assumes ties are rare: a tie needs **exact** weighted-score equality between the top
two labels across seven members weighted 100 / 100 / 70 / 100 / 70 / 40 / 60. Such equalities exist
(200 = Opus+Fable against Sol+Gemini+mini; 270 = Anthropic against GPT+Google; 170 = Sol+Terra
against Fable+Sonnet), but each is a coincidence of one particular vote split, so the design treats
`tied` as a named edge case rather than a routine outcome. The dry run is the first and only chance
to check that assumption before production spend.

The tripwire is therefore declared **in the audit policy at this phase, before the dry run exists**,
and is **frozen thereafter — it may not move at the checkpoint**. A tripwire whose value is chosen
after seeing the ties is not a tripwire.

It is expressed as a **count**, never a rate. The dry run aggregates roughly **70 pairs**
(≈10 utterances × 7 goals) and **10 utterance-level votes** — the 560 figure counts individual
member verdicts, not aggregated outcomes. A rate over 70 outcomes invites exactly the extrapolation
this change retires.

```jsonc
"tie_tripwire": {
  "max_tied_pairs": 3,                    // candidate; operator-confirmed at this phase
  "max_tied_none_of_roster_votes": 0,     // candidate; operator-confirmed at this phase
  "evaluated_on": "dry_run_aggregation",
  "frozen": true
}
```

*Candidate reasoning, for operator confirmation.* At ~70 aggregated pairs, 0–1 ties is what a rare
coincidence looks like and 4 or more is not: four in seventy is about 6%, which extrapolated to the
798-pair campaign would mean roughly 45 tied pairs — enough to matter for cells whose support sits
between 4 and 9, which is most of the thin ones. Statistically, a threshold of "more than 3" almost
never fires if the true tie probability is around 2% and fires about half the time at 5%, which is
the behaviour wanted from a tripwire: quiet when the mechanism behaves as designed, loud when it is
materially more active. For the boolean utterance-level vote the bar is stricter at **any** tie,
because a boolean tie requires precisely 270/270 — an Anthropic-against-the-rest split — and one
occurrence in ten votes is already worth an operator's attention.

*Firing does not auto-fail the checkpoint.* It converts the tie assumption from "absorbed by the
`+2` margin" into an explicit decision recorded in the checkpoint brief, choosing between a larger
dry run, revisiting the aggregation, or proceeding with the limitation recorded in the methodology
note. **The decision is recorded either way**, so a tripwire that fires and is accepted leaves a
trace rather than evaporating.

### Which thresholds may move — the complete rule

Three kinds, so the rule is total and nothing falls between them:

| Kind | Values | May it move? |
|---|---|---|
| **Performance thresholds** | `R_min`, `R_floor`, `F_max`, `A_max`, `M_min` | No. Derived here from the sweep and the stated failure model; frozen; never touched again. |
| **Evidence-adequacy threshold** | `min_relevant_support` | Yes, at the pool-size checkpoint only. It asks "is there enough data here to measure anything?", and setting it against the pool's real shape is not tuning. |
| **Mechanism-assumption tripwire** | `tie_tripwire` | No. Declared here before the dry run, frozen; firing triggers a recorded operator decision, not a threshold change. |

Mechanically: if the checkpoint moves `min_relevant_support`, the policy is re-versioned, and **a
test asserts the five performance thresholds and the tie tripwire are byte-identical across policy
versions.**

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo clippy --all-targets -- -D warnings`;
  then `cargo fmt`.
- Rejection tests, one per gate rule:
  - auditor correct on every negative and wrong on every gold-`relevant` pair → fails;
  - auditor answering `not_relevant` everywhere → fails;
  - auditor answering `ambiguous` everywhere → fails on `abstain_rate`;
  - auditor correct on six goals and inverted on one → fails `R_floor` **while the per-split macro
    would pass**;
  - a `(goal × split)` cell below `min_relevant_support` → fails as insufficient evidence, and is
    listed in the evidence artifact with its support count rather than silently absent;
  - a `tied` pair the auditor called `relevant` does **not** enter the FP denominator, and a
    consensus-`ambiguous` pair the auditor called `relevant` **does** — one fixture pinning both,
    constructed so including/excluding flips the verdict;
  - an auditor adding one spurious relevant goal per utterance → fails `M_min`;
  - a missing auditor verdict → fails coverage.
- The derivation is reproducible: re-running the sweep from the recorded seeds reproduces all five
  thresholds bit-for-bit, and a test asserts the tie-break rule is total (no combination leaves two
  candidates).
- Calibration evidence and the joint firing map are written, hashed, committed, and parsed back by
  test; the effective-floor table is asserted against hand-computed values at n = 4, 5 and 12.
- The tie tripwire is present in the policy, is expressed as counts with no rate field anywhere, and
  the policy-version identity test covers it alongside the five performance thresholds.
- Manual: read the audit policy and confirm the failure model is stated in words a later version can
  argue with.

**Spends money:** no.

---

## Phase F — Freeze manifest, `labeling_input_sha256`, and rebuild-on-replay

Design increment 6, plus the retention split.

**Work**

- **The new manifest** replacing `FreezeManifest` (`artifacts.rs:96-109`), with the design's full
  hash set: identity, roster, splits, source, selection, auditor, run-provenance counts, weights,
  rubric, aggregation, audit, `none_of_roster`, and the top-level `manifest_sha256`.
  `selected_runs[]` is **grouped by `panel_member_id`** with `runs_per_member`, so eight member
  passes are legible in a list of ~60 runs.
- **`labeling_input_sha256` joins the manifest.** At 403 KB `labeling-input.jsonl` is the largest
  artifact in the pool and the load-bearing input to every hand-driven run — the ritual copies it
  into the isolated working directory. The manifest hashes generation output, each run, the guideline
  and the prompt, but not the labeling input. If that copy drifts from what the command emits
  (different roster ordering, different serialization), every labeler saw something the freeze cannot
  detect. It may be derivable from generation output plus roster, but nothing states that derivation
  is deterministic and versioned, so transitive coverage is not established.
- **`none_of_roster_raw_sha256` and `none_of_roster_derived_sha256`** now hash the two real
  artifacts defined in the contract section, not a notional value.
- Canonicalization as the design specifies: UTF-8, lexicographically sorted keys, no insignificant
  whitespace, LF endings, **no floating point anywhere in hashed content** — which the
  basis-point rule makes achievable across every replay artifact, not just the manifest.
- **Rebuild-on-replay**: the freeze re-derives every gold label, `panel_vote`, `none_of_roster` raw
  and derived value, and the full audit evidence from the selected runs, the weights file and the
  audit policy, and re-derives the split from the recorded seed. Precomputed values are compared
  against the rebuild, never accepted as authority; a mismatch fails the freeze.
- **The retention split** implemented as the layout above: replay inputs committed, transcripts not.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo clippy --all-targets -- -D warnings`;
  then `cargo fmt`.
- Canonical-serialization test: key reordering and whitespace changes produce the same
  `manifest_sha256`; a value change does not.
- A test asserts no floating-point value can appear in hashed content.
- Rebuild-on-replay tests: a tampered `gold_label`, a tampered `panel_vote`, a tampered
  `labeling-input.jsonl`, a tampered `none-of-roster-derived.jsonl`, a tampered split seed, and a
  tampered audit-evidence file each fail the freeze with a message naming the artifact.
- Artifact-parsing test over the generated manifest: every required group present, `runs_per_member`
  consistent with `selected_runs[]`.
- Byte-reproducibility: freezing twice from the same committed lineage produces identical files.

**Spends money:** no.

---

## Phase G — Retirement, the auditor gate, and the panel default path

**Work**

- **Wire the Phase-E audit metric into the gatekeeper**, replacing `blind_qa_agreement_by_slice`;
  the `0.80` constant and `HARD_QA_SLICES`' QA role go with it. All other gatekeeper rules stand:
  per-slice floors, split integrity, dense cross-product, roster binding, roster round-trip,
  recorded-seed split reproducibility, dense panel coverage and mandatory `panel_vote` (both landed
  in Phase D).
- **Deletions** (git history retains them): `review.rs`; the `review` CLI command; `ReviewStatus`;
  `ReviewDecision` / `ReviewField` / `ReviewValue`; `blind_qa_agreement_by_slice` and
  `AgreementEvidence`; the old `FreezeManifest`; and `ReconciliationRecord` plus the `reconcile`
  command — the latter is structurally two-labeler (`mini_label` / `fable_label` / `agree`,
  `artifacts.rs:60-68`) and has no role in an N-member ledger.
- **The critical distinction, stated in the code comments and the schema doc:** retiring
  `ReconciliationRecord` does **not** retire `reconciliation.jsonl`. The record *type* leaves the
  codebase; the v1 reconciliation *artifacts* are retained as historical evidence.
  `reconciliation.jsonl` (166 KB) and `reconciliation-summary.json` hold the 591/798 figure and the
  207-disagreement corpus that the census rests on, that the rubric-sensitivity check consumes, and
  that the rubric phase mines for worked examples. The same applies to `review-decisions.jsonl`.
- **Replace the default no-argument command with a panel replay smoke.** `run_cli`'s no-argument
  path today runs `run_replay_labeling_smoke` (`transport.rs:186-210`), a mini-only fixture flow
  ending in `reconcile` — which stops compiling when `reconcile` is deleted, and which would
  otherwise leave the *default* path exercising the retired pipeline while the plan claims the panel
  path is the default. The replacement runs, entirely from checked-in fixtures: a tiny fixture pool →
  seven panel member runs plus one auditor run → ledger append → snapshot selection → weighted
  aggregation → `none_of_roster` derivation → audit metric → gatekeeper. New committed fixtures:
  `crates/qsf_semantic_datagen/fixtures/panel-replay/` with one label artifact per member, a weights
  file, and an audit policy sized for the fixture pool.
  No compatibility flag preserves the mini-seeded path; the panel path is the only path, so the
  default exercises the new behavior (`Agents.md`).
- Update `Plan.GoalRelevanceFrozenSets.md`'s labeling, review and gatekeeper sections to describe
  what now exists.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo test -p qsf_semantic_eval`;
  `cargo clippy --all-targets -- -D warnings`; then `cargo fmt`.
- **A test asserts the default command constructs no live transport and makes no network call**,
  and that it exercises aggregation, `none_of_roster` derivation, the audit metric and the
  gatekeeper — asserted on the produced artifacts, not on stdout text.
- Gatekeeper teeth tests, one per rule, including a snapshot mixing `guideline_version` values and an
  auditor missing a pair.
- A test (or CI grep) asserts `ReviewStatus`, `ReconciliationRecord`, `blind_qa_agreement_by_slice`
  and the `0.80` constant no longer appear in the crates, while `git ls-files` still lists the
  evidence artifacts under `evidence/goalrel-label-v1/`.
- Dependency-graph guard still passes (`openai_provider_kit` / `reqwest` / `tokio` absent from
  `cargo tree -p qsf_semantic_eval`).

**Spends money:** no.

---

## Phase H — `goalrel-label-v2`: rubric sharpening for determinacy

Independent of Phases C–G and can run in parallel with them; it must land before Phase I and
Phase J. Its determinacy instrument doubles as the checkpoint's support estimator, so it is not a
document-only phase — and its sampling must therefore be stratified, or the checkpoint cannot
project per-cell support at all.

**Work**

- **Extend the existing conservative breadth policy into an executable test** — a procedure a reader
  executes, not a disposition they interpret. It must *extend*, never contradict, the existing
  policy; a rubric with an internal inconsistency produces fresh disagreement.
- **Keep the design's *Grow the library* / *Assemble a world picture* worked example**, and add
  examples chosen from the pairs that drove Phase B's recall gap and from sample A below.
- **The validation criterion is determinacy, not consensus.** It is *not* "does v2 resolve most of
  the 199 disagreements". That would optimize for agreement, and a rubric that makes all seven models
  agree on the wrong answer is worse than one that leaves them split; tuning until disagreement
  vanishes hand-authors the labels through the rubric, reproducing one step upstream the exact
  anchoring failure this redesign exists to remove. The criterion is: **can a careful reader apply v2
  and reach a repeatable answer without guessing.**

### Stratified sampling of the 199 (`census sample-contested`)

An unstratified 60-pair sample cannot produce per-cell projections; the sample is therefore drawn
per `(goal × split)` **stratum**, deterministically from a recorded seed:

1. Partition the 199 one-directional disagreements by `(goal, split)` using the recorded split seed.
   Fourteen strata; several are tiny (*Track the AI transition* has 3 disagreements in total).
2. Allocate 60 pairs to sample A by **proportional allocation with largest-remainder rounding** over
   non-empty strata, subject to a floor: every non-empty stratum receives at least
   `min(3, stratum_size)`. If the floors exceed 60, the sample size rises to the sum of the floors
   rather than dropping a stratum — a stratum with no sample is a cell with no projection, which is
   a checkpoint failure, not a rounding detail.
3. Draw sample B by the identical procedure from the remaining pairs, disjoint from A by
   construction.
4. Record in the evidence artifact: the seed, the per-stratum population, the allocation, and the
   drawn ids. A test reproduces the whole allocation from the seed.

### Readers, and the two signals one instrument yields

Use **panel members** as readers — you want to know how the panel will read v2 — 2–3 readers over
each sample. Agreement *between* readers on sample B gives determinacy; *which way* they land gives
the per-stratum direction signal `p_bp` that Phase J turns into projected support. Iterate the rubric
against sample A only; sample B stays untouched until the end.

**Hard boundary, enforced structurally rather than procedurally:** the operator's own reading may
inform the **rubric**, but those judgments **never enter the ledger and are never gold**. This is the
exact boundary crossed last time. The trial output uses a distinct `RubricTrialRecord` shape
(`reader_id`, `sample: a | b`, `stratum`, per-pair reading; **no** `labeling_run_id`, **no**
ledger-acceptable `guideline_version` field), stored under `pools/<pool>/evidence/rubric-v2/`, so the
ledger-ingest path structurally cannot consume it.

- Complete `goalrel-label-v2` (opened in Phase B with its direction rationale). Check
  `evaluation/contracts/GoalRelevance.TaskContract.md` and update it if v2's threshold statement
  narrows the contract's notion of relevance.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo clippy --all-targets -- -D warnings`;
  then `cargo fmt`.
- The A/B stratified allocation is reproducible from the recorded seed, satisfies the per-stratum
  floor, and A and B are disjoint (tests).
- `RubricTrialRecord` cannot be ingested as a ledger run (rejection test).
- Artifact-parsing test: the determinacy evidence file is parsed back and reports, per stratum,
  pairwise reader agreement in `_bp` and the unanimous-relevant count that feeds `p_bp`.
- **External human review (required) — document handoff:** the v2 draft, the sample-A iteration
  notes, the sample-B determinacy evidence, and the direction rationale from Phase B, handed over as
  files plus a written brief. Not interactive stepping.

**Spends money:** yes — tier 1. 2–3 readers × 40–60 pairs ≈ 80–180 judgments.

---

## Phase I — The hand-driven ritual and the full-panel dry run

This implements the design's full-panel dry run. The earlier single-model Antigravity mechanics
check could not produce tie evidence at all, because a tie is a property of seven models voting.

**Sizing is settled by the availability smoke.** Four members are automatable: Sol, Terra, and mini
through the shared OpenAI transport, plus Gemini Flash 3.6 through the Antigravity CLI
(`agy.exe --print`) at exact model id `gemini-3.6-flash-high`. The remaining four members — Opus,
Fable, Sonnet, and Kimi as auditor — are hand-driven. Gemini is automatable but does **not** run
through the shared transport, so its dry-run path exercises the CLI ritual and attestations.

**Work**

- **Commit the verbatim labeling prompt file** (`evaluation/annotations/LabelingPrompt.GoalRelevance.md`),
  pasted verbatim and never re-phrased ad hoc; `labeling_prompt_sha256` certifies nothing otherwise.
- **Generalize the Fable cross-label ritual into one hand-driven labeling ritual** in
  `AnnotationGuidelines.GoalRelevance.md`, covering every manual member and the auditor: workspace
  isolation, the two-file working directory, tool disablement (or a verbatim `tools_available[]`
  where it cannot be disabled), the verbatim prompt file, **no hand repair** (repairing malformed
  output by hand makes the operator the labeler), and the attestation fields the run record carries.
- **`prepare-session` subcommand**, so isolation is mechanical rather than remembered: it writes an
  isolated working directory containing exactly two files — the guideline at its selected version
  and a copy of `labeling-input.jsonl` (or a chunk of it) — emits `workspace_listing_sha256` and
  `labeling_input_sha256`, and supports `--chunk-size N` to emit `labeling-input.part-K.jsonl`.
  Chunking is mandatory: 403 KB across 114 lines (3.5 KB per line, because the full 7-goal roster
  repeats on every line) is roughly 100k tokens. Each chunk part becomes one **labeling run** inside
  a member's **pass**, which is why the Phase C selection relaxation is what makes the campaign
  survivable.
  The repository is **never** the workspace root for a labeling session: `generation-output.jsonl`
  carries `conditioning_goal_ref` and `intended_slice_tags`, and the label files sit beside it, so an
  agent rooted at the repo can open everything the rubric forbids it.
  `prepare-session` runs `ledger verify` first and refuses to prepare the next session if any
  recorded run's artifact is missing or hash-mismatched.
- **Full-panel dry run over ~10 utterances**: all seven panel members plus the auditor,
  ≈ 560 member pair verdicts and ~80 `none_of_roster` votes, aggregating to **~70 pairs and 10
  utterance-level votes** — under 1% of the campaign. The dry run stays this size; the tie evidence
  it produces is used as counts and partitions, never extrapolated. It yields:
  - the **tie partitions**: which goals, which splits, which hard slices the tied pairs fell in, and
    the vote shapes that produced them. Concentration is the signal — ties clustered in one goal or
    one slice mean something structurally different from ties scattered evenly, and a scalar rate
    would destroy exactly that information;
  - the **tie tripwire verdict** against the counts pre-registered in Phase E;
  - a real `aggregation_status` distribution;
  - end-to-end exercise of the aggregation and audit paths on live output rather than fixtures;
  - the **measured session-capacity data**: per-utterance wall-clock, tokens and chunk count for a
    real hand-driven member. The capacity estimate in this plan is stated as an extrapolation from
    this measurement, not asserted.
- **Regenerate `sample.dataset.jsonl` from real panel output** — confirmed as a live follow-up, and
  cleanup rather than a third migration: the schema stays at v3 and only the sample's content
  changes. The dry run produces ~10 utterances of genuine panel output against a fixture of 11 — a
  near-exact fit — so the v3 sample that carries no `panel_vote` is replaced by one whose
  `panel_vote` is real. **The stored v2 legacy-envelope fixture is not touched**: it was captured
  from the pre-bump v2 sample in Phase D and re-deriving it here would destroy the authentic v2
  envelope it exists to preserve.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo test -p qsf_semantic_eval`;
  `cargo clippy --all-targets -- -D warnings`; then `cargo fmt`.
- `prepare-session` writes exactly two files; a test asserts the directory listing contains nothing
  else and that the emitted listing hash matches; a test asserts it refuses when `ledger verify`
  fails.
- Every dry-run output validates through `parse_label_interchange` against the frozen roster with no
  hand repair; a truncated session is caught by the exactly-one-verdict rule rather than silently
  thinned.
- Aggregation over the dry-run selection produces a status distribution and the tie partition tables
  as a parsed evidence artifact of **integer counts**, with the tripwire verdict recorded; a test
  asserts the artifact carries no tie rate. The audit metric runs to completion on live auditor
  output.
- The regenerated sample parses through `Dataset::from_jsonl_path`, carries a real `panel_vote`, and
  the runner's default path still works; the stored v2 legacy line is unchanged (a test asserts its
  hash).
- **Human review (recommended) — document handoff:** the dry-run status distribution, the tie
  partitions with their vote shapes, the tripwire verdict, the measured per-member timings, and a
  sample of the raw verdicts.

**Spends money:** yes — tier 1. Eight members × ~10 utterances.

---

## Phase J — The pool-size and slice-coverage checkpoint (hard-gated)

One checkpoint decides pool size **and** slice coverage together. It falls after rubric validation
and after the dry run, and before the eight production passes, with **no path around it**. Every
census number is proxy gold, and the 199 one-directional cases are exactly what v2 will move — the
5/33-style bounds span a factor of six, so committing production money before this point commits it
to a number about to change several-fold.

**Inputs** (named, so this is a decision and not a placeholder)

1. The rubric phase's determinacy result on the **held-out sample B**.
2. The rubric phase's per-stratum direction signal `p_bp`.
3. The full-panel dry run's **tie partitions** and the **tripwire verdict** — evidence the operator
   weighs, not a number the projection consumes.
4. The recorded split seed and pool → split binding from Phase B.

### The projection algorithm (`census recensus`), stated so it is reproducible

All arithmetic is integer or basis-point; no floats; every step is recorded in the evidence artifact.

For each `(goal × split)` cell:

1. `strict_count` — pairs both v1 labelers called `relevant` (the census's lower bound; unaffected
   by v2's threshold in the conservative direction).
2. `contested_count` — the cell's one-directional disagreements (mini `relevant`, Fable
   `not_relevant`).
3. `p_bp` — from the cell's stratum in the rubric trial: `floor(10000 × unanimous_relevant_readings
   / sampled_pairs_in_stratum)`. **Unanimous** rather than majority, deliberately: a contested pair
   the readers split on is not evidence of post-v2 support.
   - If the stratum has fewer than 5 sampled pairs, fall back to the goal's pooled `p_bp` across both
     splits.
   - If that is still under 5, the cell is **unprojectable**, which is a checkpoint failure for
     "proceed at the current pool" — not a value to guess.
4. `projected_support = strict_count + floor(p_bp × contested_count / 10000)`.

**There is no tie term.** An earlier draft subtracted a pool-wide projected tie rate here; that step
is retired. A rate derived from ~70 aggregated dry-run pairs is not stable enough to multiply
cell-by-cell, and a projection that leans on it treats a coincidence count as a parameter. Tie
evidence enters the checkpoint as partitions and a tripwire verdict the operator reads, not as
arithmetic.

**Decision**

Proceed at the current pool, **or** grow the pool and regenerate the hard slices conditioned on all
seven goals.

**Exit criteria, written before the checkpoint runs**

Proceed at the current pool only if all hold:

- sample-B pairwise reader agreement clears the determinacy bar recorded in the rubric phase, with
  no individual goal below the per-goal floor recorded there (Open Question 2);
- **no cell is unprojectable**;
- every `(goal × split)` cell satisfies `projected_support ≥ min_relevant_support + 2`. The `+2` is
  the operationalisation of "a cell at 4 does not clear 3": at `min_relevant_support = 3`, a cell
  must project to 5 or more. **This margin is also what absorbs tie erosion**, now that no tie term
  is subtracted — it is a buffer, not a bare rounding allowance. A later reader must not re-add a
  tie subtraction on top of it: that would double-count the same effect and silently tighten the
  criterion by an amount nobody chose;
- the parent plan's per-slice-per-split floors still hold;
- the **tie tripwire** is evaluated and its verdict recorded. Firing does not fail the checkpoint by
  itself; it converts the tie assumption into an explicit operator choice between a larger dry run,
  revisiting the aggregation, or proceeding with the limitation recorded in the methodology note.
  Either way — fired or clear, accepted or acted on — the decision is written into the checkpoint
  brief, so an accepted tripwire leaves a trace.

Otherwise grow the pool, and take the hard-slice conditioning fix at the same time — coverage and
depth are answered by the same post-v2 support numbers and should be decided together, once.

**Which thresholds may move: `min_relevant_support` only.** The five performance thresholds are
frozen from Phase E and are never touched again (mechanism and test in Phase E).

**Firewall clause — stated in writing because "it was only ten utterances" is exactly the reasoning
by which such things creep in.** The full-panel dry run includes Kimi, so roughly 10 utterances of
**real auditor output and computable audit metrics** exist before this checkpoint. That is the
forbidden circularity in miniature — a threshold moved in light of auditor performance on data it
then gates. **Dry-run audit metrics are mechanism verification only and are barred from informing
`R_min`, `R_floor`, `F_max`, `A_max` or `M_min`.** The auditor stays in the dry run — the audit-metric
path must be exercised end to end on live output — but the numbers are firewalled. The same clause
is restated in *Cost and capacity* below, because that is the other place a reader will look for it.

**Verification**

- `cargo clippy --all-targets -- -D warnings`; then `cargo fmt` (a `min_relevant_support` move
  re-versions the policy file and must keep the performance-threshold-identity test green).
- `census recensus` is deterministic and fixture-tested: a synthetic stratum set with hand-computed
  `p_bp` and rounding reproduces the expected per-cell projection, including the unprojectable path.
  A test asserts the projection consumes **no** tie quantity, so the retired subtraction cannot
  return unnoticed.
- The evidence artifact is committed and parses, and records every intermediate quantity in the
  algorithm above plus the split seed it used.
- **External human review (required) — document handoff:** the recensus artifact, the sample-B
  determinacy evidence, the dry-run **tie partitions across goals, splits and hard slices with the
  vote shapes that produced them** plus the tripwire verdict, and a written brief stating which exit
  criterion each number addresses and the resulting decision. The brief presents partitions, not a
  projected tie rate, because there no longer is one.

**Spends money:** no. It decides how much will be spent.

---

## Phase K — The full labeling campaign

**Eight member passes** — seven panel members plus the auditor — each covering 798 pairs and 114
`none_of_roster` votes under `goalrel-label-v2`, assembled from roughly 12 chunk-level labeling runs
per hand-driven pass. This is tier-2 production spend and where most of the real effort in this plan
lives.

**Preconditions (all hard)**

- v1 lineage committed and the availability smoke green;
- the pool-size checkpoint passed with a recorded decision;
- the audit policy committed and hashed **before any auditor verdict exists**;
- `ledger verify` green.

**Work**

- Automatable through the existing shared OpenAI transport: **Sol, Terra, mini** (3 of 8 passes).
  **Gemini Flash 3.6 is also automatable**, but through the Antigravity CLI
  (`agy.exe --print`) at exact model id `gemini-3.6-flash-high`, not through that transport.
  Hand-driven: **Opus, Fable, Sonnet, Kimi** (4 of 8). Total automatable members: four; total
  hand-driven members: four.
  `crates/qsf_semantic_datagen/src/transport.rs` builds only `OpenAiProvider` / `ProviderKind::OpenAi`.
  **Stated once so it is not re-litigated on a wrong premise:** `ProviderKind` in
  `openai_provider_kit` has three variants (OpenAi, Anthropic, Google — `types.rs:31-35`), but the
  kit implements only `OpenAiProvider` (`openai.rs:16`, `openai.rs:138`) plus two mocks. There is no
  `AnthropicProvider` and no `GoogleProvider`. Adding them means a cross-repo change in
  `web_page_filet_mignon` at a pinned rev, or a local fork — not a small adapter job. Adding provider
  transports was **considered and rejected**: the design deliberately treats manual runs as
  first-class, new provider integrations to replace the remaining hand-driven passes would be a
  large expansion of a crate the parent plan scoped as lean, and Kimi stays manual regardless.
- **Per-run durable landing is a per-run gate, not a phase exit** — and "run" here means the
  chunk-level labeling run, not the member pass. Run N lands durably — artifact written to its
  lineage path, validated, ledger-appended, committed — before run N+1 begins. Implemented as a
  single check at the end of the phase, a disk failure during pass 6 costs passes 1–5, which is
  exactly the loss the gate exists to prevent. Mechanically, `prepare-session` refuses to prepare the
  next session unless `ledger verify` passes over every recorded run; the git commit itself remains
  an operator step on the labeling-campaign checklist (Open Question 4).
- Each hand-driven run sets up its isolated two-file working directory through `prepare-session`
  first and records its attestations. No hand repair; malformed output is resolved with the model and
  the replacement validated.
- Campaign reporting is token-only, following the 2026-07-21 decision-log rule for models without a
  matching checked-in price. The existing price table remains unchanged; no optional v2 price-table
  work is part of this campaign.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo clippy --all-targets -- -D warnings`;
  then `cargo fmt`.
- After every run: `validate-labels` green against the frozen roster; the ledger entry appended;
  `ledger verify` green; the artifact committed.
- After the last run: the snapshot selection resolves to dense coverage for all eight passes, with
  exactly one verdict per `(member, utterance, goal)`; `guideline_version` and guideline hash
  identical across every selected run; `runs_per_member` recorded.
- Manual: for each hand-driven run, confirm the recorded `workspace_listing_sha256` matches the
  directory `prepare-session` produced, and that `tools_available[]` is verbatim.

**Spends money:** yes — **tier 2, the production commitment.**

---

## Phase L — Cut and gate-keep frozen `v1`

**Work**

- Aggregate the selection, split by the recorded seed, run the gatekeeper, and freeze; the freeze
  rebuilds every label, vote and audit number from the selected runs rather than trusting the
  precomputed records.
- **Write `evaluation/annotations/DatasetMethodology.GoalRelevance.md`**, containing:
  - the design's ground-truth claim paragraph **verbatim** — the labels are the decision of a
    weighted frontier-model panel audited by a model from a lineage outside the panel, `consensus`
    when the winner holds more than half of total panel weight and a contested weighted `plurality`
    otherwise, with every pair carrying its full weighted vote. The set no longer claims agreement
    with human judgment and must not be described that way;
  - weights, per-lineage totals and shares, the declared 5000 bp cap;
  - the `aggregation_status` distribution, tie precedence, and the gold-`ambiguous` share with
    deadlock/rubric attribution;
  - the dry run's **tie tripwire verdict** and, if it fired and the operator chose to proceed, the
    accepted limitation in the words the checkpoint brief recorded — a fired-and-accepted tripwire
    is exactly the kind of thing that must survive into the published methodology rather than
    staying in an ephemeral brief;
  - the auditor's identity and the pre-registered audit policy with its thresholds, its failure
    model, the metric-to-corruption map, and the **effective floor per denominator**;
  - the `none_of_roster` override count;
  - run-provenance counts — how many selected runs were manual, `operator_attested`, or not
    tool-free — reported as **eight member passes composed of N labeling runs**, so a reader is not
    misled by a run count;
  - **the transcript caveat and the workspace-isolation caveat as one chain, not two hedges a reader
    must connect**: workspace isolation is an operator attestation rather than a verified property;
    the session transcript is the only artifact that could ever corroborate it; keeping transcripts
    unpublishable therefore makes the attestation uncorroborable as well as unfalsifiable; and
    `transcript_sha256` is operator-retained and not third-party verifiable;
  - **the rubric-sensitivity disclosure**: the measured recall gap between the strict and optimistic
    proxy datasets, sized per Phase B, and — regardless of the value — the statement that the
    failure floor is conditioned on `goalrel-label-v2`'s relevance threshold and is **not comparable
    across guideline versions**;
  - **the prior-rubric evidence**: that mini and Fable also labeled this pool under
    `goalrel-label-v1`, that those runs are committed as evidence but deliberately outside the
    ledger, and that the 591/798 agreement figure comes from them;
  - **the v1 known limitation**: hard slices were generated conditioned on exactly two roster goals
    each, which is why per-goal gating is per `(goal × split)` and not per `(goal × slice × split)`;
    a future dataset version conditions hard slices on all seven goals.
- **Decision-log entries** (below).
- **Update `docs/Handoff.md`** to point at whatever comes next.

**Verification**

- `cargo build`; `cargo test -p qsf_semantic_datagen`; `cargo test -p qsf_semantic_eval`;
  `cargo clippy --all-targets -- -D warnings`; then `cargo fmt`.
- Gatekeeper passes on the frozen sets and still fails on each injected violation.
- Freezing twice from the committed lineage is byte-identical.
- The methodology note's numbers are generated from the artifacts rather than typed; a test asserts
  the required sections and figures are present and match the manifest and audit evidence.
- **External human review (required) — document handoff:** the freeze manifest, the audit evidence,
  the methodology note, and a sample of frozen records with their `panel_vote`, handed over as files
  plus a written brief.

**Spends money:** no.

---

## Cost and capacity

The earlier framing that "phases before labeling cost no money" was **false** and is withdrawn. It
was true of the original brief and stopped being true when the dry run became full-panel. Three
phases spend before the checkpoint. Spend is therefore named in two tiers.

### Tier 1 — measurement spend (bounded, enumerated, gates the decision)

| Phase | Spend | Volume |
|---|---|---|
| A — model-availability smoke | 2 live calls | `gpt-5.6-sol`, `gpt-5.6-terra` |
| H — rubric determinacy readers | 2–3 readers × 40–60 of the 199 | ≈ 80–180 judgments |
| I — full-panel dry run | 8 members × ~10 utterances × 7 goals | 560 pair verdicts + ~80 `none_of_roster` votes |

Against the campaign's 6,384 pair verdicts that is roughly **10% of volume**, plus five short
hand-driven sessions — call it half of one full session.

**Hard cap:** tier-1 spend may not exceed ~10% of projected tier-2 spend without re-opening this
plan. Without a cap, "a small measurement first" is exactly the shape that creeps.

### Tier 2 — production spend (irreversible)

The eight member passes of Phase K: 6,384 pair verdicts plus 912 `none_of_roster` votes = **7,296
judgments**, assembled from roughly 60–70 chunk-level labeling runs.

### The rule that connects them

**The checkpoint gates tier 2; tier 1 is a precondition of the gate, not an exception to it.**

The ordering does not change, because all three tier-1 spends exist *because* they are checkpoint
inputs:

- the availability test determines whether three members or one are automatable, which sets the
  session-capacity estimate and the dry run's own sizing;
- the rubric readers produce both the determinacy verdict and the per-stratum direction signal that
  projects post-v2 support;
- the dry run produces the tie partitions and the tripwire verdict, which is the only check that the
  aggregation's rarity assumption holds before 798 pairs are voted on — and it must happen while a
  different answer is still affordable.

A gate informed by measurement cannot precede its own measurement. Moving the checkpoint earlier
returns it to deciding pool size on proxy numbers, which is the exact error it exists to prevent.
Splitting it in two does not help either: whether the aggregation's tie assumption survives contact
with seven real models bears on the pool-size question itself, not on a separate concern.

**Firewall, restated here because this is where a reader will look for it:** the dry run producing
real auditor output is what breaks the "no auditor output exists before the checkpoint" premise.
**Tier-1 audit metrics are mechanism verification only and are barred from informing `R_min`,
`R_floor`, `F_max`, `A_max` or `M_min`.** Only `min_relevant_support` may move at the checkpoint,
and a test enforces that the five performance thresholds are byte-identical across policy versions.

### Where the effort is

Phases B–G and L cost nothing. Tier-1 measurement spend is roughly a tenth of the campaign and gates
it. Tier-2 production spend is the commitment. And **most of the real effort in this plan is operator
sessions in the labeling campaign** — four hand-driven passes of ~12 chunks each — which the dry
run measures rather than this plan asserting.

---

## Documents to create or update

The design's *Documents to update* list is **extended, not replaced** — this section re-derives it
rather than sequencing it, because the design's list is incomplete. Sequenced by the phase whose
landing changes the document.

| Document | Change | Phase |
|---|---|---|
| `docs/Plans/Design.GoalRelevancePanelLabeling.md` | amended for the census-corrected gate and every item in Phase A | A |
| `docs/Plans/Plan.GoalRelevanceFrozenSets.md` | status line reconciled; campaign superseded | A |
| `docs/Plans/Plan.GoalRelevanceFrozenSets.md` | labeling / review / gatekeeper sections rewritten to what exists | G |
| `evaluation/frozen/goal-relevance/lineage/pools/<run>/**` | the v1 pool committed as evidence and replay input | A |
| `docs/Handoff.md` | Now/Next repointed (its current *Next* is the superseded campaign; its alternate names a review item that ceases to exist) | A, L |
| `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` | opens `goalrel-label-v2` with the threshold-direction rationale, committed before the sensitivity number | B |
| `.../pools/<run>/split-summary.json` | **new** — the fixed split seed and pool → split binding | B |
| `crates/qsf_semantic_datagen/src/census.rs` (+ CLI) | **new** — support census, rubric sensitivity, stratified contested sample, re-census | B |
| `evaluation/frozen/goal-relevance/lineage/policy/panel-weights.<v>.json` | **new** — integer units, computed lineage totals, declared cap | C |
| `crates/qsf_semantic_datagen/src/{ledger,aggregation,audit}.rs` | **new** modules | C–E |
| `crates/qsf_semantic_eval/src/schema.rs` | `DATASET_SCHEMA_VERSION` 2 → 3; `panel_vote` added; `Provenance.review` / `ReviewLineage` deleted | D |
| `crates/qsf_semantic_eval/src/runner.rs` | `aggregation_status` on `PairResult`, explicitly optional | D |
| `crates/qsf_semantic_eval/src/report.rs` | `tied_pair_count` beside `ambiguous_pair_count` | D |
| `crates/qsf_semantic_eval/tests/legacy-envelopes/` | **new** — the authentic v2 line captured *before* the bump, plus the existing v1 line moved in | D |
| `evaluation/frozen/goal-relevance/sample.dataset.jsonl` | regenerated at v3, review block removed, no `panel_vote` | D |
| `evaluation/schemas/GoalRelevanceCorpusAndRoster.md` | schema v3, `panel_vote`, review removal, lineage layout, manifest | D, F |
| `crates/qsf_semantic_datagen/src/{artifacts,frozen}.rs` | `fold_panel_pool` rename set; `panel-pool.jsonl`; dense panel coverage; mandatory `panel_vote` | D |
| `evaluation/frozen/goal-relevance/lineage/policy/audit-policy.<v>.json` + calibration | **new** — metrics, five performance thresholds, the evidence-adequacy threshold, failure model, metric-to-corruption map, joint firing map, effective-floor table, tie tripwire | E |
| `crates/qsf_semantic_datagen/fixtures/panel-replay/` | **new** — the checked-in fixtures the default command exercises | G |
| `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` | `goalrel-label-v2`: executable breadth test, worked examples, reader-trial boundary | H |
| `evaluation/contracts/GoalRelevance.TaskContract.md` | checked; updated only if v2 narrows the contract's notion of relevance | H |
| `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` | Fable ritual generalized into the hand-driven labeling ritual | I |
| `evaluation/annotations/LabelingPrompt.GoalRelevance.md` | **new** — the verbatim prompt, hashed as `labeling_prompt_sha256` | I |
| `evaluation/frozen/goal-relevance/sample.dataset.jsonl` | regenerated from **real** dry-run panel output (v3 stays; legacy fixture untouched) | I |
| `crates/qsf_semantic_datagen/pricing/goalrel-generation-price-table.v*.json` | optional panel-model prices, or token-only | K |
| `evaluation/frozen/goal-relevance/lineage/<dataset_version>/**` | selection, `none_of_roster` raw + derived, audit evidence, frozen splits, manifest | K, L |
| `evaluation/annotations/DatasetMethodology.GoalRelevance.md` | **new** — the full note listed in Phase L | L |
| `docs/DecisionLog.md` | **four** reversals plus new entries (below) | D/G, L |

No `docs/Architecture/*` document describes this subsystem, so none is affected. No `ui/` code
changes, so no `npm run check` / `npm run fmt` is required in any phase.

### Decision-log entries

**Four reversals, not two.** The design names the two 2026-07-22 entries. Two more are reversed:

- **`docs/DecisionLog.md:2023` — *2026-07-21, Goal-relevance labels use independent blind review*.**
  It commits to "a two-model OpenAI generate/label split, an independent Claude Fable cross-label,
  and **mandatory human review**", with "no model-produced label becomes reviewed data without an
  operator decision" and "authoritative operator corrections live in `review-decisions.jsonl`". That
  is precisely what the panel abolishes, and it is the sharpest of the four. *(Reversal lands with
  Phase D, where review provenance is deleted.)*
- **`docs/DecisionLog.md:2061-2066` — *2026-07-22, Goal-relevance generation uses approved anchors,
  review-authoritative vague negatives, and mode-appropriate models*.** Its **vague-negative clause**
  — "the vague `none_of_roster` batch is deliberately over-produced, with blind labeling and human
  review deciding its final status rather than generation guaranteeing it" — is reversed: the
  `none_of_roster` status is now decided by the panel's weighted utterance-level vote plus the
  relevant-pair derivation override. The entry's anchor-approval and mode-model clauses **stand**;
  the reversal must say which clause it touches so the rest is not read as withdrawn. *(Phase D.)*
- ***2026-07-22, Goal-relevance freezes are gate-kept and reproducible from committed lineage*** —
  the gatekeeper rule list loses "review completeness", and "blind-QA agreement" becomes a
  pre-registered per-goal, per-split auditor metric. Reproducibility, lineage retention and the
  remaining rules stand and are strengthened. *(Phase G.)*
- ***2026-07-22, Goal-relevance review relabels but never excludes utterances*** — the no-exclusion
  property survives and is strengthened (every pair is kept with its full weighted vote), but the
  operator relabeling mechanism it describes no longer exists. *(Phase G.)*

New entries (proposed here, committed when the behavior lands):

- *(Phase L)* **Goal-relevance labels are the decision of a weighted three-lineage model panel**
  audited by a model from a fourth lineage — the composition, the 5000 bp lineage cap, the
  consensus/plurality distinction, and the dataset's ground-truth claim.
- *(Phase G/L)* **The auditor gate is pre-registered and gated per `(goal × split)`** — thresholds
  come from planted-drift fixtures at real cell sizes with a published metric-to-corruption map, are
  never derived from the evidence they gate, insufficient evidence in a gated cell fails the freeze,
  and effective floors are published per denominator.
- *(Phase C)* **A snapshot selects a set of runs per panel member** with disjoint coverage and a
  complete union — a member *pass* composed of chunk-level *runs* — which is what makes an abandoned
  hand-driven session resumable rather than wasted. Runs whose provenance was never captured stay
  out of the ledger rather than entering it with fabricated or unknown fields.
- *(Phase F/A)* **Replay inputs are committed; provenance evidence is not** — the retention rule
  split by replay-necessity, with `transcript_sha256` retained as the binding even though the
  transcript stays out of git.
- *(Phase B/H)* **The relevance threshold's direction is a product decision recorded in the
  guideline before any sensitivity number is measured**, and the failure floor is reported as
  conditioned on its guideline version.

---

## Exit criteria (this plan)

- `DATASET_SCHEMA_VERSION` is 3 in a single bump: `PairRecord` carries `panel_vote`,
  `Provenance.review` is gone, `PairResult` carries an optional `aggregation_status`, and the schema
  doc, sample dataset and legacy-envelope fixtures match.
- A committed weights file with computed lineage totals and an enforced cap; a committed,
  content-hashed audit policy with five performance thresholds, the evidence-adequacy threshold, a
  stated failure model, a metric-to-corruption map, a joint firing map, a published effective-floor
  table and a pre-registered tie tripwire expressed as counts, all hashed into the manifest.
- An append-only ledger of fully-provenanced runs, a snapshot selection admitting disjoint partial
  runs per member pass, and a pure integer aggregation whose three statuses are all covered by
  fixtures.
- The default no-argument command exercises the panel path end to end from checked-in fixtures with
  no network call.
- Frozen `validation.dataset.jsonl` and `test.dataset.jsonl` under
  `evaluation/frozen/goal-relevance/`, every record carrying a real `panel_vote`, gate-kept by a
  gatekeeper with a teeth-proving test on every rule, and reproducible byte-for-byte from committed
  lineage including the raw and derived `none_of_roster` artifacts.
- `review.rs`, the review CLI, `ReviewStatus`, `ReviewDecision`, `blind_qa_agreement_by_slice`, the
  old `FreezeManifest`, `ReconciliationRecord` and the `reconcile` command are gone; the v1
  reconciliation and review-decision **artifacts** remain committed as evidence.
- `goalrel-label-v2` exists, extends the breadth policy into an executable test, carries its
  direction rationale, and has determinacy evidence from a stratified held-out sample.
- The methodology note carries the ground-truth claim verbatim, the audit policy, the
  ambiguous/tie attribution, the pass-and-run provenance counts, the transcript/attestation caveat
  chain, the rubric-sensitivity disclosure, the prior-rubric evidence, and the hard-slice
  conditioning limitation.

---

## Open Questions (surfaced, not resolved)

### Closed by the design amendment and operator evidence

- **1. Cross-dataset-version reuse of policy files — closed.** Policy files live under
  `lineage/policy/`, with version-stamped filenames and hashes pinned in every manifest that uses
  them. This keeps one DRY, committed, reproducible home across dataset versions. It deliberately
  deviates from the design's literal `lineage/<dataset_version>/` wording; the design now records
  the reasoning so the layout is not mistaken for an oversight.

- **3. Pricing table — closed.** The campaign report is token-only. This is the honest fallback;
  fabricating a price is not an option. A future version may add a real price table, but no artifact
  shape depends on it.

- **6. Gemini via Antigravity and tool-freeness — closed as not tool-free.** The operator measured
  `gemini-3.6-flash-high` in headless Antigravity `--print` mode with the repo as workspace:
  nineteen tools were nominally available, and `--sandbox` did not reduce the list; no agents were
  defined and no plugins were installed. Headless mode auto-denied `view_file` (`BLOCKED`),
  `grep_search` (required `read_file`), and `run_command` (required `command`). It executed
  `search_web` freely with live results and named the tool, fetched an external page with
  `read_url_content` and returned its title, and enumerated the repo filenames with `list_dir`.
  Therefore it could not read file contents or execute commands, but it could search the web and
  fetch URLs. Labeling runs must be invoked from an isolated empty directory, not the repo;
  `tools_available[]` records the tools that actually executed, and the methodology note names
  live web search specifically. The file-axis independence is preserved; live web access is the
  contamination vector.

- **8. Unreachable panel member — closed by the availability smoke.** All members were reachable;
  composition and the 5000 bp lineage cap stand unchanged, with Anthropic at 270 of 540 units on
  the cap.

### Remaining questions

- **2. The numeric values.** This plan fixes the *derivation* of `R_min`, `R_floor`, `F_max`, `A_max`
  and `M_min` (sweep, failure model, metric mapping, selection rule, tie-break, joint verification,
  representability), not the numbers — they come out of the Phase E sweep. Likewise the determinacy
  bar and its per-goal floor for Phases H/J are proposed by the rubric phase and confirmed by the
  operator before the checkpoint runs; this plan deliberately does not invent them.

- **4. Mechanical enforcement of "committed", not just "on disk".** `ledger verify` proves the
  artifact exists and hashes correctly; it cannot prove it is committed. Whether `prepare-session`
  should additionally check git cleanliness of the lineage tree (adding a git dependency or a shell
  out) is unresolved; today the commit is an operator checklist step.

- **5. Whether a future dataset version admits the prior-rubric runs into the ledger.** This plan keeps
  them out because their provenance was never captured. If a later version wants a complete
  labeling history in one place, it needs a deliberate representation for never-captured
  provenance — which is a decision about the ledger's guarantee, not a migration detail.

- **7. The regenerated sample's size.** The dry run yields ~10 utterances × 7 goals = ~70 records
  against a current fixture of 12. Whether the sample keeps roughly its current size (a subset of
  the dry run) or grows to the full dry-run pool changes the runner's default output volume.

- **9. What a concentrated tie partition means.** The stability question that stood here is closed:
  nothing projects from a tie rate any more. What survives is interpretive rather than numeric —
  ties clustered in one goal or one hard slice would indicate something structural about that
  goal's rubric or that slice's construction, and the plan does not pre-commit to what the operator
  should conclude from such a pattern. The tripwire ensures the pattern is looked at; reading it is
  a judgment the checkpoint brief presents rather than automates.

- **10. CLI-driven member representation in `run_mode`.** The ledger enum remains exactly
  `"api" | "manual"`; the choice is unsettled between adding a third enum value for a CLI-driven
  member, or reading `manual` as "not through the shared kit". This must be settled when the ledger
  is built; this plan does not choose.

- **11. Whether Antigravity deny rules can block `search_web`.** The permission categories are
  `read_file`, `write_file`, `read_url`, `execute_url`, `command`, `unsandboxed`, and `mcp`, none
  of which names web search. Deny reportedly takes precedence over allow and automatic workspace
  access, but the hand-edited `~/.gemini/settings.json` was inert: `list_permissions` loaded no
  deny rules. The operator must add the rules through interactive `/permissions` and test them
  before the dry run. Until then, Gemini remains disclosed as not tool-free.

---

## Out of scope (decided by the operator, survived external review — do not re-litigate)

Panel composition; the weights; the 5000 bp lineage cap; the consensus/plurality quorum; integer
weight units; and the guideline-v2-and-re-run-everyone policy.

Rejected with reasons recorded above: adding Anthropic/Google provider transports (Phase K);
fabricating a sample `panel_vote` (Phase D); a second schema bump for the review removal (Phase D);
an `unknown` provenance state or fabricated hashes for the prior-rubric runs (Phase C); dropping
`transcript_sha256` (retention contract); committing session transcripts (retention contract).
