# Experiment: Curiosity-Observer Persona Seed

## Status

Running. The fixture and its coupled mechanics (introduced by
[Plan.CuriosityObserverPersona.md](../Plans/Plan.CuriosityObserverPersona.md)) are implemented and all
automated verification passes. Two voice sessions (2026-07-03) confirmed the persona's felt behavior —
see Results — but the live-formation half of the gate (Human Test step 5) is still open: session 1 ran
against the mock judge, and session 2's real-judge proposals failed to deserialize because of a
formation-prompt bug (fixed 2026-07-04). The remaining step is one retest voice session against the
fixed prompt.

This doc is the durable anchor for the fixture: its path
(`docs/Experiments/Experiment.CuriosityPersonaSeed.md`) is hardcoded as every seed goal's
`evidence_refs` / `source_reference`.

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
   against what the person reports, rather than only asserting one. Feed an utterance containing at
   least two of `ai, jobs, automation, economy, money, replace…` so `track-the-ai-transition` can win
   arbitration — this is also the only path that exercises `ProposeExperiment` (the term-driven effect
   selector requires two matched terms).
3. Say "I'd rather not talk about my job" (or an equivalent decline), then spend 2–3 turns on other
   topics, and confirm the persona backs off cleanly — no repeated probing, no pressing past the decline.
4. Confirm the persona refuses to state a thesis as fact — observation, inference, and speculation stay
   distinguishable in what it says.
5. Run the live-formation probes from
   [Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md) and confirm
   goals form/decline as expected against this persona's tension set.
6. Confirm turn latency is unchanged relative to a session using the prior seed fixture.

Run the session via `.\scripts\qsf.ps1 realtime` (the launcher pins `QSF_MODEL_PROVIDER=openai`, so the
formation judge runs on the real model — DecisionLog 2026-07-03). Diagnostic tell for a healthy judge:
`live_goal_formation_performed` records with real (hundreds-of-ms) `formation_started_at` →
`formation_completed_at` durations; sub-millisecond records mean the mock client.

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

### Session 1 (2026-07-03) — persona felt behavior confirmed; formation half void

One ~10-minute voice session (`state/realtime/diagnostics/default.jsonl`, 16 trusted exchanges across
10 calls — the calls were deliberate Stop-button pauses, not failures).

**Setup gap found (fixed):** `QSF_MODEL_PROVIDER` was unset, so the live-goal-formation judge ran on
the mock client, which always returns "no candidate": all 13 `live_goal_formation_performed` records
completed in < 1 ms and proposed nothing. The formation half of the test (Human Test step 5) was
structurally void. Fix: `qsf.ps1 realtime` now pins `QSF_MODEL_PROVIDER=openai` (DecisionLog
2026-07-03).

**Confirmed felt behaviors:**

- **Unprompted person-curiosity (step 1):** after "Busy week, heads down on the project",
  `learn-what-drives-this-person` activated (matched `i`, `project`), won initiative, and the reply
  asked what the project is about and what matters to the person. Traceable end-to-end.
- **Thesis/fact discipline (step 4):** `keep-theses-distinct-from-fact` won on `really` / `actually`
  matches and the responses explicitly separated observation / inference / speculation. Works, though
  the phrasing narrates the discipline a bit mechanically.
- **Decline-backoff (step 3):** clean in the one (unscripted) instance tested.
- **Latency (step 6):** volition injection 0 ms every turn; formation provably after response dispatch;
  transcript→first-audio avg 604 ms (max 906). No parity concern.
- **Keyword breadth handled by arbitration:** `learn-what-drives-this-person` activated on nearly every
  turn via `i`, but `serve-the-present-person` / `keep-theses-distinct-from-fact` won whenever they
  matched. The conversation did not feel interrogated.
- **Snapshot continuity:** all 10 reconnects restored the snapshot at the correct tick.

### Session 2 (2026-07-03 evening) — real judge confirmed; formation voided by a prompt bug

One ~4.5-minute session, one call, 10 trusted exchanges. The launcher fix worked: the formation judge
ran on the real model (1.1–2.1 s per call, all after response dispatch), and `prefix_cache_eligible`
flipped true from exchange 1 on. But both turns where the judge tried to propose a goal failed
deserialization — see [Experiment.LiveGoalFormationAndCoherence.md](Experiment.LiveGoalFormationAndCoherence.md)
Results for the root cause (the v1 prompt never enumerated the candidate JSON schema; fixed
2026-07-04). Persona behavior again looked right: `keep-theses-distinct-from-fact` won on `actually` /
`prove`, `serve-the-present-person` on `how` / `can` / `please`, `track-the-ai-transition` on `ai`
(only one matched term, so the `ProposeExperiment` threshold-2 path stayed unexercised). Anti-nag
suppressed repeats at exchanges 2/5/7; `protected_no_opportunity` at 8. Latency: transcript→first-audio
avg 848 ms (max 1267), volition injection 0 ms.

### Remaining gate

Steps 2 (AI-transition probing with ≥ 2 matched terms) and 5 (live-formation probes) are unvalidated;
re-run one voice session per the Human Test Steps against the fixed formation prompt.

## Open Items

- **Keyword tuning (resolved by weighted activation):** `learn-what-drives-this-person`'s broad
  first-person keywords (`i`, `my`, `me`) are intentionally near-universal. First live evidence
  (2026-07-03) showed activation was near-universal but arbitration handled it. The deterministic
  fix is now in place: activation keywords carry coarse weight classes and a global qualification
  threshold gates arbitration wins (weighted goal activation, DecisionLog 2026-07-04). Broad
  keywords like `i` / `my` / `me` / `what` / `how` are curated **Weak**, so they
  activate but cannot win a turn on their own — the step-2 AI-transition gate and stopword-only
  suppression are retested under the new mechanics via
  [Experiment.WeightedGoalActivation.md](Experiment.WeightedGoalActivation.md).
- **Internal-state narration tone:** one unprompted narration in session 1 ("In my simulated internal
  state, I've got a neutral focus on…") — the injected packet voiced verbatim. Tone issue to watch,
  not a defect.
- **Snapshot-discard guard never exercised live:** it only fires when a snapshot's goal ids mismatch
  the fixture, so it needs no live attention unless the fixture changes.

## Final Status

Not yet evaluated — the persona's felt behavior is confirmed (sessions 1–2), but the live-formation
half of the gate awaits a retest against the fixed formation prompt.

## Notes

Session-handling observations for whoever runs the retest (not bugs):

- **Stop button = new provider conversation.** A stopped call's transcript does not carry into the
  next call, so "please say again" after a Stop gets "I don't have anything to repeat yet" — re-ask
  the question instead. Volition state is unaffected (snapshot restore).
- **Room noise becomes hallucinated ASR text** (e.g. `いいね。`, `그게`) treated as trusted turns: each
  costs a tick and can produce a non-sequitur reply. Quiet room or push-to-talk helps.
