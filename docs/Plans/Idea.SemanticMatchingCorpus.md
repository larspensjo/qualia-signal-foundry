# Idea: A sparse semantic-matching corpus

## Status

Brainstorm. Nothing here is a commitment. This document exists so the conversation that produced
it can be continued in a fresh context without re-deriving anything.

Supersedes the direction of `Plan.GoalRelevancePanelLabeling.md` and
`Design.GoalRelevancePanelLabeling.md`, which remain accurate about *methodology* and are wrong
about *scope*. Do not delete them before reading "What carries forward" below.

## Background

The application holds a spoken conversation, and while it talks it maintains a set of **standing
goals** — persistent things it is trying to do, such as keeping what it knows separate from what it
is guessing, or not pressing a topic after someone has declined it. A few of these are fixed. Most
accumulate over time, written by the agent itself as conversations go on, so the set grows to
dozens or hundreds.

On every turn the system must decide which of those goals bear on what the person just said, and
put the relevant ones into the prompt used to generate the reply. That decision — *is this text
about that text?* — is the recurring problem. It appears again for memory: given what was just
said, which stored memories are worth recalling?

The difficulty is that the same judgment has to be made at three very different price points, and
the budget decides how much thinking is allowed.

**Fast — a few milliseconds, in-process.** No network, no model; string matching only. This is the
one tier that fits inside the current turn, so it is the only one whose answer can shape the very
next thing the agent says. Today it is an exact keyword match: each goal carries a list of trigger
words, and the goal participates only if the utterance literally contains one of them.
(`matched_keywords` and `match_strength` in the `qsf_volition` crate.)

**Medium — one LLM API call, still during the conversation.** Much better judgment, but a round
trip cannot hold up the reply being spoken, so the answer arrives one turn late — useful, though
always about the previous turn. It also costs money every time it runs.
(`live_goal_formation.rs` in the realtime server.)

**Slow — the sleep phase, between conversations.** Latency stops mattering, so this tier can afford
larger models, several passes, and analysis that would be absurd mid-sentence.
(`qsf_app/src/experiments/`.)

## Idea

Two aims, which are the same aim from opposite ends: move work down a tier wherever the quality
survives the move, and where something is already fast but bad, make the fast tier good enough that
nothing needs to move up.

**Goal relevance is the second case, and it is what this corpus is for.** It already runs in the
fast tier — it is not slow, it is wrong. Exact keyword matching breaks on paraphrase: say the same
thing in different words and nothing activates at all. The measurement further down puts a number
on it. The fast path finds roughly a third of what a careful slow judgment finds, and that is
measured under conditions deliberately chosen to flatter it.

So the target is a fast method — vector embeddings, a small trained classifier, something not yet
chosen — that agrees with the expensive judgment often enough that the expensive judgment need not
be made at all.

That is a **distillation** problem: a slow, accurate teacher; a fast student that has to imitate it;
and a body of labelled examples that reveals whether the imitation worked. **This corpus is that
body of examples.** Three consequences follow, and they explain the shape of everything below.

- **The expensive labelling is the point, not overhead.** To know what the fast method *should*
  answer, we need answers worth trusting — produced by having several strong models label the same
  pairs independently and vote. That is slow and costly on purpose: it is the standard being
  copied, so cheapening it would cheapen the target.
- **The problem shape generalizes.** Goals-against-utterance and memories-against-utterance are the
  same question with different content, so the corpus should not be built goal-specific.
- **Choosing between candidate methods is itself a form of fitting.** Try twenty approaches and
  keep the best-scoring one, and you have tuned to the data without training anything. So some
  examples have to be held back and never used while deciding.

## Summary — what we are trying to solve

We need labelled data to answer "how well does this system decide that two things are about each
other". Today that means goal ↔ utterance relevance, but the same question recurs elsewhere —
memory ↔ utterance being the obvious next one — and the existing corpus cannot answer it even for
goals.

The existing frozen-set design is a **dense cross-product over a 7-goal roster**: 114 utterances
× 7 goals = 798 pairs, every pair labelled. That design has a scope defect that no amount of
labelling fixes.

## The problem with the current corpus

**The roster is a closed world, and the corpus was generated inside it.** Every utterance carries a
`conditioning_goal_ref` — the utterances were *written to be about* those seven goals. So the
dataset does not merely test seven goals; it lives entirely in a seven-goal world.

**Production is not that world.** A small subset of goals is fixed and permanent. On top of that,
new goals accrete to dozens or hundreds, updated every conversation, and phrased by the agent
itself rather than hand-authored.

Four consequences, in order of severity:

1. **Competition scales with roster size; the measurement does not.** Spurious activations per turn
   ≈ false-positive rate × number of goals. A 20% FP rate is ~1.4 spurious activations at 7 goals
   and ~30 at 150. The failure already observed live is crowd-out — `present-person-priority`
   displacing weaker matches — so the failure mode that actually threatens the system is the one a
   7-goal corpus structurally cannot see.
2. **Keyword provenance differs.** The scorer keys off each goal's `activation_keywords` list, not
   its prose. For the permanent seven those lists are hand-authored and individually weighted; for
   dynamic goals they are derived mechanically or written by the model that proposed the goal, and
   arrive unweighted (see "Evidence in hand" below). Measuring against seven curated lists says
   little about a hundred auto-generated ones.
3. **The dense method has a hard ceiling.** 7 goals is 798 pairs and 6,384 panel verdicts. 150
   goals would be 17,100 pairs and 136,800 verdicts. Dense cross-product cannot grow to the
   production roster, so it will never become realistic by adding goals — it caps out.
4. **The fixed goals are the most favourable case available.** The permanent seven are precisely
   the goals that *could* be given hand-built, offline-tested matchers, because they never change.
   Dynamic goals can never have that: they exist only from the moment they are written, mid
   conversation. So measuring on the fixed seven measures the friendliest regime — and any per-goal
   tuning added for them later widens the gap between measured and production behaviour rather than
   closing it.

**Nonstationarity remains unsolved by any version of this.** Goals update every conversation, so
any frozen set ages. A larger roster ages more slowly; it does not stop ageing. Either the corpus
gets a refresh cadence, or the durable claim is limited to the permanent core. This is not decided.

## Evidence in hand — measurements that survive the pivot

These were measured against the current pool. They are facts about the **scorer**, not about the
pool's shape, so they remain valid input.

**The exact-token scorer finds roughly a third of what it should, at best.**

| Gold bound | Relevant pairs | Matched by scorer | Recall |
|---|---|---|---|
| strict (both v1 labelers `relevant`) | 113 | 38 | **33.6%** |
| optimistic (either labeler `relevant`) | 317 | 69 | **21.8%** |

Measured with the real runtime scorer (`qsf_volition::normalize_terms` → `matched_keywords` →
`match_strength`), not a reimplementation. This is at 7 goals against utterances *written to be
about those goals* — the friendliest possible conditions. Organic phrasing and a larger roster
would be worse.

**Why it is this low: what the fast tier actually matches on.** Each goal carries an
`activation_keywords` list of exact lowercase tokens, each with a weight class — Weak 1, Normal 4,
Strong 8 (`crates/qsf_volition/src/model.rs`). A goal is selected when any keyword equals a
normalized input token; `match_strength` is the sum of the matched weights; and a selection must
reach the fixture's `arbitration_qualification_threshold` (4 — one Normal, one Strong, or four
Weak) before it may win arbitration.

**Those lists come from two different places, and only one of them is curated.** This is a
correction to an earlier version of this section, which stated that keywords are derived from the
goal's own identifier everywhere. That is true only of dynamic goals.

- **The permanent seven are hand-authored and weighted.** `crates/qsf_volition/src/fixture.rs`
  gives each seed goal an explicit list. `keep-theses-distinct-from-fact` carries *evidence* and
  *prove* as Strong, *certain*/*true*/*fact* as Normal, *sure*/*really*/*actually*/*know*/*why* as
  Weak. None of these are taken from its id.
- **Dynamic goals get uncurated keywords, by three separate paths.**
  `propose_goal_candidates` (`candidate.rs`) splits the *matched tension* id on hyphens —
  `continuity-preservation` becomes `["continuity", "preservation"]`.
  `explicit_goal_request_candidate` (`live_goal_formation.rs`) takes the first eight normalized
  tokens of the requested goal text. The live-formation model may also supply
  `activation_keywords` itself. All three land as `ActivationKeyword::normal` in `into_goal`, so
  every dynamic keyword weighs 4 and any single match clears the qualification threshold alone.

Two behaviours were previously attributed to id-derivation. Both are real; neither has that cause:

- **Technically-named goals are not unreachable.** `epistemic-integrity` never contributes
  *epistemic* or *integrity* to anything. The goal serving it fires on *evidence*, *prove*,
  *certain*, *true* and *fact* — reachable words. The gap that remains is paraphrase: "Was that
  your own guess, or something you read somewhere?" is unmistakably about that goal and activates
  nothing at all, because no curated list can enumerate the ways people avoid the obvious word.
- **Crowd-out is a curation and weighting effect, not a naming accident.**
  `serve-the-present-person`'s list is deliberately generic — *what*, *how*, *can*, *do*, *tell*,
  *show*, *make*, *want*, *need* — so it fires on almost any question. But they are all Weak, so
  three of them (strength 3) fail to qualify while one Strong keyword on an unrelated goal clears
  the threshold by itself. "Don't tell me about AI and jobs again — what I actually want is a plan
  for my own work" puts `track-the-ai-transition` at strength 12 on the topic the person just
  refused, and `serve-the-present-person` at 3, below the threshold, on the request they actually
  made. Displacement comes out of the weight/threshold interaction over hand-chosen generic terms.

Renaming still couples to matching, but only for dynamic goals — and via the *tension* id for
candidate-derived keywords, not the goal id. Seed goals are immune, because their lists are
explicit.

Two consequences for this corpus:

- **The 33.6% recall figure stands and is if anything generous.** It was measured with the real
  scorer against the seven curated, weighted lists. Dynamic goals have flat unweighted lists drawn
  from their own id or request text, so the production regime is worse than the measured one.
- **This sharpens consequence 4 above rather than softening it.** The permanent seven are not
  merely the goals that *could* be given curated matchers — they already have them. A corpus over
  those seven measures the curated regime exclusively, and the roster it needs to predict is
  mostly uncurated.

**The rubric-direction prediction was confirmed.** Recall gap = −11.86 points: a conservative
(tighter) relevance threshold *raises* measured recall, because tightening preferentially discards
low-lexical-overlap pairs, which are exactly the ones a keyword scorer misses. This was predicted
in writing and committed *before* the number existed, which is the only reason the improvement is
interpretable rather than convenient.

**Labeler disagreement is one-directional and large.** GPT-5.4-mini called 39.1% of pairs relevant;
Claude Fable 5 called 14.8%. 199 of 207 disagreements are mini-`relevant` against
Fable-`not_relevant`. Both had the same guideline. This is a threshold disagreement, not confusion
— which is why the threshold direction had to be decided explicitly.

**Contested pairs are extremely unevenly distributed** (one-directional disagreements, per goal):

| Goal | pool-a | pool-b | total | strict a/b |
|---|---|---|---|---|
| Grow the library | 28 | 28 | 56 | 5 / 5 |
| Assemble a world picture | 25 | 22 | 47 | 9 / 9 |
| Keep theses distinct from fact | 24 | 16 | 40 | 8 / 8 |
| Serve the present person | 9 | 16 | 25 | 4 / 4 |
| Learn what drives this person | 9 | 12 | 21 | 4 / 4 |
| Respect a person's boundaries | 2 | 5 | 7 | 20 / 23 |
| Track the AI transition | 2 | 1 | 3 | 5 / 5 |

Two goals are too thin to support any iterate-then-measure sampling design. Expect the same shape
at larger rosters: a long tail of goals with almost no contested evidence.

## The proposed new shape (not settled)

- **~40 goals**, including the 7 permanent ones, the rest generated to resemble self-authored
  production goals.
- **~200 utterances.**
- **Sparse sampling**, not dense — 8,000 possible pairs is too many to label exhaustively.
- **Generalised beyond goals**: the corpus should serve any "are these two things about each
  other" question, memory ↔ utterance being the next case.
- **Split between developing a method and measuring it**, since comparing candidates is itself
  fitting (see open question 1 — the division and its size are unsettled).
- Reuse the money and infrastructure already spent; purge what no longer applies.

## Open questions, in the order they should be settled

**1. How the corpus is divided between choosing a method and measuring it.** The Idea settles what
used to be the fork here: investigating candidate algorithms *is* fitting, so a held-out portion is
mandatory whether or not anything is trained in the machine-learning sense. Picking the best of N
methods against the whole corpus overfits it as surely as gradient descent would — fewer
parameters, but no more honesty about it.

What stays open is the budget, and it still needs settling before the sampling plan, because it
sets the size:

- **The division.** Development data (iterate, compare candidates) versus held-out data (the number
  actually quoted). Two-way, or three-way if a candidate is genuinely trained rather than tuned.
- **Size.** ~8,000 possible pairs is generous for evaluation and small for training. If a serious
  candidate needs gradient fitting rather than threshold selection, the corpus is probably
  undersized, and that changes both the sampling plan and the labelling budget.
- **Reuse.** A held-out set consulted once per candidate stops being held out. Decide in advance how
  many looks it gets and what happens when they run out — refresh, or freeze and report the number
  of prior looks alongside the result.

**2. One corpus or a family?** Memory ↔ utterance is not goal ↔ utterance with a renamed field.
The rubric and the task contract are both task-specific: the conservative goal-relevance threshold
was argued from *what should make a goal activate in the realtime system*, and memory recall has
different costs — a missed memory is invisible, a spurious one is intrusive. Is this one dataset
with a `match_kind` field and per-kind rubrics, or a family sharing infrastructure?

**3. The base rate collapses, so sampling cannot be uniform.** At 7 goals ~15–20% of pairs were
relevant. At 40, an utterance probably touches 1–3, so **~3–7%**. Uniform random sparse sampling
yields ~95% negatives and starves every thin goal. Stratification is mandatory, and any
stratification toward positives biases the sample in a way the metrics must then account for.

**4. Random sampling is the wrong instrument for evaluating a ranker.** What matters is whether
the right candidates come out on top, which is decided by the pairs a scorer finds *confusable* —
and random sampling will mostly label obvious negatives and may never touch them. This is an
**information-retrieval test-collection problem**; the standard solution is pooling (run several
candidate scorers, pool their top-k, label the pool), with known failure modes such as pool bias
against systems absent from the pool. Worth inheriting deliberately rather than rediscovering.

**5. What replaces the retired metrics, and what shape is the gate?** Sparse coverage kills
`utterance_relevant_set_match` (needs the full relevant set per utterance) and `none_of_roster`
(cannot assert "none of 40" from a sample). Natural replacements are ranking metrics —
precision@k, recall@k, MRR — which are also closer to what arbitration actually does. But the
current gate is per-cell pass/fail thresholds, and ranking metrics are distributional. The gate
architecture has to change, not just its numbers.

**6. Where do the extra goals come from, and are they realistic?** Generated in one batch by one
model they will be homogeneous in length and style, and you would be measuring against a goal-text
distribution that does not exist. Also undecided: whether the 7 permanent goals are over-sampled,
since they are always present live while the synthetic ones are stand-ins.

**7. Refresh cadence.** See nonstationarity above.

**8. What does the panel look like at the new size?** Eight passes over a dense 798-pair pool was
justified by wanting high-quality gold on a small set. A larger, sparser corpus has different
economics; three members plus an auditor may be the better trade. Not decided.

**9. Cold-start and warm performance are different regimes, and a frozen corpus can only measure
one.** A dynamic goal is at its worst the instant it is created: goal text only, no accumulated
evidence, no per-goal state. If the sleep phase later refines matching for a goal — a tuned
threshold, mined paraphrases, a centroid over utterances it turned out to match — the system has
two regimes, and a single aggregate number over a mixed roster conflates them. The corpus can
honestly measure the cold-start function `f(goal_text, utterance)`; warm performance needs replay
or an online audit, which is a different instrument.

One constraint is already clear if sleep-phase tuning happens: it must be driven by the slow tier's
judgment of past turns, never by the fast tier's own activation history. Tuning a matcher on its
own past decisions teaches it to agree with itself and entrenches precisely the errors this work
exists to remove. That is the same distillation loop as the corpus, running continuously rather
than once — which also means the corpus and the online loop should share a definition of what the
teacher's judgment is.

## What carries forward

**Reusable as-is:**

- Both crates. `qsf_semantic_eval` holds the scorer under test and the baseline runner;
  `qsf_semantic_datagen` holds generation, labelling, validation, split preflight, the gatekeeper
  and the reproducible freeze.
- The **panel methodology**: weighted multi-model labelling, integer weight units, the lineage cap,
  an independent auditor excluded from the panel weights.
- The **provenance and lineage architecture**: append-only hash-pinned ledger indexing run
  artifacts, snapshot selection over disjoint complete run sets, byte-reproducible freeze,
  committed replay inputs.
- The **114 utterances** as text, extensible toward 200.
- The generation pipeline, anchor approval, and the operator pool-review gate.

**Survives as evidence, not as answers:**

- The 798 v1 labels. They are bound to 7 goals under `goalrel-label-v1` and the threshold has since
  moved, so they are not gold for anything new. They cost real money and remain the only labelled
  data in existence — **do not delete them**. Possible uses: a sampling prior, a weak baseline, and
  the disagreement analysis above.
- The measurements in "Evidence in hand".

**Superseded:**

- The pool's shape, its census, its pool-a/pool-b split, and the per-`(goal × split)` gate built on
  them.
- The hard-slice design (negation / quoted / hypothetical) as *gate cells*. The categories may
  survive as diagnostics.
- `none_of_roster` as a concept, if the corpus goes sparse.

**Candidates for purge** (each touches the schema, so expect a version bump):

- Review machinery: `review.rs`, the review CLI, `ReviewStatus`, `ReviewDecision`,
  `blind_qa_agreement_by_slice`, `Provenance.review`, `ReviewLineage`.
- `ReconciliationRecord` and the `reconcile` command — the two-labeler design.
- The dense cross-product invariant and its gatekeeper rule.
- `none_of_roster` schema field and its validator invariant, if retired.

## Constraints that must survive the refactor

These were expensive to learn. Re-deriving them would be the real waste.

- **Anchoring is the central threat.** The first attempt failed because the review CLI pre-filled a
  model's answer, making ten reviewed utterances unusable. The same hazard recurs anywhere an
  answer reaches a labeller before they judge — including worked examples in the rubric, which is
  why examples are authored fresh rather than lifted from the pool.
- **Pre-register thresholds before seeing the evidence they gate**, and make the ordering visible
  in git history rather than asserted. This is what makes a favourable result interpretable.
- **No floating point in persisted artifacts.** Basis points or explicit integer
  numerator/denominator, so hashes stay stable and re-derivation is exact.
- **Replay inputs are committed; transcripts are not.** If the methodology note cites a number
  derived from an artifact, that artifact is in git.
- **Every artifact has a `deny_unknown_fields` type and a test that parses the real generated file**
  — not merely that the command exited zero.
- **Measure with the real scorer**, never a reimplementation.
- **Line endings**: `.gitattributes` pins `eol=lf` for repo and working tree. Any hash computed
  from disk must be verified against the committed blob, or it will not reproduce on another
  platform.
- **The operator, not the model, decides product questions** — threshold direction is the worked
  example.

## Environment notes

- Codex in this repo cannot write `.git` and has no network. Commits and live API calls are
  operator steps.
- Reachable API model ids, confirmed live: `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.4-mini`.
- Gemini Flash 3.6 is drivable non-interactively via the Antigravity CLI (`agy --print`) at
  `gemini-3.6-flash-high`. Its headless tool profile: file reads and command execution are
  auto-denied; `list_dir`, `search_web` and `read_url_content` execute freely. So a labelling run
  there is not tool-free, and must be invoked from a directory outside the repository.

## Pointers

- `docs/Plans/Plan.GoalRelevancePanelLabeling.md` — methodology detail; scope superseded.
- `docs/Plans/Design.GoalRelevancePanelLabeling.md` — panel design; scope superseded.
- `docs/Reviews/Review.GoalRelevanceGateFeasibility.md` — the census that first showed the gate was
  structurally unreachable.
- `evaluation/annotations/AnnotationGuidelines.GoalRelevance.md` — `goalrel-label-v2`, carrying the
  threshold-direction rationale.
- `evaluation/contracts/GoalRelevance.TaskContract.md` — names both error costs.
- `evaluation/frozen/goal-relevance/lineage/pools/goalrel-generation-live/` — the committed v1
  lineage, 18 artifacts.
- `docs/DecisionLog.md` — roster binding, generation conditioning, the conservative threshold.
