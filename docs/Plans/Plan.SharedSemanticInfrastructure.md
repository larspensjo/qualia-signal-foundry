# Plan: Shared Semantic Infrastructure — the Local Semantic Inference Runtime

Status: Proposed — not started
Maturity: Candidate
Area: Semantic inference runtime / Model assets / Benchmarking

## Why this plan exists

Qualia Signal Foundry has committed to measuring, before replacing, its lexical semantic
decisions (`Plan.SemanticEvaluationFoundation.md`, first phase landed 2026-07-19). The next
piece of the tech brief `docs/Research/TechBrief.QSF_Local_Semantic_Classification.md` (a
candidate brief — planning input, not commitment) is the reusable **local semantic inference
runtime**: one English sentence encoder loadable and runnable from Rust, with batch and
single-input inference, fully traceable outputs, measured CPU latency / cold-start / peak
memory on this machine, and deterministic, *loudly observable* fallback on inference failure.
The upcoming goal-relevance pilot (the brief's model-comparison ladder over the production
weighted lexical scorer) and later semantic behaviors consume this runtime; nothing in this
plan changes existing systems.

A deliberate sequencing caution shapes this plan: **the evidence that a semantic model is
needed at all does not exist yet.** The lexical failure-floor experiment and the frozen
human-reviewed datasets are still pending in the parallel evaluation track. The runtime spike
is cheap and runs regardless; the heavier deliverables (pair scoring, classifier heads,
calibration, registry hardening) are sequenced behind an explicit stop/go gate so a null
failure-floor result can cheaply stop the effort.

### Naming and ephemerality

This document owns its ephemeral phase labels. Durable artifacts — the crate, its modules,
trace record types, manifests, decision-log entries — name stable behaviors
(`semantic_inference`, `goal_relevance`, `embedding`, `pair_scoring`) and never cite a brief
"stage" or a plan "phase" number (`Agents.md`; `ProjectWorkflow.md`).

## Current baseline behavior and task-contract linkage

- **Production baseline**: the weighted lexical scorer in `crates/qsf_volition/src/selection.rs`
  with the tokenizer in `crates/qsf_volition/src/terms.rs` (accepted design, DecisionLog
  2026-07-04). It is the no-model fallback for every future learned scorer and is graded
  through its public API by `crates/qsf_semantic_eval` (DecisionLog 2026-07-19). Nothing in
  this plan touches it.
- **Task contract**: `evaluation/contracts/GoalRelevance.TaskContract.md` (versioned, active)
  defines the pair unit, labels, baseline prediction contract, the 5–30 ms latency
  *hypothesis*, and the trace obligations for the first consuming behavior. This plan's
  runtime is shaped so a pilot can satisfy that contract; it does not modify the contract.
- **Benchmark fixtures**: the frozen roster snapshot
  (`evaluation/frozen/goal-relevance/realtime-seed.roster.json`) and sample dataset
  (`evaluation/frozen/goal-relevance/sample.dataset.jsonl`) supply realistic goal-description
  and utterance texts for runtime benchmarks. (Their gold labels are pilot territory; this
  plan uses only the texts.)

## Scope

**In scope** (everything the goal-relevance pilot consumes, nothing more):

- Shared text normalization (an *addition* — the production `qsf_volition` tokenizer cannot
  change; DecisionLog 2026-07-19).
- Local embedding provider (one English MiniLM-class encoder), single + batch inference,
  CPU-only.
- Character n-gram representation.
- Linear classifier-head execution — fixture-scoped (no trained model exists yet), plus the
  committed head/calibrator **artifact interchange format**.
- Pair scoring over `(utterance, candidate-description)` pairs, persona-as-data.
- Calibration and abstention API — fixture-scoped.
- Semantic inference trace schema (the trace-emission seam is decided here).
- Model manifest + download/verify workflow; deterministic model identifiers.
- Benchmark harness; measured CPU latency, cold-start load time, peak memory on this machine.
- Precomputation/memoization of stable candidate (goal-description) embeddings with an
  explicit invalidation story.
- Loud, observable failure behavior: the crate proves "inference ran" vs "every attempt
  failed"; the caller's fallback execution is its own declared trace obligation.

**Deferred (no consuming behavior yet)**: grounded-artifact provenance schema and the
review/abstention boundary for durable writes; a general embedding cache; cross-encoder
execution; GPU/CUDA backend (the runtime abstraction is shaped so a CUDA execution provider
can be added later without redesign — a `hardware_path` trace field and a backend-owned
session — but no CUDA code is built); shadow-mode wiring into live surfaces (see Open
Question 4); multilingual anything (English only, including degraded English ASR transcripts).

## Design commitments made by this plan

Settled inputs from the design brief are honored as-is (scope, spike-decided runtime,
CPU-only, manifest-based assets, fixture-scoped heads + committed interchange format). The
following are the concrete design commitments this plan adds. They become project commitments
only when the corresponding Decision Log entries land (see "Documents to create or update").

1. **Crate name and boundary: `crates/qsf_semantics`, a lean crate.** It owns normalization,
   embedding, n-gram features, pair scoring, classifier-head execution, calibration, the
   trace record types, and manifest/registry parsing. It depends only on `engine_logging`
   and third-party crates (serde, sha2, thiserror, the chosen inference runtime, `tokenizers`
   as applicable). It must **not** depend on `qsf_app`, `qsf_realtime_server`, `qsf_volition`,
   or `qsf_semantic_eval` — domain crates own policy adapters and call `qsf_semantics`, never
   the reverse (DecisionLog 2026-06-10, 2026-07-09, 2026-07-19). The crate owns no runtime
   state and no policy: learned inference is an effect-layer concern producing data for pure
   reducers/selectors, and deterministic policy (thresholds acted on, arbitration, fallback
   *decisions*) stays in the caller.
2. **Trace seam: the crate returns trace-record values; callers own persistence.** Every
   inference API returns a **`Traced<T>` outcome** — a serde-serializable
   `SemanticTraceRecord` paired with a `Result<T, SemanticFailure>` — so the trace carrier
   is defined for failures exactly as for successes; an `Err` can never be traceless. The
   crate never writes into `RunContext`/`TraceRecord` or
   `DiagnosticWriter`/`DiagnosticRecord`; consumers embed the record value into their own
   backend, mirroring how `ModelInvoker` (DecisionLog 2026-07-01) kept `qsf_models` free of
   both. Rationale over a writer trait: every semantic call is synchronous and already
   returns a value; a record-in-the-result keeps the crate pure-data at the boundary, keeps
   the unidirectional flow (effects produce data), and gives the artifact-parsing test a
   well-defined canonical serialization. The crate's own harness and tests write the records
   as `semantic-trace.jsonl` — the authoritative artifact for this plan's trace contract.
3. **The immutable model artifact hash joins the trace/replay compatibility contract — and
   has one canonical definition.** `model_artifact_hash` is the versioned canonical
   **artifact digest** (`artifact_digest_v1`): sha256 over a canonical serialization
   (sorted-key, whitespace-free JSON) of every inference-affecting manifest field — the
   model file's sha256, the tokenizer file's sha256, the pooling spec, max sequence length,
   embedding dimension, truncation policy, and output-normalization flag. Two asset sets
   that can produce different outputs can therefore never share a hash, while cosmetic
   manifest edits (source URLs, license text) do not change it. The digest scheme is
   versioned; the crate computes it from the manifest, and `ManifestFormat.md` documents
   the recipe. The digest is a required field of every `SemanticTraceRecord`, and any
   consumer persisting the record for replay inherits it. Replaying or comparing semantic
   outputs across runs is meaningful only per artifact digest. This makes the resolution of
   the long-open replay question explicit (brief key decision 13; Open Question 2 of
   `Plan.SemanticEvaluationFoundation.md`).
4. **Loud observability is a first-class runtime surface — counting what the crate can
   truthfully observe: inference failures, not fallbacks.** The crate returns an inference
   failure; the **caller** owns and executes the deterministic fallback, so only the caller
   can attest a fallback ran. The split: the runtime handle exposes a queryable
   `SemanticRuntimeStatus` — model id, artifact digest, load state
   (`NotLoaded | Loading | Ready | Failed(reason)`), load duration, inference attempt
   count, success count, and failure counts by reason — which makes "every inference
   attempt failed this session" (attempts > 0, successes == 0) a *provable* state, not an
   inference from absence. Load failures and the first inference failure of a process are
   logged as errors through `engine_logging` with model id, asset path, and operation. The
   complementary caller obligation is stated here and inherited by the pilot plan: a
   consumer that executes its deterministic fallback must record that fallback execution in
   its own trace/diagnostics alongside the crate's failure record, and its diagnostics
   surface should display the crate's status counters. Together these answer the repo's
   silent-fallback incident class (2026-07-03 mock-judge entry) at the infrastructure
   level; wiring the status into live diagnostics panels is pilot work.
5. **Default tests exercise real inference via a tiny committed test model.** A genuinely
   small ONNX encoder (target well under ~200 KB: 2-layer, small vocab, same input/output
   signature and pooling as the real encoder) plus a matching real `tokenizer.json` are
   committed under `models/testdata/`, with the one-off generator script and a provenance
   README beside them. A fresh checkout's `cargo test` runs the full
   tokenize→ONNX→pool→normalize path — not the failure path — satisfying "defaults must
   exercise the new code path" despite gitignored real assets. The failure path is *also*
   tested deliberately (missing assets, hash mismatch). Real MiniLM-class assets remain
   operator-fetched for benchmarks and the pilot. (The generator script may be a one-off
   Python script; committing it as fixture provenance does **not** decide the open Python
   training-pipeline location — it is a one-shot artifact generator, and this is stated in
   its README.)
6. **Normalization per ladder rung is declared, so model-vs-baseline differences are never
   confounded with normalizer differences.** The rung-to-normalizer mapping (details in the
   Architecture section): the production lexical scorer keeps its own tokenizer untouched;
   the char n-gram rung uses the new shared normalizer under a declared versioned config; the
   encoder rungs see minimally normalized text (Unicode NFC + whitespace collapse only) with
   the encoder's own `tokenizer.json` normalizer authoritative beyond that. Every trace
   record carries a `normalizer_id`.
7. **Tokenizer fidelity is defined against the model's own `tokenizer.json` interpreted by
   the Rust `tokenizers` crate** — not against a Python reference. Fidelity means: candidate
   runtimes produce identical token ids, attention masks, special tokens, and truncation for
   a committed probe set, and embeddings agreeing within tolerance (per-probe cosine
   ≥ 0.999 at FP32) across runtimes. If runtimes disagree and adjudication needs an external
   reference, a one-off comparison against Python `sentence-transformers` may be run and
   recorded in the spike report as an explicit escalation — not a standing dependency
   (keeping the deliberately open Python-environment question open).
8. **Candidate-embedding precomputation is content-addressed memoization; invalidation
   belongs to the domain adapter; no candidate is silently unscored.** The store key is
   sha256 over the canonical (sorted-key, whitespace-free JSON) encoding of the structured
   tuple `{ key_scheme: "candidate_key_v1", model_artifact_hash, normalizer_id,
   candidate_text }` — an unambiguous encoding, never raw string concatenation (no
   delimiter-collision ambiguity). A changed description, normalizer, or model can
   therefore never serve a stale embedding — staleness is structurally impossible; old
   entries are garbage, not lies. The default behavior is
   **embed-on-demand-with-memoization** (a cold candidate — e.g. a live-formed goal admitted
   mid-session — is embedded at first scoring, which is acceptable because pilot shadow
   scoring runs off the hot path), with a batch `warm()` the adapter may call at load. The
   domain/policy-adapter crate owns the candidate set and its lifecycle (admission,
   retirement, persona swap — 2026-07-01/2026-07-03 lifecycle decisions); `qsf_semantics`
   owns only the content-addressed store. Every pair-score trace record carries
   `candidate_embedding_source: precomputed | computed_now | unavailable` with a reason when
   `unavailable` — a live-formed goal is either scored (and marked `computed_now`) or loudly
   recorded as unscored, never silent. This is deliberately *not* the deferred general
   embedding cache: it is scoped to pair-scoring candidates, in-memory, with persistence
   deferred until a consumer needs it.
9. **Asset workflow: manifest + fetch + verify, launcher-integrated.** Versioned manifests
   under `models/manifests/` (schema-versioned JSON: deterministic `model_id`, source URLs
   per file, sha256 per file, embedding dimension, max sequence length, pooling spec,
   license); assets download into gitignored `models/assets/` via a `qsf_semantics` CLI
   `fetch` subcommand that verifies hashes. A separate **read-only `verify` subcommand**
   (no network, no writes) re-checks manifest/asset presence and hashes, emits
   machine-readable output with distinct exit codes for missing / valid / corrupt, and is
   backed by the same registry verification code the loader uses. `scripts/qsf.ps1` gains a
   `models` command wrapping `fetch`, and its `doctor` check calls `verify` — never the
   mutating fetch path and never duplicated PowerShell hash logic (the
   `OPENAI_API_KEY`-style prerequisite pattern). Load-time verification re-checks hashes,
   so a tampered or partial asset is a typed, loud failure. Any candidate runtime that
   insists on managing its own downloads outside this manifest discipline is disqualified
   or wrapped (spike constraint).

## Repository placement

- **`crates/qsf_semantics`** — the lean runtime crate (modules named by behavior:
  `normalization`, `embedding`, `ngram`, `pair_scoring`, `classification`, `calibration`,
  `trace`, `registry`), with a `qsf_semantics` binary exposing `fetch`, read-only
  `verify`, and `bench` subcommands.
- **`models/`** (top-level):
  - `models/manifests/` — committed versioned manifests + `ManifestFormat.md` (the Rust
    types remain authoritative; the doc explains them — the `evaluation/schemas/` precedent).
  - `models/assets/` — gitignored downloaded model assets (`.gitignore` gains this entry).
  - `models/classifiers/` — committed head/calibrator artifacts (the fixture head now; real
    trained heads later) + `HeadArtifactFormat.md`.
  - `models/testdata/` — the tiny committed test model, tokenizer, generator script,
    provenance README, and the tokenizer-fidelity probe set.
- **Benchmark artifacts** — generated under `runs/<run-id>/` per the normal boundary;
  deliberately frozen benchmark reports are copied into `evaluation/reports/` stamped with
  machine/build profile, model artifact hash, and code commit (DecisionLog 2026-07-19
  evaluation-tree rule).

The perf benchmark harness lives in `qsf_semantics` and reads plain probe-text fixtures
(derived from the frozen roster-snapshot descriptions and sample-dataset utterances, with
provenance noted) rather than importing `qsf_semantic_eval` schema types — importing them
would create `qsf_semantics -> qsf_semantic_eval -> qsf_volition`, reversing the
domain-calls-semantics boundary. Quality/ladder comparison over gold labels is pilot work and
belongs in `qsf_semantic_eval` (which may then depend on `qsf_semantics`; that direction is
fine).

## Architecture and API shape

Mirroring the brief's Section 4.1 traits, adapted to the commitments above:

- `SemanticRuntime::configure(manifest_path, model_id) -> SemanticRuntimeHandle` — cheap,
  I/O-free construction (manifest parse only). Loading is decoupled from construction:
  `handle.ensure_loaded()` or lazy first-use load, so the consuming realtime server can load
  off its readiness path (the ~17 s corpus-ingest readiness problem, `docs/Handoff.md`
  Horizon, must not be repeated). The handle is `Send + Sync` with synchronous inference.
  Guidance to consumers (stated here, wired in the pilot): the realtime server loads in a
  background task after port bind; offline `qsf_app` runs load on first use.
- `Traced<T>`: the universal inference outcome — `{ record: SemanticTraceRecord,
  result: Result<T, SemanticFailure> }` (commitment 2). Every inference API below returns
  it, so failures carry their trace record by construction.
- `EmbeddingProvider`: `embed(&self, text) -> Traced<Embedding>` and
  `embed_batch(&self, texts) -> Traced<Vec<Embedding>>`.
- `NgramFeaturizer`: deterministic hashed character n-gram vectors (default n = 3..=5,
  2^18 dims, fixed seed — defaults exercise the path via the fixture head and unit tests).
- `PairScorer`: `score_pairs(task, utterance, candidates) -> Traced<Vec<PairScore>>` where
  a candidate is `{ id, description_text }` — never a fixture-bound persona structure
  (persona-as-data, DecisionLog 2026-07-19).
- `TaskClassifier` + `ConfidenceCalibrator`: execute a loaded head artifact over features
  assembled by `PairFeatureBuilder` strictly per the artifact's `pair_feature_spec` (see
  the pair-feature contract below); calibrate raw scores; apply the artifact's abstention
  band producing an explicit `Abstain` outcome with reason.
- `SemanticRuntimeStatus`: the loud-observability surface (commitment 4 — attempt/success/
  failure counters by reason, load state, health record).

### Pair-feature contract (deterministic head inputs)

The pilot's learned rungs (char n-gram classifier; encoder + linear head) need one
deterministic recipe turning an `(utterance, candidate_description)` pair into a head
input. That recipe is defined by the head artifact, never invented at a call site — it is
part of the interchange contract with the future training pipeline. The head artifact
carries a versioned **`pair_feature_spec`**: an ordered list of feature blocks, each with a
type, dimensionality, and config. The crate's `PairFeatureBuilder` assembles the input
strictly in spec order and fails loudly (`IncompatibleHead`) on any dimension or config
mismatch. Block types in scope for this plan:

- `utterance_embedding` / `candidate_embedding` — dense, embedding-dim, L2-normalized per
  the model's pooling spec;
- `elementwise_product` and `absolute_difference` of the two embeddings — dense,
  embedding-dim; the standard pair-interaction features for a linear head over a
  bi-encoder;
- `utterance_ngrams` / `candidate_ngrams` — sparse hashed char n-grams represented as
  `(u32 index, f32 value)` pairs over the block's declared dimension; values are term
  frequencies with a declared optional L2 normalization; n-range, dimensions, and hash
  seed come from the block config (matching the featurizer defaults or declaring their
  own).

Dense blocks concatenate in spec order; a sparse block is executed as a sparse dot product
against that block's weight slice. Candidate-description construction — how a goal's
title/summary/tension text becomes the single `candidate_text` string — is **owned by the
domain adapter / pilot schema**, declared there, and pinned in traces by
`candidate_content_hash`; the crate takes one canonical text string per candidate and
never composes persona structures itself.

Normalization per ladder rung (commitment 6):

| Ladder rung | Normalization seen |
|---|---|
| Production weighted lexical scorer | `qsf_volition::normalize_terms` — untouched, production-owned |
| Char n-gram classifier | shared normalizer, declared versioned config (`shared_v1`: Unicode NFC, lowercase, apostrophe-preserving, punctuation-stripping — exact policy fixed in its phase) |
| Encoder + cosine / encoder + head | minimal cleanup only (`encoder_min_v1`: Unicode NFC + whitespace collapse); the encoder's own `tokenizer.json` normalizer is authoritative beyond that |

## Trace completeness contract — `semantic_inference` trace contract

Named by behavior; the pilot and any durable document refer to this contract, never to a plan
phase. Every `SemanticTraceRecord` is produced via the `Traced<T>` carrier (commitment 2),
so a failed call yields a record exactly like a successful one. Required fields:

- `operation` — the discriminator: `embed | embed_batch | pair_score | classify`
- `task` (stable behavior name, e.g. `goal_relevance`; benchmark runs use `runtime_benchmark`)
- `model_id`, `model_artifact_hash` (the canonical artifact digest — commitment 3),
  `tokenizer_ref`
- `normalizer_id`
- the operation's input identity: for embed operations, `input_text` +
  `input_content_hash`; for pair scoring and classification, `utterance_text` +
  `utterance_content_hash` **and**, per candidate, `candidate_id` +
  `candidate_content_hash` — both sides of every scored pair are pinned, not just an
  ambiguous single input. (Consumers may drop raw texts at persistence per their own
  privacy rules; the content hashes always survive.)
- operation-specific payload: embedding dimension + L2-norm flag for embed calls; per
  candidate for pair scoring: `embedding_score`, `candidate_embedding_source`
  (`precomputed | computed_now | unavailable` + reason); for classification: label scores,
  `selected`, `calibrated_confidence`, `abstained`, `threshold_artifact_version`, and the
  `pair_feature_spec` version in force
- `failure_reason` (typed: `AssetsMissing | HashMismatch | LoadFailed |
  TokenizationFailed | InferenceFailed | IncompatibleHead`) — present exactly when the call
  failed; the record itself always exists
- `latency_micros`, `hardware_path` (always `cpu` in this plan), `schema_version`

Artifact boundary:

```text
semantic-trace.jsonl (runs/<run-id>/…):
  Authoritative structured record stream — canonical serde serialization of
  SemanticTraceRecord, written by the crate's harness and tests. Consumer backends
  (RunContext traces, DiagnosticRecords) embed the same record type; verifying those
  embeddings is pilot-plan work.

benchmark metrics JSON:
  Derived structured measurements (latency percentiles, cold-start, peak memory).

human-readable benchmark summary:
  Derived view; never authoritative.
```

Artifact-parsing verification: an automated test runs an end-to-end inference, writes
`semantic-trace.jsonl`, re-parses it, and asserts every required field above exists with the
right shape — including `model_artifact_hash` and, for a deliberately failed call,
`failure_reason` on a fully-formed record. `SemanticTraceRecord` and this test land with the
benchmark harness (asset-gated at first); once the tiny committed test model exists the test
runs un-gated by default. The trace criterion is not marked complete until generated
artifacts satisfy this.

## Experiments

None of this plan's phases is a consciousness-simulation mechanism experiment (2026-07-04
scope decision): runtime selection, asset plumbing, featurizers, and benchmarks are routine
engineering whose outcome is not in doubt, carried by code, tests, and commits. The genuine
mechanism questions live elsewhere: the lexical failure floor
(`Experiment.GoalRelevanceLexicalFailureFloor`, evaluation-track plan) and "does a semantic
model beat the lexical scorer" (pilot). No `Experiment.*.md` is created here.

---

## Phase 1 — Runtime spike: one encoder under `ort` and `fastembed`, measured, decided

The cheap phase that runs regardless of the failure-floor outcome. It resolves the brief's
key decision 5 (standard Rust inference runtime) by measurement, not taste.

**Work**

- Create `crates/qsf_semantics` with the `EmbeddingProvider` trait and two backend
  implementations behind cargo features `backend-ort` (`ort` + `tokenizers`) and
  `backend-fastembed` (`fastembed` in **user-supplied-model mode** — loading exactly the
  manifest-fetched files; if `fastembed` cannot cleanly run user-supplied assets, it is
  disqualified or wrapped, per the settled constraint). Both features on during the spike.
- Minimal manifest v1 under `models/manifests/` for the spike encoder(s) + the `fetch`
  subcommand (download, sha256 verify, atomic move into `models/assets/`); `.gitignore`
  entry for `models/assets/`.
- Suggested spike candidates (Open Question 1 — exact set confirmed at spike time, choice
  recorded in the spike report): `sentence-transformers/all-MiniLM-L6-v2` ONNX export as the
  primary MiniLM-class candidate; optionally `BAAI/bge-small-en-v1.5` as a second data point.
- `SemanticTraceRecord`, the `Traced<T>` outcome shape, and the canonical
  `artifact_digest_v1` computation land **here** — the harness needs all three. The
  harness writes `semantic-trace.jsonl` per the `semantic_inference` trace contract, and a
  first artifact-parsing test (asset-gated until the tiny test model exists) asserts the
  required fields.
- Benchmark harness v1 (`bench` subcommand), per backend, CPU-only, on this machine:
  cold-start load time (construction + load to first-inference-ready), single
  short-utterance embed p50/p95 after warmup, batch embed throughput (e.g. 64 texts), peak
  working set (Windows process counters). Probe texts derived from the frozen roster
  descriptions and sample-dataset utterances. Emits `semantic-trace.jsonl` + metrics JSON +
  summary under `runs/<run-id>/`. The metrics artifact records the measurement environment
  alongside the numbers: Rust toolchain version, runtime crate versions
  (`ort` / `fastembed` / `tokenizers`), model export provenance (source URLs, per-file
  hashes, artifact digest), and CPU model/core count.
- Tokenizer-fidelity checks per commitment 7: committed probe set (casing loss, punctuation
  loss, ASR-style degradation, contractions, long input at/over max length,
  empty/whitespace) with tests asserting token-id identity and cross-runtime embedding
  agreement. These tests require fetched assets, so they are `#[ignore]`-gated and run
  explicitly during the spike (`cargo test -p qsf_semantics -- --ignored`); Phase 2's tiny
  test model gives the un-gated default coverage.
- **Decide the winner** on: manifest/hash-compatible asset loading (hard requirement),
  tokenizer fidelity, latency/cold-start/peak memory, dependency weight and maintenance
  posture, and API fit for a lazy `Send + Sync` handle. Freeze the spike benchmark report
  into `evaluation/reports/` (stamped: machine/build profile, model artifact hashes, code
  commit) and write the runtime Decision Log entry.

**Verification (automated)**: `cargo build` (repo root); `cargo test -p qsf_semantics`
(manifest parsing, artifact-digest determinism, hash-verify failure cases, probe-set
structure); **both backend feature configurations built and tested explicitly**
(`--no-default-features --features backend-ort`, `--no-default-features --features
backend-fastembed`, and both together); a standing **dependency-boundary test** (via
`cargo metadata` or `Cargo.toml` parsing) asserting `qsf_semantics` depends on none of
`qsf_app`, `qsf_realtime_server`, `qsf_volition`, `qsf_semantic_eval`; ignored fidelity and
artifact-parsing tests green when run with fetched assets;
`cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Human testing (required — this phase's product is a measurement)**: the operator runs
`cargo run -p qsf_semantics -- fetch …` then `… bench …` on this machine for both backends.
Evidence to collect: the frozen benchmark report (cold-start, p50/p95, peak memory per
backend), fidelity-test outcomes, and any cross-runtime disagreement notes.

---

## Phase 2 — Committed embedding runtime: lazy loading, loud failure observability, tiny test model

The smallest slice a consumer could actually use. After this phase the crate is a real,
observable, fail-safe embedding runtime.

**Work**

- Remove the losing backend and its feature flag; the winner compiles unconditionally
  (default path exercises it).
- `SemanticRuntime` handle per the Architecture section: cheap construction, loading
  decoupled (`ensure_loaded()` + lazy first use), `Send + Sync`, thread-safe synchronous
  inference; load-time manifest hash re-verification.
- `encoder_min_v1` minimal input cleanup (NFC + whitespace collapse), `normalizer_id` in
  every record.
- Loud observability per commitment 4: `SemanticRuntimeStatus` (attempt/success/failure
  counters by reason), startup health record, `engine_logging` error logs carrying model
  id, asset path, and operation.
- The `semantic_inference` artifact-parsing test becomes **un-gated and default**: tiny
  test model end-to-end, including a deliberately failed call whose `Traced` record
  carries `failure_reason` on a fully-formed record.
- Tiny committed test model per commitment 5 (`models/testdata/`: ONNX + `tokenizer.json` +
  generator script + provenance README); default `cargo test` runs real inference.
- Failure-path tests: missing assets, hash mismatch, malformed model → typed error, status
  `Failed(reason)`, failure counters incremented, error logged — and no invented
  prediction.
- Read-only `verify` subcommand per commitment 9 (no network, no writes; machine-readable
  output; distinct exit codes for missing / valid / corrupt), sharing the loader's registry
  verification code.
- Launcher integration: `qsf.ps1 models` (wrapping `fetch`) and a `doctor` check that
  calls `verify` — never the mutating fetch path; launcher help text updated. Underlying
  `fetch`/`verify` stay independently runnable (launcher conventions, DecisionLog
  2026-05-22, 2026-06-08).
- Decision Log entries land: asset manifest/fetch discipline (commitment 9); trace seam +
  model-hash-in-replay (commitments 2–3); crate boundary (commitment 1, folded into
  whichever entry fits best).

**Verification (automated)**: `cargo build`; workspace `cargo test` green on a tree with
**no** fetched assets (proves defaults exercise real inference via the test model and that
missing real assets are a typed loud state, not a silent one); artifact-parsing trace test
green; a test asserting construction does no model I/O and load is deferred; launcher tests
in `scripts/qsf.Tests.ps1` covering the `models` command and `doctor`'s three asset states
(missing, valid, corrupt — corrupt via a tampered-asset fixture), all driven through the
read-only `verify` path; clippy + fmt.

**Human testing (recommended)**: fresh-checkout simulation — delete `models/assets/`, run
`cargo test` (green), run `qsf.ps1 doctor` (reports assets missing), run `qsf.ps1 models`,
re-run `doctor` (reports valid). Evidence: doctor output before/after; a bench run's status
snapshot showing `Ready` with attempts == successes and zero failures, and a no-assets bench
attempt showing the loud failure.

---

## Stop/go gate — failure-floor checkpoint

Before Phase 3 begins, consult the lexical failure-floor experiment result from the parallel
evaluation track (`Plan.SemanticEvaluationFoundation.md`'s failure-floor phase /
`Experiment.GoalRelevanceLexicalFailureFloor`):

The gate is **one boundary**: nothing after Phase 2 lands until the failure-floor result
exists (this matches the Dependencies section — the result gates Phases 3–5, and Phase 6
follows whichever phases were built).

- **Null result** (the lexical scorer's paraphrase/morphology/negation floor is acceptable):
  **stop after Phase 2; Phases 3–5 are not built** (the shared normalizer, n-gram
  featurizer, pair scorer, and head execution serve only the pilot ladder and have no other
  consumer yet). Phases 1–2 stand on their own — a measured runtime decision, a reusable
  observable embedding runtime, and frozen benchmark evidence that any later semantic
  behavior (e.g. retrieval) would need anyway. Run Phase 6's report-freezing and
  documentation-closure steps in truncated form, record the stop in the Decision Log entry
  that cites the failure-floor evidence, and update `docs/Handoff.md`.
- **Failure floor confirmed** (a semantic ladder is justified): proceed to Phases 3–6.
- **Result not yet available**: Phases 1–2 may complete meanwhile; **Phases 3–5 wait** —
  no discretionary early start.

---

## Phase 3 — Shared normalizer and character n-gram representation

**Work**

- `TextNormalizer` trait + the `shared_v1` configuration: explicit Unicode normalization
  form (NFC), lowercasing, apostrophe behavior (preserve intra-word apostrophes),
  punctuation handling, whitespace policy — each an explicit declared config field, not an
  implicit behavior; versioned `normalizer_id`. This is an **addition**; no existing
  tokenizer changes, and the rung table in the Architecture section is restated in the crate
  docs so future rungs must declare their normalization.
- `NgramFeaturizer`: hashed character n-grams (default n = 3..=5, 2^18 dims, fixed
  documented seed), deterministic across runs and platforms.
- Original-to-normalized span mapping is **not** built (its consumer — grounded provenance —
  is deferred); the config format leaves room for it.

**Verification (automated)**: `cargo build`; unit tests for every declared normalization
rule including ASR-degraded inputs (casing/punctuation loss); determinism test for the
featurizer (fixed input → fixed sparse vector, stable hash seed); a test asserting the
encoder path does *not* route through `shared_v1` (confound guard); clippy + fmt.

**Human testing**: none required.

---

## Phase 4 — Pair scoring and candidate-embedding memoization

**Work**

- `PairScorer` with cosine similarity over L2-normalized embeddings; single-utterance ×
  N-candidates and batch shapes; `pair_score` trace records per the `semantic_inference`
  contract — `utterance_content_hash` plus per-candidate `candidate_id`,
  `candidate_content_hash`, and `candidate_embedding_source`.
- `CandidateEmbeddingStore` per commitment 8: content-addressed, in-memory,
  embed-on-demand-with-memoization default, batch `warm()`, unavailable-case loud in the
  record. A doc-comment states the invalidation ownership rule: the domain adapter owns the
  candidate set; the store cannot serve stale content.
- Benchmark harness extension: `bench pairs` — one short utterance against the frozen
  roster's ~7 candidate descriptions, warm (all `precomputed`) and cold (all
  `computed_now`), p50/p95 on this machine. This is the first direct evidence against the
  5–30 ms live-pair-scoring **hypothesis** (never a promise — precursor
  `Idea.SemanticGoalActivation.md`; contract records it as a target).

**Verification (automated)**: `cargo build`; tests — cosine correctness against
hand-computed fixtures (tiny model); memoization behavior (second score of the same
candidate is `precomputed`; changed description text yields a new key and `computed_now`;
changed model hash likewise); trace records for warm/cold/unavailable cases parse and carry
the required fields; clippy + fmt.

**Human testing (recommended)**: with real fetched assets, run `bench pairs` and a
paraphrase sanity probe (a paraphrase of a roster description should out-score an unrelated
utterance — a mechanical sanity check, not a quality gate; quality is pilot territory).
Evidence: warm/cold pair-scoring percentiles vs the 5–30 ms hypothesis, recorded for the
consolidated report.

---

## Phase 5 — Linear-head execution, calibration/abstention, and the interchange format

Fixture-scoped: no trained model exists; the training-pipeline location is deliberately open.
This phase builds proven execution code plus the artifact contract that the future (still
unlocated) Python training pipeline must produce.

**Work**

- **Head/calibrator artifact interchange format** (`models/classifiers/*.head.json` +
  `HeadArtifactFormat.md`): schema_version; task behavior name (e.g. `goal_relevance`);
  base `model_id` + `model_artifact_hash` (the canonical artifact digest) the head was
  trained against (checked at load — mismatch is a typed `IncompatibleHead` failure, never
  a silent wrong-basis prediction); the versioned ordered **`pair_feature_spec`** per the
  pair-feature contract (Architecture section) — block types, order, dimensions, sparse
  representation, and per-block config; weights + bias (with the weight layout keyed to the
  spec's block order); label space; calibrator (`none | platt | temperature` with params);
  thresholds including an abstention band; provenance (training-data version, date).
  Content-hashed and referenced from the model manifest (registry grows head/calibrator
  references here).
- `PairFeatureBuilder` + `TaskClassifier` execution: features assembled strictly per the
  artifact's `pair_feature_spec` → linear head → raw scores; `ConfidenceCalibrator` →
  calibrated confidence; abstention band → explicit `Abstain` with reason. Deterministic
  policy on what to *do* with an abstention stays with the caller.
- **Hand-authored fixture heads + calibrator** with known weights and expected outputs,
  committed as the default test artifacts — at least one dense-interaction spec
  (embeddings + elementwise product + absolute difference) and one spec including a sparse
  n-gram block, so both assembly paths have pinned expected outputs (defaults exercise the
  classify path end-to-end over the tiny test model).
- Decision Log entry: the interchange format is the committed contract between this crate
  and the future training pipeline (its location stays open).

**Verification (automated)**: `cargo build`; tests — both fixture heads produce the
expected scores/confidences/abstentions exactly; feature-block dimension mismatch, block
config mismatch, and base-model-digest mismatch each fail loudly as `IncompatibleHead`;
`PairFeatureBuilder` assembly order is pinned by a fixture (reordering blocks changes the
output and fails the expected-value test); classification trace records satisfy the
`semantic_inference` contract (label scores, `calibrated_confidence`, `abstained`,
`threshold_artifact_version`, `pair_feature_spec` version); artifact round-trip +
content-hash verification; clippy + fmt.

**Human testing**: none required (fixture-scoped by design).

---

## Phase 6 — Consolidated benchmark report, exit-criteria check, documentation closure

**Work**

- Consolidated CPU benchmark run with real assets: cold-start load, single/batch embed,
  warm/cold pair scoring, head + calibration overhead, peak working set; optionally dynamic
  INT8 vs FP32 if the chosen runtime makes it cheap (brief Section 12) — optional, recorded
  if run. Freeze into `evaluation/reports/` (stamped), superseding the spike report as the
  runtime's reference numbers.
- Compare measurements against the 5–30 ms pair-scoring hypothesis and record the informed
  (still non-fixed — Open Question 3) latency picture for the pilot.
- Exit-criteria checklist (below) walked and evidenced.
- Documentation closure: Decision Log entries all landed; `docs/Handoff.md` recommendation
  updated (pointer only); decide the Architecture question — recommendation: **defer**
  `Architecture.SemanticInference.md` until the pilot proves the subsystem (workflow Stage 8
  promotes architecture from results, and today there is no consuming behavior), recording
  that deferral here rather than in the decision log.

**Verification (automated)**: full workspace `cargo build` + `cargo test`;
`cargo clippy --all-targets -- -D warnings`; `cargo fmt`; a `git status`-level check that no
existing crate outside the new surfaces changed (existing-systems-unchanged exit criterion).

**Human testing (required)**: the operator runs the consolidated benchmark and reviews the
frozen report. Evidence: the report itself plus the doctor/status outputs showing a healthy
default environment.

---

## Metrics and thresholds

Measured (this machine, CPU, frozen in `evaluation/reports/`): cold-start load time;
single-utterance embed p50/p95; batch embed throughput; warm and cold pair-scoring p50/p95
for a ~7-candidate roster; classifier-head + calibration overhead; peak working set;
tokenizer-fidelity outcomes. Thresholds deliberately **not** fixed by this plan: the 5–30 ms
live pair-scoring budget stays a hypothesis the benchmarks inform (Open Question 3); no
quality metrics exist here (quality is graded by the pilot against the frozen sets and task
contract). The one hard gate is structural: a runtime that cannot load manifest-verified
user-supplied assets is disqualified (Phase 1).

## Safety and fallback behavior

On any local inference failure: no invented prediction; a typed error inside a fully-formed
`Traced` record carrying `failure_reason`; the caller's existing deterministic
implementation (for goal relevance, the production weighted lexical scorer) remains the
behavior of record. No remote dependency exists for local inference — the crate performs
network I/O only in the explicit `fetch` subcommand, never during inference or `verify`.
Failure is loud per commitment 4: the crate's attempt/success/failure counters, health
record, and error logs make "every inference attempt failed this session" provable from
crate status alone; proving "the caller then fell back" is the caller's own trace
obligation (commitment 4), inherited by the pilot plan. The crate owns no policy and no
durable writes; nothing it emits can reach durable state except through a domain adapter's
own guarded path.

## Rollout notes

This plan ships **no live-surface wiring**: existing systems remain byte-identical, so there
is no shadow/assist/authoritative mode to flag here. The brief's `QSF_SEMANTIC_MODE=shadow`
default and per-component shadow execution belong to the plan that wires a consumer (Open
Question 4); when that happens, the default must exercise the new code path (shadow on by
default, `off` an explicit rollback) per Agents.md and the brief's rollout section. At the
crate level, "defaults exercise the new path" is satisfied now: default builds compile the
chosen runtime unconditionally and default tests run real inference through it.

## Migration strategy

None. New crate, new `models/` tree, new launcher subcommand; no existing behavior, schema,
or state changes. The `qsf_semantic_eval` crate and `evaluation/` tree are read as fixtures
only.

## Dependencies

- **Satisfied**: `evaluation/contracts/GoalRelevance.TaskContract.md`; the frozen roster
  snapshot and sample dataset (benchmark fixture texts); the lean-crate precedents.
- **Parallel-track (gating Phases 3–5 only)**: the lexical failure-floor experiment result.
- **New third-party**: the winning inference runtime (`ort` or `fastembed`), `tokenizers`
  (if `ort` wins), a download client for the `fetch` binary; license review at adoption
  (expected Apache-2.0/MIT throughout).
- **Hardware**: this machine, CPU-only (RTX 3070 Ti explicitly unused in this plan).

## Exit criteria (whole plan)

- One MiniLM-class English encoder loads and runs from Rust through the spike-chosen,
  decision-logged runtime; batch and single-input inference both supported.
- Construction is cheap and loading is decoupled/lazy behind a `Send + Sync` handle
  (consumer readiness paths are not forced to block on model load).
- Model outputs are fully traceable: generated artifacts satisfy the `semantic_inference`
  trace contract, verified by an artifact-parsing test, with the canonical artifact digest
  in every record — including records for failed calls (the `Traced` carrier).
- CPU latency (single, batch, pair warm/cold), cold-start load time, and peak memory are
  measured on this machine and frozen in `evaluation/reports/`.
- Inference failure produces a typed deterministic error, no invented prediction, and loud
  observability (attempt/success/failure counters, health record, error logs — "every
  attempt failed" is provable from crate status); a fresh checkout without fetched assets
  has green default tests that exercise real inference (tiny committed model) and a loud,
  doctor-visible state (read-only `verify`) for missing or corrupt real assets.
- Pair scoring over persona-as-data candidates works with content-addressed memoization; no
  candidate can be silently unscored.
- Head execution + calibration/abstention run against the fixture artifacts, and the
  head/calibrator interchange format is committed and documented.
- Existing systems remain unchanged (no modifications outside the new crate, `models/`,
  launcher additions, `evaluation/reports/`, `.gitignore`, and docs).
- If the failure-floor gate stopped the plan early: Phases 1–2 exit criteria hold, the stop
  is recorded, and the truncation is visible in the Handoff and Decision Log.

## Rollback plan

The crate is consumed by nothing in production; rollback is "do not consume it." Individual
phases revert cleanly (no state or schema migrations). Fetched assets are deletable
(`models/assets/` is gitignored); the launcher `models`/`doctor` additions are additive. If
a later pilot regresses, the deterministic lexical scorer and its frozen roster remain the
rollback path per the task contract's promotion/rollback section.

## Documents to create or update (`ProjectWorkflow.md`)

- **Create** `crates/qsf_semantics` (crate + tests + `fetch`/`verify`/`bench` binary), the
  `models/` tree (`manifests/` + `ManifestFormat.md`, gitignored `assets/`, `classifiers/`
  + `HeadArtifactFormat.md`, `testdata/` with provenance README), and this plan.
- **Update** `.gitignore` (`models/assets/`), `scripts/qsf.ps1` (`models` command, doctor
  check via `verify`, help text), and `scripts/qsf.Tests.ps1` (launcher tests for `models`
  and the doctor asset states).
- **Freeze** the spike and consolidated benchmark reports into `evaluation/reports/`.
- **Decision Log entries** (proposed by this plan; committed only when they land):
  1. The standard local inference runtime (winner + measured rationale + the
     manifest-compatibility disqualification rule) — after the spike.
  2. Model assets are distributed by versioned manifest + fetch/verify into a gitignored
     assets tree, with the tiny-committed-test-model rule keeping default tests on the real
     inference path; the lean `qsf_semantics` crate boundary and dependency direction.
  3. The semantic trace seam returns `Traced` record values to callers, and the immutable
     model artifact hash — under its canonical, versioned digest definition — joins the
     trace/replay compatibility contract.
  4. The head/calibrator artifact interchange format is the committed contract with the
     future (separately located) training pipeline.
  5. If the failure-floor gate stops the effort: an entry recording the stop and the
     evidence.
- **Handoff** (`docs/Handoff.md`): pointer updates when a phase or the gate changes a
  Now/Next/Horizon recommendation.
- **Architecture**: `Architecture.SemanticInference.md` deliberately deferred until the
  pilot proves the subsystem (recorded in Phase 6, not the decision log).
- **Do not** cite this plan's phase numbers from any durable artifact; name the behaviors
  (`semantic_inference` trace contract, `goal_relevance`, the runtime/asset/interchange
  decisions).

## Open Questions (surfaced, not silently resolved)

1. **Which encoder model(s) the spike benchmarks.** Suggested default:
   `all-MiniLM-L6-v2` ONNX export, optionally `bge-small-en-v1.5` as a second point; the
   exact set is confirmed when the spike runs and recorded in the spike report. Selection is
   by measurement, never fixed by this plan.
2. **Model-artifact hash in the trace/replay contract** — *resolved by this plan's design*
   (commitment 3: it is a required trace field and joins the replay contract), pending only
   the Decision Log entry that commits it. Listed here because it was a deliberately open
   question upstream; the resolution is explicit, not silent.
3. **The latency budget for a future learned scorer.** The contract records 5–30 ms as a
   target hypothesis; this plan's benchmarks inform it but deliberately do not fix it. The
   pilot sets the operating budget from measured evidence.
4. **Where shadow-mode hook wiring into live surfaces lands.** This plan's working
   assumption: the goal-relevance pilot plan owns shadow-mode design and therefore the hook
   wiring, keeping this plan's "existing systems unchanged" strict. If the user prefers the
   hooks in this plan (the brief's Stage-1 exit criteria permit "shadow-mode hooks"), a
   wiring phase would be appended after Phase 4 — confirm before implementation reaches that
   point.
5. **Python training-pipeline location** (this repo vs a separate producer project) — out of
   scope; the interchange format (Phase 5) is this plan's only contact with it. The one-off
   test-model generator script does not decide it.
6. **Persistence of the candidate-embedding store.** In-memory-only here; whether the pilot
   (or a later retrieval stage) needs a persisted, versioned embedding store — and its
   invalidation-on-disk story — is deferred to the first consumer that restarts often enough
   to care.
