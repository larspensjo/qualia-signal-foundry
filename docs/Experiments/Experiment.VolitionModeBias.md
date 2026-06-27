# Experiment: Volition Mode Bias

## Experiment ID

`Experiment.VolitionModeBias`

## Status

Running (automated tests pass; awaiting human review). The phase sequencing lives in
[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md); the design decisions live in
[Design.VolitionModeBias.md](../Plans/Design.VolitionModeBias.md); the rationale and terminology
live in [Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md).

## Summary

Add an inspectable **mode** — a named, declared bias over arbitration ordering — and show that it
can deterministically shift which goal wins a conflict **without** being able to override the
safety/boundary floor. A mode's meaning is its declared `bias_vector()`; the label is only a
handle (no free-form mood drives the bias).

The bias reorders goals **only within a biasable band** (effective tier ≥ 4 — coherence,
continuity, experiment mode, curiosity, exploration). A protected floor (effective tier ≤ 3 —
safety/boundary, explicit user intent, current task completion) is immune: a biased band goal is
clamped so it can never enter the floor. Bias applies to arbitration only; salience/selection and
proposal-threshold behavior are out of scope.

The scripted sequence covers four turns:

1. **Neutral baseline** — a band-only conflict under `Mode::Neutral`; record the winner.
2. **Mode flips winner** — the same input under `Mode::Exploratory`
   (`ModeChanged { mode: Exploratory, tick: 2 }`); the winner flips from the continuity goal to the
   curiosity goal.
3. **Floor immunity** — a conflict that also activates a tier-1 (boundary) goal, under a biasing
   mode; the winner stays the floor goal regardless of mode.
4. **Focused suppresses tangent** — the band-only conflict under `Mode::Focused`; the winner stays
   the continuity goal and the curiosity goal's `biased_tier` is recorded as increased (demoted).

Every turn applies **exactly one** `ModeChanged { mode, tick }` event that sets that turn's mode:
the baseline applies `ModeChanged { mode: Neutral, tick: 1 }`, and turns 2–4 apply `Exploratory`,
`Exploratory`, and `Focused` respectively (turn 3 reuses `Exploratory` and still applies its own
`ModeChanged`). The mode is always event-sourced — never an implicit default — so each turn's
`events_applied` contains a `ModeChanged` and replay reproduces `state.mode` exactly.

`executed_effects = 0` on every turn (no external side effect; no initiative is executed in this
slice).

## Motivation

The arbitration slice resolves cross-goal conflict by a fixed tier order. The project wants to
study whether a deterministic, inspectable bias can shift that outcome — the seed of a "mode" or
"mood" mechanism — while proving it cannot weaken the project's core boundary. Before any such
bias is trusted, the project needs evidence that:

- A mode is explicit, inspectable state, changed by a replayable event, with its bias expressed as
  a declared vector rather than a free-form label.
- The bias can change the arbitration winner among biasable goals.
- The protected floor is immune by construction: no mode can move a band goal into the floor, and
  a present floor goal wins under every mode.
- The whole bias chain (active mode → per-goal pre/post-bias tier → winner vs. the neutral winner)
  is traceable and replayable, with no model call and no external effect.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Design.VolitionModeBias.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionArbitrationConflict.md
Experiments/Experiment.VolitionBoundedInitiativeExecution.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
DecisionLog.md  (2026-06-26 "Arbitration tier is separate from priority bias")
```

## Hypothesis

A `Mode` whose declared `bias_vector()` adjusts a goal's effective arbitration tier — applied only
within the biasable band and clamped away from the protected floor — will deterministically flip
the arbitration winner among band goals, while leaving the winner unchanged whenever a
protected-floor goal is in contention. The shift is fully explained by the recorded per-goal bias
chain and reproduces identically on replay, with no model call and no external action.

## What This Experiment Does NOT Measure

- Mode bias on salience/selection scoring (which goals enter context) — out of scope this slice.
- Mode bias on the threshold for proposing new goal candidates/questions — out of scope.
- Whether the chosen `Focused`/`Exploratory` bias magnitudes are the *best* values — they are a
  starting fixture, easy to tune.
- Whether a mode *should* change mid-conversation in real use, or how a mode would be chosen
  outside a script (no model-inferred mode selection here).
- Cross-session persistence of the active mode.

## Scripted Inputs

Using the existing fixture keywords:

- `resurface-open-thread` — continuity-preservation, tier 5
  (`continuity`, `thread`, `revisit`, `open`, `unresolved`).
- `clarify-weak-evidence-topic` — research-curiosity, tier 7
  (`voice`, `memory`, `evidence`, `unclear`, `unsettled`).
- `avoid-overstating-impl-status` — coherence + boundary, effective tier 1, protected
  (`implemented`, `status`, `complete`, `done`, `ready`).

Band-only conflict input (turns 1, 2, 4) — matches continuity and curiosity, not the tier-1 goal:

```text
"The open thread about voice memory evidence is unresolved."
```

Floor input (turn 3) — additionally matches the tier-1 goal:

```text
"Is the voice memory work complete, or is the evidence thread still unresolved?"
```

## New Code

- New `Mode` enum (`Neutral`, `Focused`, `Exploratory`) with `Display`, serde
  (`#[serde(rename_all = "snake_case")]`), and `#[default]` on the `Neutral` variant;
  `Mode::bias_vector(self) -> BTreeMap<String, i8>` (the declared source of truth; empty for
  `Neutral`).
- `pub const PROTECTED_TIER_FLOOR: u8 = 3`.
- New `BiasOutcome`, `ModeArbitrationLoser`, `ModeArbitrationResult` types (per
  `Design.VolitionModeBias.md`).
- New pure `arbitrate_with_mode(selections, fixture, mode) -> Option<ModeArbitrationResult>`. Sort
  key `(biased_tier asc, base_priority desc, goal_id asc)`; band goals clamped to
  `>= PROTECTED_TIER_FLOOR + 1`; protected goals receive zero bias.
- `arbitrate` refactored to delegate to `arbitrate_with_mode(.., Mode::Neutral)` and map the neutral
  `ModeArbitrationResult` back into `ArbitrationResult` — its `Option<ArbitrationResult>` signature,
  fields, and serialized shape are unchanged (`ModeArbitrationResult` is not exposed through
  `arbitrate`).
- `VolitionState` gains `#[serde(default)] mode: Mode`; `from_fixture` seeds `Mode::Neutral`.
- `VolitionEvent::ModeChanged { mode, tick }`; reducer sets `state.mode`; `event_tick` handles it.
- New `volition-mode-bias` experiment registered in the experiment registry; it scripts the four
  turns and writes per-turn structured trace records plus a human-readable report.

## Success Criteria

### Automated (all must pass before the experiment is considered complete)

- [ ] `Mode::Neutral.bias_vector()` is empty, and `arbitrate_with_mode(.., Neutral)` produces the
  same winner/loser ordering as `arbitrate` for the same selection.
- [ ] `ModeChanged` updates `state.mode`; replay of the event log reproduces the same `state.mode`.
- [ ] Flip turn: under `Exploratory`, the winner differs from the `Neutral` winner on the same
  selection (`mode_changed_winner == true`), and the new winner is the curiosity goal.
- [ ] Floor turn: under a biasing mode, the winner equals the `Neutral` winner
  (`mode_changed_winner == false`) and is the protected tier-1 goal with `protected == true`.
- [ ] No band goal's `biased_tier` is ever `< PROTECTED_TIER_FLOOR + 1` (floor invariant holds by
  construction).
- [ ] Bias arithmetic is overflow-safe and clamps correctly: a goal with `effective_tier == u8::MAX`
  (no fixture tension) stays at `u8::MAX` under a positive demotion (no wrap); a large positive
  demotion never panics or wraps; and a promotion (negative bias) on a band goal is clamped to
  `PROTECTED_TIER_FLOOR + 1` rather than entering the protected floor.
- [ ] `Focused` turn: the curiosity goal's `bias_applied` is positive and its `biased_tier`
  exceeds its `effective_tier`; the winner stays the continuity goal.
- [ ] Bias is attributed to the goal's effective tension; `per_goal_bias` records
  `effective_tension_id`, `bias_applied`, and `biased_tier` for every contending goal.
- [ ] `arbitrate_with_mode` is deterministic: same selection + fixture + mode → identical result.
- [ ] All prior tests pass; existing `arbitrate` / selector / reducer behavior is unchanged.
- [ ] `arbitrate` keeps its `Option<ArbitrationResult>` return type and serialized shape; it is not
  changed to return `ModeArbitrationResult`.
- [ ] `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- [ ] `executed_effects = 0` on every experiment turn.
- [ ] The run artifacts satisfy the trace contract (the runner parses the generated trace records
  and asserts the required fields and the flip/floor outcomes above).

### Human (requires running the experiment and reading the report)

- [ ] The active mode and its bias vector are legible in each turn's trace; a reader can see *why*
  the ordering changed without rerunning the code.
- [ ] The flip (Exploratory) and the non-flip (Focused, and the floor turn) read as sensible,
  deterministic consequences of the declared vectors — not arbitrary.
- [ ] The floor-immunity turn is convincing: nothing suggests a mode could elevate a band goal
  above the safety/boundary goal.
- [ ] The mode is clearly a structural handle over an explicit vector — there is no free-form mood
  label doing hidden work.
- [ ] Nothing in the output implies the mode caused any external write-capable action.

## Results / Interpretation

_To be filled in after the first run._

## Failure Modes

- A bias magnitude too small to cross the gap between two band tiers would never flip the winner;
  the `Exploratory` vector must move curiosity below continuity for the flip turn to demonstrate
  anything.
- If `arbitrate` is not kept equivalent to `arbitrate_with_mode(.., Neutral)`, the existing
  arbitration experiment and tests would diverge — the delegation must be behavior-preserving.
- A mode that biases a tension in the protected floor produces no effect; this is correct but
  could surprise someone who expected the bias to apply, so the trace marks `protected` per goal.
- A goal whose `tension_ids` name a tension absent from the fixture has effective tier `u8::MAX`;
  bias cannot rescue it. Correct, but a signal to review the missing tension assignment.

## Follow-Up Questions

- Should mode also bias salience/selection scoring (which goals enter context), not just
  arbitration ordering?
- Should a mode raise the threshold for proposing new goal candidates/questions, completing the
  idea doc's `Focused` example?
- How would a mode be chosen outside a script — by user setting, by an explicit experiment mode,
  or (later, gated) by a model-assisted evaluator emitting `ModeChanged`?
- Should the active mode persist across sessions, and if so, which fields are live vs. durable?
