# Idea: Semantic Goal Activation via a Local Specialized Model

## Status

Brainstorm

## Summary — What We Are Trying to Solve

Goal activation and arbitration in `qsf_volition` are **lexically brittle**. Two mechanisms
compound:

1. **Activation is a binary token gate** (`selection.rs::matched_keywords`): a goal participates
   in a turn only if the utterance contains an exact activation keyword. A paraphrase that
   carries the same meaning but different tokens activates nothing.
2. **Arbitration ignores match strength** (`arbitration.rs`, sort key
   `(biased_tier asc, base_priority desc, goal_id asc)`): the winner is the lowest-tier goal
   that matched *at all*. One near-stopword at a protected tier beats a five-term, on-topic
   match at a malleable tier.

Live evidence (voice session 2026-07-04, recorded in
`Experiment.CuriosityPersonaSeed.md` / `Experiment.LiveGoalFormationAndCoherence.md` Results):

- "Do you believe machines will replace many jobs, and **what** does that **do** to the
  economy?" — `track-the-ai-transition` matched five terms (`ai, automation, replace, jobs,
  economy`) yet lost arbitration to `serve-the-present-person`, which matched only `what`/`do`.
- "For **what** it's worth, the hospital near me just started using AI scribes…" — the same
  protected goal won on a stopword inside an idiom.

The consequence: semantically identical phrasings take different volitional paths, and running
the human voice tests degenerates into engineering phrases around a ~30-word stoplist. At that
point the tests measure the keyword table, not the persona.

**Robustness, defined:** (a) *paraphrase invariance* — same meaning in different words selects
the same goal; (b) *stray-word immunity* — an incidental common word does not flip the winner.

This idea proposes closing the gap with a **small, locally-run, specialized relevance model**:
per trusted turn, score the utterance against each tension/goal and feed those scores into
selection and arbitration, replacing (or gating) the binary keyword match. Inference runs on
the local GPU (NVIDIA GeForce RTX 3070 Ti, 8 GB) so it fits the hot-path latency budget; the
model is small enough to be trained or fine-tuned in-house on the same card.

## Why This Matters

- The volition thesis is that the persona is *felt* in conversation. If which goal speaks
  hinges on stopwords, the causal chain stays trace-legible but semantically arbitrary — the
  opposite of felt.
- Volition injection currently costs 0 ms on the hot path, and the experiments gate on latency
  parity. A remote model call (hundreds of ms to seconds, plus tail risk) is disqualified for
  per-turn routing; a local specialized model at ~5–30 ms is the only model-shaped option that
  fits the budget.
- A *pair scorer* (utterance × tension summary) — rather than a classifier over this persona's
  seven goals — preserves the "persona swaps are data-only" property established when mode bias
  moved into fixture data: new personas need new fixture text, not a retrained model.
- Independence from network and provider on the hot path keeps live sessions self-contained.

## What the Model Does

One scoring call per trusted turn:

```text
input:  utterance text  ×  { per goal: activation summary (goal/tension text from the fixture) }
output: relevance score per goal  (plus, optionally, per-term attributions for the trace)
```

Selection consumes scores instead of (or in addition to) binary matches; arbitration requires a
minimum relevance before a goal can claim the win, with tier ordering applying among qualified
goals. Protected-floor semantics (never cancelled, never press past a decline) are untouched —
this changes who *speaks*, not who is *protected*.

## Design Dimensions and Options

### What is scored (the generalization question)

- **Pair scorer over (utterance, tension/goal summary)** — generalizes to any persona whose
  fixture carries good summaries; nothing persona-specific is baked into weights. *Strongly
  preferred for the persona-as-data property.*
- **Fixed-class classifier over the current seven goals** — simplest to train, highest accuracy
  ceiling for this one persona, but every persona edit invalidates the model. Rejected unless
  the pair scorer proves too weak.

### Model architecture

- **Bi-encoder (embedding model)** — embed goal summaries once at fixture load; per turn, embed
  the utterance and take dot products. One forward pass per turn regardless of goal count.
  MiniLM-class (22–110 M params) is the natural size.
- **Cross-encoder** — one forward pass per (utterance, goal) pair; more accurate, cost scales
  with goal count (7 today, fine; larger goal sets less so).
- **Bi-encoder retrieval + cross-encoder rerank of the top few** — the standard compromise if
  the bi-encoder alone is too coarse near the arbitration threshold.

### Training ladder (measure before training)

1. **Zero-shot baseline:** an off-the-shelf multilingual sentence-embedding model, no training
   at all. Build the evaluation set first (see below) and measure. It is possible this already
   clears the robustness bar.
2. **Fine-tune** the same model contrastively on generated (utterance, goal, label) pairs only
   if the baseline falls short.
3. **Distil a tiny custom model** from the fine-tuned one only if inference cost or size
   demands it. (Full from-scratch training is almost certainly unnecessary and data-hungry.)

### Where it runs in the architecture

- **On the hot path, synchronous** — scores available for *this* turn's selection/arbitration.
  Budget: tens of ms. Requires local inference (below). Preferred: it preserves the semantics
  "this turn matched".
- **Off the hot path, next-turn salience** — the formation-judge pattern: score after response
  dispatch, bias the next turn. Zero hot-path cost but salience lags one turn behind the
  conversation. Fallback position if hot-path inference disappoints.
- Either way, purity is preserved by computing scores in the effect layer and feeding them to
  the pure reducer/selector as input data; the trace records the scores (and model version) so
  every arbitration outcome stays reconstructable and replayable from recorded values.

### Relation to keyword matching

- **Replace** keywords entirely with semantic scores.
- **Hybrid:** semantic score as the primary signal, keywords retained as a cheap deterministic
  fallback when the model is unavailable (no GPU, model file missing) and as trace-friendly
  attribution. The near-term weighted-lexical fix (keyword specificity weights + qualification
  threshold) is complementary, not competing: it likely ships first, becomes the fallback layer,
  and its paraphrase probes become this idea's evaluation harness.

### Inference runtime (Rust integration)

- **ONNX Runtime via the `ort` crate**, CUDA execution provider (or DirectML on Windows as a
  vendor-neutral alternative). Mature, model exported once from the training stack.
- **`candle`** (pure-Rust, CUDA) — fewer moving parts at runtime, younger ecosystem.
- Practicalities either way: goal-side embeddings precomputed at fixture load; fp16 inference;
  a warm-up inference at server start so the first turn does not pay CUDA initialization.

## Training Data — How to Get Enough

The central uncertainty. The honest answer is that almost none of it will be hand-written;
the realistic sources, in order of leverage:

1. **LLM distillation (primary source).** A frontier model generates utterances and labels
   their relevance against tension/goal summaries — the small model learns to imitate cheap,
   abundant teacher judgments. Thousands to tens of thousands of labeled pairs are attainable
   for small cost. Generate against *multiple invented personas' tension sets*, not just the
   curiosity-observer, so the pair scorer learns the task ("does this utterance bear on this
   tension?") rather than one persona's topics.
2. **Paraphrase expansion with label propagation.** Seed utterances (the experiments' human-test
   scripts, real session transcripts) are paraphrased N ways by an LLM; paraphrases inherit the
   seed's labels. This *directly encodes paraphrase invariance* — the property we want — into
   the training signal.
3. **Adversarial stray-word augmentation.** Take a labeled utterance and inject stopwords and
   idioms ("for what it's worth…", "you know…", "can I just say…"); the label must not change.
   This encodes stray-word immunity. Hard negatives (topic near-misses, stopword-dense
   off-topic chatter) sharpen the decision boundary where arbitration lives.
4. **ASR-noise augmentation.** Real sessions show hallucinated noise turns (`はい。`, `그게`) and
   transcription artifacts; training data should include such inputs labeled irrelevant-to-all,
   so noise does not light up goals.
5. **Real diagnostics as evaluation only.** The recorded sessions are far too small to train on
   (tens of turns) but are the truest held-out test set: real phrasing, real noise, known
   correct winners.

Sizing gut check: contrastive fine-tuning of a MiniLM-class bi-encoder becomes useful around
5–50 k pairs — comfortably within LLM-generation reach. The discipline that matters more than
volume: a **human-curated held-out evaluation set** of paraphrase clusters and stray-word
variants, built *before* any training, so "did training help" is measured, not felt.

## Hardware Fit — RTX 3070 Ti (8 GB)

- **Training:** fine-tuning a 22–110 M-param embedding model fits easily in 8 GB (fp16, modest
  batch sizes); LoRA on models up to ~0.5–1 B is also feasible if the ladder ever climbs that
  far. Training stack is Python/PyTorch (`sentence-transformers`) with CUDA on Windows; the
  artifact crossing into the Rust workspace is a single exported ONNX file plus tokenizer.
- **Inference:** a MiniLM-class encoder over one short utterance is single-digit ms on this
  card; even CPU fallback stays within tens of ms. VRAM footprint at inference is a few hundred
  MB — no contention concerns.

## Open Questions

- Is the zero-shot baseline already good enough? (Decides whether training happens at all.)
- Bi-encoder or cross-encoder at the arbitration threshold, where scores are closest?
- How is the score threshold for "qualified to win arbitration" calibrated, and is it global or
  per-tier fixture data?
- How does model versioning interact with the fixture-compatibility guard and with replay —
  is the model hash part of the trace contract?
- Cold start: is a warm-up inference at server start sufficient, or does the first-turn budget
  need protecting another way?
- What exactly does the goal-side text consist of — tension summary, goal summary, keywords
  folded in, or a dedicated "activation description" field added to the fixture?
- Does multilingual robustness (the being may be addressed in Swedish or English) come free
  from a multilingual base model, or does it need targeted training data?
- Where does the training pipeline live — inside this repo (a `training/` corner with Python)
  or as a sibling project producing versioned model artifacts?

## Relationship to Existing Work

- `Architecture.VolitionSystem.md` — selection/arbitration mechanics this would change.
- `Experiment.CuriosityPersonaSeed.md` / `Experiment.LiveGoalFormationAndCoherence.md` — the
  2026-07-04 session Results are the evidence base; their keyword-tuning Open Items are what
  this idea resolves properly.
- The near-term **weighted-lexical scorer** (keyword specificity weights + qualification
  threshold) — ships first, doubles as the no-GPU fallback, and its paraphrase-robustness
  probes are this idea's acceptance harness.
- The **formation judge** (`qsf_models::live_goal_formation`) — the established precedent for
  model judgment feeding deterministic resolution; this idea extends the same philosophy to
  per-turn salience, but locally and fast enough for the hot path.
- `docs/DecisionLog.md` 2026-07-03 — the launcher/model-provider decision; a local model adds a
  provider-independent path for the volition hot path specifically.
