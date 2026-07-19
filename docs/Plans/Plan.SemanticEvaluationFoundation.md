# Plan: Semantic Evaluation Foundation and the Goal-Relevance Task Contract

Status: Proposed — not started
Maturity: Candidate
Area: Evaluation infrastructure / Volition (goal relevance) / Telemetry

## Why this plan exists

Qualia Signal Foundry decides "which goal is relevant to this turn?" with an exact-token
weighted lexical scorer (`crates/qsf_volition/src/selection.rs`, inventory ID T5). The project
intends, over later stages, to test local semantic models against that scorer. Today there is no
held-out labeled data, no shared metric report, and no frozen record of what remote OpenAI usage
currently costs. Every future replacement would be judged by anecdote and could overfit the
existing hand-authored fixtures.

This plan builds the **measurement and data foundation** for the `goal_relevance` behavior only,
plus a **frozen remote-usage telemetry baseline**, so that a later goal-relevance replacement
pilot can start against a measurable quality, latency, call-volume, token, and cost baseline. It
is the "evaluation foundation" slice of
`docs/Research/TechBrief.QSF_Local_Semantic_Classification.md` (a candidate brief — planning
input, not commitment), with its precursor `docs/Plans/Idea.SemanticGoalActivation.md` and the
standing 2026-07-04 weighted-goal-activation decisions in `docs/DecisionLog.md`.

**This plan builds no model.** It builds the contract, the frozen data, the baseline runner over
production code, the metric/error reports, the regression gate, and the telemetry baseline.

### Naming and ephemerality

This document owns ephemeral phase labels. Durable artifacts it produces — the eval crate,
schema types, the task contract, the experiment spec, decision-log entries — name the behavior
(`goal_relevance`) and never cite a brief "stage" or a plan "phase" number
(`Agents.md`; `ProjectWorkflow.md`). The tech brief's `T5`/"Stage 0" labels are cross-references
into a point-in-time inventory, not runtime names.

## Decisions this plan proposes to commit

These directions were confirmed with the user during the brief's brainstorm, but only one of them
is recorded in `docs/DecisionLog.md` today: the accepted weighted scorer/threshold and the
experiment-scope rule (2026-07-04). The rest are **proposed here and become committed only when
the Decision Log is updated** as this plan lands (see "Documents to create or update"). They are
not yet settled project rules; this section states the intended commitments, not existing ones.

1. **Scope: the `goal_relevance` slice plus the remote-usage telemetry baseline** (proposed).
   Contracts for other inventory families (T1–T4, T6–T13) are explicitly out of scope. The
   telemetry baseline is in scope now because current usage drifts and the later remote-gating
   stage needs it frozen early.
2. **English only for the first frozen sets** (proposed). Swedish and code-switched slices are
   deferred to a later dataset version. The corpus schema still carries a language metadata field
   so later versions add languages without a schema change.
3. **LLM-generated labels require human review before validation/test use** (proposed). An OpenAI
   teacher model generates utterances, paraphrases, and hard negatives (tech brief Section 10.4);
   the single operator accepts or corrects everything that enters the frozen validation/test
   sets. Walking-skeleton sample records remain `draft` until that review occurs. Real session
   transcripts (tens of turns) become a small true held-out slice.
4. **Label unit is the persona-independent pair** (proposed). Gold labels attach to
   `(utterance, goal-description/tension-summary)` pairs, not to fixture goal IDs. This survives
   persona edits (personas are data, 2026-07-03) and matches the eventual pair-scorer design. The
   current seven seed goals become one referenced, versioned **roster snapshot** of descriptions.
   Production goals are not a fixed set (live goal formation creates goals at runtime), so the
   design acknowledges a live-formed-goal slice as a named limitation even though v1 data covers
   only the seed roster snapshot.
5. **The baseline prediction contract lives in the task contract, not the runner** (proposed). The
   production scorer emits per-goal `match_strength` (an unbounded integer sum of matched keyword
   weights) plus an arbitration winner; the qualification threshold (4) is an arbitration gate,
   not a relevance boundary. The contract specifies the graded quantity: treat `match_strength`
   as a graded score, report precision/recall across a threshold sweep, and additionally report
   the production operating point (qualification threshold 4) as one marked point. The
   deterministic lexical scorer **never abstains** — every pair receives a numeric score — so
   `Abstain` is a prediction state reserved for future learned models and is recorded in the
   contract as "not emitted by the baseline."
6. **Baseline fidelity: the eval crate calls production code** (proposed). The runner calls
   `qsf_volition`'s production tokenizer (`normalize_terms` / `grounded_terms_from_text` in
   `crates/qsf_volition/src/terms.rs`) and scorer (`matched_keywords`, `match_strength`,
   `select_goals_ranked` in `selection.rs`) through their public API — never a reimplementation.
   The inventory documents seven mutually incompatible tokenizers as cross-cutting problem #1; a
   re-tokenizing eval would measure a different function than production. Dependency direction:
   **`qsf_semantic_eval` depends on `qsf_volition`**, never the reverse.
7. **Telemetry baseline unifies accounting in a lean shared crate** (proposed). The 2026-07-07
   decision shipped a session-scoped token ledger that is private to the realtime-server boundary
   (`crates/qsf_realtime_server/src/realtime/token_usage.rs` is `pub(crate)`, re-exported only
   within the crate at `realtime/mod.rs:12`) and today records only `realtime_voice` and
   `goal_formation`. Sleep summarization and article extraction run in `qsf_app` (which does not
   depend on `qsf_realtime_server`) and their usage currently lands only in offline `RunContext`
   traces. Rather than bolt a parallel accounting path onto that split (DRY rule) or settle for a
   fragile cross-source aggregator, this plan **extracts the ledger into a lean shared crate** so
   realtime voice, goal formation, sleep summarization, and article extraction all record into one
   accounting boundary. This is deliberately the larger-refactor option; it is scoped honestly in
   its phase.
8. **Label QA: a blind self-re-annotation consistency pass** (proposed). After a delay, the
   operator re-annotates a shuffled sample of the hard slices (negation, quoted speech,
   hypotheticals); per-slice agreement is recorded in the dataset methodology to bound
   single-operator label reliability on the decisive slices.

## Repository placement (decided here, with justification)

The tech brief suggests a `qsf_semantic_eval` crate and a top-level `evaluation/` directory. The
workspace is `members = ["crates/*"]` (`Cargo.toml`), so:

- **`crates/qsf_semantic_eval`** — a new lean crate owning schema types (the DRY source of truth
  for the corpus/contract shapes), the dataset loader/validator, the baseline runner, the metric
  generator, and the error-analysis report generator. It depends on `qsf_volition` (for the
  production tokenizer/scorer) and serde/serde_json; it must **not** depend on `qsf_app` or the
  realtime server. This mirrors the lean-crate discipline established for `qsf_memory`,
  `qsf_context`, `qsf_corpus`, and `qsf_realtime_protocol` (DecisionLog 2026-06-10).
- **`evaluation/`** (top-level, version-controlled) — the durable data-and-spec artifacts that
  are not crate-scoped:
  - `evaluation/contracts/` — the versioned task-contract format and its `goal_relevance`
    instance.
  - `evaluation/schemas/` — the human-readable corpus/roster schema doc (the Rust types in the
    crate remain authoritative; the doc explains them).
  - `evaluation/annotations/` — annotation guidelines and the privacy/sanitization rules.
  - `evaluation/frozen/goal-relevance/` — the roster snapshot and the frozen validation/test
    sets, content-hashed and versioned.
  - `evaluation/reports/` — the checked-in frozen baseline report and the frozen telemetry
    baseline.
- **Generated per-run** metric/error artifacts land under the existing `runs/<run-id>/`
  convention (`ProjectWorkflow.md`); a deliberately frozen baseline is copied into
  `evaluation/reports/` stamped with dataset version, roster/fixture hash, and code commit.

The `evaluation/` tree is co-located with the data it governs so later task families (T1–T13) add
sibling contracts and frozen sets without touching `docs/` or the crate.

---

## Phase: Walking skeleton — contract, schema, and baseline over a sample slice

The smallest viable end-to-end slice: prove the whole pipeline (schema → dataset → production
baseline → metrics → report) on a tiny in-repo sample dataset **before** paying for teacher
generation and human annotation.

**Work**

- New `crates/qsf_semantic_eval` crate depending on `qsf_volition`.
- **Corpus schema types** (versioned, `schema_version: u16` per the repo's per-record versioning
  habit):
  - The **pair record** `(utterance, goal_ref, gold_label)` with per-pair `slice_tags`.
  - **Gold label space** (per pair): `Relevant`, `NotRelevant`, `Ambiguous`. `Ambiguous` is a
    first-class gold value, not a prediction state.
  - **`NoneOfRoster` is an utterance-level annotation**, not a pair label: it asserts the
    utterance bears on no roster goal. A consistency check requires that when an utterance is
    marked `NoneOfRoster`, every one of its pair records is `NotRelevant` (or `Ambiguous`); a
    dataset-validation test enforces this so utterance-level and pair-level annotations cannot
    silently disagree.
  - `goal_ref` is a **content-addressed description key** — a stable hash of the goal's frozen
    description text (title + summary + tension summaries) plus the roster-snapshot version — that
    resolves to a goal *inside the frozen roster snapshot*, never the live fixture ID. This is
    what keeps labels persona-swap-safe (decision 4) while still resolving to a complete scorer
    input.
  - Language metadata (defaults to `en`); `session_id` and `semantic_cluster_id` for leakage-safe
    splitting; provenance (`teacher` vs `real-session`, teacher model id, review status).
  - **Slice tags**: paraphrase-cluster id, hard-negative, explicit-negation, implicit-negation,
    quoted-speech, hypothetical, subject-confusion, punctuation-casing-loss, synthetic-asr,
    real-asr, rare-high-cost. (`subject-confusion` and `real-asr` are added per review
    recommendation 1; `real-asr` is carried by the real-session slice, `subject-confusion` by
    teacher generation.)
- **Goal-roster snapshot that freezes complete scorer inputs.** `matched_keywords` needs a whole
  `qsf_volition::Goal` and `select_goals_ranked` additionally needs a `VolitionState` and
  `VolitionFixture` (`crates/qsf_volition/src/selection.rs:22-42,138-142`). So the snapshot is the
  **serialized complete `VolitionFixture`** produced by `realtime_seed_fixture()` (goals with full
  activation keywords + weight classes, tensions, and the qualification threshold), plus an
  explicit empty/default `VolitionState`, plus a content hash of the serialized fixture. The
  baseline therefore runs the real scorer against frozen, reproducible inputs rather than
  re-deriving a partial `Goal`. Each frozen goal also exposes its `goal_ref` description key so
  pair labels resolve to a goal within this snapshot. A drift-guard test asserts the snapshot
  still round-trips to the current `realtime_seed_fixture()` (or fails loudly so the snapshot is
  re-versioned deliberately). `qsf_volition`'s `Goal`/`VolitionFixture`/`VolitionState` are serde
  types today; if any needed field is not publicly serializable, this phase adds the minimal
  public accessor/serialization to `qsf_volition` rather than reconstructing state in the eval
  crate.
- **The `goal_relevance` task contract** (`evaluation/contracts/GoalRelevance.TaskContract.md`),
  populated per the tech brief's required fields: unit of input; label space (Relevant /
  NotRelevant / Ambiguous pair labels, the utterance-level NoneOfRoster annotation, and the
  prediction-side Abstain state — recorded as **not emitted by the deterministic baseline**);
  action boundary (current-turn volition shaping, not a durable write); false-positive/
  false-negative cost sketch; latency budget (recorded as a target, and *measured* in the
  failure-floor phase — see Open Questions for the budget number itself); availability
  requirements; explanation/trace requirements; dataset slices; primary/secondary metrics
  (the mandatory set below); promotion and rollback requirements; and the **baseline prediction
  contract** from decision 5 (grade `match_strength` as a graded score; PR across a threshold
  sweep; mark the production operating point at qualification threshold 4; the baseline never
  abstains).
- **Baseline runner**: loads a dataset + the frozen roster snapshot, deserializes the frozen
  `VolitionFixture`/`VolitionState`, and for each `(utterance, goal)` pair calls
  `qsf_volition::normalize_terms` then `matched_keywords` + `match_strength` from the crate's
  public API to produce a graded score. It must not tokenize or score independently. A fidelity
  test asserts that, for a known input, the runner's per-goal strengths equal what
  `select_goals_ranked` reports for the same query against the same frozen fixture — proving one
  scoring function, not two.
- **Mandatory v1 metrics** (the "standard package," resolving what the contract grades on):
  - **Quality**: pair-level precision / recall / F1 across the `match_strength` threshold sweep,
    plus the same at the production operating point (qualification threshold 4); paraphrase-cluster
    recall; and per-slice breakdowns (at minimum negation, quoted-speech, hypothetical, and ASR
    corruption). `Ambiguous` pairs are **excluded from binary precision/recall** and reported as
    their own counted slice with its own totals. `NoneOfRoster` utterances contribute their pairs
    as negatives.
  - **Latency**: measured p50/p95/p99 of the baseline scorer per utterance on this machine
    (produced in the failure-floor phase; the contract records the method here).
  - **Deferred**: cost-weighted error and calibration metrics are explicitly deferred until a
    learned model exists (no probability to calibrate; no asymmetric action yet).
- **Metric report generator**: emits both a **machine-readable `metrics.json`** (the structured
  artifact the regression gate compares — never rendered prose, per review recommendation 2) and
  a human-readable summary derived from it. Covers the mandatory metrics above.
- **Error-analysis report format**: worst false-negatives (relevant pairs the scorer scores
  zero/low) and false-positives (not-relevant pairs above the operating point), grouped by slice.
- **A tiny in-repo sample dataset** (about a dozen pairs spanning paraphrase, stray-word, negation,
  and one `Ambiguous`/`NoneOfRoster` example) that the runner uses **by default** when no frozen
  dataset path is configured, so the pipeline code path is exercised by default (Agents.md:
  defaults exercise the new path). The `WeightedGoalActivation` probes seed a few of these sample
  pairs.

**Trace/artifact completeness contract** (required — the reports explain a scoring chain;
`ProjectWorkflow.md`). This is the stable **`goal_relevance` per-pair result contract** that the
durable experiment spec refers to by name (never a plan-phase label):

- Authoritative artifact boundary: the **per-pair result record** (JSONL) is the structured
  causal chain; `metrics.json` is the derived structured metrics artifact; the metric summary and
  error-analysis report are human-readable views derived from those.
- Required per-pair fields: `dataset_version`, `roster_snapshot_hash`, `utterance`, `goal_ref`,
  `gold_label`, `slice_tags`, `matched_terms` (with weight classes), `match_strength`,
  `qualification_threshold_in_force` (the numeric threshold, **not** only a boolean — required by
  the 2026-07-04 weighted-activation decision, `docs/DecisionLog.md`),
  `qualifies_at_threshold_in_force`, and `scorer_source` (crate + fn path).
- Automated verification parses the generated JSONL and asserts every required field is present
  (including the numeric threshold) and that `matched_terms`/`match_strength` are consistent with
  the production API for a sampled record. Do not mark the trace criterion complete until
  generated artifacts satisfy this.

**Verification (automated)**

- `cargo build`; crate `cargo test` green.
- Schema round-trip: the sample dataset parses; an off-version or malformed record errors loudly.
- NoneOfRoster/pair consistency check and Ambiguous-exclusion accounting both covered by tests.
- Roster drift guard and frozen-fixture round-trip as above.
- Baseline fidelity test (runner strengths == `select_goals_ranked` strengths).
- Report generation produces the JSONL + `metrics.json` + summaries; the artifact-parsing test
  asserts the numeric threshold field is present.
- Workspace hygiene: `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Experiment scaffold**: none. This is routine engineering (schema, loader, a runner that wraps
production code, report formatting) whose outcome is not in doubt; code, tests, and commit carry
it (2026-07-04 experiment-scope decision).

**Human testing**: not required this phase.

---

## Phase: Frozen validation and test sets for goal relevance

Generate, human-review, split, and freeze the real English datasets the walking skeleton was
built to consume.

**Work**

- **Teacher generation tooling**, living **in this repository beside the `evaluation/` tree**
  (decided; versioned with the corpus schema it produces — only the later training pipeline's
  location stays open), produces utterances, paraphrase clusters, hard negatives, explicit and
  implicit negation, quoted speech, hypotheticals, subject-confusion examples, punctuation/casing
  loss, synthetic ASR corruption, and rare high-cost examples — English only — labeled against the
  roster snapshot's goal descriptions. Generate against multiple invented tension sets (not just
  Ari's) so labels reflect the pair task ("does this utterance bear on this tension?"), per the
  precursor idea. Generated output conforms to the frozen-set schema before human review.
- **Human accept/correct** every pair that enters the frozen validation and test sets.
- **Real session transcripts** (the existing tens of turns) are sanitized per the rules below and
  become a small true held-out slice, tagged `real-session`.
- **Split by session and semantic cluster**, never random utterance: no `semantic_cluster_id`
  and no `session_id` spans both the validation and test sets (prevents paraphrase leakage).
- **Blind self-re-annotation QA**: after a delay, the operator re-annotates a shuffled sample of
  the negation, quoted-speech, and hypothetical slices; per-slice agreement is recorded in a
  dataset methodology note under `evaluation/annotations/`.
- **Freeze**: content-hash and version the validation/test sets and record the roster snapshot
  hash they were labeled against.
- **Privacy/sanitization rules** (`evaluation/annotations/SanitizationRules.md`): what must be
  stripped/transformed before a real-session transcript enters the corpus. Exact rules for a
  single-operator local research project are an Open Question surfaced there; the phase commits
  the rules that are decided and marks the rest open rather than silently including raw text.

**Verification (automated)**

- Schema validation over the full frozen sets.
- Slice-coverage assertions: every required slice is present with at least a minimum count,
  including subject-confusion, synthetic-asr, and the real-session/real-asr slice; rare high-cost
  examples are present.
- NoneOfRoster consistency check over the frozen sets: each `NoneOfRoster` utterance has only
  `NotRelevant`/`Ambiguous` pairs.
- Split-integrity test: no cluster or session id appears in both validation and test.
- Label-QA agreement figures are present in the methodology note and parse.

**Experiment scaffold**: none for the dataset construction itself (engineering). The mechanism
question rides on the next phase.

**Human testing (recommended)**: the operator annotation and the blind re-annotation pass **are**
the human step and are required to produce the frozen sets. Evidence to collect: per-slice
self-agreement rates, count per slice, and a short note on any label categories that were hard to
adjudicate (feeds the "how much of the metric menu applies" Open Question). An external reviewer
sampling a subset of labels is recommended before the sets are treated as frozen.

---

## Phase: Baseline failure-floor measurement, latency baseline, and the regression gate

Run the production baseline over the frozen sets, measure its latency on this machine, freeze the
report, quantify the lexical failure floor as a mechanism experiment, and make goal-relevance
regressions detectable automatically.

**Work**

- Run the baseline runner over the frozen validation/test sets; produce the **frozen baseline
  report** (`metrics.json` plus the human summary) and check it into `evaluation/reports/`,
  stamped with dataset version, roster/fixture hash, and code commit.
- **Latency baseline** (resolves the plan's promise of a *measured* latency baseline, not a target
  in the contract): add a measurement step that times the production scorer per utterance across
  the frozen set on this machine and records **p50/p95/p99** into `metrics.json`. The measurement
  times the `qsf_volition` scoring calls specifically (excluding dataset I/O), warms up before
  sampling, and records the machine/build profile alongside the numbers. This makes the "measurable
  latency baseline" exit criterion real rather than an unvalidated contract target.
- **Experiment** `docs/Experiments/Experiment.GoalRelevanceLexicalFailureFloor.md`: on the broad
  frozen held-out set, measure how badly exact-token matching fails on paraphrase recall,
  morphology, and negation — the gap that justifies or kills a later semantic model. Framed as
  **quantifying the failure floor**, explicitly **not** as re-validating paraphrase invariance or
  stray-word immunity (`Experiment.WeightedGoalActivation.md` already validated those on
  hand-authored cases; the 2026-07-04 experiment-scope decision requires a genuine mechanism
  question). The evaluation infrastructure is plain engineering; this measurement is the
  experiment.
- **Regression gate**: a test or command re-runs the baseline and compares the produced
  **`metrics.json`** (structured data, never rendered prose — review recommendation 2) against the
  frozen `metrics.json` within a stated tolerance, so a change that alters the goal-relevance
  family's measured behavior fails loudly. This is what makes "regressions for the goal-relevance
  family are detectable automatically" (exit criterion) true.

**Trace/artifact completeness contract**: the experiment relies on the per-pair JSONL from the
stable **`goal_relevance` per-pair result contract** (defined in the walking-skeleton phase, named
by behavior, not by phase) to explain each failure; its spec restates the required fields —
including the numeric `qualification_threshold_in_force` — and the artifact-parsing check, and
adds that the failure-floor tables are derived from that JSONL, not re-computed independently.

**Verification (automated)**

- Baseline run over frozen sets reproduces the checked-in `metrics.json` (quality metrics are
  deterministic given dataset + roster + code; latency percentiles are recorded but excluded from
  the deterministic-reproduction assertion since timing varies run to run).
- Regression gate compares structured `metrics.json` fields, passes on `main`, and fails on an
  injected metric perturbation (a test proves the gate has teeth).
- Experiment automated criteria: artifacts satisfy the `goal_relevance` per-pair result contract.

**Human testing (recommended)**: the operator reviews the failure-floor report and confirms the
worst-case examples are genuinely mislabeled by the lexical scorer (not annotation errors). This
review is the experiment's human-review criterion; it can remain unchecked until read.

---

## Phase: Frozen remote-usage telemetry baseline (parallelizable)

Independent of the dataset work; can proceed in parallel after the walking skeleton. Freezes what
remote OpenAI usage costs today per semantic surface so the later remote-gating stage measures
reduction against a fixed point.

**Honest scope note.** The current ledger cannot capture the three surfaces as-is. `token_usage.rs`
is `pub(crate)` to `qsf_realtime_server` and records only `realtime_voice` and `goal_formation`;
sleep summarization and article extraction run in `qsf_app` (which does not depend on the realtime
server) and their usage lands only in offline `RunContext` traces. Making one accounting boundary
is therefore a **real refactor**, larger than a read-only aggregator — chosen deliberately as the
proper long-term solution (Agents.md prefers long-term solutions over minimal patches).

**Work**

- **Extract the ledger into a new lean shared crate** (candidate name `qsf_token_ledger`) owning
  `TokenClassCounts`, `ModelTokenUsage`, `TokenUsageSnapshot`, the provider-usage parsing
  (`usage_number`), and the persistence/aggregation contract. Both `qsf_realtime_server` and
  `qsf_app` depend on it; the realtime server's `record_token_usage` and the app's sleep/article
  model-invoker seams write into the *same* ledger type. This follows the lean-crate dependency
  pattern (DecisionLog 2026-06-10) and keeps the diagnostics behavior (2026-07-07) intact — it is
  a behavior-preserving move plus two new recording call sites. This is a Decision Log candidate
  (the shared accounting boundary).
- **Persisted source artifacts**: each recording surface writes a per-run ledger snapshot to a
  durable artifact (under `runs/<run-id>/` and/or the state dir) carrying its own content hash,
  capture timestamp, and the model versions in force. The frozen baseline references those source
  snapshots by hash so it is reconstructible and auditable (review recommendation 3).
- **Aggregation contract**: an aggregation step rolls the per-surface source snapshots up into a
  versioned frozen baseline artifact under `evaluation/reports/`, listing each contributing
  snapshot hash. Because all three surfaces now record into the same ledger type, the aggregation
  reads one shape rather than reconciling realtime snapshots against offline `RunContext` traces.
- **Cost estimation**: the diagnostics ledger deliberately carries no price table (2026-07-07 kept
  it raw). The frozen baseline needs estimated cost, so it introduces an **explicit, versioned
  price table** (with a provenance date and content hash) used only by the aggregation/estimation
  step and kept out of the live ledger. This split (raw ledger stays raw; estimation is a separate
  versioned layer) is a Decision Log candidate.
- The frozen baseline records, per surface: call counts, input tokens (fresh/cached), output
  tokens, model id(s), and estimated cost, plus the capture date, provider model versions, source
  snapshot hashes, and price-table version/hash.

**Verification (automated)**

- Aggregation test over sample ledger snapshots yields expected per-surface totals.
- A recompute test parses the frozen baseline, re-derives per-surface totals and estimated cost
  from the referenced source snapshot hashes and the versioned price table, and asserts they match
  the frozen figures (review recommendation 3 — the frozen cost is reproducible, not asserted).
- Frozen-baseline artifact parses and carries the required fields (capture date, model versions,
  source snapshot hashes, price-table version/hash).
- Ledger-extraction refactor is behavior-preserving: existing realtime token-usage/diagnostics
  tests pass unchanged against the shared crate; new tests cover the sleep and article recording
  seams.
- Hygiene: clippy + fmt.

**Human testing (recommended and required for real numbers)**: run a real `qsf.ps1 realtime`
session, a real `qsf.ps1 sleep` consolidation, and a real article extraction so the ledger
captures actual usage; then run the aggregation to freeze the baseline. Evidence to collect: the
per-surface call/token/cost snapshot and the model versions in force. (Sample-ledger tests keep
the code path exercised without spending real API budget by default.)

---

## Exit criteria (whole plan)

- A versioned task-contract format exists and is populated for `goal_relevance`.
- A versioned corpus schema and annotation guidelines exist.
- Frozen validation/test sets for goal relevance exist, split by session and semantic cluster,
  covering the required slices, with recorded label-QA agreement.
- A baseline runner executes the production weighted lexical scorer (via `qsf_volition`'s public
  API) against the frozen sets; a metric generator (structured `metrics.json` plus human summary)
  and error-analysis format exist; the baseline scorer's p50/p95/p99 latency is **measured** on
  this machine; and goal-relevance-family regressions are detected automatically against the
  structured metrics artifact.
- A frozen remote-usage telemetry baseline exists for live goal formation, sleep summarization,
  and article extraction (calls, tokens, estimated cost).
- Dataset privacy/sanitization rules exist.
- Net effect: a later goal-relevance replacement pilot can start against a measurable quality,
  latency, call-volume, token, and cost baseline.

## Documents to create or update (`ProjectWorkflow.md`)

- **Create** `crates/qsf_semantic_eval` (crate + tests) and the `evaluation/` tree (contracts,
  schemas, annotations, frozen sets, reports).
- **Create** `docs/Experiments/Experiment.GoalRelevanceLexicalFailureFloor.md` (the mechanism
  experiment) and add it to `docs/Experiments/Experiment.Backlog.md`.
- **Consider** a `docs/Plans/Design.SemanticTaskContracts.md` recording the rationale for the
  versioned task-contract format (a focused design decision supporting this plan), with the
  authoritative format artifact living under `evaluation/contracts/`.
- **DecisionLog entries** (the "proposed" directions in the Decisions section become committed
  only when these land; until then the plan describes them as proposed):
  - The versioned semantic task-contract format is adopted and populated incrementally by task
    family, starting with `goal_relevance`, and its evaluation scope is that one behavior plus the
    remote-usage telemetry baseline.
  - The first frozen goal-relevance dataset is **English only**, with the corpus schema carrying
    language metadata so later versions add languages without a schema change.
  - Goal-relevance labels are **LLM-teacher-generated and require human review before use as a
    validation/test set**, with a blind self-re-annotation consistency pass on the hard slices
    recorded in the dataset methodology. Walking-skeleton sample records remain `draft` until
    that review occurs.
  - The evaluation crate depends on `qsf_volition` and must grade the production tokenizer/scorer
    through their public API (no reimplementation); gold labels attach to content-addressed
    `(utterance, goal-description)` pairs, not fixture goal IDs.
  - The durable evaluation artifacts live in a top-level `evaluation/` tree and the runner in a
    lean `qsf_semantic_eval` crate (the repository/schema boundary).
  - The token ledger is extracted into a lean shared crate so realtime voice, goal formation, sleep
    summarization, and article extraction share one accounting boundary, with a separate versioned
    price-table/estimation layer kept out of the diagnostics ledger and a durable, reconstructible
    aggregation snapshot.
  - The teacher/data-generation tooling lives in this repository beside the `evaluation/` tree; the
    later Python model-*training* pipeline location remains open.
- **Architecture**: if the eval subsystem warrants standing documentation, add a short
  `docs/Architecture/Architecture.SemanticEvaluation.md` with an Implementation Status section;
  otherwise defer until a second task family joins.
- **Handoff**: update `docs/Handoff.md` when a phase lands and changes a Now/Next/Horizon
  recommendation (pointer only, not content).
- **Do not** cite this plan's phase labels from any of the above; name the behavior.

## Open Questions (surfaced, not silently resolved)

1. **Latency budget number** for the `goal_relevance` contract. This plan now *measures* the
   baseline lexical scorer's p50/p95/p99 on this machine (failure-floor phase), so the baseline
   latency is real. What stays open is the **target budget for a future learned scorer** — the
   precursor's 5–30 ms live-path estimate is still a hypothesis to validate when such a model
   exists; the contract records it as a target, not a measured promise.
2. **Model-artifact hash in the trace/replay compatibility contract.** A later shared-infra stage
   needs to decide whether the immutable model hash joins the trace/replay contract; the contract
   format leaves room for it now without committing.
3. **Where the Python model-*training* pipeline lives** — this repo vs a separately versioned
   producer project. Resolved for this slice: the teacher/data-*generation* tooling lives in this
   repository beside the `evaluation/` tree, versioned with the corpus schema it produces. Only the
   later training pipeline's location remains open; it is out of scope here.
4. **Sanitization specifics** for real-session transcripts entering the corpus (single-operator
   local research project): which rules actually matter. Decided rules are committed in
   `SanitizationRules.md`; the rest are marked open there.
5. **How much of the tech brief's metric menu applies to v1** (per-goal PR curves, macro F1,
   paraphrase consistency, cost-weighted error, calibration). v1 ships the subset the contract
   names; the annotation experience and failure-floor review inform which additions are worth it.
6. **Live-formed-goal slice.** v1 frozen data covers only the seed roster snapshot. Evaluating
   relevance for goals created at runtime (with model-supplied `Normal`-weight keywords) is a
   named limitation recorded in the contract, not resolved here.
