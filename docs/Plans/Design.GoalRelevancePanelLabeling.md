# Design: Goal-relevance labels from a weighted model panel

Status: proposed (revised 2026-07-24 against `docs/Reviews/Review.GoalRelevancePanelLabeling.md`)
Date: 2026-07-24
Supports: `docs/Plans/Plan.GoalRelevanceFrozenSets.md`
Reverses parts of two 2026-07-22 decision-log entries (see *Decisions this changes*).

## What the dataset claims

> These labels are the decision of a weighted frontier-model panel, audited by an independent
> model from a lineage outside the panel. A label is marked **consensus** when the winning label
> holds more than half of total panel weight; otherwise it is recorded as a weighted **plurality**
> and flagged as contested. Every pair carries its full weighted vote.

This paragraph is the dataset's ground-truth claim and belongs verbatim in the methodology note.
The frozen set no longer claims agreement with human judgment, and must not be described that way.
It also must not describe every label as a consensus: under the declared weights a winner can hold
less than half the panel, and that case is named rather than hidden.

## Problem

The current review path does not produce independent human labels, and cannot.

- `fold_reviewed_pool` seeds every gold label from `item.mini_label` (`artifacts.rs:376`).
- The review CLI pre-fills each prompt with that same label (`transport.rs:778`).
- The operator therefore judges *after seeing mini's answer*, so an accepted label carries almost
  no information beyond mini's output. Agreement measured this way proves nothing.
- A pair reaches `ReviewStatus::Reviewed` only through an explicit per-pair human decision plus an
  utterance-level `none_of_roster` decision (`artifacts.rs:417`), and the gatekeeper rejects any
  `Draft` pair (`frozen.rs:510`). Mini/Fable agreement promotes nothing, so all 798 pairs of the
  production pool (114 utterances × 7 roster goals) require hand-touching regardless of how strongly
  the labelers already agree.

Operator experience during the production run confirmed the design fault rather than an execution
fault: per-pair adjudication against a capable model is not a task a human reviewer can perform
reliably, and the anchoring makes the resulting decisions untrustworthy even where they were made.

## Design

### Panel and auditor

Seven labelers form the panel. One model is held out as the auditor and never influences a label.

Weights are carried as **integer weight units** (1 unit = 0.01 weight). Nothing in the aggregation,
the record, or the manifest uses floating point; see *Aggregation* for why.

| Model | Lineage | Role | Weight | Units |
|---|---|---|---|---|
| Claude Opus 5 | Anthropic | panel | 1.0 | 100 |
| Claude Fable 5 | Anthropic | panel | 1.0 | 100 |
| Claude Sonnet 5 | Anthropic | panel | 0.7 | 70 |
| GPT-5.6-Sol | GPT | panel | 1.0 | 100 |
| GPT-5.6-Terra | GPT | panel | 0.7 | 70 |
| GPT-5.4-mini | GPT | panel | 0.4 | 40 |
| Gemini Flash 3.6 | Google | panel | 0.6 | 60 |
| Kimi K3 | Moonshot | auditor | — | excluded from panel |

Panel total: **540 units**. Lineage totals: **Anthropic 270 (50.00%)**, **GPT 210 (38.89%)**,
**Google 60 (11.11%)**.

The auditor is chosen for lineage independence rather than raw capability: an auditor's job is to
observe from outside the panel's shared failure modes, and Kimi K3 is from a fourth training lineage
present nowhere in the panel.

#### No lineage can produce a consensus label on its own

The panel spans three lineages, and no lineage holds **more** than half the weight. Anthropic sits
at exactly 270 of 540 — precisely half, not a majority. Combined with the consensus quorum
(`2 * S(winner) > W`, see *Aggregation*), this yields a property the earlier two-lineage panel could
not have:

- **No lineage can reach `consensus` alone.** Consensus needs more than 270 units. A unanimous
  Anthropic vote reaches exactly 270 and stops one unit short; GPT tops out at 210 and Google at 60.
  Every consensus label in the set carries at least one cross-lineage vote.
- **A unanimous Anthropic vote ties against a unanimous rest-of-panel.** 270 against GPT+Google's
  270 is `tied`, not an Anthropic win.
- **A unanimous Anthropic vote still wins a `plurality`** when GPT and Google disagree with each
  other — 270 against 210 and 60. It is recorded as contested, never as consensus.

What the design claims about the panel:

- Weights are declared operator policy, not fitted parameters, and live in a versioned weights file.
- Per-lineage totals and shares are computed by a validator, recorded in the weights file, and
  hashed into the freeze manifest. They are never left to prose.
- **v1 declares a lineage cap of 5000 bp** (no lineage strictly above half of total panel weight),
  and the validator fails any snapshot that violates it. Anthropic sits exactly on the cap, so the
  constraint is live rather than decorative: raising any Anthropic weight, or dropping a GPT or
  Google member, fails the validator until the operator deliberately changes the declared cap.

Independence remains a stronger claim for the **auditor**: a fourth lineage, excluded from the
panel, whose labels never feed the weights.

### Aggregation

A pure function maps the selected ledger runs plus the weights file to a gold label and a vote
record per pair. All arithmetic is integer.

```text
S(L)             = sum of weight units over panel members voting label L
W                = sum of weight units over panel members selected for the snapshot   // 540
rank             = labels sorted by (S(L) descending, label order ascending)
winner, runner_up= rank[0], rank[1]
gold_label       = winner                                       // except on a tie, see below
winner_share_bp  = S(winner)      * 10000 / W                   // floor division
margin_bp        = (S(winner) - S(runner_up)) * 10000 / W       // floor division

aggregation_status =
    tied       if S(winner) == S(runner_up)
    consensus  if 2 * S(winner) >  W
    plurality  otherwise
```

Declared label order for deterministic ranking: `relevant` < `not_relevant` < `ambiguous`. This
order is used only to break equal scores when identifying the runner-up; it never overrides a
strictly higher score.

Rules:

- **Dense panel coverage is required.** Every selected panel member must carry a verdict for every
  pair in a version, the same discipline the existing dense cross-product invariant already
  enforces. `W` is therefore constant across the version and `winner_share_bp` is comparable
  between pairs. A missing verdict fails the freeze; it is never treated as an abstention.
- **`aggregation_status` is separate from the semantic label.** `ambiguous` keeps only its guideline
  meaning: a property of the utterance. Panel deadlock is `aggregation_status: tied`. A consumer
  distinguishes a rubric-ambiguous pair the panel agreed on (`ambiguous` / `consensus`) from a
  deadlock (`tied`) by reading the status field, not by inspecting a numeric margin.
- **On `tied`, `gold_label` is `ambiguous`.** The status field carries the reason, so the label is
  not overloaded — a consumer that must not conflate the two reads `aggregation_status`.
- **Contested pairs are kept and scored.** No pair is dropped for `plurality` or `tied` status.
  Contested pairs are exactly the hard slices (negation, quoted speech, hypothetical) that the set
  exists to measure; dropping them would break the per-slice floors and leave an eval containing
  only easy cases. Per-slice floors count them. A consumer that wants a clean subset filters on
  `aggregation_status == consensus`; the methodology note publishes the status distribution.

#### All three statuses are reachable under the declared v1 weights

The declared weights must exercise every branch, or the branch is untested policy:

| Status | Example vote (units) | Result |
|---|---|---|
| `consensus` | Opus+Fable+Sol `relevant` = 300; Sonnet+Terra+mini+Gemini `not_relevant` = 240 | 600 > 540 → `consensus`, 5555 bp share, 1111 bp margin |
| `plurality` | Opus+Fable+Sonnet `relevant` = 270; Sol+Terra `not_relevant` = 170; mini+Gemini `ambiguous` = 100 | 540 ≯ 540 → `plurality`, 5000 bp share, 1851 bp margin |
| `tied` | Opus+Fable `relevant` = 200; Sol+Gemini+mini `not_relevant` = 200; Sonnet+Terra `ambiguous` = 140 | tie at 200 → `tied`, gold `ambiguous`, 0 bp margin |

The `plurality` row is the Anthropic-unanimous case, which lands exactly on the quorum boundary and
must come out `plurality` rather than `consensus`; an off-by-one in the quorum comparison flips it.
Fixtures are required for each row, plus the Anthropic-unanimous tie (270 `relevant` against
GPT+Google's 270 `not_relevant`).

#### Record shape

`PairRecord` gains one nested `panel_vote` value. It contains no floating point, so `PairRecord`
keeps its `derive(Eq)` (`crates/qsf_semantic_eval/src/schema.rs:133`):

```text
panel_vote:
  aggregation_version   string
  weights_version       string
  total_units           u32          // W
  relevant_units        u32
  not_relevant_units    u32
  ambiguous_units       u32
  aggregation_status    enum { consensus, plurality, tied }
  winner_share_bp       u32
  margin_bp             u32
```

There is no `panel_confidence` field. The quantity is a vote margin, not a calibrated probability,
and naming it "confidence" invited exactly that misreading. Adding this requires a dataset
schema-version bump.

### `none_of_roster`

The utterance-level annotation is aggregated by the same integer weighted vote over a boolean, and
records the same status and share fields.

The existing invariant — `none_of_roster: true` cannot coexist with any `relevant` pair
(`frozen.rs:496`) — is enforced by derivation rather than by validation failure: if any pair for the
utterance wins `relevant`, `none_of_roster` is false regardless of the vote. Both the **raw vote**
and the **derived value** are persisted, and the override count is recorded so the methodology note
can report how often it fired.

Ledger selection (below) applies to the `none_of_roster` vote exactly as it applies to pair
verdicts: exactly one selected utterance-level vote per selected panel member.

### Ledger and versioned snapshots

Three separate identities, so that labels improve over time without the ruler moving and without
"latest" ever being inferred:

- **`panel_member_id`** — the stable panel slot that carries a weight (`opus-5`, `gpt-5-6-sol`, …).
- **Model build identity** — `model_id` plus the provider's immutable build string, recorded on
  every run. Re-running "the same" labeler later is a different build and a different run.
- **`labeling_run_id`** — one run of one member over the pool, content-hashed.

On top of those:

- **An append-only ledger** holds every `(utterance, goal, panel_member_id, labeling_run_id)`
  verdict and every `(utterance, panel_member_id, labeling_run_id)` `none_of_roster` vote, with
  model build identity and `guideline_version`. Adding a model or re-running one is a pure append;
  nothing is ever rewritten.
- **A snapshot selection configuration** names **exactly one `labeling_run_id` per
  `panel_member_id`**, plus exactly one auditor run, each pinned by content hash. Aggregation reads
  the selection, never the ledger's ordering or timestamps.
- Aggregation requires **exactly one selected verdict** for every
  `(snapshot, panel_member_id, utterance_id, goal_ref)` and for every
  `(snapshot, panel_member_id, utterance_id)` `none_of_roster` vote. Missing or duplicate selections
  are hard errors. **"Latest" is never inferred**, so replay does not depend on ledger growth.
- **Immutable versioned snapshots** are cut from a selection. A freeze aggregates one selection and
  pins it by SHA-256 over all inputs. Adding Kimi K4 later cuts `v2`; it never mutates `v1`. Scores
  are comparable within a version, and a version's labels never move under a model being measured
  against them.

### Manual and IDE-driven labeling runs

Not every panel member is reached through an API. Mini's production labels were produced by hand
(`labeling_run_id: mini-manual-run`), the Fable cross-label ritual is manual by construction, and
Gemini Flash 3.6 is labeled by driving the Antigravity IDE with the model chosen in its picker.

A manual run is a **first-class ledger run**, not a degraded one. Replay rebuilds labels from the
selected run artifact and never re-invokes a model, so determinism lives in the artifact rather than
in the session that produced it. `labeling-input.jsonl` is already a self-contained handoff: the
utterance, its language, and the roster with full goal summaries, carrying no `conditioning_goal_ref`
and no slice tags.

An agentic IDE is nonetheless **not** equivalent to an API call. The model has a workspace and
tools, so it can acquire information the rubric forbids it. Four preconditions close that gap, and
they apply to every hand-driven member, not just Gemini.

#### Workspace isolation

The guideline forbids giving a labeler generation output, intended goals, slice tags, prior labels,
reconciliation, or review decisions. Under an API call that holds by construction. Under an IDE it
does not: `runs/goalrel-production/generation-output.jsonl` carries `conditioning_goal_ref` — the
goal each utterance was generated *for* — and `intended_slice_tags`, with `label-mini.jsonl` and
`label-fable.jsonl` beside it. An agent rooted at the repository can open all of them.

- A hand-driven run executes in an **isolated working directory** holding exactly two files: the
  guideline at its selected version, and a copy of `labeling-input.jsonl`.
- The repository is **never** the workspace root for a labeling session.
- The run record carries `workspace_isolation_attested: true` plus a hash of the directory listing.

No downstream check can detect a leak after the fact, so this is an **operator attestation, not a
verified property**, and the methodology note must describe it that way.

#### Run identity when the provider exposes no build string

The ledger requires immutable model build identity on every run. A model picker may surface only a
display name such as "Flash 3.6". Recording an invented version string would put a fiction into a
hashed manifest, so the run record distinguishes what is known:

- `build_attestation: provider_reported | operator_attested`
- `model_build` — the provider string when reported; otherwise the displayed model name verbatim
- `harness` — IDE or client name and version, for `operator_attested` runs
- `run_mode: api | manual`

A snapshot may select `operator_attested` runs; the manifest records how many it selected, so a
reader can weigh the lineage evidence rather than assume every run is provider-pinned.

#### Tool availability

An agentic session with web search or file tools active is not purely a model reading a rubric — a
verdict could rest on retrieved material the other panel members never saw.

- Tools are **disabled** for a labeling session where the harness allows it.
- Where they cannot be disabled, the run record lists `tools_available[]` verbatim, and the
  methodology note reports which selected runs were not tool-free.

#### Prompt identity and output handling

- The instruction text handed to the model is a **committed file pasted verbatim**, never ad-hoc
  phrasing. `labeling_prompt_sha256` certifies nothing otherwise.
- Output is the same `LabelInterchange` JSONL shape, validated through `parse_label_interchange`
  against the frozen roster — the identical path `label-mini.jsonl` takes.
- **No hand repair.** Repairing malformed output by hand makes the operator the labeler. Resolve it
  with the model and validate the replacement, exactly as the Fable ritual already requires.
- The production pool is **114 utterances × 7 goals = 798 pair verdicts** plus 114 `none_of_roster`
  votes per member. Chunking a session across several exchanges is expected; the exactly-one-verdict
  rule fails the freeze on any gap, so a truncated session is caught rather than silently thinned.
- The session transcript is retained beside the run artifact and hashed into `transcript_sha256`,
  so "what was this model actually shown" stays answerable after the freeze.

### Guideline policy for v1

**The guideline is sharpened before v1, and every labeler is re-run.**

The guidelines do not draw a sharp boundary between *Grow the library* and *Assemble a world
picture*, and observed labeler disagreement concentrates there. That boundary is fixed before any
label is frozen rather than being settled seven ways by a weighted vote.

- `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` gains a worked example separating
  the two goals, and its version becomes `goalrel-label-v2`.
- **All seven panel members and the auditor label under `goalrel-label-v2`**, including GPT-5.4-mini
  and Claude Fable 5, whose existing production labels were made under `goalrel-label-v1`.
- Those `goalrel-label-v1` runs remain in the append-only ledger and are simply **not selected** by
  the v1 snapshot configuration. Nothing is rewritten or deleted.
- The earlier "existing labels are not re-paid for" property is **dropped**. Eight labeling runs are
  paid for — seven panel members plus the auditor — where the original design assumed five. This is
  the accepted cost of not aggregating verdicts cast under two rubrics.
- **The freeze gate requires one `guideline_version` and one guideline content hash across every
  selected panel and auditor run.** A snapshot mixing rubrics fails the freeze.

### The gate

The freeze gate keeps its per-slice shape and its position in the gatekeeper, but its metric,
denominators, and threshold source all change. Micro-averaged pair agreement is retired.

#### Why pair agreement had to go

`blind_qa_agreement_by_slice` counts every audited pair carrying the utterance's slice tag and
divides agreements by total pairs (`crates/qsf_semantic_datagen/src/frozen.rs:190-258`). The roster
holds **seven** goals (`evaluation/frozen/goal-relevance/realtime-seed.roster.json`) and the dataset
is a dense cross-product, so most utterances have one or two relevant goals and five or six easy
negatives. An auditor that gets all six negatives right and the one relevant goal wrong scores
`6 / 7 = 0.857` and passes a `≥ 0.80` gate — while being wrong about the only decision the task is
for. That failure is structural, not incidental, and it is exactly the single-goal skew the risk
section already names.

#### Auditor coverage

The auditor labels **every pair in both splits**, the same dense cross-product discipline the panel
carries. A missing auditor verdict fails the freeze. Validation and test are gated **separately**,
never pooled.

#### Metrics, per hard slice

Computed against the frozen gold labels, per hard slice and **per goal**:

- `relevant_recall` — of pairs with gold `relevant`, the fraction the auditor also called
  `relevant`. An auditor `ambiguous` against a gold `relevant` is a **miss**, not partial credit.
- `relevant_false_positive_rate` — of pairs with gold not `relevant`, the fraction the auditor
  called `relevant`.
- `abstain_rate` — the fraction of all audited pairs the auditor called `ambiguous`. This exists so
  that an auditor cannot pass by abstaining everywhere.
- `utterance_relevant_set_match` — the fraction of utterances whose gold set of relevant goals
  equals the auditor's set of relevant goals. This is the per-utterance check the review asked for:
  it cannot be satisfied by negatives.

#### Gate conditions, per hard slice and per split

1. **Macro**-averaged `relevant_recall` across evaluated goals ≥ `R_min`. Macro, not micro, so six
   easy negatives cannot hide one systematically wrong goal.
2. **Per-goal** `relevant_recall` ≥ `R_floor` for every evaluated goal.
3. `relevant_false_positive_rate` ≤ `F_max`.
4. `abstain_rate` ≤ `A_max`.
5. `utterance_relevant_set_match` ≥ `M_min`.

Denominator rules, so a gate can never pass on an empty cell:

- A goal is **evaluated** in a slice only if it has at least `min_relevant_support` gold-`relevant`
  pairs there. Skipped goals are listed in the evidence artifact with their support counts — they
  are never silently absent.
- If fewer than `min_evaluated_goals` qualify in a slice, the slice **cannot be gated** and the
  freeze fails. Insufficient evidence is a failure, not a pass.

Required rejection tests, mirroring the existing gatekeeper's one-test-per-violation convention:

- an auditor correct on every negative and wrong on every gold-`relevant` pair must fail;
- an auditor that answers `not_relevant` everywhere must fail;
- an auditor that answers `ambiguous` everywhere must fail on `abstain_rate`;
- an auditor correct on six goals and inverted on one must fail on `R_floor` even when the macro
  average would pass;
- a slice with too few evaluated goals must fail rather than pass.

#### Thresholds are pre-registered, and not derived from the evidence they gate

The earlier plan — measure Kimi K3 against the existing mini and Fable labels, then set the floor
from that measurement — is **dropped**. It was circular twice over: the threshold would have been
tuned on the same utterances and auditor outputs it then gates, and mini and Fable are themselves
panel members, so their labels are part of the thing being audited.

Instead:

- All metric definitions and all thresholds (`R_min`, `R_floor`, `F_max`, `A_max`, `M_min`,
  `min_relevant_support`, `min_evaluated_goals`) are written into a committed, content-hashed
  **audit policy file** *before* Kimi K3 produces any verdict that gates v1.
- Thresholds are derived from **planted-drift fixtures**: synthetic label sets built by corrupting
  known-good labels at declared rates — flip `x%` of `relevant` to `not_relevant`, flip `y%` of
  `not_relevant` to `relevant`, abstain on `z%`. The policy states the failure model it is calibrated
  against, for example *a panel whose relevant labels are wrong at ≥ 15% must fail every hard slice*.
  No real auditor output is involved, so nothing is tuned on the evidence it gates.
- The audit policy hash and the calibration fixture hash both enter the manifest, and both are
  disjoint from the v1 gate evidence.
- The stale `0.80` constant (`frozen.rs:149`) is removed with the metric it belonged to. It was
  calibrated for human self-agreement and carries no meaning for a model auditor.
- If a future version derives weight corrections or threshold corrections from measured agreement,
  it must take that evidence from a *previous* frozen version's auditor labels, never from the
  version being gated.

Independence remains structural: the auditor is excluded from the panel, its labels never feed the
weights, and in v1 the weights are declared policy fitted to nothing.

#### Unchanged gatekeeper rules

Review-completeness gating is retired. `ReviewStatus::Draft`/`Reviewed` no longer gates the freeze;
the replacement precondition is dense panel coverage — every pair carries a selected verdict from
every selected panel member.

All other gatekeeper rules stand: per-slice floors, split integrity, dense cross-product, roster
binding, roster round-trip, and recorded-seed split reproducibility.

### The freeze manifest

The existing `FreezeManifest` (`crates/qsf_semantic_datagen/src/artifacts.rs:96-109`) names only
generation, mini, Fable, review, roster, split, and dataset hashes. It cannot reproduce a panel
aggregation. It is replaced by a manifest that hashes every input the aggregation and the audit
evidence consume:

| Group | Contents |
|---|---|
| Identity | `dataset_version`, `schema_version`, `frozen_at` |
| Roster | `roster_snapshot_version`, `roster_fixture_hash` |
| Splits | `split_seed`, `validation_sha256`, `test_sha256`, `per_slice_counts_by_split` |
| Source | `generation_output_sha256` |
| Selection | `ledger_snapshot_sha256`; `selected_runs[]` of `{panel_member_id, model_id, model_build, build_attestation, run_mode, harness, tools_available[], workspace_isolation_attested, labeling_run_id, run_sha256, transcript_sha256, lineage, weight_units}` |
| Auditor | `auditor_run` in the same shape, plus `auditor_coverage_pairs` |
| Run provenance | `operator_attested_run_count`, `manual_run_count`, `tool_enabled_run_count` — so a reader can weigh the runs rather than assume they are uniform |
| Weights | `weights_version`, `panel_weights_sha256`, `lineage_totals_units`, `total_units` |
| Rubric | `guideline_version`, `guideline_sha256` (single values, enforced identical across all selected runs), `labeling_prompt_sha256`, `interchange_schema_version` |
| Aggregation | `aggregation_version`, `aggregation_sha256`, `aggregation_status_counts` |
| Audit | `audit_policy_version`, `audit_policy_sha256`, `calibration_fixture_sha256`, `audit_evidence_sha256` |
| `none_of_roster` | `none_of_roster_raw_sha256`, `none_of_roster_derived_sha256`, `none_of_roster_override_count` |
| Top level | `manifest_sha256` over the canonical serialization of everything above |

Canonicalization, so the hashes mean something: UTF-8, JSON with lexicographically sorted keys, no
insignificant whitespace, LF endings, and **no floating-point values anywhere in hashed content**.
That last constraint is why margins are basis points and weights are integer units.

**Replay rebuilds rather than trusts.** A freeze re-derives every gold label, panel vote,
`none_of_roster` value, and the full audit evidence from the selected ledger runs, the weights file,
and the audit policy. Precomputed `PairRecord` values are compared against the rebuild, never
accepted as authority; a mismatch fails the freeze.

### The operator's role

Per-pair labeling leaves the pipeline entirely. What remains is the work where a human has
leverage that no panel member has:

- **Own the rubric.** `AnnotationGuidelines.GoalRelevance.md` steers all seven labelers at once; a
  worked example is worth more than hundreds of hand corrections.
- **Declare the trust weights** and the panel/auditor split.
- **Pre-register the audit policy** — metrics, thresholds, and the failure model — before the
  auditor runs.
- **Approve the freeze** on the gatekeeper's evidence.

## Risks and limitations

- **A bias shared by the whole panel goes uncaught.** This is the real cost of removing human
  labels, and it is not hypothetical. Across four reviewed utterances, both Anthropic-lineage
  models read *Assemble a world picture* as not-relevant to personal knowledge-management
  utterances while mini read it as relevant. The guideline sharpening addresses that specific
  boundary before v1, and the per-goal auditor floor makes a single-goal skew a gate failure rather
  than something a slice average absorbs. Neither closes the general case.
- **The Anthropic lineage holds exactly half.** At 270 of 540 units it cannot reach the consensus
  quorum alone, but three Anthropic models voting together still win a `plurality` whenever GPT and
  Google disagree with each other, and they cannot be outvoted except by a unified rest-of-panel.
  A lineage-wide misreading of a goal therefore still shapes the contested labels; it just can no
  longer be stamped `consensus`. The declared 5000 bp cap keeps this from drifting further without
  a deliberate policy change.
- **The counterweight to Anthropic is thin.** GPT+Google reach 270 only by voting as a bloc, and
  60 of that is a single small model. The three-lineage property holds arithmetically but rests on
  the two lightest members; losing either one to an API change or a failed run re-creates an
  Anthropic majority, which the validator will catch as a cap violation rather than let through.
- **The auditor is one model.** A per-slice agreement gate against a single outside model is a
  weaker guarantee than a diverse human sample would be. It is the guarantee the dataset now claims,
  and the methodology note must not overstate it.
- **v1 contains no non-model evidence at all.** Retiring the human review path removes the last
  human signal from the ground-truth chain. The claim paragraph says so plainly; anyone reading a
  score against this set is reading a model-panel agreement number.
- **Thresholds are calibrated against a synthetic failure model.** Planted-drift fixtures remove the
  circularity, but they test the drift the operator imagined. A real panel failure with a different
  shape can still pass. The failure model is recorded in the audit policy so a later version can
  argue with it.
- **Hand-driven runs rest on attestation, not verification.** Workspace isolation and tool
  disablement cannot be checked from the run artifact — a contaminated session produces output
  indistinguishable from a clean one. Two v1 members (mini and Gemini) are manual, so this is not a
  corner case. The manifest records which runs were manual and operator-attested so the weakness is
  visible to a reader rather than buried; the mitigation is the ritual and the isolated directory.
- **Model versions drift.** A ledger entry is only meaningful with its model build recorded;
  re-running "the same" labeler later appends a new run with a new `labeling_run_id`, and the
  snapshot selection decides which run a version used.

## Decisions this changes

Both entries dated 2026-07-22 need recorded reversals rather than silent replacement:

- *Goal-relevance freezes are gate-kept and reproducible from committed lineage* — the gatekeeper
  rule list loses "review completeness", and its "blind-QA agreement" becomes a pre-registered
  per-slice, per-goal auditor metric. The reproducibility, lineage-retention, and remaining
  gatekeeper rules stand unchanged and are strengthened by the new manifest and by rebuild-on-replay.
- *Goal-relevance review relabels but never excludes utterances* — the no-exclusion property
  survives and is strengthened (every pair is kept with its full weighted vote), but the operator
  relabeling mechanism it describes no longer exists.

A new entry records the panel methodology, the three-lineage composition and its 5000 bp cap, the
consensus/plurality distinction, and the dataset's ground-truth claim.

## Documents to update

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- `docs/Plans/Plan.GoalRelevanceFrozenSets.md` — the labeling, review, and gatekeeper sections.
- `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` — the *Grow the library* /
  *Assemble a world picture* worked example, version bump to `goalrel-label-v2`, and the Fable
  cross-label ritual generalized into one **hand-driven labeling ritual** covering every manual
  member and the auditor: workspace isolation, the two-file working directory, tool disablement,
  the verbatim prompt file, no hand repair, and the attestation fields the run record must carry.
- The committed labeling prompt file the manual ritual pastes verbatim — new, and hashed into the
  manifest as `labeling_prompt_sha256`.
- `docs/DecisionLog.md` — two reversals and one new entry.
- The methodology note beside the frozen manifest — the ground-truth claim paragraph, weights and
  per-lineage totals, the `aggregation_status` distribution, tie precedence, auditor identity, the
  pre-registered audit policy and its thresholds, the `none_of_roster` override count, and the run
  provenance counts — how many selected runs were manual, operator-attested, or not tool-free, with
  workspace isolation named as an operator attestation rather than a verified property.
- `docs/Handoff.md` — only if this changes the Now/Next recommendation.

No `Experiment.*.md`: this is data-production methodology, not a consciousness-simulation mechanism
under question (`ProjectWorkflow.md`, *Document Tracks*).

## Suggested implementation increments

Increments 1–6 are verifiable against fixtures before any paid labeling.

1. **Weights file and validator** — integer units, `weights_version`, per-lineage totals and shares
   computed rather than asserted, and the declared lineage cap enforced. Because Anthropic sits
   exactly on the v1 cap of 5000 bp, the boundary case is the default case: tests must cover
   at-cap (passes) and one-unit-over (fails). Pure, unit-tested.
2. **Ledger artifact and snapshot selection** — the three identities, append-only writes, the run
   provenance fields (`run_mode`, `build_attestation`, `harness`, `tools_available[]`,
   `workspace_isolation_attested`, `transcript_sha256`), and the selector that requires exactly one
   verdict per `(snapshot, panel_member_id, utterance_id, goal_ref)` and per utterance-level vote.
   Rejection tests for missing and duplicate selections, and a test that "latest" is never
   consulted. Because Gemini and mini are `manual`/`operator_attested` in v1, the manual path is the
   default path and must be covered, not treated as an exception.
3. **N-labeler weighted aggregation** as a pure integer function — scores, ranking precedence,
   quorum, `aggregation_status`, `winner_share_bp`, `margin_bp`, plus the `none_of_roster` vote and
   its relevant-pair derivation override with the override counter. Fixtures for the `consensus`,
   `plurality`, and `tied` rows above, for a lineage-unanimous vote, and for equal-score runner-up
   precedence.
4. **`panel_vote` on `PairRecord`** and the dataset schema-version bump, with a test asserting
   `PairRecord` still satisfies `Eq`.
5. **Audit policy file and calibration** — the policy schema, the planted-drift fixtures and their
   stated failure model, and the per-slice/per-goal audit metric as a pure function with every
   rejection test listed under *The gate*.
6. **New freeze manifest** — canonical serialization, the full hash set, the top-level
   `manifest_sha256`, and freeze replay that rebuilds labels and audit evidence from the selected
   runs instead of trusting precomputed records.
7. **Pipeline switchover** — `fold_reviewed_pool` takes gold from the panel vote instead of
   `mini_label`; review-completeness gating retired in favour of dense panel coverage; the auditor
   gate replaces `blind_qa_agreement_by_slice` and the `0.80` constant is removed.
8. **Guideline sharpening, ritual, and labeling** — write the worked example, bump to
   `goalrel-label-v2`, commit the verbatim labeling prompt file and the hand-driven ritual, then run
   all seven panel members and the auditor over the existing production pool. Each hand-driven run
   sets up its isolated two-file working directory first and records its attestations. This is the
   paid step, and it comes after every mechanism above is tested.

   A dry run on a handful of utterances through the Antigravity path is worth doing before the full
   798-pair session: it exercises isolation setup, chunking, the interchange shape, and validation
   while a mistake is still cheap to discard.
9. **Cut and gate-keep frozen `v1`.**
