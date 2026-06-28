use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Goal, GoalSelection, Tension, VolitionFixture};

/// A goal selection that lost cross-goal arbitration. Records the full selection plus
/// the structured tension provenance that determined its effective tier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArbitrationLoser {
    pub selection: GoalSelection,
    /// The effective arbitration tier for this goal (minimum tier among parent tensions).
    pub effective_tier: u8,
    /// The tension responsible for this goal's effective tier.
    pub effective_tension_id: String,
    /// Human-readable name of the effective tension.
    pub effective_tension_title: String,
    /// Rendered convenience reason, e.g. "tier 7 lost to winner at tier 1 (boundary-preservation)".
    /// Tests must assert the structured fields above, not this string.
    pub reason: String,
}

/// The result of deterministic cross-goal arbitration. The winner is the goal with the
/// lowest effective tier (minimum `arbitration_tier` among its parent tensions); ties are
/// broken by higher `base_priority`, then lower `goal_id` lexicographically.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArbitrationResult {
    pub winner: GoalSelection,
    /// Effective tier that placed the winner.
    pub winner_effective_tier: u8,
    /// Tension responsible for the winner's effective tier.
    pub winner_effective_tension_id: String,
    /// Human-readable name of the winner's effective tension.
    pub winner_effective_tension_title: String,
    /// Losing goals sorted: effective tier ascending, base_priority descending, goal_id ascending.
    pub losers: Vec<ArbitrationLoser>,
}

/// Tiers 1..=PROTECTED_TIER_FLOOR are immune to mode bias.
pub const PROTECTED_TIER_FLOOR: u8 = 3;

/// An inspectable arbitration bias. Its meaning is its declared `bias_vector()`; the
/// label is only a handle. Default = Neutral (zero bias).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Neutral,
    Focused,
    Exploratory,
}

impl Mode {
    /// Declared bias over tension ids. Negative promotes (lower tier), positive demotes.
    /// Source of truth for the bias; empty for Neutral.
    pub fn bias_vector(self) -> BTreeMap<String, i8> {
        match self {
            Self::Neutral => BTreeMap::new(),
            Self::Focused => {
                let mut map = BTreeMap::new();
                map.insert("research-curiosity".to_string(), 3i8);
                map.insert("continuity-preservation".to_string(), -1i8);
                map
            }
            Self::Exploratory => {
                let mut map = BTreeMap::new();
                map.insert("research-curiosity".to_string(), -2i8);
                map.insert("continuity-preservation".to_string(), 1i8);
                map
            }
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Neutral => "Neutral",
            Self::Focused => "Focused",
            Self::Exploratory => "Exploratory",
        })
    }
}

/// Per-goal record of how mode bias affected this goal's arbitration tier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BiasOutcome {
    /// Pre-bias effective tier (minimum arbitration_tier among parent tensions).
    pub effective_tier: u8,
    /// Applied bias delta (0 for protected goals, pre-clamp signed delta for band goals).
    pub bias_applied: i8,
    /// Post-bias tier; band goals clamped to >= PROTECTED_TIER_FLOOR + 1.
    pub biased_tier: u8,
    /// True when effective_tier <= PROTECTED_TIER_FLOOR; such goals receive zero bias.
    pub protected: bool,
}

/// A goal selection that lost mode-aware cross-goal arbitration.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModeArbitrationLoser {
    pub selection: GoalSelection,
    pub effective_tension_id: String,
    pub effective_tension_title: String,
    pub bias: BiasOutcome,
    /// Rendered convenience string. Tests must assert structured fields, not this string.
    pub reason: String,
}

/// The result of mode-aware cross-goal arbitration. Sort key: biased_tier asc,
/// base_priority desc, goal_id asc. Floor goals always sort ahead of band goals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModeArbitrationResult {
    pub mode: Mode,
    pub winner: GoalSelection,
    pub winner_effective_tension_id: String,
    pub winner_effective_tension_title: String,
    pub winner_bias: BiasOutcome,
    pub losers: Vec<ModeArbitrationLoser>,
}

/// Resolve cross-goal conflict by tension tier. Returns `None` for empty input. For a
/// single selection, the sole goal is the winner with an empty losers list. Pure and
/// stateless — reads `fixture` only to resolve tensions for each goal.
/// Delegates to `arbitrate_with_mode(.., Mode::Neutral)` — one sort implementation.
pub fn arbitrate(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
) -> Option<ArbitrationResult> {
    arbitrate_with_mode(selections, fixture, Mode::Neutral).map(|mode_result| {
        let winner_tier = mode_result.winner_bias.effective_tier;
        let winner_tension_id = mode_result.winner_effective_tension_id.clone();
        let losers = mode_result
            .losers
            .into_iter()
            .map(|l| {
                let reason = format!(
                    "tier {} lost to winner at tier {} ({})",
                    l.bias.effective_tier, winner_tier, winner_tension_id
                );
                ArbitrationLoser {
                    selection: l.selection,
                    effective_tier: l.bias.effective_tier,
                    effective_tension_id: l.effective_tension_id,
                    effective_tension_title: l.effective_tension_title,
                    reason,
                }
            })
            .collect();
        ArbitrationResult {
            winner: mode_result.winner,
            winner_effective_tier: winner_tier,
            winner_effective_tension_id: mode_result.winner_effective_tension_id,
            winner_effective_tension_title: mode_result.winner_effective_tension_title,
            losers,
        }
    })
}

/// Returns the `(effective_tier, tension_id, tension_title)` for a goal. The effective
/// tier is the minimum `arbitration_tier` among the goal's parent tensions in the fixture.
/// When multiple tensions share the minimum tier, the lexicographically smallest
/// `tension_id` is chosen as the effective tension. Returns `(u8::MAX, "", "")` when the
/// goal has no parent tensions in the fixture.
fn effective_tension_for_goal(goal: &Goal, fixture: &VolitionFixture) -> (u8, String, String) {
    let parent_tensions: Vec<&Tension> = goal
        .tension_ids
        .iter()
        .filter_map(|tension_id| {
            fixture
                .tensions
                .iter()
                .find(|tension| tension.id == *tension_id)
        })
        .collect();

    if parent_tensions.is_empty() {
        return (u8::MAX, String::new(), String::new());
    }

    let min_tier = parent_tensions
        .iter()
        .map(|tension| tension.arbitration_tier)
        .min()
        .unwrap();

    let effective = parent_tensions
        .iter()
        .filter(|tension| tension.arbitration_tier == min_tier)
        .min_by_key(|tension| &tension.id)
        .unwrap();

    (min_tier, effective.id.clone(), effective.title.clone())
}

/// Compute the `BiasOutcome` for one goal given its effective tier and the active bias vector.
fn compute_bias_outcome(
    effective_tier: u8,
    tension_id: &str,
    bias_vector: &BTreeMap<String, i8>,
) -> BiasOutcome {
    if effective_tier <= PROTECTED_TIER_FLOOR {
        BiasOutcome {
            effective_tier,
            bias_applied: 0,
            biased_tier: effective_tier,
            protected: true,
        }
    } else {
        let bias_applied = bias_vector.get(tension_id).copied().unwrap_or(0);
        let raw = effective_tier as i16 + bias_applied as i16;
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        BiasOutcome {
            effective_tier,
            bias_applied,
            biased_tier,
            protected: false,
        }
    }
}

/// Mode-aware cross-goal arbitration. Sort key: `(biased_tier asc, base_priority desc,
/// goal_id asc)`. Band goals are clamped so `biased_tier >= PROTECTED_TIER_FLOOR + 1`,
/// ensuring floor goals always sort ahead. Returns `None` for empty input.
/// `arbitrate` delegates here with `Mode::Neutral`, producing identical results.
pub fn arbitrate_with_mode(
    selections: Vec<GoalSelection>,
    fixture: &VolitionFixture,
    mode: Mode,
) -> Option<ModeArbitrationResult> {
    if selections.is_empty() {
        return None;
    }

    let bias_vector = mode.bias_vector();

    let mut with_bias: Vec<(GoalSelection, String, String, BiasOutcome)> = selections
        .into_iter()
        .map(|selection| {
            let (effective_tier, tension_id, tension_title) =
                effective_tension_for_goal(&selection.goal, fixture);
            let bias = compute_bias_outcome(effective_tier, &tension_id, &bias_vector);
            (selection, tension_id, tension_title, bias)
        })
        .collect();

    // Sort: biased_tier ascending, base_priority descending, goal_id ascending.
    with_bias.sort_by(|a, b| {
        a.3.biased_tier
            .cmp(&b.3.biased_tier)
            .then(b.0.goal.base_priority.cmp(&a.0.goal.base_priority))
            .then(a.0.goal.id.cmp(&b.0.goal.id))
    });

    let (winner_sel, winner_tension_id, winner_tension_title, winner_bias) = with_bias.remove(0);

    let winner_biased_tier = winner_bias.biased_tier;
    let winner_tid = winner_tension_id.clone();
    let losers = with_bias
        .into_iter()
        .map(|(sel, tension_id, tension_title, bias)| {
            let reason = format!(
                "biased tier {} (effective {}) lost to winner at biased tier {} ({})",
                bias.biased_tier, bias.effective_tier, winner_biased_tier, winner_tid
            );
            ModeArbitrationLoser {
                selection: sel,
                effective_tension_id: tension_id,
                effective_tension_title: tension_title,
                bias,
                reason,
            }
        })
        .collect();

    Some(ModeArbitrationResult {
        mode,
        winner: winner_sel,
        winner_effective_tension_id: winner_tension_id,
        winner_effective_tension_title: winner_tension_title,
        winner_bias,
        losers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, Goal, GoalScope, GoalSelection, GoalStatus, InitiativeProposal, Tension,
        TensionPriority, VolitionEvent, VolitionFixture, VolitionState, apply, static_fixture,
    };

    fn make_goal_for_arbitration(
        id: &str,
        tension_ids: Vec<String>,
        base_priority: u8,
    ) -> GoalSelection {
        let goal = Goal {
            id: id.to_string(),
            title: id.to_string(),
            summary: id.to_string(),
            tension_ids,
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority,
            activation_keywords: vec!["test".to_string()],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: id.to_string(),
            evidence_refs: vec![],
            estimated_tokens: 10,
            source_reference: id.to_string(),
        };
        GoalSelection {
            goal: goal.clone(),
            relevance_score: goal.base_priority as f64,
            matched_terms: vec!["test".to_string()],
            initiative: InitiativeProposal {
                goal_id: goal.id.clone(),
                goal_title: goal.title.clone(),
                effect: AllowedEffect::Reflect,
                rationale: "test".to_string(),
                matched_terms: vec!["test".to_string()],
                scope: GoalScope::Session,
            },
        }
    }

    fn make_tension(id: &str, tier: u8) -> Tension {
        Tension {
            id: id.to_string(),
            title: format!("{id} title"),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: tier,
        }
    }

    // ── Arbitration ─────────────────────────────────────────────────────────

    #[test]
    fn arbitrate_empty_returns_none() {
        let fixture = static_fixture();
        assert!(arbitrate(vec![], &fixture).is_none());
    }

    #[test]
    fn arbitrate_same_tier_same_priority_lower_goal_id_wins() {
        let fixture = VolitionFixture {
            tensions: vec![make_tension("test-tension", 5)],
            goals: vec![],
        };
        // "goal-a" < "goal-b" lexicographically; same tier and priority
        let sel_b = make_goal_for_arbitration("goal-b", vec!["test-tension".to_string()], 80);
        let sel_a = make_goal_for_arbitration("goal-a", vec!["test-tension".to_string()], 80);
        let result = arbitrate(vec![sel_b, sel_a], &fixture).unwrap();
        assert_eq!(result.winner.goal.id, "goal-a");
        assert_eq!(result.losers[0].selection.goal.id, "goal-b");
    }

    #[test]
    fn arbitrate_same_minimum_tier_picks_lexicographic_tension_id() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("beta-tension", 3),
                make_tension("alpha-tension", 3),
            ],
            goals: vec![],
        };
        // Goal backed by both tensions at tier 3; alpha < beta lexicographically
        let sel = make_goal_for_arbitration(
            "test-goal",
            vec!["alpha-tension".to_string(), "beta-tension".to_string()],
            80,
        );
        let result = arbitrate(vec![sel], &fixture).unwrap();
        assert_eq!(result.winner_effective_tier, 3);
        assert_eq!(result.winner_effective_tension_id, "alpha-tension");
        assert_eq!(result.winner_effective_tension_title, "alpha-tension title");
    }

    #[test]
    fn arbitrate_losers_are_sorted_by_tier_then_priority_then_id() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("tier-1-tension", 1),
                make_tension("tier-5-tension", 5),
                make_tension("tier-7-tension", 7),
            ],
            goals: vec![],
        };
        let sel_tier7 =
            make_goal_for_arbitration("goal-z-tier7", vec!["tier-7-tension".to_string()], 80);
        let sel_tier5 =
            make_goal_for_arbitration("goal-a-tier5", vec!["tier-5-tension".to_string()], 90);
        let sel_tier1 =
            make_goal_for_arbitration("goal-m-tier1", vec!["tier-1-tension".to_string()], 95);
        let result = arbitrate(vec![sel_tier7, sel_tier5, sel_tier1], &fixture).unwrap();

        assert_eq!(result.winner.goal.id, "goal-m-tier1");
        assert_eq!(result.winner_effective_tier, 1);
        // Losers: tier 5 before tier 7 (ascending tier)
        assert_eq!(result.losers[0].selection.goal.id, "goal-a-tier5");
        assert_eq!(result.losers[0].effective_tier, 5);
        assert_eq!(result.losers[1].selection.goal.id, "goal-z-tier7");
        assert_eq!(result.losers[1].effective_tier, 7);
    }

    // ── Mode floor immunity ─────────────────────────────────────────────────

    #[test]
    fn floor_goal_wins_over_band_goals_under_exploratory_mode() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("floor-tension", 1),
                make_tension("band-tension", 7),
            ],
            goals: vec![],
        };
        let floor_goal =
            make_goal_for_arbitration("floor-goal", vec!["floor-tension".to_string()], 80);
        let band_goal =
            make_goal_for_arbitration("band-goal", vec!["band-tension".to_string()], 95);

        // Under Exploratory, band-tension would normally be promoted; floor-tension must be immune.
        let result =
            arbitrate_with_mode(vec![floor_goal, band_goal], &fixture, Mode::Exploratory).unwrap();

        assert_eq!(result.winner.goal.id, "floor-goal");
        assert!(result.winner_bias.protected, "floor goal must be protected");
        assert_eq!(
            result.winner_bias.biased_tier, result.winner_bias.effective_tier,
            "protected goal must have no bias applied"
        );
        for loser in &result.losers {
            if !loser.bias.protected {
                assert!(
                    loser.bias.biased_tier > PROTECTED_TIER_FLOOR,
                    "band goal biased_tier must not enter the protected floor"
                );
            }
        }
    }

    #[test]
    fn floor_goal_wins_under_all_modes() {
        let fixture = VolitionFixture {
            tensions: vec![
                make_tension("tier-2-tension", 2),
                make_tension("tier-7-tension", 7),
            ],
            goals: vec![],
        };
        let floor_goal =
            make_goal_for_arbitration("floor-goal", vec!["tier-2-tension".to_string()], 70);
        let band_goal =
            make_goal_for_arbitration("band-goal", vec!["tier-7-tension".to_string()], 99);

        for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
            let result =
                arbitrate_with_mode(vec![floor_goal.clone(), band_goal.clone()], &fixture, mode)
                    .unwrap();
            assert_eq!(
                result.winner.goal.id, "floor-goal",
                "floor goal must win under {mode}"
            );
            assert!(
                result.winner_bias.protected,
                "floor goal must be protected under {mode}"
            );
        }
    }

    // ── Mode bias vectors ───────────────────────────────────────────────────

    #[test]
    fn mode_neutral_bias_vector_is_empty() {
        assert!(Mode::Neutral.bias_vector().is_empty());
    }

    #[test]
    fn mode_focused_bias_vector_matches_spec() {
        let vec = Mode::Focused.bias_vector();
        assert_eq!(vec.get("research-curiosity"), Some(&3i8));
        assert_eq!(vec.get("continuity-preservation"), Some(&-1i8));
        assert_eq!(vec.len(), 2);
    }

    #[test]
    fn mode_exploratory_bias_vector_matches_spec() {
        let vec = Mode::Exploratory.bias_vector();
        assert_eq!(vec.get("research-curiosity"), Some(&-2i8));
        assert_eq!(vec.get("continuity-preservation"), Some(&1i8));
        assert_eq!(vec.len(), 2);
    }

    #[test]
    fn mode_changed_event_updates_state_mode() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        assert_eq!(state.mode, Mode::Neutral, "initial mode must be Neutral");

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 1,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Focused,
                tick: 2,
            },
        );
        assert_eq!(state.mode, Mode::Focused);

        let state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Neutral,
                tick: 3,
            },
        );
        assert_eq!(state.mode, Mode::Neutral);
    }

    #[test]
    fn mode_changed_replay_reproduces_mode() {
        let fixture = static_fixture();
        let apply_seq = || {
            let s = VolitionState::from_fixture(&fixture);
            let s = apply(
                s,
                VolitionEvent::ModeChanged {
                    mode: Mode::Exploratory,
                    tick: 1,
                },
            );
            apply(
                s,
                VolitionEvent::ModeChanged {
                    mode: Mode::Focused,
                    tick: 2,
                },
            )
        };
        assert_eq!(apply_seq().mode, apply_seq().mode);
    }

    #[test]
    fn band_goal_biased_tier_never_enters_floor() {
        let effective_tier: u8 = 5;
        let bias_applied: i8 = i8::MIN; // -128, extreme promotion attempt
        let raw = effective_tier as i16 + bias_applied as i16; // 5 - 128 = -123
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        assert_eq!(biased_tier, PROTECTED_TIER_FLOOR + 1);

        let outcome = BiasOutcome {
            effective_tier,
            bias_applied,
            biased_tier,
            protected: false,
        };
        assert_eq!(outcome.biased_tier, PROTECTED_TIER_FLOOR + 1);
        assert!(!outcome.protected);
    }

    #[test]
    fn bias_arithmetic_u8_max_stays_at_max_under_positive_demotion() {
        let fixture = VolitionFixture {
            tensions: vec![],
            goals: vec![],
        };
        let sel = make_goal_for_arbitration("no-tension-goal", vec![], 80);
        let result = arbitrate_with_mode(vec![sel], &fixture, Mode::Focused).unwrap();
        assert_eq!(result.winner_bias.effective_tier, u8::MAX);
        assert_eq!(result.winner_bias.biased_tier, u8::MAX);
    }

    #[test]
    fn mode_field_serde_default_is_neutral() {
        let json = serde_json::json!({
            "tick": 0,
            "goals": {},
            "pending_candidates": [],
            "accepted_candidates": {}
        });
        let state: VolitionState = serde_json::from_value(json).unwrap();
        assert_eq!(state.mode, Mode::Neutral);
    }
}
