# Experiment: World Memory Consolidation

## Status

Running

## Question

Can offline sleep turn a bounded set of new external corpus articles into inspectable, correctly
attributed, decaying durable world-memories without polluting the memory store?

## Hypothesis

When sleep receives the corpus content-hash delta, it can summarize only sandboxed external
article text and promote a small, substantive subset as untrusted world observations. Full source
attribution, the time-sensitive decay profile, and URL-based supersession should keep recall
honest and prevent stale or low-value news from accumulating.

## Scope

- `qsf_app sleep` corpus refresh, content-hash delta, provider-backed article summary, and
  durable promotion.
- `WorldMemoryConsolidated` run artifact, source-to-ledger joins, eligibility decisions, decay,
  associations, and supersession-lite validation.
- Recall framing for durable external observations.

Out of scope: live consultation writes, semantic retrieval, general contradiction resolution,
and tuning the rule beyond the first conservative implementation.

## Setup and Baseline

Run sleep with the bundled fixture corpus by default, or select the producer output with
`QSF_WORLD_CORPUS_PATH` / `qsf.ps1 sleep -WorldCorpusPath <output>`. Use the mock provider for
deterministic automated coverage. The baseline is a corpus delta with no eligible article:
sleep must leave durable world-memory unchanged while writing an eligibility reason.

The current provisional eligibility rule is named and intentionally narrow: articles require at
least 60 non-whitespace body characters, and sleep promotes at most the two newest delta articles
per run. Articles rejected only because that cap was reached remain pending in consolidation state
and are reconsidered on the next sleep run even when the corpus is unchanged; rule-based
ineligible articles remain marked as seen. The current substrate default is a 7-day
world-observation half-life.

The bundled fixture remains promotion-capable when no corpus path is configured, so the default
continues to exercise world-memory consolidation. If an explicitly configured corpus path is
unavailable and resolution degrades to that fixture, sleep still evaluates and records the delta
but promotes nothing from the fallback. The degradation reason is logged and retained in the
authoritative run artifact.

## Procedure

1. Run sleep against the fixture corpus and inspect `runs/<run-id>/world-memory-consolidation.json`.
2. Join each promoted `content_hash` back to the persisted corpus ledger/index.
3. Inspect the memory store for external provenance, untrusted trust tier, structured source
   attribution, and time-sensitive decay.
4. Add a newer fixture article at an existing URL, run sleep again, and confirm the older memory
   is superseded rather than resurfacing.
5. Add or select an ineligible delta article and confirm no memory is promoted and its reason is
   recorded.
6. Leave one substantive delta article beyond the per-run cap, rerun sleep without changing the
   corpus, and confirm the deferred article is reconsidered and promoted.
7. Configure an unavailable corpus path and confirm the fallback delta is recorded without any
   promotion; then remove the configuration and confirm the ordinary fixture default can promote.
8. In a live voice session after sleep, confirm recalled world facts say they are recalled
   untrusted source claims rather than claiming a fresh lookup.

## Measurements and Success Criteria

- Promotion count versus eligible and ineligible delta counts.
- Every promoted memory has world-observation provenance, `untrusted_external` trust, complete
  source attribution, and a decay profile.
- Every model input containing article text includes the shared untrusted-external wrapper.
- A newer same-URL source leaves exactly one non-superseded current world-memory.
- A review can parse the authoritative artifact and resolve all promoted hashes to the index.
- Human testing finds useful recall without an unacceptable accumulation of low-value news.

## Trace Contract

`WorldMemoryConsolidated` in `runs/<run-id>/world-memory-consolidation.json` is authoritative.
For every promoted memory it carries content hash, title, URL, source domain, fetch UTC, trust
tier, decay profile, eligibility decision and reason, any supersession link, and formed
association identifiers. It also records every delta article's eligibility decision and reason.
When corpus resolution degraded from an explicitly configured path, it also records the
degradation reason and the per-article promotion skips.

The corpus ledger remains a separate artifact and is joined only through `content_hash`.
Automated verification parses the record, verifies each promoted hash exists in the indexed
corpus, and requires `untrusted_external` plus a non-empty decay profile.

## Risks

- The lexical corpus may contain many short or repetitive articles; the cap could exclude a more
  valuable older article.
- URL equality is conservative: it prevents accidental semantic merges but misses broader topic
  supersession.
- The 7-day half-life and 60-character/two-article limits are first-evidence defaults, not
  validated tuning results.
- Provider summaries can be inaccurate, so external attribution and untrusted framing remain
  mandatory even after durable promotion.

## Results-pending

Automated fixture coverage is complete for promotion, untrusted framing, ineligibility,
artifact-to-index joins, and same-URL supersession. A human live-session recall probe and
real-corpus evidence are pending. The experiment owns two open questions: whether the
provisional eligibility rule admits useful durable memories without store pollution, and whether
the 7-day news half-life matches observed usefulness.
