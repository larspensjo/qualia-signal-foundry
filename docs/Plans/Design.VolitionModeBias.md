# Design: Volition Mode/Bias

## Status

Active — spec for the optional mode/bias slice of
[Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md). Decisions to be promoted to
[DecisionLog.md](../DecisionLog.md) on completion.

## Context

The earlier volition slices built: a static fixture and deterministic selector, pre-initiative
traces, durable-within-a-run salience/satisfaction/blocking/cooldown state, deterministic
cross-goal arbitration, reflection-generated goal candidates, and bounded internal initiative
execution. Arbitration (`arbitrate` in `crates/qsf_app/src/volition.rs`) is **strict-tier**: a
goal's effective tier is the minimum `arbitration_tier` among its parent tensions, and the lowest
tier wins (ties broken by `base_priority` descending, then `goal_id` ascending).

This slice adds an inspectable **mode** — a named, declared bias over arbitration ordering — so
the project can study whether a deterministic, traceable bias can shift selection behavior
without introducing free-form simulated emotion. This document records the decisions made before
implementation begins.

## Decisions

### D1: Mode biases arbitration only (this slice)

A mode perturbs **arbitration ordering only**. Salience/selection scoring
(`select_goals_with_salience`) and any proposal-threshold behavior are explicitly out of scope
for this slice and are recorded as follow-ups.

Rationale: arbitration is the smallest, most isolable place to demonstrate a deterministic,
inspectable bias, and it matches the plan's framing ("shifts arbitration weights"). The idea
doc's fuller `Focused` example (which also raises task salience and the threshold for proposing
new questions) is a richer behavior whose value is easier to judge once the arbitration-only bias
is proven and legible.

### D2: Protected floor + biasable band

Bias may reorder goals **only within a biasable band**; a protected floor is immune.

- **Protected floor:** effective tier `1..=PROTECTED_TIER_FLOOR` (tiers 1–3 — safety/boundary,
  explicit user intent, current task completion). These goals receive zero bias; their biased
  tier always equals their effective tier.
- **Biasable band:** effective tier `> PROTECTED_TIER_FLOOR` (tiers ≥ 4 — coherence, continuity,
  active experiment mode, research curiosity, optional exploration).

`PROTECTED_TIER_FLOOR = 3`. A band goal's biased tier is clamped to a **lower bound of
`PROTECTED_TIER_FLOOR + 1`** (i.e. 4), so a biased band goal can **never enter the protected
floor**. The safety invariant — no mode can elevate a curiosity/exploration goal above a
safety/boundary/user-intent/task goal — holds **by construction**, not by convention.

Rationale: this preserves the project's core boundary ("internal initiative, not uncontrolled
agency") and the arbitration invariant that safety and boundaries always win, while still letting
a mode produce visibly different outcomes among the lower-priority goals.

Alternatives considered: *tiebreak-only* bias (never changes cross-tier order) was rejected as too
weak to demonstrate anything; *bias all tiers* (mode can reweight any tension, including
boundary-preservation) was rejected because it lets a mode demote safety goals, conflicting with
the project boundary.

### D3: Bias sign and attribution

The bias vector is `BTreeMap<String, i8>` keyed by **tension id**. Convention: **negative
promotes** (lowers the tier number, more likely to win), **positive demotes**.

Bias is attributed to a goal's **effective tension** — the single tension that determined its
effective tier (the lexicographically smallest id among the tensions at the goal's minimum tier,
matching the existing `effective_tension_for_goal` tiebreak). This gives one deterministic,
traceable bias per goal even when a goal has several parent tensions.

### D4: Mode is event-driven state

The active mode lives in `VolitionState` and changes via a pure `VolitionEvent::ModeChanged`
applied by the reducer — replayable and traceable like every other transition in the module.
Mode changes are scripted by the experiment; there is no model call. A run-level config parameter
was rejected because it cannot change mid-run or be replayed as an explicit transition and would
sit outside the established event/reducer pattern.

The `mode` field carries `#[serde(default)]` (default `Neutral`) so run artifacts written before
this slice still deserialize, mirroring the backward-compatibility handling used when
`activation_keywords` was added.

### D5: Labels are handles; the vector is the source of truth

A `Mode` is an enum variant whose meaning **is** its declared `bias_vector()`. The label
("Focused", "Exploratory") is only a handle for inspection. No free-form mood label drives the
bias. This satisfies the idea doc's constraint that mood-like state, if introduced, must be an
inspectable bias vector over arbitration, not free-form simulated emotion.

### D6: One sort implementation (DRY)

`arbitrate` is refactored to delegate to `arbitrate_with_mode(.., Mode::Neutral)` and then **map
the neutral `ModeArbitrationResult` back into its existing `ArbitrationResult`**. There is a single
sort implementation in `arbitrate_with_mode`; `Neutral` applies a zero bias, so under `Neutral`
each goal's `biased_tier == effective_tier` and the mapping is lossless (`winner_effective_tier =
winner_bias.effective_tier`; each loser's `effective_tier`/tension fields copied across; `losers`
order preserved). `arbitrate` therefore keeps its exact `Option<ArbitrationResult>` signature and
serialized shape — `ModeArbitrationResult` is **not** exposed through `arbitrate` — so existing
callers and arbitration tests stay green. This is the only place existing code is touched.

### D7: Default exercises the new path

The runtime default mode is `Neutral` (zero bias) for production safety. The experiment, however,
scripts a biasing mode by default, so the new `arbitrate_with_mode` path runs on every experiment
run (per `Agents.md`: a feature behind a flag/threshold must default to exercising the new path).

### D8: Probabilistic bias is out of scope

Consistent with the arbitration slice, all bias here is deterministic. Any future probabilistic
mode must be gated behind an explicit experiment-mode flag and recorded in traces.

## Initial Mode Set

| Mode | `research-curiosity` | `continuity-preservation` | Intent |
|---|---:|---:|---|
| `Neutral` (default) | 0 | 0 | identical to current `arbitrate()` |
| `Focused` | +3 | −1 | suppress tangents; favor holding the open thread |
| `Exploratory` | −2 | +1 | promote curiosity above continuity |

Worked example — two band goals selected, `resurface-open-thread` (continuity, effective tier 5)
vs `clarify-weak-evidence-topic` (research-curiosity, effective tier 7):

- **Neutral:** continuity (5) beats curiosity (7).
- **Exploratory:** curiosity 7→5, continuity 5→6 → **winner flips to curiosity**.
- **Focused:** curiosity 7→10, continuity 5→4 → continuity still wins; the tangent is pushed
  further down.

With a tier-1 goal (`avoid-overstating-impl-status`, effective tier 1 via `boundary-preservation`)
also selected, **no mode changes the winner** — the floor goal wins under every mode.

## New Types

```rust
/// An inspectable arbitration bias. Its meaning is its declared bias_vector(); the label
/// is only a handle. Default = Neutral (zero bias).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]   // serializes as "neutral" | "focused" | "exploratory"
pub enum Mode {
    #[default]
    Neutral,
    Focused,
    Exploratory,
}
// Display renders the handle ("Neutral"/"Focused"/"Exploratory"); serde uses the snake_case above,
// so `active_mode` in artifacts reads e.g. "exploratory".

impl Mode {
    /// Declared bias over tension ids. Negative promotes (lower tier), positive demotes.
    /// Source of truth for the bias; empty for Neutral.
    pub fn bias_vector(self) -> std::collections::BTreeMap<String, i8>;
}

/// Tiers 1..=PROTECTED_TIER_FLOOR are immune to bias.
pub const PROTECTED_TIER_FLOOR: u8 = 3;

/// Per-goal record of how mode bias affected this goal's arbitration tier.
pub struct BiasOutcome {
    pub effective_tier: u8,   // pre-bias (minimum arbitration_tier among parent tensions)
    pub bias_applied: i8,     // 0 for protected goals
    pub biased_tier: u8,      // post-bias; band goals clamped to >= PROTECTED_TIER_FLOOR + 1
    pub protected: bool,      // effective_tier <= PROTECTED_TIER_FLOOR
}

pub struct ModeArbitrationLoser {
    pub selection: GoalSelection,
    pub effective_tension_id: String,
    pub effective_tension_title: String,
    pub bias: BiasOutcome,
    pub reason: String,       // rendered convenience string; tests assert structured fields
}

pub struct ModeArbitrationResult {
    pub mode: Mode,
    pub winner: GoalSelection,
    pub winner_effective_tension_id: String,
    pub winner_effective_tension_title: String,
    pub winner_bias: BiasOutcome,
    pub losers: Vec<ModeArbitrationLoser>,
}
```

## New Function Signature

```rust
pub fn arbitrate_with_mode(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
    mode: Mode,
) -> Option<ModeArbitrationResult>
```

Pure and stateless apart from reading `fixture` to resolve each goal's effective tension. Returns
`None` for empty input. Sort key: `(biased_tier asc, base_priority desc, goal_id asc)`. Because
floor goals keep tier ≤ 3 and band goals are clamped to ≥ 4, floor goals always sort ahead of band
goals. `arbitrate` delegates to `arbitrate_with_mode(.., Mode::Neutral)` and maps the neutral result
back into `ArbitrationResult` (see D6).

## Bias Arithmetic

Per goal, the biased tier is computed from the effective tier and the goal's attributed bias (D3)
in a **widened signed integer**, so neither the `u8` tier nor the `i8` bias can overflow or wrap:

```text
if protected (effective_tier <= PROTECTED_TIER_FLOOR):
    bias_applied = 0
    biased_tier  = effective_tier
else:
    bias_applied = bias for the goal's effective tension (0 if absent from the vector)
    raw          = effective_tier as i16 + bias_applied as i16
    biased_tier  = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8
```

The clamp lower bound (`PROTECTED_TIER_FLOOR + 1 = 4`) is what keeps a band goal out of the
protected floor. The upper bound (`u8::MAX`) keeps a goal with no fixture tension
(`effective_tier == u8::MAX`) from wrapping under a positive demotion — it simply stays at
`u8::MAX`, already lowest. `bias_applied` recorded in `BiasOutcome` is the pre-clamp signed delta
from the vector (0 for protected goals); the clamp only affects `biased_tier`.

## New State / Event

```rust
// VolitionState gains:
#[serde(default)]
pub mode: Mode,

// VolitionEvent gains:
ModeChanged { mode: Mode, tick: u64 },   // reducer: state.mode = mode
```

`event_tick` returns the event's `tick`; `from_fixture` seeds `mode: Mode::Neutral`.

## Trace Completeness Contract

Each conflict turn applies **exactly one** `ModeChanged { mode, tick }` event that sets the turn's
active mode — including the Neutral baseline turn, which applies `ModeChanged { mode: Neutral, tick:
1 }`. A turn that reuses the previous turn's mode still applies its own `ModeChanged`, so the active
mode is always event-sourced (never an implicit default) and `events_applied`/`events.jsonl` always
carry the mode fact. `from_fixture` still seeds `Mode::Neutral` so artifacts written before this
slice deserialize, but the experiment never relies on that default to establish a turn's mode.

Per [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md), each conflict turn records:

```text
input
events_applied              # exactly one ModeChanged (this turn's mode)
active_mode
mode_bias_vector            # the declared vector for active_mode
selector_output             # selected goal ids
per_goal_bias[]             # { goal_id, effective_tier, effective_tension_id,
                            #   bias_applied, biased_tier, protected }
mode_arbitration            # { winner_goal_id, winner_biased_tier, losers[] }
neutral_winner_goal_id      # arbitrate_with_mode(.., Neutral) on the same selection
mode_changed_winner         # bool: mode winner != neutral winner
executed_effects            # always 0
artifact_reference
```

Artifact boundary: `events.jsonl` holds chronological facts (including `ModeChanged`); structured
trace records hold the per-turn bias chain above; the human-readable report summarizes and lists a
review checklist derived from the structured artifacts.

Automated verification parses the generated artifacts and asserts: required fields exist on every
conflict turn; the flip turn has `mode_changed_winner == true`; the floor turn has
`mode_changed_winner == false` with the winner being the protected tier-1 goal (`protected ==
true`); no band goal's `biased_tier` is `< PROTECTED_TIER_FLOOR + 1`; and `executed_effects == 0`
on every turn. Replay must produce identical structured trace fields.

## Out of Scope (follow-ups)

- Mode bias on salience/selection scoring (which goals enter context).
- Mode bias on the threshold for proposing new goal candidates/questions.
- Probabilistic or model-inferred mode selection.
- Cross-session persistence of the active mode.

## Extension Points

- New modes are added as `Mode` variants with a declared `bias_vector()`; no other code changes.
- Biasing a tension in the protected floor has no effect by construction — a signal that the floor
  is the wrong place to express that intent.
- A goal with effective tier `u8::MAX` (no fixture tension) is biasable but already lowest; bias
  cannot rescue it, which is the correct signal to review its missing tension assignment.
