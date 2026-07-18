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
- weighted query terms tagged `goal_activation` or `current_topic`;
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

## Follow-Up Questions

- Is the provisional 5 ms inline lookup budget appropriate under load?
- What anti-repeat window avoids nagging without suppressing genuinely useful updates?
- Does goal activation or current topic deserve greater lexical weight?
