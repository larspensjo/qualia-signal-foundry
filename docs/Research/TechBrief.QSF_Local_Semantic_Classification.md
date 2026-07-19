# Technical Brief: Local Semantic Classification and Routing

## Status

- **Purpose:** Candidate planning input for Tech Lead plans; not an accepted direction or a
  claim about implemented behavior
- **Source inventory:** [Research.TextClassificationInventory.md](Research.TextClassificationInventory.md)
- **Direct precursor:** [Idea.SemanticGoalActivation.md](../Plans/Idea.SemanticGoalActivation.md)
- **Existing constraint:** the 2026-07-04 weighted-goal-activation decision in
  [DecisionLog.md](../DecisionLog.md)
- **Target system:** Qualia Signal Foundry
- **Target runtime:** Windows, Rust, CPU-first with optional NVIDIA GPU acceleration
- **Target language:** English only, including degraded English ASR transcripts
- **Primary constraints:** High quality, low latency, limited training cost, inspectability, and safe failure behavior

---

## 1. Executive Summary

Qualia Signal Foundry currently uses a mixture of exact keyword rules, hand-weighted lexical scoring, model-mediated routing, and OpenAI-backed structured judgments.

The recommended direction is not to replace everything with one local generative model. Instead, build a reusable semantic inference layer around:

1. A shared text normalization pipeline.
2. One shared English text encoder.
3. Small task-specific classifiers or pair scorers.
4. Optional cross-encoder reranking for difficult pairwise decisions.
5. Calibrated confidence and abstention.
6. Deterministic policy that remains outside learned models.
7. Remote OpenAI calls only for open-ended generation, ambiguous cases, and high-value fallback.

The same encoder can support many tasks, but each task should keep its own label set, thresholds, metrics, and failure policy.

The recommended implementation order is:

1. Establish evaluation infrastructure and task contracts.
2. Build shared semantic infrastructure.
3. Pilot goal relevance and semantic retrieval.
4. Add local intent and conversational classifiers.
5. Add local gates in front of remote OpenAI calls.
6. Decompose durable extraction into grounded stages.
7. Improve realtime interruption handling using text plus audio-session metadata.
8. Optimize, calibrate, and operationalize the system.

---

## 2. Goals

### 2.1 Primary goals

- Replace brittle keyword-only decisions where semantic understanding materially improves quality.
- Reduce unnecessary OpenAI API calls.
- Preserve or improve latency on live paths.
- Keep models small enough for CPU execution.
- Optionally use the NVIDIA GeForce RTX 3070 Ti for development, batch processing, or low-latency inference.
- Keep deterministic policies explicit and replayable.
- Add confidence, abstention, versioning, and traceability.
- Build a shared evaluation framework before broad replacement work.
- Support English, including paraphrases, conversational phrasing, and degraded English ASR
  transcripts.

### 2.2 Non-goals

- Training a large language model from scratch.
- Replacing all deterministic logic with neural inference.
- Moving permission, lifecycle, or protected-goal policy into learned models.
- Requiring CUDA for correct operation.
- Treating one global accuracy score as sufficient.
- Automatically promoting low-confidence model outputs into durable state.
- Replacing current production behavior without shadow-mode evaluation.
- Supporting Swedish, code-switching, or general multilingual coverage.

---

## 3. Design Principles

### 3.1 Separate semantic inference from policy

A learned component may predict:

- relevance;
- intent;
- contradiction;
- uncertainty;
- memory-worthiness;
- likely tool choice;
- likely current-information need.

Deterministic Rust policy must continue to decide:

- whether an action is permitted;
- which protected goal tier wins;
- whether a durable write is allowed;
- when review is required;
- how many results enter context;
- whether to call a remote model;
- how to fail when confidence is insufficient.

### 3.2 Prefer a cascade over a single expensive model

Recommended inference cascade:

```text
Deterministic high-precision rules
    ↓
Cheap lexical and embedding scorer
    ↓
Small task-specific classifier
    ↓
Optional cross-encoder
    ↓
Remote OpenAI fallback
```

Not every task needs every stage.

### 3.3 Preserve inspectability

Every learned decision should record:

- stable semantic task name;
- model and tokenizer version;
- immutable model artifact hash;
- normalized input;
- candidate labels or pairs;
- raw scores;
- calibrated confidence;
- threshold and policy version;
- evidence spans or matched examples where available;
- abstention or fallback reason;
- latency;
- hardware execution path;
- final deterministic action.

### 3.4 Optimize for asymmetric error costs

Thresholds must be task-specific.

Examples:

- False interruption cancellation is expensive.
- Missing one weak memory candidate is relatively cheap.
- Incorrect durable memory creation is expensive.
- Incorrect goal contradiction can be very expensive.
- Diagnostics-only attribution errors are low risk.

### 3.5 CPU-first, GPU-optional

Production correctness must not depend on CUDA.

GPU execution may be used for:

- development benchmarks;
- bulk embedding generation;
- sleep processing;
- reranking larger candidate sets;
- model experimentation.

### 3.6 Preserve precursor constraints

The goal-relevance pilot inherits four constraints from
[Idea.SemanticGoalActivation.md](../Plans/Idea.SemanticGoalActivation.md):

- score utterance-to-goal pairs rather than train a classifier over one persona's fixed goals,
  preserving persona-as-data;
- treat the accepted weighted lexical scorer as the no-model fallback and reuse its paraphrase
  probes as the first evaluation harness;
- decide whether the immutable model hash belongs in the trace and replay compatibility contract;
- keep the live-path target in the previously estimated 5–30 ms range and validate the estimate
  on this machine rather than treating it as a promise.

The location and ownership of the Python training pipeline remains an explicit early decision,
not an implied addition to the Rust workspace.

---

## 4. Target Architecture

```text
                         ┌──────────────────────────┐
Raw text + metadata ───► │ Shared normalization     │
                         │ Unicode, ASR, versions,   │
                         │ sentence and token rules  │
                         └─────────────┬────────────┘
                                       │
                       ┌───────────────┴────────────────┐
                       │                                │
              Lexical representation           Semantic representation
              BM25 / char n-grams              Shared English encoder
                       │                                │
                       └───────────────┬────────────────┘
                                       │
                            Task-specific inference
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
             Linear classifier   Pair similarity   Cross-encoder
             or small tree       / retrieval       reranking
                    │                  │                  │
                    └──────────────────┴──────────────────┘
                                       │
                         Calibration and abstention
                                       │
                           Deterministic QSF policy
                                       │
                       Optional remote OpenAI fallback
```

### 4.1 Shared services

Recommended reusable components:

```rust
pub trait TextNormalizer {
    fn normalize(&self, input: &str, context: NormalizationContext)
        -> NormalizedText;
}

pub trait EmbeddingProvider {
    fn embed(&self, input: &NormalizedText) -> Result<Embedding>;
    fn embed_batch(&self, inputs: &[NormalizedText]) -> Result<Vec<Embedding>>;
}

pub trait TaskClassifier {
    fn classify(
        &self,
        task: SemanticTask,
        input: &ClassificationInput,
    ) -> Result<ClassificationResult>;
}

pub trait PairScorer {
    fn score_pairs(
        &self,
        task: SemanticTask,
        query: &str,
        candidates: &[SemanticCandidate],
    ) -> Result<Vec<PairScore>>;
}

pub trait ConfidenceCalibrator {
    fn calibrate(
        &self,
        task: SemanticTask,
        raw_score: f32,
    ) -> CalibratedConfidence;
}
```

### 4.2 Suggested model classes

Use a model ladder rather than committing to one implementation.

| Model class | Suggested use |
|---|---|
| Character n-grams + logistic regression | Very fast baseline, ASR errors, spelling variation |
| Static distilled embeddings | Ultra-fast semantic prefilter |
| Small English sentence encoder | Default shared semantic representation |
| Mini cross-encoder | Pair reranking and subtle relevance |
| Small NLI-style pair model | Contradiction, entailment, compatibility |
| Remote OpenAI model | Open-ended formulation, summarization, ambiguous fallback |

Candidate technologies to benchmark include:

- English MiniLM-class encoders and cross-encoders;
- Model2Vec-style static embeddings;
- ONNX Runtime;
- `fastembed`;
- `tokenizers`;
- `linfa-logistic` or a custom linear head.

Exact model selection should be made through benchmarks, not fixed by this brief.

---

## 5. Inventory-to-Solution Map

| ID | Current responsibility | Recommended direction |
|---|---|---|
| T1 | Final transcript disposition | Multimodal classifier using text, ASR, timing, overlap, and phase metadata |
| T2 | Tool routing | Local multi-label intent router and candidate shortlist |
| T3 | Live memory capture | Semantic intent classifier plus deterministic span extraction |
| T4 | Memory relevance | Hybrid lexical and embedding retrieval with optional reranking |
| T5 | Goal relevance | Utterance-to-goal pair scorer |
| T6 | Opportunity and shaping | Contextual multi-label classifier plus ordinal intensity model |
| T7 | World consultation | Current-information intent classifier, entity extraction, hybrid retrieval |
| T8 | Goal formation and contradiction | Local gate, pairwise contradiction scorer, remote fallback |
| T9 | Sleep session extraction | Separate grounded extraction and classification stages |
| T10 | Article consolidation | Eligibility classifier, extractive evidence selection, optional abstractive compression |
| T11 | Question-to-tension mapping | Reuse goal pair scorer |
| T12 | Project-document retrieval | Hybrid BM25 and semantic retrieval, optional reranking |
| T13 | Influence diagnostics | Sentence-level semantic alignment or entailment diagnostics |

T1–T13 are cross-references into the point-in-time inventory, not stable runtime names. Code,
configuration, traces, and derived durable documents must use behavior names such as
`goal_relevance`, `memory_reranking`, and `goal_formation_gate`. T12 reranking and T13 semantic
influence diagnostics are optional, lowest-priority work because their current deterministic
implementations are comparatively low-risk.

---

## 6. Recommended Delivery Stages

The stage numbers below are local to this candidate brief. Derived plans, experiment specs,
architecture, decisions, code, configuration, and traces must refer to the named behavior, not
these numbers. A derived `Plan.*.md` may define and own its own ephemeral phase numbers.

### Stage 0: Evaluation Foundation and Task Contracts

#### Objective

Create the measurement and data foundation required to compare current rules, local models, and remote models without relying on anecdotes.

#### Scope

Define contracts incrementally by task family. A task family's contract, frozen evaluation data,
failure costs, and current baseline are required before that family's replacement work begins;
the first goal-relevance milestone does not wait for contracts covering every inventory item.

Each contract should include:

- unit of input;
- label space;
- explicit `none`, `ambiguous`, and `abstain` states where appropriate;
- action boundary;
- false-positive and false-negative costs;
- latency budget;
- availability requirements;
- explanation requirements;
- dataset slices;
- primary and secondary metrics;
- promotion and rollback requirements.

#### Required datasets

Create a versioned sanitized corpus with task-specific labels.

Shared conversation data may support:

- T1 acknowledgement, interruption, and noise;
- T2 tool and no-tool intent;
- T3 memory intent and memory type;
- T4 query-to-memory relevance;
- T5 utterance-to-goal relevance;
- T6 uncertainty, contradiction opening, and receptiveness;
- T7 current-information intent and topic spans;
- T8 durable-goal-worthiness and goal-pair relations.

Separate corpora should be created for:

- T9 session extraction;
- T10 article-summary grounding;
- T12 project-document retrieval;
- T13 source-to-reply support.

#### Dataset requirements

Include:

- paraphrase clusters;
- hard negatives;
- explicit and implicit negation;
- quoted speech;
- hypothetical statements;
- subject confusion;
- English;
- varied English conversational styles and dialects;
- punctuation and casing loss;
- real ASR errors;
- synthetic ASR corruption;
- rare high-cost examples;
- long-session recursive-error examples.

Split by session and semantic cluster rather than random utterance.

#### Metrics

| Task family | Primary metrics |
|---|---|
| Classification | Per-class precision, recall, F1, cost-weighted error |
| Retrieval | Recall@k, precision@k, nDCG, MRR, no-result accuracy |
| Goal relevance | Per-goal precision-recall curves, macro F1, paraphrase consistency |
| Ordinal shaping | Weighted kappa, over-steering rate |
| Contradiction and durable writes | High-precision recall, review rate, abstention coverage |
| Extraction | Groundedness, omission rate, duplicate rate, downstream utility |
| Confidence | Reliability curves, Brier score, expected calibration error |
| Runtime | p50, p95, p99 latency; CPU and GPU memory; throughput |

#### Deliverables

- Versioned task-contract set, populated incrementally by task family.
- Annotation guidelines.
- Versioned corpus schema.
- Frozen initial validation and test sets.
- Baseline runner for current implementation.
- Metric report generator.
- Error-analysis report format.
- Dataset privacy and sanitization rules.
- Current-usage telemetry and a frozen baseline for remote calls, input/output tokens, and
  estimated cost per semantic surface, including live goal formation, sleep summarization, and
  article extraction.

#### Exit criteria

- The next task family can be evaluated against its frozen data and current production baseline.
- That task family has explicit failure costs.
- Training, validation, and test data are separated.
- Regressions for that family can be detected automatically.
- No replacement stage starts without its measurable quality, latency, call-volume, token, and
  cost baseline.

#### Dependencies

None.

#### Main risks

- Labels such as durable-goal-worthiness may have low human agreement.
- Rare high-cost classes may initially have insufficient examples.
- Synthetic ASR noise may fail to represent real sessions.

---

### Stage 1: Shared Text and Semantic Infrastructure

#### Objective

Create the reusable runtime and trace infrastructure needed by all local semantic models.

#### Scope

Implement:

- shared Unicode normalization;
- configurable task-specific token preservation;
- version and acronym handling;
- sentence segmentation;
- ASR metadata input;
- lexical feature extraction;
- character n-gram representation;
- shared embedding service;
- batch embedding support;
- embedding cache;
- model registry;
- classifier-head registry;
- confidence calibration;
- abstention;
- trace schema;
- CPU and optional GPU backends.
- reusable grounded source-reference and source-span contracts;
- reusable abstention and review-queue boundary for proposed durable writes.

#### Normalization requirements

The shared normalizer should explicitly define:

- Unicode normalization form;
- lowercasing or case preservation;
- apostrophe behavior;
- punctuation handling;
- dotted versions;
- acronyms;
- short domain terms;
- sentence boundaries;
- original-to-normalized span mapping;
- English ASR locale metadata;
- ASR confidence metadata.

Task-specific configuration may still differ, but differences must be declared rather than reimplemented.

#### Runtime requirements

- Models loaded once per process.
- Thread-safe inference.
- Batch support for sleep and retrieval.
- Precomputation for stable candidates.
- Configurable CPU or CUDA execution.
- Deterministic model identifiers.
- Fail-safe behavior when model loading or inference fails.
- Model assets versioned separately from source code.
- No remote dependency for local inference.

#### Deliverables

- `qsf_semantics` Rust crate or equivalent.
- Shared normalization and span-mapping API.
- Local embedding provider.
- Linear classifier execution.
- Pair-scoring API.
- Calibration and threshold API.
- Trace event schema.
- Benchmark harness.
- Model manifest format.
- CPU and GPU benchmark report.
- Grounded-artifact provenance schema and review/abstention adapter that later durable-write
  work can adopt without waiting for learned extraction.

#### Exit criteria

- One model can be loaded and invoked from Rust.
- Batch and single-input inference are supported.
- Model outputs are fully traceable.
- CPU latency is acceptable for pilot tasks.
- GPU is optional.
- Inference failures produce deterministic fallback behavior.
- Existing systems remain unchanged except for shadow-mode hooks.

#### Dependencies

The evaluation-foundation contract and benchmark fixtures for the first consuming behavior.

#### Main risks

- ONNX model compatibility.
- Tokenizer mismatch between training and Rust inference.
- Embedding cache invalidation.
- GPU launch overhead exceeding CPU latency for small inputs.
- Too much abstraction before a validated pilot exists.

---

### Stage 2: Goal Relevance Pilot

#### Objective

Validate the architecture on T5 before modifying higher-risk durable-state or interruption paths.

#### Why T5 first

- The candidate labels are explicit goals.
- Current keyword behavior is well traced.
- Semantic relevance can be separated from arbitration.
- Existing deterministic logic remains a strong baseline.
- Errors affect shaping but do not directly create durable memories.
- The same pair scorer can later support T11.
- Scoring `(utterance, goal/tension description)` pairs preserves persona-as-data; a fixed
  classifier over Ari's current goals would make persona edits model changes.
- This is a low-risk way to validate shared inference mechanics, while source/provenance and
  review safeguards for the higher-risk durable paths are already being established in parallel.

#### Candidate representations

Each goal should expose:

- ID;
- title;
- summary;
- positive examples;
- hard negative examples;
- optional lexical terms;
- policy metadata kept outside semantic scoring.

#### Models to compare

1. Current weighted keyword scorer.
2. Character n-gram classifier.
3. Shared sentence encoder with cosine similarity.
4. Shared encoder plus learned linear head.
5. Optional cross-encoder pair scorer.

#### Shadow-mode design

For every live utterance, record:

- current keyword result;
- embedding result;
- classifier result;
- optional cross-encoder result;
- final production action;
- latency;
- disagreements;
- confidence.

Production behavior must initially remain controlled by the current implementation.
Shadow inference for live utterances must be asynchronous or otherwise off the response hot path;
it must never delay current-turn selection, arbitration, context injection, or audio dispatch.
Only an explicitly promoted synchronous decision path may spend the agreed live latency budget.

#### Policy boundary

The learned model predicts semantic relevance only.

The existing deterministic system retains:

- protected tiers;
- salience;
- tension priority;
- qualification policy;
- arbitration;
- allowed effects.

#### Deliverables

- Goal description schema.
- Goal example schema.
- Goal-relevance shadow scorer.
- Comparison dashboard or report.
- Per-goal thresholds.
- Calibration report.
- Question-to-tension pair-scoring adapter.
- Migration recommendation.

#### Exit criteria

- Local model improves paraphrase recall.
- Stray-word activation decreases.
- Negation and quoted-speech slices improve or are explicitly escalated.
- p95 live latency remains within budget.
- Thresholds are calibrated per goal or goal family.
- No unacceptable regression on protected-goal cases.
- Tech Lead can decide whether to replace, augment, or retain the current scorer.

#### Dependencies

The goal-relevance evaluation contract and the shared semantic infrastructure.

#### Main risks

- Goal descriptions may be too abstract.
- Existing labels may encode policy rather than semantic relevance.
- Dynamically formed goals may lack good examples.
- Similar goals may need pairwise disambiguation.

---

### Stage 3: Hybrid Semantic Retrieval

#### Objective

Improve T4 and T12 using a shared lexical and semantic retrieval stack.

#### Scope

##### T4 memory retrieval

Persist or cache:

```text
embed(title + summary + tags)
```

Compute:

- lexical score;
- semantic similarity;
- association score;
- importance;
- recency;
- reinforcement;
- identity-specific policy evidence.

Initially combine features with an explicit formula. Later fit a small ranking or logistic model using human relevance judgments.

##### T12 project-document retrieval

Use:

- BM25 or equivalent lexical retrieval;
- semantic embedding retrieval;
- heading and path metadata;
- optional top-k reranking;
- deterministic authority and maturity metadata.

Search relevance and authority must remain separate.

#### Optional reranking

Use a cross-encoder only on the top lexical or embedding candidates.

Example:

```text
All memories or documents
    ↓
Lexical + vector retrieval
top 20
    ↓
Cross-encoder
top 5
    ↓
Deterministic context budget
```

#### Deliverables

- Shared hybrid retrieval API.
- Memory embedding index.
- Project-document embedding index.
- Index versioning and rebuild command.
- Shadow comparison against current retrieval.
- Ranking evaluation reports.
- Optional reranker integration.
- Deterministic fallback to lexical retrieval.

#### Exit criteria

- Recall@k and nDCG improve on held-out judgments.
- No-result accuracy is acceptable.
- Retrieval remains inspectable.
- Index rebuild behavior is deterministic.
- Existing importance, recency, associations, and authority remain visible.
- CPU latency is within target.
- Fallback works without semantic model availability.

#### Dependencies

The relevant retrieval evaluation contract and shared semantic infrastructure. Goal-relevance
experience should inform pair scoring but is not a hard dependency.

#### Main risks

- Semantic similarity may over-retrieve broadly related but irrelevant memories.
- Existing associations may amplify weak semantic matches.
- Index updates may add lifecycle complexity.
- Document authority may be confused with relevance.

---

### Stage 4: Local Intent and Conversational Classifiers

#### Objective

Replace narrow cue tables in lower-risk stable-label tasks with small local classifiers.

#### Scope

Target tasks:

- T2 tool intent;
- T6 conversational opportunity and shaping inputs;
- T7 current-information intent;
- selected T3 live memory intents.

#### T2 tool intent

Implement a multi-label router with:

- one label per tool family;
- explicit no-tool label;
- legitimate multi-tool cases;
- candidate tool descriptions;
- local shortlist passed to the response model.

Initial deployment should not force the local prediction. It should:

- improve tool advertisement;
- reduce irrelevant tools;
- provide observability;
- detect likely missed tools.

#### T6 opportunity and shaping

Separate:

##### Multi-label semantic predictions

- uncertainty;
- contradiction opening;
- receptiveness;
- explicit invitation to explore;
- resistance or rejection.

##### Ordinal prediction

- none;
- low;
- medium;
- high.

The classifier should see the current utterance and, where useful, the previous assistant turn.

#### T7 world consultation

Separate:

- current-information intent;
- entity or topic spans;
- exact version detection;
- article retrieval;
- source relevance;
- anti-repeat and latency policy.

Capitalization must no longer be the primary named-entity mechanism.

This work has direct live evidence, not only a predicted failure mode. In the 2026-07-19
real-corpus voice run, ASR lowercased `high bandwidth memory` past the capitalization gate and
winner-takes-the-turn arbitration let `serve-the-present-person` crowd out a weak world-goal
match; no consultation was requested
([Experiment.WorldConsultation.md](../Experiments/Experiment.WorldConsultation.md)). This makes
current-information intent and topic/entity extraction the highest-priority items in this stage.

#### T3 memory intent

Local classification may detect:

- states user name;
- assigns assistant name;
- explicitly requests remembering;
- explicitly revokes or negates remembering.

Extraction should remain deterministic or span-based, with high confidence required for automatic durable writes.

#### Deliverables

- Task-specific classifier heads.
- Per-label calibration.
- Local tool-shortlisting adapter.
- Opportunity and receptiveness model.
- Current-world intent model.
- Memory-intent detector.
- Shadow-mode reports.
- Safe rollout switches.

#### Exit criteria

- Current keyword baseline is beaten on hard negatives and paraphrases.
- Per-label thresholds are calibrated.
- No-tool precision is acceptable.
- T6 over-steering decreases.
- T7 recall improves under lowercase and ASR text.
- T3 does not lower durable-write precision.
- Remote response quality does not regress due to excessive tool filtering.

#### Dependencies

The relevant task-family contracts and shared semantic infrastructure. The goal-relevance pilot
provides useful classifier patterns.

#### Main risks

- Multi-label tool intent may be difficult to annotate.
- Previous-turn context may increase runtime and data complexity.
- T3 extraction may remain the dominant failure point after intent improves.
- Tool shortlist errors can hide useful tools.

---

### Stage 5: Remote LLM Gating and Pairwise Coherence

#### Objective

Reduce OpenAI usage for T8–T10 without immediately replacing open-ended generation.

#### Scope

Implement local high-recall gates before remote calls.

#### T8 goal-worthiness gate

Classify whether an exchange contains plausible evidence for a durable autonomous goal.

The gate should reject clear negatives:

- acknowledgements;
- ordinary factual questions;
- temporary operational requests;
- corrections;
- casual statements;
- unrelated discussion.

Positive and ambiguous cases continue to OpenAI.

#### T8 contradiction candidate generation

Use local pair scoring to classify goal pairs as:

- contradictory;
- operationally competing;
- redundant;
- compatible;
- uncertain.

Initially use the model only to shortlist pairs. Deterministic lifecycle policy and remote fallback remain authoritative.

#### T9 sleep-call gate

Detect sessions or sections that contain no plausible:

- memory candidates;
- decisions;
- open questions;
- future hints.

Do not skip remote processing until recall is demonstrated on frozen data.

#### T10 article gate

Classify article eligibility based on:

- content quality;
- factual density;
- relevance;
- novelty;
- duplication;
- source and ingestion metadata.

The existing length and per-run rules may remain as deterministic preconditions.

#### Cost accounting

Record:

- calls avoided;
- false negatives;
- fallback frequency;
- latency changes;
- cost per accepted durable artifact;
- cost per useful classification;
- model disagreement.

#### Deliverables

- Goal-formation local gate.
- Goal-pair relationship scorer.
- Sleep session-content gate.
- Article-eligibility classifier.
- OpenAI fallback adapter.
- Cost and latency dashboard.
- Rollout policy with conservative thresholds.

#### Exit criteria

- API-call, token, and estimated-cost reduction meets the target agreed against the frozen
  current-usage baseline on held-out and shadow traffic.
- High recall for durable-goal and memory-worthy cases.
- No increase in semantically invalid durable actions.
- Pair scorer reliably removes clearly compatible goal pairs.
- All uncertain cases still reach remote judgment.
- Model failure falls back safely.

#### Dependencies

The gating/coherence evaluation contracts, shared semantic infrastructure, and frozen current
OpenAI-usage baseline; experience from the earlier pilots is preferred but not required.

#### Main risks

- A high-recall gate may save little cost.
- A cost-effective threshold may miss rare important cases.
- Remote model behavior may change over time.
- Local and remote errors may be correlated.

---

### Stage 6: Grounded Durable Extraction

#### Objective

Reduce hallucination and recursive error propagation in T3, T9, and T10 by grounding durable artifacts in source spans.

#### Scope

Replace broad single-pass extraction with staged processing.

#### T3 live memory capture

Pipeline:

```text
Intent classification
    ↓
Source-span extraction
    ↓
Entity or topic validation
    ↓
Confidence and policy check
    ↓
Durable write or review
```

For names:

- support multiword names;
- support lowercase and non-Latin names;
- retain exact source span;
- distinguish identity from transient state;
- permit correction and revocation.

For remember requests:

- distinguish the user's text from the assistant content being remembered;
- store provenance;
- avoid automatically promoting unsupported assistant claims.

#### T9 sleep extraction

Separate stages:

```text
Transcript
  ├─ candidate span detection
  ├─ memory-worthiness classification
  ├─ open-question classification
  ├─ decision classification
  ├─ future-context classification
  ├─ deduplication
  └─ optional summarization of accepted spans
```

Store source evidence before generating abstractions.

#### T10 article consolidation

Pipeline:

```text
Article cleanup
    ↓
Sentence segmentation
    ↓
Claim-bearing sentence selection
    ↓
Novelty and relevance scoring
    ↓
Grounded durable record
    ↓
Optional OpenAI compression
```

Persist:

- source URL;
- article identifier or hash;
- selected source sentences;
- generated summary if any;
- model version;
- confidence;
- trust tier;
- attribution.

#### Deliverables

- Grounded candidate schema.
- Source-span mapping.
- Span extraction implementation.
- Claim-bearing sentence scorer.
- Semantic duplicate detection.
- Review queue or abstention handling.
- Migration path for existing records.
- Artifact verification tests.

#### Exit criteria

- Every new durable artifact has source evidence.
- Automatic durable writes meet high-precision targets.
- Paraphrased duplicates are reduced.
- Generated summaries can be checked against retained evidence.
- Bad extraction does not silently overwrite or reinforce previous state.
- Sleep output categories can be evaluated independently.

#### Dependencies

The durable-extraction evaluation contracts, shared provenance/review infrastructure, and the
relevant intent or span-classification component. Remote-call gating is optional: grounded
article extraction and other source-evidence work may proceed independently.

#### Main risks

- Span annotations are more expensive than document-level labels.
- Extractive records may be less readable than generated summaries.
- Deduplication across paraphrases may create false merges.
- Existing storage schemas may need migration.

---

### Stage 7: Realtime Turn Disposition

#### Objective

Improve T1 using real session evidence rather than a larger text-only allow-list.

#### Why this stage is later

T1 has high interaction cost and depends on evidence not represented by text alone.

Relevant inputs include:

- ASR transcript;
- ASR confidence;
- utterance duration;
- overlap with assistant audio;
- time since assistant speech began;
- time since assistant speech ended;
- current turn phase;
- pause duration;
- partial transcript history;
- acknowledgement probability;
- imperative or stop intent.

#### Suggested model

A small classifier over:

- transcript embedding;
- character n-gram features;
- scalar timing and ASR features;
- phase one-hot features.

Possible model forms:

- logistic regression;
- gradient-boosted trees;
- small multilayer perceptron;
- calibrated ensemble.

No large audio model is required initially.

#### Label space

- start new turn;
- acknowledgement or continuation noise;
- interrupt;
- explicit stop;
- ambiguous.

Explicit stop phrases should retain deterministic high-priority handling.

#### Data collection

Use recorded metadata and sanitized transcripts from real realtime sessions.

Capture false-positive and false-negative outcomes:

- assistant response cancelled unnecessarily;
- user interruption ignored;
- harmless acknowledgement treated as interruption;
- ASR hallucination treated as speech;
- meaningful continuation discarded.

#### Deliverables

- Realtime event dataset.
- Timing-feature schema.
- Realtime turn-disposition classifier.
- Replay simulator.
- Cost-weighted threshold tuning.
- Shadow and canary rollout.
- Immediate rollback switch.

#### Exit criteria

- Accidental cancellation rate decreases.
- Missed meaningful interruptions do not increase beyond the agreed limit.
- ASR hallucination robustness improves.
- p99 decision latency remains negligible relative to the realtime pipeline.
- Failure falls back to a conservative deterministic rule.

#### Dependencies

The turn-disposition evaluation contract, shared semantic infrastructure, and sufficient real
session data.

#### Main risks

- Labels may depend on subjective conversational intent.
- User behavior may change across languages and speaking styles.
- Provider ASR behavior may change.
- Offline replay may not capture the perceived live interaction.

---

### Stage 8: Production Hardening and Continuous Improvement

#### Objective

Turn successful pilots into a maintainable production semantic subsystem.

#### Scope

- model registry and compatibility policy;
- frozen compatibility suite;
- rollout and rollback tooling;
- threshold versioning;
- model asset distribution;
- startup and warmup behavior;
- monitoring;
- drift detection;
- active learning;
- periodic calibration;
- privacy controls;
- cost tracking;
- regression gates;
- trace replay.

#### Active-learning loop

Prioritize annotation for:

- local and remote disagreement;
- predictions near thresholds;
- durable-action candidates;
- user corrections;
- underrepresented English dialect, accent, and ASR-error slices;
- new ASR error patterns;
- newly formed goals;
- model upgrade regressions.

#### Upgrade policy

Block model, prompt, tokenizer, or threshold changes unless they pass:

- frozen task benchmarks;
- high-cost error slices;
- latency budgets;
- memory limits;
- calibration checks;
- trace schema compatibility;
- rollback readiness.

#### Deliverables

- Production model registry.
- Compatibility test suite.
- Release checklist.
- Drift and disagreement reports.
- Annotation queue.
- Model-card format.
- Automated benchmark pipeline.
- Operational runbook.

#### Exit criteria

- Every model change is versioned and benchmarked.
- Production decisions can be replayed from stored outputs and policy versions.
- Drift is observable.
- Rollback is fast and deterministic.
- API savings and local runtime costs are measured continuously.
- Training data lineage is documented.

#### Dependencies

All prior behavior foundations applicable to the promoted components.

#### Main risks

- Operational complexity exceeds model complexity.
- Shadow traces consume excessive storage.
- Training data becomes biased toward uncertain examples.
- Calibration drifts when upstream ASR or remote models change.

---

## 7. Recommended Delivery Order

### Recommended sequence

```text
Evaluation foundation
    ↓
Shared semantic infrastructure and early durable-write safeguards
    ↓
Goal relevance pilot
    ↓
Hybrid retrieval
    ↓
Local intent classifiers
    ↓
Remote LLM gating
    ↓
Grounded durable extraction
    ↓
Realtime turn disposition
    ↓
Production hardening
```

The inventory ranks durable extraction immediately after evaluation because false positives
persist and recursively re-enter later classification. This brief still pilots goal relevance
first because it offers a reversible, low-risk test of shared inference and evaluation mechanics.
It does not defer all durable-write safety: source/provenance contracts plus review and abstention
infrastructure belong in the shared foundation and can be adopted alongside goal-relevance and
retrieval work. Only the learned, annotation-heavy extraction decomposition remains later.

### Parallelization opportunities

After the shared semantic infrastructure exists:

- Goal relevance and hybrid retrieval can partially overlap.
- T12 document retrieval can proceed independently of T4 memory storage changes.
- T2, T6, and T7 classifiers can be developed as separate local-intent workstreams.
- T8 contradiction scoring can begin before T9 and T10 gates.
- Grounded article extraction can proceed separately from live-memory extraction.
- As soon as the shared contracts exist, current durable-write adapters can retain available
  source/provenance and route unsupported or abstained proposals to review; this safeguard does
  not depend on completing learned extraction.

### Do not parallelize prematurely

Avoid broad parallel replacement before:

- task contracts exist;
- trace schemas are stable;
- model loading is shared;
- evaluation data is frozen;
- fallback behavior is agreed.

---

## 8. Suggested Repository Structure

```text
crates/
  qsf_semantics/
    src/
      normalization/
      embeddings/
      lexical/
      classifiers/
      pair_scoring/
      calibration/
      traces/
      model_registry/
      runtime/
  qsf_semantic_eval/
    src/
      datasets/
      metrics/
      reports/
      replay/
      slices/
models/
  manifests/
  tokenizers/
  classifiers/
  calibrators/
evaluation/
  schemas/
  annotations/
  frozen/
  reports/
```

Domain crates should own policy adapters, not duplicate model runtime logic.

Example:

```text
qsf_volition
    owns goal policy and arbitration
    calls qsf_semantics for goal relevance

qsf_memory
    owns memory lifecycle and ranking policy
    calls qsf_semantics for lexical and semantic evidence

qsf_realtime_server
    owns turn cancellation and realtime timing
    calls qsf_semantics for turn-disposition predictions
```

---

## 9. Data Contracts

### 9.1 Classification result

```rust
pub struct ClassificationResult {
    pub task: SemanticTask,
    pub model_version: String,
    pub model_artifact_hash: String,
    pub labels: Vec<LabelScore>,
    pub selected: Option<String>,
    pub abstained: bool,
    pub fallback_reason: Option<String>,
    pub latency_micros: u64,
    pub hardware_path: HardwarePath,
    pub evidence: Vec<Evidence>,
}
```

### 9.2 Pair score

```rust
pub struct PairScore {
    pub candidate_id: String,
    pub lexical_score: Option<f32>,
    pub embedding_score: Option<f32>,
    pub model_score: Option<f32>,
    pub calibrated_confidence: Option<f32>,
    pub evidence: Vec<Evidence>,
}
```

### 9.3 Durable semantic artifact

```rust
pub struct GroundedArtifact {
    pub artifact_type: ArtifactType,
    pub normalized_value: String,
    pub source_refs: Vec<SourceReference>,
    pub source_spans: Vec<TextSpan>,
    pub confidence: f32,
    pub model_version: String,
    pub policy_version: String,
    pub trust_tier: TrustTier,
    pub review_status: ReviewStatus,
}
```

---

## 10. Model Training Strategy

### 10.1 Default approach

Use frozen embeddings with lightweight heads.

```text
Input text
    ↓
Frozen 384-dimensional embedding
    ↓
Logistic regression or small linear layer
    ↓
Calibrated probability
```

Advantages:

- minimal training resources;
- fast iteration;
- simple versioning;
- CPU-friendly inference;
- easy Rust execution;
- task-specific thresholds;
- shared encoder across tasks.

### 10.2 When to use character n-grams

Use for:

- T1;
- short command and acknowledgement classes;
- ASR-corrupted text;
- names and spelling-sensitive patterns;
- narrow stable intents.

Character features may be concatenated with embeddings.

### 10.3 When to fine-tune an encoder

Fine-tune only when frozen embeddings fail on well-defined recurring error classes.

Possible triggers:

- similar goals cannot be separated;
- English paraphrase, dialect, or ASR robustness is inadequate;
- negation remains poor;
- domain-specific terminology is not represented;
- cross-encoder quality is necessary but too slow.

Use parameter-efficient or contrastive fine-tuning rather than full model training.

### 10.4 Remote model as teacher

Remote OpenAI outputs may accelerate labeling, but should not be accepted as ground truth without review.

Recommended process:

```text
Production or synthetic example
    ↓
OpenAI proposal
    ↓
Human accept or correct
    ↓
Versioned training example
    ↓
Local model training
```

---

## 11. Deployment and Rollout Strategy

### 11.1 Standard progression

Every learned replacement should move through:

1. Offline evaluation.
2. Replay evaluation.
3. Shadow mode.
4. Decision support without authority.
5. Conservative canary.
6. Gradual authority increase.
7. Full rollout with fallback.
8. Periodic recalibration.

### 11.2 Required feature flags

Examples:

```text
QSF_SEMANTIC_MODE=off|shadow|assist|authoritative
QSF_SEMANTIC_DEVICE=cpu|cuda|auto
QSF_GOAL_RELEVANCE_MODEL_VERSION=...
QSF_MEMORY_RERANKER_ENABLED=true|false
QSF_GOAL_FORMATION_LOCAL_GATE_ENABLED=true|false
```

New semantic components must default to an exercised code path. The initial default is `shadow`,
with component-specific shadow execution enabled; it records predictions but leaves production
authority with the existing implementation. An `off` mode remains an explicit rollback and
diagnostic option, not the shipped default.

### 11.3 Failure policy

On local inference failure:

- do not invent a prediction;
- emit a trace;
- use the existing deterministic implementation;
- preserve current safe behavior;
- avoid durable writes unless another trusted path approves them.

---

## 12. Hardware and Performance Expectations

### CPU

Design live classifiers for CPU execution.

Likely suitable workloads:

- one short utterance embedding;
- a few classifier heads;
- precomputed candidate comparisons;
- reranking a small candidate set;
- character n-gram inference.

### NVIDIA RTX 3070 Ti

Useful for:

- batch embedding generation;
- development benchmarks;
- larger sleep batches;
- cross-encoder evaluation;
- experimentation with FP16 models.

GPU should not be required for correctness.

The precursor estimates a 5–30 ms live pair-scoring budget and a few hundred MB of inference
VRAM for a MiniLM-class model on the available 8 GB card. Treat both as hypotheses for the
benchmark harness; promotion requires measured p95/p99 latency, cold-start behavior, and peak
memory on the actual runtime.

### Quantization

Benchmark:

- FP32 CPU;
- dynamic INT8 CPU;
- FP16 GPU;
- static embedding alternatives.

Select based on measured end-to-end latency, not theoretical throughput.

---

## 13. Planning Guidance for the Tech Lead

Each detailed phase plan should include:

- explicit inventory IDs covered;
- current baseline behavior;
- task contract;
- dataset requirements;
- implementation components;
- migration strategy;
- shadow-mode design;
- metrics and thresholds;
- safety and fallback behavior;
- rollout stages;
- dependencies;
- operational ownership;
- exit criteria;
- rollback plan;
- unresolved decisions;
- a trace completeness contract whenever validation relies on traces: required fields, the
  authoritative artifact boundary, and a test that parses the artifact and proves the fields;
- explicit external human-testing recommendations and what evidence they should collect;
- documentation updates required by
  [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md), including a Decision Log entry when
  the plan records a committed direction.

Each plan should avoid selecting a model before the benchmark design is agreed.

---

## 14. Key Decisions to Resolve Early

1. Which English dialect, accent, and ASR-error slices are mandatory at first release?
2. Which live latency budgets apply to each task?
3. Which durable actions require human or remote-model confirmation?
4. How should sanitized realtime data be collected?
5. Which inference runtime will be the standard Rust backend?
6. How are model assets distributed and versioned?
7. How much shadow trace storage is acceptable?
8. Which tasks may share training data?
9. Which tasks require per-label calibration?
10. Which tasks are permitted to abstain?
11. What level of API-cost reduction is considered successful?
12. Which current rules remain permanent high-precision overrides?
13. Does the immutable model artifact hash participate in trace replay and fixture compatibility?
14. Does the Python training/export pipeline live in this repository or in a separately versioned
    producer project?

---

## 15. Recommended First Concrete Milestone

The first end-to-end milestone should include:

- goal-relevance task contract and frozen evaluation slices from the evaluation foundation;
- minimal shared embedding runtime;
- T5 goal descriptions and examples;
- current keyword scorer as baseline;
- one English sentence encoder;
- one static or character-based baseline;
- shadow-mode trace collection;
- a held-out goal relevance report;
- a clear replace, augment, or reject decision.

This milestone validates the architecture while limiting risk and implementation scope.

---

## 16. Final Recommendation

Adopt a shared semantic infrastructure, but not a universal classifier.

Use:

- deterministic rules for exact high-confidence policy;
- character and lexical models for narrow robust classification;
- one shared English encoder for semantic representation;
- task-specific lightweight heads for stable labels;
- cross-encoders only for difficult candidate pairs;
- OpenAI only for open-ended formulation, summarization, or ambiguous fallback;
- deterministic Rust code for all action, permission, lifecycle, and protected-goal policy.

The proposed order prioritizes measurement first, establishes durable-write provenance and review
safeguards early, then uses low-risk semantic pilots to validate the shared machinery. Retrieval
and observed world-intent failures follow, then cost reduction, grounded learned extraction, and
the high-interaction-cost realtime disposition path.
