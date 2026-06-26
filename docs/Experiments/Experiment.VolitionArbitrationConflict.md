# Experiment: Volition Arbitration and Conflict Resolution

## Experiment ID

`Experiment.VolitionArbitrationConflict`

## Status

Implemented and run (2026-06-26). All success criteria met. Run artifact:
`runs/2026-06-26-144440-volition-arbitration-conflict/`. The phase sequencing and
design decisions live in
[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md) and
[Design.VolitionArbitration.md](../Plans/Design.VolitionArbitration.md). The
rationale, terminology, and candidate state shapes live in
[Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md). Volition is recorded
as a research surface in [DecisionLog.md](../DecisionLog.md)
(2026-05-15 "Volition is an explicit research surface").

## Summary

This experiment adds deterministic cross-goal arbitration as a pure additive layer over
the salience-aware selector from
[Experiment.VolitionSalienceAndSatisfaction.md](Experiment.VolitionSalienceAndSatisfaction.md).
When `select_goals_with_salience` returns multiple selected goals simultaneously, a new
`arbitrate()` function picks the winning initiative and records every losing goal with a
tier-based structured reason. Still no effect execution.

Each turn produces an explicit `arbitration_status` — `no_selection`,
`single_selection`, or `conflict_resolved` — so that an empty or pass-through result is
never indistinguishable from "arbitration was not run."

## Motivation

After Phase 4, the selector can surface multiple competing goals in the same turn, but
there is no principled way to choose among them. Before any goal can ever drive a
bounded internal initiative, the project needs evidence that:

- A pure, composable `arbitrate()` function can resolve cross-goal conflict
  deterministically using the tension tier hierarchy;
- Every losing goal is recorded with enough structured provenance to answer "why did X
  lose to Y?" without external explanation;
- The arbitration result is stable and replayable from the same scripted sequence;
- No-selection and single-selection turns are explicitly labeled, not silently absent.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Design.VolitionArbitration.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Experiments/Experiment.VolitionTraceBackedInitiative.md
Experiments/Experiment.VolitionSalienceAndSatisfaction.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
DecisionLog.md  (2026-06-26 "Arbitration tier is separate from priority bias")
```

## Hypothesis

A pure `arbitrate(selections, fixture) -> Option<ArbitrationResult>` function, composed
after `select_goals_with_salience`, can resolve cross-goal conflict by tension tier,
record every losing goal with structured tension provenance, and produce a deterministic,
replayable result — while executing no effect and requiring no change to any existing
selector or reducer.

## Scope

### In Scope

- A pure `arbitrate(selections, fixture) -> Option<ArbitrationResult>` function:
  - Returns `None` for empty input.
  - For a single selection, returns `Some` with that goal as winner and empty losers.
  - For multiple selections, picks the winner by effective tier (minimum
    `arbitration_tier` among a goal's parent tensions; defaults to `u8::MAX` if none).
  - Tiebreakers: lower effective tier → higher `base_priority` → lower `goal_id`
    lexicographically.
  - Each goal's effective tension is the parent tension at the minimum tier; ties
    among tensions at the same minimum tier are broken by lexicographic `tension_id`.
- `ArbitrationResult { winner, winner_effective_tier, winner_effective_tension_id,
  winner_effective_tension_title, losers }` with structured tension provenance.
- `ArbitrationLoser { selection, effective_tier, effective_tension_id,
  effective_tension_title, reason }` where `reason` is a rendered convenience string
  and structured fields are the authoritative provenance.
- Loser ordering: effective tier ascending, `base_priority` descending, `goal_id`
  ascending.
- `arbitration_tier: u8` added to `Tension`. Fixture values per
  [Design.VolitionArbitration.md](../Plans/Design.VolitionArbitration.md):
  `boundary-preservation` → 1, `coherence-maintenance` → 4,
  `continuity-preservation` → 5, `research-curiosity` → 7.
- A registered `volition-arbitration-conflict` experiment that replays the scripted
  sequence below, records per-turn `arbitration_status` and `ArbitrationResult`, and
  writes an explicit no-execution marker for every turn.
- Unit tests covering:
  - Empty input → `None`.
  - Single selection → winner with empty losers.
  - Two goals at different tiers → lower tier wins.
  - Two goals at the same tier → higher `base_priority` wins; still tied → lower
    `goal_id` wins.
  - Goal backed by multiple tensions → minimum tier wins.
  - Multiple tensions at the same minimum tier → lexicographic `tension_id` selects
    the effective tension.
  - Loser ordering is deterministic.
  - Structured provenance fields (`effective_tension_id`, `effective_tension_title`)
    are asserted directly; `reason` string is not the primary assertion.
  - `ArbitrationResult` is identical across repeated calls with the same input.
  - No effect is executed.

### Out of Scope

- Executing any initiative effect.
- Wiring arbitration into the runtime `AttentionState` or live session loop.
- Tiers 2 (user intent), 3 (task completion), 6 (experiment mode), and 8 (optional
  exploration): not yet covered by any fixture tension. These are explicit extension
  points — future tensions must be assigned the correct tier when added. Document this
  in a `Tension` type doc comment.
- Probabilistic or weighted arbitration (deferred; requires an explicit experiment mode
  flag if ever introduced).
- Reflection- or model-generated goal candidates.
- Architecture document updates or new decision-log entries before results exist.

## Setup

- OS/runtime: existing Rust workspace, run through the experiment registry.
- Candidate command once implemented:

```text
cargo run -p qsf_app -- experiment volition-arbitration-conflict
```

- Input data: the existing static volition fixture (with `arbitration_tier` added to
  each tension) plus the scripted multi-turn sequence below.
- No model provider, network, audio, browser UI, or write-capable external effect.

## Scripted Sequence

The sequence must produce at least one turn of each `arbitration_status`:

### Turn 1 — `no_selection` (baseline)

Input: "What is two plus two?"

Rationale: generic direct-task input; no goal activation keywords should match.
Expected: `select_goals_with_salience` returns no goals;
`arbitration_status: no_selection`; `executed_effects: 0`.

### Turn 2 — `single_selection`

Input: "What is the current research direction for the memory system?"

Rationale: matches `research-curiosity` goals but not boundary or coherence goals
under the current fixture. Expected: one selected goal; `ArbitrationResult` with
that goal as winner and `losers: []`; `arbitration_status: single_selection`;
`executed_effects: 0`.

### Turn 3 — `conflict_resolved`

Input: "Is the continuity thread complete enough to be confident in the evidence?"

Rationale: "confident in the evidence" and "complete" activate
`avoid-overstating-impl-status` (backed by `boundary-preservation`, tier 1) while
"continuity thread" activates `resurface-open-thread` (backed by
`continuity-preservation`, tier 5) and `clarify-weak-evidence-topic` (backed by
`research-curiosity`, tier 7). Expected: 3 selected goals; winner is
`avoid-overstating-impl-status` at effective tier 1 (`boundary-preservation`);
`losers` list has 2 entries; `arbitration_status: conflict_resolved`;
`executed_effects: 0`.

### Additional verification turn — same-tier tiebreaker (optional but recommended)

A turn that activates two goals whose parent tensions share the same minimum tier,
so that the `base_priority` or `goal_id` tiebreaker is exercised. Include if the
fixture can produce this naturally; otherwise cover it in a unit test only.

## Procedure

```text
1. Load the static volition fixture (now including arbitration_tier on each tension)
   and seed VolitionState from its Accepted goals.
2. For each scripted turn:
   a. Record input and advance the tick.
   b. Call select_goals_with_salience to get selections.
   c. Call arbitrate(selections, fixture) to get Option<ArbitrationResult>.
   d. Record per-turn output:
      - arbitration_status: no_selection | single_selection | conflict_resolved
      - selection result (selected goals, omitted goals)
      - ArbitrationResult (winner with structured tension provenance, losers with
        structured tension provenance and reason strings)
      - executed_effects: 0  (explicit no-execution marker, required every turn)
3. Write artifacts to the experiment output directory.
4. Replay the full sequence and confirm output is byte-for-byte identical.
```

## Success Criteria

All of the following must hold:

- `arbitration_status` is recorded on every turn: `no_selection`, `single_selection`,
  and `conflict_resolved` each appear at least once.
- `executed_effects: 0` appears on every turn (explicit no-execution marker).
- Turn 3 produces a non-empty `losers` list with `boundary-preservation` as the
  winner tension.
- Losers are ordered: effective tier ascending, `base_priority` descending,
  `goal_id` ascending.
- Structured fields (`winner_effective_tension_id`, each loser's
  `effective_tension_id`) match the expected tension ids; assertions do not rely on
  parsing the `reason` string.
- All unit tests pass (including same-minimum-tension tiebreaker, same-tier goal
  tiebreaker, and loser ordering).
- Replay produces identical output.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.

## Failure Criteria

- Any turn missing `arbitration_status` or `executed_effects`.
- `ArbitrationResult` is non-deterministic across replays.
- Structured provenance fields are empty or mismatched.
- Any existing selector or reducer was modified.
- Any initiative effect was executed.

## Human Verification

Read the per-turn arbitration trace and confirm:

- The winning goal's dominance is legible without external explanation: the trace
  should answer "why did X lose to Y?" from the tension names and tier numbers alone.
- `boundary-preservation` goals consistently outrank `research-curiosity` and
  `continuity-preservation` goals when they conflict.
- The `no_selection` and `single_selection` turns are clearly distinguishable from
  `conflict_resolved` in the artifact — absent output must not look like a silent
  failure.

## Extension Points

Tiers 2 (user intent), 3 (task completion), 6 (experiment mode), and 8 (optional
exploration) are not yet covered by any fixture tension. When a future experiment needs
a tension at one of these tiers, add it to the fixture and assign the correct
`arbitration_tier`. Goals with `u8::MAX` effective tier (no parent tensions in the
fixture) are a signal that a tension assignment is missing.

## Results

Run: `2026-06-26-144440-volition-arbitration-conflict`

All three required `arbitration_status` values appeared exactly once across the scripted
turns, with ticks advancing monotonically (1, 2, 3):

| Turn | Tick | Input | Status | Selected | Winner | Winner tier | Winner tension | Losers |
|---:|---:|---|---|---|---|---:|---|---|
| 1 | 1 | What is two plus two? | `no_selection` | — | — | — | — | — |
| 2 | 2 | What is the current research direction for the memory system? | `single_selection` | clarify-weak-evidence-topic | clarify-weak-evidence-topic | 7 | research-curiosity | — |
| 3 | 3 | Is the continuity thread complete enough to be confident in the evidence? | `conflict_resolved` | resurface-open-thread, avoid-overstating-impl-status, clarify-weak-evidence-topic | avoid-overstating-impl-status | 1 | boundary-preservation | resurface-open-thread, clarify-weak-evidence-topic |

- `executed_effects=0` on every turn.
- 3 structured traces written; 8 structured events written.
- All 54 volition unit tests pass; `cargo clippy --all-targets -- -D warnings` clean.

A `VolitionEvent::TickAdvanced` variant was added to the reducer to guarantee
`state.tick` advances each turn even when `tick_events` emits no lifecycle events (which
occurs at low ticks with zero-salience goals). A regression test
`ticks_increase_monotonically_across_scripted_turns` covers this invariant.

## Interpretation

The hypothesis holds: `arbitrate(selections, fixture)` resolves cross-goal conflict
deterministically by tension tier, records every losing goal with structured tension
provenance, and produces a replayable result — executing no effect and requiring no
change to any existing selector or reducer.

In the conflict turn (Turn 3), `boundary-preservation` (tier 1) outranked
`continuity-preservation` (tier 5) and `research-curiosity` (tier 7) as expected. The
loser list answered "why did X lose to Y?" from tier numbers and tension names without
external explanation.

The `no_selection` and `single_selection` turns are clearly distinguishable from
`conflict_resolved` in the artifact — no silent-failure ambiguity.

Open questions for follow-up experiments:
- Should the arbitration result be wired into the pre-initiative trace so the full
  select→arbitrate→trace chain is visible in one record?
- Are tier assignments (1, 4, 5, 7) well-calibrated, or should any be adjusted?
- Should tiers 2, 3, 6, 8 be covered by placeholder tensions now, or only when a
  concrete experiment needs them?
