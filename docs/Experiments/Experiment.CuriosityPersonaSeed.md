# Experiment: Curiosity-Observer Persona Seed

## Status

Planned. This is the scaffold for the seed persona introduced by
[Plan.CuriosityObserverPersona.md](../Plans/Plan.CuriosityObserverPersona.md); it is created before the
fixture that hardcodes this doc's path (`docs/Experiments/Experiment.CuriosityPersonaSeed.md`) as every
seed goal's `evidence_refs` / `source_reference`, so no commit ever ships a fixture pointing at a missing
doc. This doc is the durable anchor and the index of automated coverage; it is updated as the fixture and
its coupled mechanics land, and completed once the human voice test (the real gate) has been run.

## Summary

`realtime_seed_fixture()` (`crates/qsf_volition/src/fixture.rs`) is rewritten as a standalone,
outward-facing **curiosity-observer** persona: seven tensions — three protected
(`person-respect`, `epistemic-integrity`, `present-person-priority`) and four malleable
(`knowledge-stewardship`, `person-curiosity`, `ai-trajectory-concern`, `world-curiosity`) — backing seven
`Accepted` seed goals (`respect-persons-boundaries`, `keep-theses-distinct-from-fact`,
`serve-the-present-person`, `grow-the-library`, `learn-what-drives-this-person`,
`track-the-ai-transition`, `assemble-world-picture`). This experiment validates that the persona is felt
in conversation, not just structurally present in fixture data.

## Motivation

The prior realtime seed fixture was not an intentional persona — it was a superset of the static test
fixture. This slice gives the realtime session a deliberate character (curious about the person present,
tracking the AI transition, careful about the boundary between thesis and fact) and, as an enabling
side effect, moves mode bias out of hardcoded `Mode::bias_vector()` and into per-tension fixture data, so
future persona swaps are data-only. What we learn: whether a fixture-level roster of tensions and goals
actually surfaces as the intended conversational behavior, or whether the persona is invisible/inert in
practice (in which case the fixture, keyword set, or selection mechanics need revision before the
persona can be trusted).

## Related Documents

```text
docs/Plans/Plan.CuriosityObserverPersona.md
docs/superpowers/specs/Design.curiosity-observer-persona.md
docs/Architecture/Architecture.VolitionSystem.md
docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md
docs/Experiments/Experiment.VolitionModeBias.md
docs/DecisionLog.md
crates/qsf_volition/src/fixture.rs
crates/qsf_volition/src/selection.rs
crates/qsf_volition/src/reducer.rs
crates/qsf_realtime_server/src/realtime/volition.rs
```

## Hypothesis

The curiosity-observer seed is felt in conversation: the persona asks about the person and their work
unprompted, probes AI-transition theses, backs off cleanly from a topic the person declines, and refuses
to state a thesis as fact.

## Scope

### In Scope

- The seven-tension / seven-goal `realtime_seed_fixture()` roster and its structural invariants.
- The mechanics the persona's intended behavior exercises: per-tension mode bias (not hardcoded), a
  term-driven effect selector so `track-the-ai-transition` can reach `ProposeExperiment`, idle-retirement
  immunity for seed-fixture goals, and a fixture-compatibility guard on snapshot resume.
- The human-voice validation of the persona's felt behavior: unprompted curiosity about the person,
  AI-transition probing, clean decline-backoff, and thesis/fact discipline.

### Out of Scope

- New volition mechanics beyond the four listed above (this experiment validates the persona seed, not
  new architecture).
- Cross-session persistence of persona state (covered elsewhere).
- Any change to arbitration, coherence, or sleep-formation logic beyond what already exists per
  [Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md).

## Setup

- The rewritten `realtime_seed_fixture()` in `crates/qsf_volition/src/fixture.rs`.
- The Rust workspace test suite (`cargo test`), which carries all automated verification below —
  no separate harness or fixture file is needed for the automated checks.
- A live voice session against the realtime server for the human test.

## Procedure

### Automated Verification

Already carried by the Rust suite; listed here so this doc is the index a reader can check against, not
a duplicate of the test code:

1. **Fixture invariants:** tension and goal ids are unique; every goal's `tension_ids` resolve to a
   tension in the fixture; at least one tension sits at or below the protected floor; every seed goal is
   `Accepted` with non-empty `activation_keywords`; every protected-tier tension has zero
   `focused_bias` / `exploratory_bias`; the fixture is standalone (not a `static_fixture()` superset); and
   every goal's `evidence_refs` / `source_reference` resolve to a durable doc that exists on disk
   (this doc and `docs/DecisionLog.md`).
2. **Stance ordering:** the rendered volition stance places the minimum-`arbitration_tier` tension first.
3. **Effect reachability:** `track-the-ai-transition` proposes (`AllowedEffect::ProposeExperiment`) on a
   rich match of AI-transition terms and falls back to `Reflect` otherwise.
4. **Neutral-mode zero bias from data:** `Mode::Neutral.tension_delta(..)` is `0` for every tension,
   sourced from tension data rather than a hardcoded vector.
5. **Idle-retirement immunity:** seed-fixture goals never idle-retire under `tick_events`, while a
   live-formed candidate absent from the fixture does retire when idle past the inactivity window.
6. **Snapshot discard on fixture mismatch:** a resumed session snapshot that is incompatible with the
   current fixture is discarded rather than installed.

### Human Test Steps

The real gate. Recommended over a live voice session:

1. Confirm the persona asks about the person and their work unprompted (not only in direct response to a
   question about them).
2. Confirm it probes AI-transition theses — testing a thesis about AI's effect on work/economy/power
   against what the person reports, rather than only asserting one.
3. Say "I'd rather not talk about my job" (or an equivalent decline) and confirm the persona backs off
   cleanly — no repeated probing, no pressing past the decline.
4. Confirm the persona refuses to state a thesis as fact — observation, inference, and speculation stay
   distinguishable in what it says.
5. Run the live-formation probes from
   [Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md) and confirm
   goals form/decline as expected against this persona's tension set.
6. Confirm turn latency is unchanged relative to a session using the prior seed fixture.

## Baseline

The prior (superset-of-`static_fixture()`) realtime seed fixture, and — for latency — a session with no
formation/coherence overhead, per
[Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md).

## Measurements

### Quantitative Measurements

- Rust test pass/fail counts for the automated verification items above.
- Turn latency, compared against the prior seed fixture.

### Qualitative Observations

- Whether curiosity about the person and the AI transition is legible in the persona's actual utterances.
- Cleanliness of the decline-backoff (no repeated probing after a decline).
- Clarity of the thesis/fact distinction in what the persona says.
- Any keyword-activation surprises (see Open Items).

## Success Criteria

The experiment succeeds if every automated verification item passes and the human voice test confirms
all five felt behaviors in the hypothesis (unprompted person-curiosity, AI-transition probing, clean
decline-backoff, thesis/fact discipline, and unchanged turn latency).

## Failure Criteria

- Any automated invariant fails (e.g. a fixture reference points at a missing doc, or a protected
  tension carries non-zero bias).
- The persona is structurally present but not felt — e.g. it never surfaces person-curiosity or
  AI-transition interest unprompted in a real session.
- The decline-backoff is not clean (the persona presses past a stated decline).
- The persona states a thesis as settled fact.
- Turn latency regresses relative to the prior seed fixture.

## Required Observability

- Fixture-invariant test output (`cargo test -p qsf_volition`).
- Volition stance / injection records already produced per
  [Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md) — this
  experiment introduces no new trace fields.

### Trace Completeness Contract

Not applicable. This experiment validates fixture content and existing mechanics; it introduces no new
trace fields or artifacts beyond those already specified in
[Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md).

## Risks and Confounders

- Subjective evaluation of "felt" persona behavior in the human test.
- Keyword tuning: several seed goals' `activation_keywords` include very common words (see Open Items),
  which could cause over-activation or under-signal in the live selection/arbitration interplay in ways
  the offline fixture tests cannot surface.
- Model variability across voice sessions.

## Expected Output

- This experiment doc, as the durable index of automated coverage and the human-test gate for the
  curiosity-observer persona.
- Passing Rust suite across the fixture invariants and the three bounded mechanics fixes.
- A completed human voice test session with observations recorded in Results.

## Results

Pending. To be filled in once the fixture (Task 2.2) and the three bounded mechanics fixes (Phase 3 of
`Plan.CuriosityObserverPersona.md`) have landed and the human voice test has been run.

## Open Items

- **Keyword tuning:** `learn-what-drives-this-person`'s activation keywords include `i`, `my`, `me`,
  which are intentionally near-universal (almost any first-person utterance matches). This is a
  deliberate starting point, not an oversight — observe how selection scoring and arbitration actually
  interact with such broad keywords in a live session before narrowing them.

## Final Status

Not yet evaluated — automated verification and the human voice test are pending as the fixture and its
coupled mechanics land.

## Notes

None yet.
