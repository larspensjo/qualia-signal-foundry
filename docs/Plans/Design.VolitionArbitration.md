# Design: Volition Phase 5 Arbitration

## Status

Active — spec for Phase 5 of [Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md).
Decisions recorded in [DecisionLog.md](../DecisionLog.md) (2026-06-26).

## Context

Phases 2–4 built: static fixture (Phase 2), pre-initiative traces (Phase 3), and
durable-within-a-run salience/satisfaction/blocking/cooldown state (Phase 4).
`select_goals_with_salience` can return multiple selected goals simultaneously but
there is no cross-goal conflict resolution yet. This document records the decisions
made before Phase 5 implementation begins.

## Decisions

### D1: Separate `arbitration_tier` from `priority_bias`

Add `arbitration_tier: u8` to `Tension`. Lower tier wins. The existing `priority_bias`
field remains provenance-only (`TENSION_PRIORITY_NOTE` stays accurate and unchanged).

Rationale: `priority_bias` answers "how much does this tension matter generally?"
while `arbitration_tier` answers "when two goals conflict, whose tension takes
precedence?" These concepts can diverge as the system grows — a tension can have high
general priority but still lose arbitration to a safety boundary concern. Keeping them
separate prevents the two from drifting into each other.

Existing fixture mapping:

| Tension | `arbitration_tier` | Idea-doc category |
|---|---|---|
| `boundary-preservation` | 1 | Safety and project boundaries |
| `coherence-maintenance` | 4 | Coherence and self-correction |
| `continuity-preservation` | 5 | Continuity preservation |
| `research-curiosity` | 7 | Research curiosity |

Tiers 2 (explicit user intent), 3 (current task completion), 6 (active experiment
mode), and 8 (optional exploration) are not yet covered by any fixture tension. This
is an intentional extension point, documented in the `Tension` type and the experiment
spec. Future tensions should be assigned the correct tier when they are added rather
than squeezed into existing levels.

### D2: Thin arbitration layer — not baked into the selector

`arbitrate(selections, fixture) -> Option<ArbitrationResult>` is a pure function
composable after selection. Existing selectors and reducers are untouched. The
experiment runner calls: select → arbitrate → trace. Each phase's contribution
remains independently readable and testable.

Alternatives considered: integrating arbitration into a new combined selector (blurs
two distinct operations), or modeling arbitration as a `VolitionEvent` (arbitration is
a cross-goal view-model result, not a per-goal lifecycle transition).

### D3: A goal's effective tier = minimum arbitration tier among its parent tensions

A goal backed by multiple tensions (e.g. `avoid-overstating-impl-status` is backed by
both `boundary-preservation` tier 1 and `coherence-maintenance` tier 4) competes at
the lowest (best) tier among its parents: tier 1. A goal with no fixture tensions
defaults to `u8::MAX` (lowest priority).

When multiple parent tensions share the same minimum tier, the effective tension
recorded in the result is chosen by lexicographic `tension_id` (the lexicographically
smallest id among those at the minimum tier). This tiebreaker is deterministic and
avoids introducing a secondary sort field.

### D4: Tiebreaker order

Same effective tier → higher `base_priority` wins. Still tied → lower `goal_id`
lexicographically. Purely deterministic; no new fields required.

Loser ordering in `losers: Vec<ArbitrationLoser>` is also deterministic: sort by
effective tier ascending, then `base_priority` descending, then `goal_id`
lexicographically ascending (the winner is excluded).

### D5: Probabilistic arbitration is out of scope

The idea doc mentions probabilistic arbitration as a future option. If ever introduced,
it must be gated behind an explicit experiment mode flag and recorded in traces. Phase 5
is deterministic only.

## New Types

```rust
pub struct ArbitrationLoser {
    pub selection: GoalSelection,
    pub effective_tier: u8,
    pub effective_tension_id: String,    // tension responsible for this goal's effective tier
    pub effective_tension_title: String, // human-readable name of that tension
    pub reason: String,  // rendered sentence, e.g. "tier 7 lost to winner at tier 1 (boundary-preservation)"
}

pub struct ArbitrationResult {
    pub winner: GoalSelection,
    pub winner_effective_tier: u8,
    pub winner_effective_tension_id: String,    // tension that placed the winner at this tier
    pub winner_effective_tension_title: String, // human-readable name of that tension
    pub losers: Vec<ArbitrationLoser>,
}
```

The structured `effective_tension_id` and `effective_tension_title` fields make the
arbitration provenance testable without parsing the `reason` string. Tests must assert
the structured fields; `reason` is a convenience for display only.

## New Function Signature

```rust
pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationResult>
```

Returns `None` when `selections` is empty (no conflict to resolve). The function is
pure and stateless — it reads `fixture` only to resolve tensions for each goal.

For a single selection, returns `Some(ArbitrationResult)` with the sole goal as winner
and an empty `losers` list.

## Experiment Design

A `volition-arbitration-conflict` experiment scripts a multi-turn sequence covering
three distinct `arbitration_status` outcomes:

- `no_selection`: a turn where `select_goals_with_salience` returns no goals (baseline
  or direct-task input).
- `single_selection`: a turn with exactly one selected goal (no conflict); passes
  through as winner with empty losers list.
- `conflict_resolved`: a turn with ≥2 selected goals from different tension tiers;
  `arbitrate` resolves it and records a non-empty `losers` list.

A conflict turn like "Is the continuity thread complete enough to be confident in the
evidence?" can match keywords for `avoid-overstating-impl-status` (tier 1 via
`boundary-preservation`), `resurface-open-thread` (tier 5), and
`clarify-weak-evidence-topic` (tier 7), forcing arbitration to record two losers.

Each turn records: input, `arbitration_status`, selection result, `ArbitrationResult`
(winner + losers with structured tier and tension provenance), and an explicit
no-execution marker. Replay must produce identical output.

## Extension Points

- Add tensions for tiers 2, 3, 6, 8 when the corresponding experiment needs them.
- A goal with `u8::MAX` default tier is a signal that it should be reviewed for a
  missing tension assignment before it becomes durable.
