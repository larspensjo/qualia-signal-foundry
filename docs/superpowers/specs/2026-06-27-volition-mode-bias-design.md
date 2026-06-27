---
name: volition-mode-bias-design
description: Design decisions for the volition mode/bias slice — Mode enum with declared bias vector, protected-floor/biasable-band arbitration bias, ModeChanged event, experiment structure
metadata:
  type: project
---

# Volition Mode/Bias Design

Authoritative copy: [docs/Plans/Design.VolitionModeBias.md](../../Plans/Design.VolitionModeBias.md).
This is the brainstorm-stage spec captured before implementation.

## Context

The earlier volition slices built the static fixture/selector, pre-initiative traces,
salience/satisfaction/blocking/cooldown state, deterministic cross-goal arbitration,
reflection-generated goal candidates, and bounded internal initiative execution. Arbitration
(`arbitrate` in `crates/qsf_app/src/volition.rs`) is strict-tier: a goal's effective tier is the
minimum `arbitration_tier` among its parent tensions; lowest tier wins (ties by `base_priority`
desc, then `goal_id` asc). This slice adds an inspectable **mode** — a declared bias over
arbitration ordering — to study whether a deterministic, traceable bias can shift behavior without
free-form simulated emotion.

## Decisions

- **D1 — Arbitration only.** A mode biases arbitration ordering only; salience/selection and
  proposal-threshold behavior are out of scope (follow-ups).
- **D2 — Protected floor + biasable band.** Effective tiers `1..=PROTECTED_TIER_FLOOR` (1–3:
  safety/boundary, explicit user intent, task completion) are immune. Tiers ≥ 4 are biasable.
  `PROTECTED_TIER_FLOOR = 3`. A band goal's biased tier is clamped to a lower bound of
  `PROTECTED_TIER_FLOOR + 1`, so a band goal can never enter the floor — the safety invariant
  holds by construction. Bias is added in a widened signed integer, then clamped, so the `u8`/`i8`
  math never overflows (a `u8::MAX` no-tension goal stays at `u8::MAX`). Rejected: tiebreak-only
  (too weak), bias-all-tiers (can demote safety).
- **D3 — Bias sign and attribution.** Bias vector is `BTreeMap<String, i8>` keyed by tension id;
  negative promotes, positive demotes. Bias is attributed to a goal's effective tension (the
  lexicographically smallest id at its minimum tier), giving one deterministic bias per goal.
- **D4 — Mode is event-driven state.** `mode` lives in `VolitionState`; changes via a pure
  `VolitionEvent::ModeChanged`. `#[serde(default)] = Neutral` keeps prior artifacts deserializable.
  Rejected: run-level config (can't change mid-run or replay as a transition).
- **D5 — Labels are handles; the vector is the source of truth.** A `Mode`'s meaning is its
  declared `bias_vector()`; no free-form mood label drives the bias.
- **D6 — One sort implementation (DRY).** `arbitrate` delegates to
  `arbitrate_with_mode(.., Mode::Neutral)` and maps the neutral `ModeArbitrationResult` back into
  its existing `ArbitrationResult`; its `Option<ArbitrationResult>` signature and serialized shape
  are unchanged (the single sort lives in `arbitrate_with_mode`).
- **D7 — Default exercises the new path.** Runtime default mode = `Neutral`; the experiment scripts
  a biasing mode by default so the new path runs on every run.
- **D8 — Deterministic only.** Any future probabilistic mode must be gated behind an explicit
  experiment-mode flag and recorded in traces.

## Initial Mode Set

| Mode | `research-curiosity` | `continuity-preservation` | Intent |
|---|---:|---:|---|
| `Neutral` (default) | 0 | 0 | identical to current `arbitrate()` |
| `Focused` | +3 | −1 | suppress tangents; favor holding the open thread |
| `Exploratory` | −2 | +1 | promote curiosity above continuity |

Example — `resurface-open-thread` (continuity, tier 5) vs `clarify-weak-evidence-topic` (curiosity,
tier 7): Neutral → continuity wins; Exploratory → curiosity 7→5, continuity 5→6, winner flips to
curiosity; Focused → curiosity 7→10, continuity 5→4, continuity still wins. With a tier-1 goal also
in contention, no mode changes the winner.

## New Types and Function

```rust
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]                          // "neutral" | "focused" | "exploratory"
pub enum Mode { #[default] Neutral, Focused, Exploratory }   // + Display for the handle label
impl Mode { pub fn bias_vector(self) -> std::collections::BTreeMap<String, i8>; }

pub const PROTECTED_TIER_FLOOR: u8 = 3;

pub struct BiasOutcome { effective_tier: u8, bias_applied: i8, biased_tier: u8, protected: bool }
pub struct ModeArbitrationLoser { selection, effective_tension_id, effective_tension_title, bias: BiasOutcome, reason }
pub struct ModeArbitrationResult { mode, winner, winner_effective_tension_id, winner_effective_tension_title, winner_bias, losers }

pub fn arbitrate_with_mode(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
    mode: Mode,
) -> Option<ModeArbitrationResult>;

// VolitionState += #[serde(default)] mode: Mode
// VolitionEvent += ModeChanged { mode, tick }
```

## Experiment

`volition-mode-bias` scripts four turns: (1) Neutral band-only conflict baseline; (2) same input
under Exploratory → winner flips; (3) conflict including a tier-1 goal under a biasing mode →
winner unchanged (floor immune); (4) Focused → winner stays continuity, curiosity demoted. Every
turn applies exactly one `ModeChanged { mode, tick }` (the baseline applies
`ModeChanged { mode: Neutral, tick: 1 }`), so the active mode is always event-sourced, not an
implicit default. Each
conflict turn records `active_mode`, `mode_bias_vector`, `per_goal_bias[]`
(`effective_tier`/`effective_tension_id`/`bias_applied`/`biased_tier`/`protected`), the mode-aware
winner/losers, the `neutral_winner` for comparison, `mode_changed_winner`, and `executed_effects =
0`. Automated checks parse the artifacts and assert the flip/floor outcomes and the floor invariant
(`biased_tier >= PROTECTED_TIER_FLOOR + 1` for band goals). Replay must reproduce identical trace
fields.

## Out of Scope

Salience/selection bias, proposal-threshold bias, probabilistic/model-inferred mode selection, and
cross-session mode persistence — all follow-ups.
