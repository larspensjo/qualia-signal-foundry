---
name: volition-arbitration-phase5-design
description: Design decisions for Phase 5 cross-goal arbitration — arbitration_tier field on Tension, thin arbitrate() function, experiment structure
metadata:
  type: project
---

# Volition Phase 5 Arbitration Design

## Context

Phases 2–4 built: static fixture (Phase 2), pre-initiative traces (Phase 3), and
durable-within-a-run salience/satisfaction/blocking/cooldown state (Phase 4).
`select_goals_with_salience` can return multiple selected goals simultaneously but
there is no cross-goal conflict resolution yet. This design document records the
decisions made before Phase 5 implementation begins.

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

### D4: Tiebreaker order

Same effective tier → higher `base_priority` wins. Still tied → lower `goal_id`
lexicographically. Purely deterministic; no new fields required.

### D5: Probabilistic arbitration is out of scope

The idea doc mentions probabilistic arbitration as a future option. If ever introduced,
it must be gated behind an explicit experiment mode flag and recorded in traces. Phase 5
is deterministic only.

## New Types

```rust
pub struct ArbitrationLoser {
    pub selection: GoalSelection,
    pub effective_tier: u8,
    pub reason: String,  // e.g. "tier 7 lost to winner at tier 1 (boundary-preservation)"
}

pub struct ArbitrationResult {
    pub winner: GoalSelection,
    pub winner_effective_tier: u8,
    pub losers: Vec<ArbitrationLoser>,
}
```

## New Function Signature

```rust
pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationResult>
```

Returns `None` when `selections` is empty (no conflict to resolve). The function is
pure and stateless — it reads `fixture` only to resolve tensions for each goal.

## Experiment Design

A `volition-arbitration-conflict` experiment scripts a multi-turn sequence where at
least one turn activates goals from at least two different tension tiers simultaneously.
A conflict turn like "Is the continuity thread complete enough to be confident in the
evidence?" can match keywords for `avoid-overstating-impl-status` (tier 1 via
`boundary-preservation`), `resurface-open-thread` (tier 5), and
`clarify-weak-evidence-topic` (tier 7), forcing arbitration to record two losers.

Each turn records: input, selection result, `ArbitrationResult` (winner + losers with
tier reasons), and an explicit no-execution marker. Replay must produce identical output.

## Extension Points

- Add tensions for tiers 2, 3, 6, 8 when the corresponding experiment needs them.
- A goal with `u8::MAX` default tier is a signal that it should be reviewed for a
  missing tension assignment before it becomes durable.
