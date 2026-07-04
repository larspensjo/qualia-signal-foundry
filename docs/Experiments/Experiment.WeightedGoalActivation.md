# Experiment: Weighted Goal Activation

## Status

Planned. The deterministic lexical layer (coarse keyword weight classes plus a global
qualification threshold) is being implemented; this doc is its durable validation gate. The
automated trace-contract criteria are carried by the Rust suite; the human voice retest is the
remaining gate.

## Summary

Goal activation gains a first-class **match strength**: every activation keyword carries a
coarse weight class (`Weak = 1`, `Normal = 4`, `Strong = 8`), a selection's `match_strength` is
the sum of its matched keywords' weights, and a global fixture-level
`arbitration_qualification_threshold` (default 4) gates arbitration *wins*. A selection still
activates, bumps salience, and appears in ranked selection below the threshold — only the
arbitration win is gated. When no selection qualifies, the turn is quiet and records a dedicated
`below_qualification_threshold` suppression instead of promoting a weak winner or falling back
to a protected-goal initiative.

This experiment validates that the mechanics produce the intended live behavior: a protected
goal can no longer win the initiative line on a stopword while a multi-term on-topic match
loses.

## Motivation

Live voice evidence (2026-07-04, `Experiment.CuriosityPersonaSeed.md` /
`Experiment.LiveGoalFormationAndCoherence.md`) showed binary token activation plus
strength-blind tier sorting letting a protected goal win the initiative line on a stopword
(`what` / `do`) against a five-term on-topic match. The deterministic lexical layer ships first
as a readable, tunable, no-GPU fallback; the long-term semantic direction is preserved
separately in `Idea.SemanticGoalActivation.md`. What we learn: whether coarse weights plus a
single threshold are enough to make qualification outcomes match human intuition on natural
phrasing, and whether the threshold default of 4 survives a live retest.

## Related Documents

```text
docs/Plans/Design.WeightedGoalActivation.md
docs/Plans/Idea.SemanticGoalActivation.md
docs/Architecture/Architecture.VolitionSystem.md
docs/Experiments/Experiment.CuriosityPersonaSeed.md
docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md
docs/DecisionLog.md
crates/qsf_volition/src/fixture.rs
crates/qsf_volition/src/selection.rs
crates/qsf_volition/src/arbitration.rs
crates/qsf_realtime_server/src/realtime/volition_injection.rs
```

## Hypothesis

With coarse keyword weights and a global qualification threshold of 4, the natural step-2
persona probe ("…what does that do to the economy?") selects `track-the-ai-transition` over
`serve-the-present-person`, and stopword-only turns produce a recorded
`below_qualification_threshold` suppression instead of a protected-goal initiative.

## Scope

### In Scope

- The deterministic lexical layer only: coarse keyword weight classes, `match_strength` as the
  single scoring quantity, the fixture-level qualification threshold, the arbitration
  qualification partition, and the no-winner turn decision with `below_qualification_threshold`
  suppression.
- Both shipped fixtures' curated weight classes.
- The human-voice validation that qualification outcomes match intuition on natural phrasing.

### Out of Scope

- Semantic scoring — stays in `Idea.SemanticGoalActivation.md`.
- Protected-floor semantics, mode bias mechanics, and the live-formation judge.
- Per-tier thresholds, corpus-derived weights, stemming, and phrase matching (deliberately
  deferred).

## Setup

- The curated `realtime_seed_fixture()` and `static_fixture()` in
  `crates/qsf_volition/src/fixture.rs`.
- The Rust workspace test suite (`cargo test --workspace`), which carries all automated
  verification including the trace-contract check.
- A live voice session against the realtime server for the human test, launched via
  `.\scripts\qsf.ps1 realtime` (pins `QSF_MODEL_PROVIDER=openai`).

## Procedure

### Automated Verification

Carried by the Rust suite; listed here so this doc is the index a reader checks against:

1. **Design probe strengths:** the natural step-2 probe gives `serve-the-present-person` a
   strength below the threshold and `track-the-ai-transition` a strength at or above it;
   stopword-only turns leave every selection below the threshold.
2. **Paraphrase robustness:** three wordings of the same AI-transition meaning select the same
   winner; a stray idiom prefix does not flip the winner.
3. **Qualification partition:** a sub-threshold protected goal loses to a qualified malleable
   goal and is recorded in `below_threshold`, never in the arbitration losers.
4. **No-qualifier turn:** an all-stopword turn yields no winner with the partition recorded and
   a `below_qualification_threshold` suppression.
5. **Persistence compatibility:** legacy plain-string activation keywords load and upgrade to
   Normal; a live-formed goal's single model-supplied keyword clears the default threshold.
6. **Trace contract (artifact-parsing):** serialized `VolitionContextInjectionTrace` JSON is
   reparsed, `match_strength` is recomputed from the recorded terms-with-weights, and the
   winner / no-winner outcome is checked against the recorded threshold (see below).

### Trace Completeness Contract

Required trace fields, per trusted realtime turn that emits a volition context packet (a
qualified winner, a below-threshold candidate, or a declined candidate exists; a trusted turn
with no lexical activation at all emits no packet and is outside this contract's scope):

```text
input                      — transcript ref (existing)
events_applied             — existing
selector_output            — existing + per-selected-goal matched keywords with weight
                             classes and match_strength
omitted_or_suppressed      — existing + matched keywords with weight classes and
  _candidates                match_strength on every below-threshold and
                             arbitration-losing candidate; below-threshold candidates
                             categorized `below_qualification_threshold`, never
                             `lower_arbitration_rank`
arbitration_result         — existing summary when a goal qualified; absent on a
                             no-qualifier turn
qualification_threshold    — the threshold in force, on the packet summary and the
                             turn-decision record
turn decision              — winner block optional; a no-qualifier turn records winner =
                             none plus suppression reason `below_qualification_threshold`
bounded_or_external_output — unchanged; the bounded-initiative trace stays reserved for
                             executed initiatives
```

Artifact boundary: diagnostics JSONL records (`VolitionContextInjected`, inspection captures)
carry the structured chain; the UI volition panel is a derived read-only view. Artifact-parsing
verification reparses serialized trace JSON, recomputes `match_strength` from the recorded
terms-with-weights, and checks the winner/no-winner outcome against the recorded threshold.

### Human Test Steps (voice session)

1. Start a realtime session via `.\scripts\qsf.ps1 realtime`.
2. Step-2 persona probe with natural phrasing: "Do you believe machines will replace many jobs,
   and what does that do to the economy?" — expect `track-the-ai-transition` to win and, on a
   rich match, `ProposeExperiment` to fire.
3. Deliberately weak turn: "For what it's worth, thanks." — expect no initiative and a
   `below_qualification_threshold` suppression in the diagnostics / inspection panel.
4. Latency parity: confirm the recorded
   `final_transcript_received_to_volition_context_injected` latency shows no regression
   (injection stays at 0 ms as established by the anti-nag work).

## Baseline

The prior binary-token activation with strength-blind tier sorting (pre-weight
`realtime_seed_fixture()`), which let a protected goal win on a stopword.

## Measurements

### Quantitative Measurements

- Rust test pass/fail counts for the automated verification items above.
- Recorded `final_transcript_received_to_volition_context_injected` latency, compared against
  the anti-nag baseline (expected 0 ms).

### Qualitative Observations

- Whether the winner on the natural step-2 probe matches intuition.
- Whether the weak turn is visibly quiet with a legible suppression reason.

## Success Criteria

The experiment succeeds if the three human-test observations hold, each tied to a trace field:
`track-the-ai-transition` wins the natural step-2 probe (and `ProposeExperiment` fires on a rich
match); the weak turn records a `below_qualification_threshold` suppression with no initiative;
and injection latency shows no regression.

## Failure Criteria

- `serve-the-present-person` (or any protected goal) still wins on a stopword-only turn.
- The weak turn produces an initiative instead of a recorded suppression.
- Injection latency regresses.
- The natural step-2 probe fails to select `track-the-ai-transition` despite a rich match.

## Required Observability

- The automated trace-contract test output (`cargo test -p qsf_realtime_server`).
- `VolitionContextInjected` diagnostics records and inspection captures carrying the weighted
  chain (matched keywords with weight classes, `match_strength`, `qualification_threshold`, and
  the winner / no-winner turn decision).

## Risks and Confounders

- Model variability across voice sessions.
- Curation subjectivity: the weight-class assignments are a reviewed fixture-data diff, not a
  corpus-derived optimum.
- Whether the threshold default of 4 is right is itself under test (see Results).

## Expected Output

- This experiment doc, as the durable validation gate.
- Passing Rust suite across the automated verification items, including the artifact-parsing
  trace-contract check.
- A completed human voice session with observations recorded in Results.

## Results

_To be filled in after the voice retest._

### What Happened

### Measurements

### Observations

### Surprises

### Failure Modes

## Interpretation

_To be filled in after the voice retest._

```text
Observed:
Interpreted:
Uncertain:
```

Record whether the threshold default of 4 survives (open decision — tune as fixture data if
not).

## Follow-Up Questions

- Does the threshold default of 4 survive the retest, or does live evidence favor another value?
- Do any goals need per-tier thresholds once semantic scoring lands?

## Follow-Up Experiments

```text
(Semantic activation — see Idea.SemanticGoalActivation.md)
```

## Decision Candidates

- Candidate: promote the surviving threshold value to a settled design decision.

## Final Status

Not yet evaluated — awaiting the human voice retest.

## Notes

Free-form notes to be added during the retest.
