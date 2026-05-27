# Ideas: Association Proposers

## Status

Idea backlog. Opened alongside the Phase 5 landing of the sleep proposer
interface in `Plan.AssociativeRecallAndDropDrivenAssociations.md`. None of the
entries below are committed work; they are candidate proposers for the sleep
pipeline that need a measurable signal before promotion.

## Purpose

The Phase 5 design split mechanical association work (live, on drop and on
session end) from non-obvious associations (sleep, via a pluggable proposer
interface). Two proposers ship as of Phase 5: `LlmCandidateProposer` and
`SafetyNetCoRetrievalProposer`. Future proposers should enter through this
backlog with a stated signal, a noise risk, and an evaluation plan — not by
direct addition to the sleep pipeline.

This document lives outside the design because each candidate is an
experimental signal source, not a commitment. Promote an idea to a plan only
once it has been evaluated against a corpus and shows a measurable benefit
over what the current proposers already cover.

## Related

- `docs/Plans/Plan.AssociativeRecallAndDropDrivenAssociations.md`
- `docs/Plans/Design.AssociativeRecallAndDropDrivenAssociations.md`
- `docs/Architecture/Architecture.SleepPhase.md`
- `docs/Architecture/Architecture.MemorySystem.md`

## Candidate Proposers

### Two-hop bridge

Signal:
- Memories X and Z where no direct X↔Z edge exists but both connect to a
  shared neighbor Y. The presence of a bridge node Y suggests latent
  association between X and Z worth surfacing.

Risk of noise:
- Combinatorial blowup: every hub memory becomes a bridge for many
  unrelated pairs.
- Low-weight Y nodes act as weak bridges and inflate noise.

Evaluation criteria:
- Pick a per-pair lower bound on the bridge weight (or `min(w(X,Y),
  w(Y,Z))`).
- Measure precision on a fixture: how many proposed X↔Z edges look
  meaningful to a human reviewer vs. how many are spurious.
- Compare against the safety-net co-retrieval proposer on the same
  corpus; only promote if it surfaces edges the safety net misses.

### Common-substring / n-gram overlap

Signal:
- Shared rare n-grams across record summaries or titles indicate shared
  vocabulary that keyword/tag scoring may miss when phrasing differs.

Risk of noise:
- False positives on common terms; needs an IDF-style rarity weight.
- Sensitive to tokenization choices (punctuation, casing).

Evaluation criteria:
- Define rarity threshold against a session-local n-gram frequency table.
- Score proposed edges against a small human-labeled fixture.
- Verify that proposed edges do not duplicate edges the LLM-candidate
  proposer already surfaces.

### Cross-session co-retrieval

Signal:
- Pairs co-retrieved across separate sessions, not only within one. A
  durable cross-session co-occurrence is stronger evidence of association
  than a single in-session window.

Risk of noise:
- Requires per-session retrieval history, which is not currently
  persisted; today the cross-turn window lives inside one session run.
- Stale history may inflate edges that were once relevant but are no
  longer.

Evaluation criteria:
- First decide whether to persist a bounded per-session retrieval log.
- Define a recency horizon so cross-session evidence ages out.
- Compare proposed edges to those the in-session safety net already
  produces; the cross-session signal must add edges, not duplicate them.

### Tag-overlap rarity

Signal:
- Memories sharing rare tags. A tag held by only a handful of records is
  much stronger evidence of shared topic than a frequently used tag.

Risk of noise:
- Tag granularity drifts over time as memories are added; a tag that is
  rare today may become common.
- Tag policy is set elsewhere, so this proposer is sensitive to tagging
  conventions outside its control.

Evaluation criteria:
- Compute tag-frequency stats from the live store, not a fixed
  threshold.
- Track proposed-edge counts over time as the tag distribution shifts;
  recalibrate or disable if drift dominates.

### Hint-utility decay (resolves review A2)

Signal:
- Whether the model response references a hint memory, identified by a
  substring match on `hint.memory.title` or `hint.memory.id`. Edges
  whose hints are repeatedly unused decay; edges whose hints are used
  strengthen. This closes the live feedback loop the design explicitly
  scoped out.

Risk of noise:
- Substring matching is brittle for hints whose titles overlap with
  common words.
- Requires deciding what counts as a hint "being used" (text match,
  semantic match, downstream tool invocation, etc.).
- Decay schedule must avoid pruning rarely used but still valid edges.

Evaluation criteria:
- Start with strict title/id substring matching and measure precision
  on a fixture of real sessions.
- Define an explicit decay schedule (e.g. weight decrement per N
  unreferenced turns) and verify edges do not vanish on first non-use.
- Compare the resulting edge set against the no-feedback baseline.

### Edge-direction provenance (follow-up to review A1)

Signal:
- Today the `Association` record does not distinguish co-retrieval edges
  from LLM-asserted edges; `expand_neighbors` therefore treats all
  edges the same. Adding `edge_source: CoRetrieval | LlmCandidate |
  ...` would let the expander keep undirected behavior for co-retrieval
  edges while honoring LLM-asserted direction.

Risk of noise:
- Schema bump on `Association`; requires careful handling for legacy
  stores under `ASSOCIATION_SCHEMA_VERSION`.
- LLM-asserted direction may itself be unreliable; promoting it to
  first-class data must not exceed the LLM's actual reliability.

Evaluation criteria:
- Add the field behind a `#[serde(default)]` so existing stores load.
- Measure whether direction-aware expansion changes the hint set in a
  way that improves precision; if not, the schema cost is not
  justified.
- If a versioned migration is required, coordinate with the schema
  versioning rules in `docs/DecisionLog.md` (entry `2026-05-10 - Memory
  schema versioning is per record type and run artifacts are sealed`)
  and the bump-policy comments next to `MEMORY_RECORD_SCHEMA_VERSION` /
  `ASSOCIATION_SCHEMA_VERSION` in `crates/qsf_memory`.

## Notes

- Evaluation requires a corpus of memories — either a real session log
  or a fixture under `crates/qsf_app/src/experiments/`.
- A proposer must surface edges the existing pipeline misses; producing
  the same edges as the safety net or LLM-candidate proposer is not a
  reason to add a new proposer.
- Once a proposer ships, document it in
  `docs/Architecture/Architecture.SleepPhase.md` and update the entry
  here to point at the landed work.
