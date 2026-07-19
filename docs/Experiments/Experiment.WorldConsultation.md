# Experiment: World Consultation

## Status

Running

## Question

Can Ari's curiosity-backed goals consult the read-only external AI-news corpus and place a
relevant, clearly attributed, untrusted external claim into live context without disturbing the
realtime response budget?

## Hypothesis

For a user turn that activates `track-the-ai-transition` or `assemble-world-picture`, a lexical
lookup grounded in both goal activation and current-topic terms can surface a relevant fact with
full provenance. User-input queries can be injected inline when they remain within the hard
budget; otherwise the same fact is deferred to the next turn.

## Scope

- Pure `ConsultWorld` volition request, including query-term provenance.
- Read-only corpus lookup, eligibility and per-session anti-repeat suppression.
- Untrusted-external framing, inline/deferred delivery, diagnostic trace, and browser diagnostic
  capture.
- Real-corpus latency measurement.

Out of scope: semantic retrieval, durable world memory, trust-tier memory fields, sleep
consolidation, and a true open-question substrate.

## Setup and Baseline

Use the bundled fixture by default and optionally set `QSF_WORLD_CORPUS_PATH` to the WPFM
`output/` directory. The baseline is a normal realtime turn without consultation. The measured
pre-adapter real-corpus lexical probe (6,304 articles, development build) is **2 ms**, after
bounded top-candidate selection replaced full candidate materialization and sorting.

## Procedure

1. Run a controlled user turn activating an AI-transition or world-picture goal.
2. Inspect the emitted consultation trace and model-visible turn context.
3. Repeat with the same article eligible to verify anti-repeat suppression.
4. Run an answer-derived query and verify it is deferred.
5. Compare inline turn timing with the established 600–850 ms transcript-to-first-audio envelope.

## Measurements and Success Criteria

- Lookup latency, injection point, candidate count, and surfaced-fact count.
- Every surfaced fact resolves by `content_hash` to the index and is framed as an external,
  untrusted claim with title, source domain, and fetch time.
- `external_effect_executed` is true only for `ConsultWorld`; `RetrieveContext` remains false.
- A lookup exceeding `WORLD_CONSULT_INLINE_BUDGET_MS` defers rather than delaying a response.
- A human live-session run confirms relevance, honest first-person wording, and no audible
  latency regression.

## Trace Completeness Contract

`WorldConsultationPerformed` is the authoritative JSONL record. It carries:

- session/exchange/request references; serving goal and tension ids;
- query terms tagged `goal_activation` or `current_topic` (no numeric weights: required-anchor
  decisions and candidate omission reasons are the relevance surface);
- chosen required anchors, plus every candidate's score, matched terms, provenance, and
  eligibility or omission reason (including `missing_required_anchor`);
- exact injected untrusted blocks and their source provenance;
- lookup latency, injection point and reason, corpus marker metadata, and
  `bounded_or_external_output.external_effect_executed: true`.

Artifact boundary: diagnostics JSONL is authoritative; the `world_perception` socket message is
the live diagnostic projection; the persisted corpus ledger/index remains separate and is joined
only through `content_hash`.

Automated verification parses a generated record, checks every required field, verifies each
surfaced hash resolves in the index, and checks the external-effect boundary.

## Risks and Confounders

Common lexical terms may return weakly related news. The current v1 query is not a true open
question, only goal activation plus current topic. Provider and network latency are outside the
lookup measurement; a 2 ms development probe is evidence for the adapter gate, not a completed
live-audio result.

## Results

The realtime adapter is now wired. It loads `QSF_WORLD_CORPUS_PATH` once at startup (or the
bundled fixture), retains a read-only index, logs ingestion/schema degradation visibly, combines
goal-activation and current-topic terms, frames only hash-resolved articles as untrusted external
material, and records `WorldConsultationPerformed` JSONL. Fixture and synthetic tests cover a
framed fact, delimiter neutralization, session anti-repeat suppression, deferred slow lookup,
JSONL parsing/hash resolution, and the `ConsultWorld: true` / `RetrieveContext: false` effect
boundary. The 2 ms real-corpus query probe remains the baseline; a human live-session latency
measurement is still pending.

The relevance follow-up adds a narrow explicit-topic trigger: a named entity or dotted version
must occur with a current-information cue, so ordinary turns remain outside the external-read
path. Query construction drops generic interrogatives, retains other meaningful terms for lexical
ranking, and requires only the entity/version signals detected from the original prompt. Fixture
tests verify that both `Grok 4.5 release` and `Tell me about the latest Grok release` inject only a
framed untrusted Grok fact, an otherwise lexical generic release article is omitted as
`missing_required_anchor`, and an unknown named release records the performed external read with
no injection. Two-character AI-domain entity anchors (`AI`, `AR`, and `VR`) remain searchable. The
existing content-hash repeat, latency-budget, sandbox, and effect-boundary tests remain in place.

A 2026-07-18 live voice probe verified the explicit-topic trigger end-to-end against the bundled
fixture corpus (session launched without `QSF_WORLD_CORPUS_PATH`; corpus marker
`fixture-producer`, 4 articles). The prompt naming the Grok 4.5 release produced a
`WorldConsultationPerformed` record with anchors `grok` and `4.5`, a 0 ms lookup, inline
same-turn injection of exactly one hash-resolved framed fact (a lexical generic candidate was
omitted `missing_required_anchor`), `external_effect_executed: true`, and a spoken answer that
attributed the claim to "what that external source claims" rather than presenting it as own
knowledge. This closes the trigger/relevance/framing question live. Still open: the same probe
against the real WPFM corpus with a topic the corpus actually covers — the most recent
real-corpus session (2026-07-18, 6,925 articles) surfaced nothing, with every candidate omitted
`missing_required_anchor`, so real-corpus relevance remains unobserved.

A 2026-07-19 real-corpus voice session (6,925 articles) confirmed and sharpened that no-surface
result. A turn about high-bandwidth memory in AI data centers activated
`track-the-ai-transition` (`ai-trajectory-concern`) and performed a genuine external read: 4 ms
lookup, inline same-turn, `external_effect_executed: true`, eight candidates. The goal-activation
gate — which requires a candidate to match **all** meaningful query terms — produced 13 required
anchors including uninformative terms ("was", "by", "used", "little", "example", "thinking"),
and omitted every candidate as `missing_required_anchor`, including an exactly on-topic article
("Memory Chips Skyrocket Amid AI Data Center Buildout") that matched 11 of 13. Nothing was
injected; the spoken answer honestly used only parametric knowledge. A follow-up turn — "Can you
find information about what the biggest players are in this market?" — triggered no consultation
at all: the explicit path's cue lexicon lacks plain search-request forms and the turn named no
entity, while goal activation had no lexical signal ("this market" is anaphoric). Two additional
observations: the live goal-formation judge admitted a new goal
(`understand-hbm-memory-constraints`) from the topic, and the `world_perception` diagnostic
capture carried the complete trace that made this analysis possible. Consultation-turn
transcript-to-first-audio latency was 820 ms (top of the 600–850 ms envelope; the lookup itself
contributed ~4 ms). Conclusion: the external-effect, budget, framing, and trace mechanisms hold;
the goal-activation require-all anchor policy and the cue lexicon are the remaining blockers to
useful real-corpus surfacing. The anchor-relaxation and search-request-cue slice in
`Plan.WorldPerception.md` owns the fix.

## Follow-Up Questions

- Is the provisional 5 ms inline lookup budget appropriate under load?
- What anti-repeat window avoids nagging without suppressing genuinely useful updates?
- Does goal activation or current topic deserve greater lexical weight?
