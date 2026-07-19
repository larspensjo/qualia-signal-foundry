# Text Classification and Semantic Routing Inventory

## Status

Point-in-time code investigation for external machine-learning review.

- Code reviewed: 2026-07-19
- Scope: implemented Rust paths in `crates/`, their prompts, configuration, tests, and the
  project documents that explain their intent
- Authority: the source code is authoritative; linked plans and experiments provide rationale
  or evidence only
- This document records no architecture decision and proposes no implementation plan

## Purpose

Qualia Signal Foundry turns text into labels, rankings, routing decisions, extracted records,
and durable state through several unrelated mechanisms. Some are exact keyword rules. Some are
weighted lexical scorers. Some delegate a structured judgment to a language model. They are not
currently one subsystem and do not share one evaluation harness.

This inventory is intended to help a machine-learning expert answer three questions:

1. Which decisions are genuinely semantic classification problems?
2. Which ones are adequately served by deterministic policy or information retrieval?
3. Where would a learned model materially improve quality without sacrificing latency,
   inspectability, or safe failure behavior?

## Scope and terminology

This document uses **classification** broadly for any implemented path that derives one of the
following from text:

- a discrete label or yes/no decision;
- a relevance score or ranked candidate list;
- a route or tool choice;
- a structured semantic category such as memory candidate, open question, goal, or
  contradiction.

Pure state classification is listed separately. Ordinary response generation, text
summarization with no downstream semantic category, schema parsing, and audio-only voice
activity detection are not treated as core text classifiers.

The inventory distinguishes four deployment surfaces:

- **Live deterministic** — runs synchronously on a turn and can affect the current response.
- **Live model-mediated** — a conversational model implicitly routes the current turn.
- **Off-hot-path model judgment** — runs after the response or during sleep and can affect
  later turns or durable state.
- **Experiment/introspection support** — implemented and tested, but not a general live-runtime
  decision path.

## Executive summary

The project currently has no shared text-classification abstraction, shared tokenizer, labeled
evaluation corpus, confidence contract, or end-to-end semantic-quality metric. The mechanisms
were added as focused slices and are individually inspectable, but their normalizers, thresholds,
failure semantics, and validation depth differ.

The most important findings are:

- Exact-token matching is the dominant live semantic mechanism. It controls memory retrieval,
  goal activation, opportunity detection, and external-world consultation.
- The current weighted volition matcher fixed one known failure mode—weak incidental words can
  activate a goal but cannot win below the qualification threshold—but paraphrases, morphology,
  negation, multilingual input, and ASR errors remain largely unhandled.
- LLM-backed classifications are structured as JSON and some outputs receive strong schema and
  identifier validation. JSON validity is not semantic validation, however.
- Classification consequences range from display-only metadata to response cancellation,
  external information injection, automatic memory promotion, and durable goal changes.
- The low-level model-client selector falls back to `mock` when `QSF_MODEL_PROVIDER` is absent or
  unrecognized. That is not the practical default for the main application: `qsf.ps1 realtime`
  always pins `QSF_MODEL_PROVIDER=openai` for the server process. Its live LLM classifications
  therefore use OpenAI in normal operation; the fixed mock responses primarily serve tests and
  direct invocations outside the launcher.
- Existing tests are mostly hand-authored positive/negative regression cases. There is no
  held-out, human-labeled set from which to report precision, recall, ranking quality,
  calibration, multilingual robustness, or robustness to ASR noise.
- The brainstorm note
  [Idea.SemanticGoalActivation.md](../Plans/Idea.SemanticGoalActivation.md) already describes a
  promising evaluation-first local pair-scorer direction for volition. Its original observation
  that arbitration ignored match strength is partly superseded by the implemented weighted
  qualification gate; its broader diagnosis of lexical brittleness remains current.

## Inventory at a glance

| ID | Decision | Method | Surface | Consequence if wrong |
|---|---|---|---|---|
| T1 | Start turn, ignore continuation, or interrupt | Phase plus exact allow-list | Live deterministic | Drops text or cancels an in-flight response |
| T2 | Which read-only/computation tool to call | Prompted LLM function calling | Live model-mediated | Wrong tool, missed tool, extra latency, or irrelevant context |
| T3 | Capture assistant name, user name, or remembered topic | Phrase patterns, token checks, stopwords | Live deterministic | Writes an incorrect or misses a durable memory |
| T4 | Which memories are relevant | Exact lexical/tag match plus hand-weighted ranking and gates | Live deterministic | Injects distracting memory or misses continuity |
| T5 | Which goal is relevant and which effect it requests | Curated weighted keywords plus thresholds and tier arbitration | Live deterministic | Changes the simulated motivation shaping the response |
| T6 | Whether an opening exists and how strongly to shape | Cue lists plus score/count thresholds | Live deterministic | Over-steers or under-steers the response |
| T7 | Whether and what current-world information to consult | Cue/entity/version rules plus lexical ranking and anchor gates | Live deterministic | Performs an irrelevant read or injects misleading external context |
| T8 | Form a goal and detect contradictions | Structured LLM judgment plus deterministic validation/resolution | Off hot path and sleep | Admits, declines, or cancels durable goals |
| T9 | Split a session into memory/question/decision/hint categories | Structured LLM extraction | Sleep | Automatically promotes bad memories or loses useful continuity |
| T10 | Convert an external article into a memory summary | Rule eligibility plus structured LLM extraction | Sleep | Persists a distorted external claim |
| T11 | Map open questions to tension-backed goal candidates | Exact token overlap | Experiment support | Produces implausible or missing candidate goals |
| T12 | Rank and classify project documents | Substring counts, paths, headings, fixed tags | Introspection support | Gives the model irrelevant or wrongly hedged project context |
| T13 | Decide whether a document excerpt influenced a reply | Exact four-word overlap | Introspection diagnostics | Misattributes or misses provenance; no runtime behavior change |

## Detailed inventory

### T1. Realtime final-transcript disposition

**Implementation**

- [`turn_integrity.rs`](../../crates/qsf_realtime_server/src/realtime/turn_integrity.rs):
  `classify_final_transcript`
- Consumer:
  [`sideband_provider_event.rs`](../../crates/qsf_realtime_server/src/realtime/sideband_provider_event.rs)

**Input and output**

- Input: final ASR transcript plus current `TurnPhase` (`Idle`, `AwaitingResponse`, or
  `ToolLoop`).
- Output: `StartTurn`, `IgnoreAsNoise`, or `Interrupt`.

**Current method**

- In `Idle`, every non-empty final transcript starts a turn.
- During a response or tool loop, normalized empty text and exactly `cheers`, `thanks`, or
  `thank you` are ignored.
- Every other non-empty transcript is classified as an interruption.
- Normalization trims, lowercases, and removes trailing ASCII punctuation only.

**Why it is needed**

The provider can finalize continuation speech while a response is in flight. QSF must decide
whether the text is harmless acknowledgement or a new user intervention before it cancels the
old response and starts another exchange.

**Problems and risks**

- The allow-list is extremely small and English-only.
- Semantically equivalent acknowledgements (`okay`, `got it`, `mm-hm`) interrupt; meaningful
  text containing an allow-listed phrase plus more words also interrupts.
- Pragmatics depend on prosody and timing, but only the final text and coarse phase are used.
- ASR hallucinations become interruptions unless they are empty or one of three phrases.
- There is no confidence score or abstention state.
- False positives are costly: the old response is cancelled and the exchange is marked
  interrupted. False negatives discard user speech.

**Validation today**

Unit tests cover phase behavior, the three allow-list forms, punctuation normalization, empty
text, and `stop`/`wait`. There is no recorded-session confusion matrix.

### T2. Model-mediated tool routing

**Implementation**

- Text loop prompt and tool instructions:
  [`prompt.rs`](../../crates/qsf_app/src/conversation/prompt.rs)
- Text-loop tool advertisement and dispatch:
  [`tool_runtime.rs`](../../crates/qsf_app/src/experiments/multi_turn_text_loop/tool_runtime.rs)
- Realtime instructions:
  [`state.rs`](../../crates/qsf_realtime_server/src/state.rs)
- Realtime tool definitions and deterministic permission checks:
  [`tools.rs`](../../crates/qsf_realtime_server/src/realtime/tools.rs)
- OpenAI function-call serialization:
  [`openai_tool_client.rs`](../../crates/qsf_models/src/openai_tool_client.rs)

**Input and output**

- Input: conversation prompt, current user text, model-visible context, tool names,
  descriptions, and JSON parameter schemas.
- Output: zero or more model tool calls with selected tool and arguments.

**Current method**

There is no explicit tool-intent classifier or central router. The conversational or realtime
model implicitly makes the routing decision while generating its response.

The text loop instructs the model to use `calculator` for exact arithmetic, `recall_turn` for
exact detail from summarized turns, and search-then-read for project documents. The realtime
persona prompt explicitly requires volition inspection tools for questions about goals or
internal state. Other realtime read-only tools are described mainly through their function
definitions.

The registry and permission layer validate an emitted call, but they do not validate whether
the chosen tool was semantically appropriate.

**Why it is needed**

The response model must route questions to exact computation, verbatim recall, project
introspection, memory retrieval, association inspection, and volition inspection without a
separate model round trip.

**Problems and risks**

- Routing quality is entangled with the response model, full prompt, tool descriptions, and
  context order.
- Tool non-selection has no explicit candidate list, score, or rationale, which makes false
  negatives hard to measure.
- Prompt changes can change routing behavior without changing tool code.
- The text and realtime surfaces advertise different tools and use different instructions.
- The deterministic mock only recognizes narrow arithmetic and literal `recall turn` patterns;
  it is a test fixture, not a quality baseline for tool intent.
- Tool-loop caps and permissions bound execution, but do not solve misrouting.

**Validation today**

Tests cover serialization, allow-lists, permission denial, tool-loop limits, and selected happy
paths. They do not form a labeled intent-routing benchmark across tools and no-tool turns.

### T3. Live memory-candidate capture

**Implementation**

- Classifier/extractor:
  [`live_capture.rs`](../../crates/qsf_app/src/memory/live_capture.rs)
- Persistence consumer:
  [`live_memory.rs`](../../crates/qsf_app/src/session/live_memory.rs)
- Quality regression:
  [Experiment.LiveMemoryCaptureQuality.md](../Experiments/Experiment.LiveMemoryCaptureQuality.md)

**Input and output**

- Input: current user text, current assistant response, and optionally the preceding turn.
- Output: zero or more candidates labeled `AssistantName`, `UserName`, or `RememberedTopic`,
  with generated title, summary, tags, importance, and source turn.

**Current method**

- Assistant-name assignment searches for five phrases such as `use the name` and `your name
  is`, extracts one name-like token, and requires the assistant response to repeat it.
- User-name capture accepts turn-initial `my name is`, `call me`, `please call me`, or `i am`,
  then requires a single uppercase, name-like token and rejects a 16-word stoplist.
- Remembered-topic capture detects a small set of positive request phrases and explicit negative
  phrases, copies a bounded excerpt from the previous assistant answer, and assigns topic tags
  from a fixed list (`volition`, `system`, `goal`, `memory`, `retrieval`, `context`, `identity`,
  `name`, `arbitration`, `preference`, and related aliases).
- Accepted candidates are deduplicated by normalized title plus summary and written immediately
  to the durable memory store with fixed importance values (`0.9` for names, `0.8` for a
  remembered topic).

**Why it is needed**

The live loop needs a cheap, deterministic path for high-value continuity facts before an
offline sleep pass, especially identity and explicit user requests to remember prior context.

**Problems and risks**

- Names are restricted to one token with ASCII-oriented shape rules; multiword, lowercase,
  non-Latin, or conversationally embedded names are missed.
- `I am X` conflates identity and transient predicates. Uppercase and short-tail restrictions
  reduce but cannot remove this ambiguity.
- Requiring the assistant to repeat its assigned name makes capture depend on generation style.
- Remember intent is an English phrase table with incomplete negation and no discourse model.
- Topic labeling is a project-specific closed keyword list; everything else becomes `prior
  discussion`.
- A remembered-topic record copies the assistant response, so assistant hallucination can be
  promoted because the user said “remember this.”
- Fixed importance values are not calibrated and there is no confidence field.
- A false positive writes durable state on the live path.

**Validation today**

Unit tests exercise positive and selected negative phrasings. The end-to-end Ari/Lars/volition
fixture checks three captures and later retrieval. The experiment explicitly says semantic
breadth and live-model quality remain unvalidated.

### T4. Memory relevance, ranking, and identity targeting

**Implementation**

- Core retrieval:
  [`retrieval.rs`](../../crates/qsf_memory/src/retrieval.rs)
- Realtime adapter:
  [`memory_store.rs`](../../crates/qsf_realtime_server/src/realtime/memory_store.rs)
- Context budgeting after retrieval:
  [`lib.rs`](../../crates/qsf_context/src/lib.rs)

**Input and output**

- Input: query text, memory records, associations, strategy, and result limit.
- Output: ranked selected and omitted memories, decomposed score, matched terms, association
  paths, and skip reason.

**Current method**

The tokenizer lowercases alphanumeric tokens, discards terms shorter than three characters,
and deduplicates them in a set. Matching is exact against title, summary, and tags.

Three strategies exist:

- `RecencyOnly`: exponential time decay; every non-superseded record remains relevant.
- `KeywordTag`: `0.8*keyword + 1.4*tag + 0.35*importance + 0.2*recency`.
- `AssociationWeighted`: `0.65*keyword + 1.1*tag + 1.35*association + 0.35*importance +
  0.2*recency + 0.25*reinforcement`.

The keyword strategies then apply a relevance gate: direct keyword/tag evidence, an association
path, or a special identity/profile allowance is required. Identity targeting uses hardcoded
English phrases such as `who are you`, `what is your name`, and `who am I` to distinguish
assistant-name from user-name memories.

The context assembler is not a semantic classifier. It consumes the retrieval score, sorts by
source-kind priority and score, and applies fragment/token limits.

**Why it is needed**

Only a small relevant subset of durable memory can enter a model context. The path must also be
fast, deterministic, inspectable, and usable without a model or vector index.

**Problems and risks**

- Exact bag-of-words matching has no stemming, synonyms, phrase semantics, word order,
  negation, or semantic equivalence.
- Tokens under three characters are lost, including names and domain terms, except in other
  subsystems that have separate exceptions.
- Hand-authored weights combine quantities on unlike scales and are not fitted to judgments.
- Importance and association edges can amplify a weak lexical seed.
- Association expansion is only as good as earlier co-retrieval and LLM-proposed edges.
- Identity logic is a second classifier layered over retrieval and is English/pronoun-specific.
- There is no calibrated relevance probability or stable score meaning across strategies.
- Retrieval uses current wall-clock time, so ranking evidence also changes through decay.

**Validation today**

Unit tests cover exact matches, associations, identity queries, decay, supersession, and legacy
records. The associative-memory experiment compares strategies on a hand-authored fixture, but
manual relevance ratings remain unautomated and the document warns that the fixture may be
overfit.

### T5. Volition goal activation, relevance, qualification, and effect selection

**Implementation**

- Tokenization and grounding:
  [`terms.rs`](../../crates/qsf_volition/src/terms.rs)
- Keyword weights and fixture-level threshold:
  [`model.rs`](../../crates/qsf_volition/src/model.rs)
- Current persona labels and keywords:
  [`fixture.rs`](../../crates/qsf_volition/src/fixture.rs)
- Selection and effect routing:
  [`selection.rs`](../../crates/qsf_volition/src/selection.rs)
- Arbitration:
  [`arbitration.rs`](../../crates/qsf_volition/src/arbitration.rs)

**Input and output**

- Input: user transcript, goal definitions, goal state/salience, tension definitions, and mode.
- Output: selected and omitted goals, matched weighted keywords, match strength, relevance score,
  qualification partition, arbitration winner/losers, and an allowed-effect choice.

**Current method**

- ASCII alphanumeric tokens are lowercased and deduplicated in order of first occurrence.
- Every goal has curated activation keywords weighted `Weak=1`, `Normal=4`, or `Strong=8`.
- `match_strength` is the sum of distinct matched keyword weights.
- Relevance is `25*match_strength + base_priority + maximum tension priority bonus + salience`.
- A goal needs strength at least `4` to enter arbitration. Weaker matches still activate and
  affect salience but cannot win.
- Qualified goals are ordered primarily by effective tension tier, then priority/id rules;
  relevance is not the final arbitration sort key.
- A `ProposeExperiment` effect requires strength at least `8` and two distinct non-weak terms;
  otherwise the first configured allowed effect wins.

The realtime fixture has seven goal labels with 8–12 keywords each. Several are broad pronouns
or function words (`i`, `my`, `what`, `how`, `do`, `why`, `system`, `change`); the weighting and
qualification gate deliberately prevent a single weak term from winning.

**Why it is needed**

The system needs a low-latency and replayable answer to “which internal goal is relevant to this
turn?” before it assembles the volition packet and bounded initiative.

**Problems and risks**

- Exact tokens miss paraphrases and morphology (`employment` versus `jobs`, for example).
- A token match ignores negation, quoted speech, hypothetical framing, and who the statement is
  about.
- Deduplication removes frequency and emphasis.
- Curated keyword weights and thresholds have no empirical calibration.
- Broad words can still cause salience activation even when they cannot win.
- Among qualified goals, tier precedence can beat a much stronger semantic match. That may be
  intended policy for protected goals, but it conflates relevance classification with normative
  priority.
- Live-formed goals receive model-supplied keywords, all defaulting to `Normal`; one keyword is
  therefore enough to qualify, unlike carefully curated fixture goals.
- Keyword lists, goal summaries, priority, salience, and arbitration policy are coupled in one
  outcome, making error attribution difficult without decomposed evaluation.

**Validation today**

There is strong deterministic unit coverage and trace completeness. Tests cover paraphrase-like
handwritten cases, stray idiom words, qualification, and effect thresholds. There is no broad
held-out utterance-to-goal set. The current brainstorm proposes one; it should be treated as a
candidate direction, not implemented behavior.

### T6. Opportunity labels and shaping intensity

**Implementation**

- Opportunity classification:
  [`opportunity.rs`](../../crates/qsf_volition/src/opportunity.rs)
- Intensity classification:
  [`shaping.rs`](../../crates/qsf_volition/src/shaping.rs)
- Live consumer:
  [`sideband_turn_injection.rs`](../../crates/qsf_realtime_server/src/realtime/sideband_turn_injection.rs)

**Input and output**

- Input: grounded input tokens, active goals, arbitration winner, and receptiveness hint.
- Output: zero or more `ExpressedUncertainty`, `IntroducedContradiction`, or
  `OpenGoalTopicMatch` labels, followed by `None`, `Low`, `Medium`, or `High` shaping intensity.

**Current method**

- Uncertainty is any exact hit in a nine-word list (`maybe`, `perhaps`, `uncertain`, `unsure`,
  `unclear`, `confused`, `question`, `questions`, `wonder`).
- Contradiction is any exact hit in a nine-word list (`but`, `however`, `though`, `yet`,
  `contradiction`, `contradict`, `inconsistent`, `conflict`, `instead`).
- Open-goal topic match reuses exact goal activation keywords.
- Intensity combines counts with winner relevance thresholds (`180`, `220`), receptiveness, and
  protected-tier clamping. The live path currently passes `Neutral` receptiveness.

**Why it is needed**

Goal relevance alone does not say whether the user has opened a conversational space in which
the goal should visibly shape the answer. Intensity is intended to keep volition subtle and
bounded.

**Problems and risks**

- Discourse markers such as `but` do not necessarily introduce contradiction; `question` does
  not necessarily express uncertainty.
- The detector counts multiple cue tokens, so verbosity can increase intensity without stronger
  meaning.
- It is English-only and inherits goal-keyword brittleness.
- Relevance thresholds are on a hand-built score whose scale changes when fixtures or salience
  policies change.
- `ReceptivenessHint` looks like a potentially learned input but is currently always neutral in
  the live path.
- Opportunity and intensity are evaluated together in effects, but need separate labels and
  metrics to identify which stage failed.

**Validation today**

Unit tests check deterministic cue emission, grounding references, and threshold branches. There
is no pragmatic-language evaluation set or human rating of appropriate shaping intensity.

### T7. Current-world consultation trigger and source relevance

**Implementation**

- Explicit-topic trigger and generic-term policy:
  [`initiative.rs`](../../crates/qsf_volition/src/initiative.rs)
- Corpus lexical index:
  [`index.rs`](../../crates/qsf_corpus/src/index.rs)
- Candidate gates and injection decision:
  [`world_consultation.rs`](../../crates/qsf_realtime_server/src/realtime/world_consultation.rs)

**Input and output**

- Input: current transcript, optionally a goal-activation consultation request, indexed external
  articles, session anti-repeat state, and lookup latency.
- Output: no consultation or a lexical query; candidate articles labeled eligible/omitted;
  surfaced external facts; inline/deferred injection.

**Current method**

An explicit user-topic consultation requires both:

- at least one current-information cue such as `find`, `search`, `release`, `latest`, `recent`,
  `news`, `current`, or `today`; and
- a capitalized token treated as a named entity or a dotted version such as `4.5`.

Sentence-initial capitalization is a fallback only when no later capitalized token exists. A
large fixed generic-word list is removed.

The corpus index exact-matches lowercase terms. Title hits count `3`, body hits `1`, and URL/domain
metadata hits `1`. It keeps special two-character `ai`, `ar`, and `vr` anchors and dotted
versions.

For goal-triggered lookup, every goal-derived anchor and at least a ceiling-rounded 50% of
meaningful current-topic terms must match. Explicit entity/version lookup requires its anchors.
The adapter then applies per-session anti-repeat and a two-source surface cap. Lookup above 5 ms
is deferred rather than injected inline.

**Why it is needed**

The system needs a narrow, auditable escape hatch for fresh external information without turning
every conversation into a corpus search or trusting lexical rank alone.

**Problems and risks**

- Capitalization is not named-entity recognition and is weakened by ASR casing, lowercase brand
  names, sentence starts, acronyms, and non-English orthography.
- Current-information cues are ambiguous (`update` can be a verb; `release` can be unrelated to
  news).
- Query normalization differs from both memory and volition tokenization.
- Exact anchors improve precision but can sharply reduce recall under inflection, aliasing, and
  spelling/transcription errors.
- The 50% threshold, title weight, candidate limit, surface limit, and 5 ms boundary are policy
  constants rather than empirically calibrated values.
- Lexical match is treated as source relevance; source quality, claim support, duplication across
  outlets, and contradiction are not classified.
- An irrelevant injected article can directly steer the response, though it is explicitly framed
  as untrusted external material.

**Validation today**

Tests cover named/version triggers, no-trigger ordinary turns, lexical ranking, anchor omission,
topic-majority omission, anti-repeat, and latency placement. The corpus fixtures are small and
hand-authored; there is no retrieval relevance judgment set.

### T8. LLM-backed live goal formation and coherence judgment

**Implementation**

- Combined formation prompt, explicit-request pre-extractor, parsing, and validation:
  [`live_goal_formation.rs`](../../crates/qsf_models/src/live_goal_formation.rs)
- Standalone contradiction prompt:
  [`coherence_judge.rs`](../../crates/qsf_models/src/coherence_judge.rs)
- Realtime queue and durable application:
  [`live_goal_formation.rs`](../../crates/qsf_realtime_server/src/realtime/live_goal_formation.rs)
- Realtime launcher provider selection: [`qsf.ps1`](../../scripts/qsf.ps1)
- Deterministic admission and sweep resolution:
  [`coherence.rs`](../../crates/qsf_volition/src/coherence.rs)
- Sleep formation and whole-set sweep:
  [`volition_continuity.rs`](../../crates/qsf_app/src/experiments/volition_continuity.rs)

**Input and output**

- Input: a trusted user/assistant exchange plus current non-retired goals (`id`, `title`,
  `summary`).
- Output: optional `ProposedGoalCandidate` and a list of contradiction pairs with rationale.

**Current method**

The combined `v2` prompt asks the model to decide whether one trusted turn warrants a durable
goal candidate and to identify contradictions in one JSON object. It uses temperature `0`, a
900-token cap, role default `gpt-5.4`, and a stable goal-set prompt prefix. It runs once per
trusted realtime turn after response dispatch, in FIFO order, and once over whole history during
sleep.

Before the model call, seven exact English markers such as `make it a goal to` and `adopt a goal
to` can pre-extract an explicit user-requested goal. The prompt must return that exact candidate;
the adapter also prefers the pre-extracted object over the model's candidate.

Validation rejects candidate ID collisions, unknown contradiction IDs, self-contradictions,
blank rationales, and invalid candidate structure. Pure policy then decides admit/reject/cancel
using tension tiers. A stale outcome is discarded if the goal set changed during inference.

The standalone `v1` coherence prompt asks only for contradictions among goals, with temperature
`0` and a 600-token cap. It is used by sleep whole-set sweep and deterministic experiments.

**Why it is needed**

Goal formation and semantic contradiction are difficult to express with keyword tables. The
LLM supplies open-ended semantic detection while deterministic code retains authority over goal
lifecycle changes.

**Problems and risks**

- The prompt does not operationalize when a conversational statement warrants a *durable* goal;
  model priors define that boundary.
- Contradiction is described in one sentence and may conflate competition, tension, redundancy,
  inconsistency, or mutually exclusive action.
- One combined call couples candidate formation and contradiction detection; errors are not
  statistically independent and attribution is harder.
- JSON mode and schema validation detect malformed structure, not plausible but incorrect goals
  or missed/imagined contradictions.
- Temperature zero does not guarantee repeatability across provider/model revisions.
- The role model name is a mutable external dependency and no labeled compatibility suite gates
  upgrades.
- The explicit-goal phrase parser can extract too much trailing text, misses paraphrases, and
  creates all keywords as `Normal`, making any one of them arbitration-qualified after admission.
- A successful but wrong judgment can alter durable goal state. Failures are logged and do not
  apply state, which is a good fail-closed property.
- Every trusted turn incurs a model call when the real provider is enabled; there is no semantic
  pre-filter. This improves recall but adds cost and exposure to noisy low-value turns.

**Provider/fallback detail**

At the low-level client boundary, an absent or unrecognized `QSF_MODEL_PROVIDER` selects `mock`.
The main `qsf.ps1 realtime` application path does not use that fallback: the launcher requires an
OpenAI API key and explicitly sets `QSF_MODEL_PROVIDER=openai` for the realtime server child
process. The mock formation response is always no candidate/no contradictions, and the mock
coherence response is always no contradictions unless a test injects a fixture. This is
deterministic test and direct-invocation behavior, not the practical realtime default or a
semantic fallback.

**Validation today**

Tests strongly cover shape validation, prefix hashing, stale-state handling, queue ordering,
resolution, event/trace contracts, and scripted semantic cases. There is no held-out set of real
turns annotated for durable-goal formation or goal-pair contradiction.

### T9. Sleep session classification and extraction

**Implementation**

- Prompt and invocation:
  [`session_summary.rs`](../../crates/qsf_app/src/sleep/session_summary.rs)
- Structured parser:
  [`sleep_report.rs`](../../crates/qsf_app/src/sleep/sleep_report.rs)
- Promotion:
  [`auto_promote.rs`](../../crates/qsf_app/src/sleep/auto_promote.rs)
- End-to-end command:
  [`update.rs`](../../crates/qsf_app/src/sleep/update.rs)
- Launcher and provider default: [`qsf.ps1`](../../scripts/qsf.ps1)

**Input and output**

- Input: persisted session transcript, prior turn summaries, retrieved-memory context, review
  notes, and diagnostics.
- Output: `session_summary`, up to four `memory_candidates`, up to three `open_questions`, up to
  two `decision_candidates`, up to three `future_context_hints`, review notes, and optional
  association candidates.

**Current method**

One structured `SleepSummarizer` call uses temperature `0`, a 1536-token output cap, and role
default `gpt-5.4`. The prompt defines field names and count caps, requires numeric importance,
and says decisions/associations are provisional.

The parser requires the expected fields, accepts string or object memory candidates, clamps
probabilities to `[0,1]`, and validates association indexes. Routine memory candidates are then
automatically converted to durable observation records unless empty or textually duplicate.
Decision candidates remain review drafts. Open questions and future hints enter the consolidated
brief. LLM-proposed associations can become durable links between promoted candidates.

**Why it is needed**

The sleep phase compresses a long session into typed artifacts that support continuity without
replaying the entire conversation.

**Problems and risks**

- This is simultaneous summarization, extraction, multi-label classification, salience scoring,
  question detection, decision detection, and relation extraction in one prompt.
- The categories are described structurally but not semantically. For example, “memory-worthy,”
  “decision candidate,” and “future context hint” lack operational definitions and examples.
- Count caps force implicit ranking, but the selection criterion is not specified.
- Model-supplied importance has no calibration or confidence meaning.
- Source references are optional, so a structurally valid memory can be weakly grounded.
- Textual deduplication removes whitespace and case only; paraphrased duplicates survive.
- Contrary to the prompt's “provisional” framing, routine memory candidates and their
  associations are auto-applied during sleep. A false positive therefore becomes durable state.
- One bad summary can propagate into future retrieval, later sleep inputs, association formation,
  and goal formation.
- The prompt constant has no explicit prompt-version identifier in persisted domain output.

**Provider/fallback detail**

The `qsf.ps1 sleep` launcher defaults `-Provider` to `openai`; choosing `-Provider mock` (or
invoking the lower-level client without a recognized provider) produces a fixed,
input-independent report. That report contains no memory candidates but does contain canned
open-question, decision, hint, and review text. It proves plumbing only.

**Validation today**

Tests cover request configuration, JSON parsing, clamping, association index compatibility,
deduplication, idempotence, artifacts, and state updates. There is no corpus of transcripts with
human-agreed memory/question/decision labels or importance judgments.

### T10. External article eligibility and LLM memory extraction

**Implementation**

- Eligibility, prompt, extraction, and durable record creation:
  [`world_memory_consolidation.rs`](../../crates/qsf_app/src/sleep/world_memory_consolidation.rs)
- Untrusted-text framing:
  [`untrusted.rs`](../../crates/qsf_corpus/src/untrusted.rs)

**Input and output**

- Input: newly ingested external articles and existing memory store.
- Output: eligible/ineligible decision and, for eligible articles, one durable attributed world
  memory summary.

**Current method**

Rule eligibility requires at least 60 non-whitespace body characters, newest article per URL in
the delta, not already stored by content hash, and one of the first two eligible items in the
run. Eligible article text is sandbox-framed as untrusted and sent to the `MemoryExtractor` role
with temperature `0`, a 300-token cap, and role default `gpt-5.4-nano`. The prompt asks for
exactly one factual, attribution-preserving candidate with numeric importance.

Only the candidate summary is consumed. The persisted record currently uses a fixed importance
of `0.5`, ignoring the returned importance. It retains source provenance, untrusted trust tier,
and faster decay.

**Why it is needed**

External articles are too large for repeated live injection. Sleep creates a compact,
attributable representation for later recall.

**Problems and risks**

- Body length is called “substantive” but measures only character count.
- Newest-per-URL and two-per-run are ingestion policies, not relevance or quality judgments.
- The summarizer has no explicit claim-evidence schema, quotation span, uncertainty, or
  multi-claim handling.
- Prompt injection is bounded by framing but not eliminated; the model still reads untrusted
  text.
- Semantic fidelity is not validated before automatic persistence.
- All selected articles become a single observation kind with fixed importance, regardless of
  content quality or relevance.
- When the mock provider is explicitly selected or reached through the low-level fallback, every
  eligible article receives the same canned memory summary (“The system compared memory retrieval
  strategies.”). This is suitable only for deterministic fixtures; using it on a real configured
  corpus would persist semantically wrong summaries. The `qsf.ps1 sleep` launcher defaults to
  OpenAI, so this is not the normal launcher behavior.

**Validation today**

Tests cover rule eligibility, caps, retries, content hashes, degradation behavior, provenance,
supersession, associations, and artifact completeness. They use deterministic summaries and do
not measure source-summary faithfulness.

### T11. Open-question to tension/goal mapping

**Implementation**

- [`candidate.rs`](../../crates/qsf_volition/src/candidate.rs): `propose_goal_candidates`
- Primary evidence:
  [Experiment.VolitionReflectionGoalCandidates.md](../Experiments/Experiment.VolitionReflectionGoalCandidates.md)

**Deployment status**

This is an implemented, deterministic experiment/support path. Current realtime and sleep goal
formation use the LLM-backed combined judge instead; this function remains a useful lexical
baseline and is reused by volition experiments.

**Current method**

Each open question is tokenized and matched if any token equals any hyphen-separated tension ID
part or any token in the tension summary. Every matched tension is attached. Activation keywords
for the new candidate are derived from tension ID parts.

**Why it is needed**

It established a pure, reviewable bridge from sleep-produced open questions to potential goals
before model-backed live formation existed.

**Problems and risks**

- A single shared token is sufficient, with no stopwords, weighting, phrase meaning, or negative
  evidence.
- The question can map to multiple tensions without a score or ambiguity signal.
- Candidate activation keywords describe the tension ID, not necessarily the question.
- Candidate IDs are content slugs and semantic duplicates are not detected.

**Validation today**

Four scripted questions cover three positives and one no-match case. The experiment explicitly
states that production precision and model-generated question quality were not measured.

### T12. Project-document search, kind, maturity, and match strength

**Implementation**

- Search/ranking: [`search.rs`](../../crates/qsf_app/src/project_docs/search.rs)
- Kind/maturity/date extraction:
  [`metadata.rs`](../../crates/qsf_app/src/project_docs/metadata.rs)
- Model-facing interpretation instructions:
  [`prompt.rs`](../../crates/qsf_app/src/conversation/prompt.rs)

**Current method**

- Query terms are lowercase ASCII-alphanumeric substrings of length at least three after a
  fixed stoplist.
- Documents match when any term occurs as a substring; occurrences are summed.
- Heading hits sort before body-only hits, then occurrence count and path order.
- Match strength is `High` for any heading hit, `Medium` for at least three body occurrences,
  otherwise `Low`.
- Document kind is inferred from directory and filename prefixes.
- Maturity is the first recognized token under an exact `## Maturity` or `## Status` heading.
- The responder prompt tells the model how strongly to hedge each kind/maturity.

**Why it is needed**

The self-introspection channel needs bounded retrieval and authority metadata so the responder
does not treat brainstorms or plans as implemented truth.

**Problems and risks**

- Substring matching can match inside unrelated words and does not require all query concepts.
- Occurrence count rewards repetition and long documents rather than relevance.
- Any heading hit becomes high strength regardless of semantic quality.
- The tokenizer is yet another incompatible normalization policy.
- Kind/maturity classification is convention-based and fails to `Unknown` when documents drift.
- Search relevance and document authority are separate axes but arrive together and may be
  conflated by the response model.

**Validation today**

Tests cover heading order, multi-term queries, stopword-only input, metadata conventions,
unknowns, and bounded reads on a very small document fixture. No relevance judgments exist for
real project questions.

### T13. Project-document influence attribution

**Implementation**

- [`influence.rs`](../../crates/qsf_app/src/project_docs/influence.rs)
- Consumer:
  [`enrichment.rs`](../../crates/qsf_app/src/project_docs/enrichment.rs)

**Current method**

A reply is marked influenced when it and a returned excerpt share an exact contiguous four-word
sequence after lowercase tokenization and removal of words shorter than three characters.

**Why it is needed**

Diagnostics try to distinguish a document that was merely retrieved from one whose text appeared
to affect the final answer.

**Problems and risks**

- Paraphrases are false negatives; common four-word phrases are false positives.
- The method detects textual reuse, not causal influence.
- It has no score, source competition, or attribution when several excerpts overlap.
- The name `influenced_reply` can overstate what the heuristic proves.

**Consequence boundary**

This is diagnostics-only and does not change response generation, so it is a low-risk candidate
for replacement compared with live or durable-state classifiers.

## Adjacent mechanisms not counted as core text classifiers

These paths are relevant to an ML review but should not be mixed into the same benchmark without
an explicit reason:

- **Warm turn summarization** — `SessionTurnSummarizer` compresses one turn into one sentence in
  [`ageing.rs`](../../crates/qsf_app/src/session/ageing.rs). It is lossy semantic compression,
  but it does not assign a semantic class. Its output can still affect later response context.
- **Functional volition signals** — `coherence_decline`, `frustration`, `satisfaction`, and
  `boredom` in [`signals.rs`](../../crates/qsf_volition/src/signals.rs) classify structured state,
  not text, and are display-only.
- **Ambient exposure** — conscious/subconscious winner exposure in
  [`volition_injection_text.rs`](../../crates/qsf_realtime_server/src/realtime/volition_injection_text.rs)
  is deterministic state policy, not semantic text interpretation.
- **Resume mode** — `ColdStart`, `AwakeContinuation`, or `ConsolidatedBrief` in
  [`resume.rs`](../../crates/qsf_session/src/resume.rs) classifies manifest state, not text.
- **Context assembly** — source priority and token budget choose among already-scored fragments;
  this is a knapsack-like policy boundary, not relevance inference.
- **Provider ASR and server VAD** — they classify audio/turn boundaries outside the repository's
  text logic. Their transcript errors are nevertheless an important input distribution for T1,
  T3, T5, T6, T7, and T8.

## Cross-cutting technical problems

### 1. Incompatible text normalization

Memory, volition, corpus, project-doc, name, remember-intent, and overlap paths each implement
their own tokenizer or normalization. They disagree on:

- ASCII versus Unicode alphanumerics;
- minimum token length;
- preservation of `ai`, `ar`, `vr`, and dotted versions;
- deduplication and order;
- substring versus whole-token match;
- apostrophes, punctuation, casing, and sentence boundaries;
- stopword lists.

The same utterance therefore exposes different evidence to different decisions, and fixes made
in one path do not propagate.

### 2. No calibrated confidence contract

Most rule outputs are categorical. Numeric “scores” are hand-composed ranking values rather than
probabilities. LLM outputs usually omit confidence, and model-supplied importance is not
calibrated. Callers cannot apply a common policy such as:

```text
high confidence -> act
medium confidence -> ask/queue for review
low confidence -> abstain or use deterministic fallback
```

Calibration may not be necessary for every path, but the absence is most costly at durable-write
boundaries.

### 3. Semantic inference and policy are interleaved

Several chains combine evidence extraction with normative policy:

- goal relevance, protected tiers, mode bias, and allowed effect;
- corpus relevance, anti-repeat, surface cap, and latency placement;
- sleep extraction, count caps, deduplication, and automatic promotion.

A learned classifier should not silently absorb policy that is currently explicit and
replayable. Evaluation should measure semantic prediction separately from deterministic action
policy.

### 4. Error costs are asymmetric and propagation is recursive

False positives and false negatives do not cost the same:

- T1 false positives cancel speech; T1 false negatives discard an interruption.
- T3, T9, and T10 false positives create durable memories that later rank, reinforce, and appear
  in future sleep inputs.
- T8 false positives can create goals; false contradiction labels can decline or cancel them.
- T4/T5 false negatives mainly omit useful context or shaping on one turn, although repeated
  misses affect salience and continuity.
- T13 errors affect diagnostics only.

One global accuracy target would conceal these differences.

### 5. Structured-output validation is necessary but insufficient

The model-backed boundaries have good mechanical checks: JSON parsing, required fields, known
goal IDs, non-empty rationales, ID collision rejection, stale-state discard, and deterministic
resolution. None verifies that a well-formed summary, goal, importance, or contradiction is
semantically correct and grounded.

### 6. Low-level fallback can be mistaken for practical runtime behavior

The model-client boundary selects the mock provider when its environment setting is absent or
unrecognized, but the supported `qsf.ps1 realtime` path always selects OpenAI and `qsf.ps1 sleep`
defaults to it. The mock provider makes experiments reproducible, but most responses are constant
and input-independent. It should be treated as a plumbing fixture, not as the practical default
for the main application. In particular, mock world-memory summaries are semantically false for
arbitrary real articles. Any future semantic fallback design needs an explicit contract distinct
from the mock test client.

### 7. Tests prove branches more than quality

The repository has substantial unit and trace-contract coverage. That is valuable and should be
preserved. What is missing is a common semantic-quality layer:

- human labels;
- train/validation/test separation;
- paraphrase clusters;
- hard negatives and negation;
- multilingual and code-switch cases;
- ASR-corrupted text;
- precision/recall or ranking metrics;
- confidence calibration;
- model/prompt version compatibility gates.

## Suggested evaluation program for expert review

This section is a handoff scaffold, not a committed implementation plan.

### Start with task contracts, not model choice

For each mechanism, define:

1. **Unit of input** — utterance, exchange, session, goal pair, query-memory pair, or article.
2. **Label space** — including explicit `none`, `ambiguous`, and `abstain` where appropriate.
3. **Action boundary** — current-turn context, cancellation, external read, diagnostics, or
   durable write.
4. **Cost matrix** — relative cost of each false-positive/false-negative pair.
5. **Latency and availability budget** — synchronous live, off-hot-path, or offline.
6. **Explanation contract** — exact spans, matched terms, source claims, or model version needed
   for trace replay.

### Build one versioned corpus with task-specific labels

A sanitized conversation corpus can share raw inputs while keeping separate annotations:

- turn acknowledgement/noise/interruption;
- memory-worthy fact and memory type;
- query-to-memory relevance grades;
- utterance-to-goal relevance grades for every goal, independent of arbitration tier;
- uncertainty, contradiction-opening, and conversational receptiveness;
- current-information intent and named-topic spans;
- durable-goal-worthiness and goal-pair contradiction;
- tool intent, including legitimate multi-tool and no-tool cases.

Article-summary faithfulness and project-document relevance likely need separate corpora.

Split by session and semantic/paraphrase cluster, not random utterance, to prevent near-duplicate
leakage. Keep a small human-curated held-out set frozen before tuning.

### Include adversarial and distributional slices

- paraphrases with the same intended label;
- stray cue words and idioms;
- explicit and implicit negation;
- quoted or reported speech;
- topic near-misses;
- short names, multiword names, lowercase names, and non-Latin names;
- English/Swedish and code-switched turns;
- punctuation/casing loss;
- real and synthetic ASR deletions, substitutions, hallucinated acknowledgements, and language
  switches;
- long sessions where earlier extracted errors recur in later inputs.

### Use metrics appropriate to the action

| Task family | Useful primary measures |
|---|---|
| Turn disposition / tool intent / memory capture | Per-class precision and recall, cost-weighted error, abstention coverage |
| Memory, corpus, and document retrieval | Recall@k, precision@k, nDCG/MRR, no-relevant-result accuracy, latency |
| Goal relevance | Per-goal PR curves, macro/micro F1, paraphrase consistency, stray-word flip rate |
| Shaping intensity | Ordinal agreement, weighted kappa, over-steering rate |
| Contradiction and durable-goal formation | High-precision PR, pairwise agreement, abstention/review rate |
| Summaries and extracted memories | Groundedness/attribution, omission rate, duplicate rate, human utility, downstream retrieval impact |
| Confidence-bearing models | Reliability curves, Brier score/ECE, threshold stability by slice |

Report metrics by language, ASR/no-ASR, input length, and label frequency. Overall accuracy will
be misleading for rare but high-cost events.

### Compare a ladder of baselines

The existing deterministic rules should remain the first baseline and a possible fail-safe. A
useful expert comparison might include:

1. current exact rules and weighted scores;
2. improved lexical retrieval such as BM25/IDF and phrase/field weighting;
3. frozen sentence embeddings with thresholded similarity;
4. a local utterance-to-description pair scorer (bi-encoder or cross-encoder);
5. a small supervised or distilled classifier where the label set is stable;
6. structured remote-LLM judging, with repeated-run and model-version analysis;
7. hybrid cascades with cheap high-precision rules, learned scoring, and abstention/human review.

The best answer may differ by surface. There is no reason for live interruption, memory ranking,
goal formation, and article consolidation to share one model.

### Preserve QSF's inspectability

Any learned replacement should emit enough data to replay the deterministic decision without
rerunning the model:

- task and model/prompt version;
- normalized input and source reference;
- candidate label set;
- raw scores and calibrated confidence if available;
- selected threshold and policy version;
- abstention/fallback reason;
- evidence spans or attribution where the task supports them;
- latency and hardware/provider path;
- final deterministic action and omitted alternatives.

For durable writes, retain the source text boundary and make automated artifact verification
assert those fields.

## Priority order for improvement work

This is a risk-oriented ordering for discussion, not an implementation commitment.

1. **Create evaluation data and error-cost contracts.** Without this, every replacement is judged
   by anecdotes and can overfit the current fixtures.
2. **Durable memory extraction (T9/T10) and live capture (T3).** These paths can persist errors and
   recursively feed them back into later classification.
3. **Live goal formation/coherence (T8).** It has good structural safety but high semantic and
   lifecycle impact.
4. **Volition relevance (T5) and memory relevance (T4).** These run frequently, already have
   deterministic baselines, and are suitable for ranking/pair-scoring evaluation.
5. **Turn disposition (T1).** The rule is tiny but the interaction cost is high; real audio-session
   examples are essential.
6. **World trigger/relevance (T7) and tool routing (T2).** Evaluate intent and retrieval separately
   from permission/execution policy.
7. **Opportunity/intensity (T6).** This benefits from human conversational judgments after core
   goal relevance is stable.
8. **Project-doc metadata/influence (T12/T13).** Useful, but low runtime risk and relatively easy
   to keep deterministic.

## Questions to take to the machine-learning expert

1. Which tasks should remain rules or ranking systems rather than classifiers?
2. For utterance-to-goal relevance, would a frozen multilingual embedding model already meet the
   robustness and latency bar, or is a cross-encoder/pair scorer justified?
3. How should relevance thresholds be calibrated when goal tiers remain a separate normative
   policy?
4. Which tasks need confidence and abstention, and which can safely make a hard decision?
5. What annotation protocol would produce reliable labels for “durable goal,” “memory-worthy,”
   “contradiction,” and shaping intensity?
6. How large must the held-out sets be to detect regressions in rare high-cost cases?
7. How should real ASR noise be sampled and augmented without leaking private session content?
8. For sleep extraction, is one multi-task prompt defensible, or should summary, memory
   worthiness, importance, relation extraction, and grounding be separate stages?
9. What is the smallest practical grounding contract for article and session memories—source
   spans, claim tuples, entailment checks, or human review?
10. Can model scores and explanations be made stable enough for trace replay, or should traces
    store only outputs and let deterministic policy own all decisions?
11. What offline model size/runtime is realistic for the available Windows/NVIDIA environment,
    and what CPU fallback preserves acceptable latency?
12. Which model and prompt changes should be blocked unless they pass a frozen compatibility
    suite?

## Source map

The most important current-truth entry points are:

- Memory: [`qsf_memory/src/retrieval.rs`](../../crates/qsf_memory/src/retrieval.rs),
  [`qsf_app/src/memory/live_capture.rs`](../../crates/qsf_app/src/memory/live_capture.rs)
- Volition lexical path: [`qsf_volition/src/selection.rs`](../../crates/qsf_volition/src/selection.rs),
  [`qsf_volition/src/opportunity.rs`](../../crates/qsf_volition/src/opportunity.rs),
  [`qsf_volition/src/shaping.rs`](../../crates/qsf_volition/src/shaping.rs)
- Volition LLM path:
  [`qsf_models/src/live_goal_formation.rs`](../../crates/qsf_models/src/live_goal_formation.rs),
  [`qsf_models/src/coherence_judge.rs`](../../crates/qsf_models/src/coherence_judge.rs)
- Runtime provider selection: [`scripts/qsf.ps1`](../../scripts/qsf.ps1),
  [`qsf_models/src/openai_provider.rs`](../../crates/qsf_models/src/openai_provider.rs)
- Sleep: [`qsf_app/src/sleep/session_summary.rs`](../../crates/qsf_app/src/sleep/session_summary.rs),
  [`qsf_app/src/sleep/world_memory_consolidation.rs`](../../crates/qsf_app/src/sleep/world_memory_consolidation.rs)
- Realtime turn/world routing:
  [`qsf_realtime_server/src/realtime/turn_integrity.rs`](../../crates/qsf_realtime_server/src/realtime/turn_integrity.rs),
  [`qsf_realtime_server/src/realtime/world_consultation.rs`](../../crates/qsf_realtime_server/src/realtime/world_consultation.rs)
- Project introspection:
  [`qsf_app/src/project_docs/search.rs`](../../crates/qsf_app/src/project_docs/search.rs),
  [`qsf_app/src/project_docs/metadata.rs`](../../crates/qsf_app/src/project_docs/metadata.rs),
  [`qsf_app/src/project_docs/influence.rs`](../../crates/qsf_app/src/project_docs/influence.rs)
