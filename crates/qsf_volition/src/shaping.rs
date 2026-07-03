use serde::{Deserialize, Serialize};
use std::fmt;

use crate::arbitration::{ModeArbitrationResult, PROTECTED_TIER_FLOOR};
use crate::opportunity::{OpportunitySignal, OpportunitySignalKind};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceptivenessHint {
    Guarded,
    #[default]
    Neutral,
    Open,
}

impl fmt::Display for ReceptivenessHint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Guarded => "Guarded",
            Self::Neutral => "Neutral",
            Self::Open => "Open",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapingIntensity {
    None,
    Low,
    Medium,
    High,
}

impl fmt::Display for ShapingIntensity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::None => "None",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ShapingIntensityInputs {
    pub winner_goal_id: String,
    pub winner_effective_tier: u8,
    pub winner_relevance_score: f64,
    pub opportunity_count: usize,
    pub uncertainty_count: usize,
    pub contradiction_count: usize,
    pub open_goal_topic_count: usize,
    pub receptiveness: ReceptivenessHint,
    pub protected_winner: bool,
}

/// Choose a conservative shaping intensity from inspectable inputs only.
pub fn choose_shaping_intensity(
    arbitration: &ModeArbitrationResult,
    opportunities: &[OpportunitySignal],
    receptiveness: ReceptivenessHint,
) -> ShapingIntensity {
    let opportunity_count = opportunities.len();
    let uncertainty_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::ExpressedUncertainty))
        .count();
    let contradiction_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::IntroducedContradiction))
        .count();
    let open_goal_topic_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::OpenGoalTopicMatch))
        .count();
    let protected_winner = arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR;

    let base = if protected_winner {
        if opportunity_count == 0 {
            ShapingIntensity::None
        } else {
            ShapingIntensity::Low
        }
    } else if open_goal_topic_count > 0
        && (uncertainty_count > 0 || contradiction_count > 0)
        && matches!(receptiveness, ReceptivenessHint::Open)
        && arbitration.winner.relevance_score >= 220.0
        && opportunity_count >= 2
    {
        ShapingIntensity::High
    } else if opportunity_count >= 2 || arbitration.winner.relevance_score >= 180.0 {
        ShapingIntensity::Medium
    } else if opportunity_count >= 1 || matches!(receptiveness, ReceptivenessHint::Open) {
        ShapingIntensity::Low
    } else {
        ShapingIntensity::None
    };

    if protected_winner {
        base.min(ShapingIntensity::Low)
    } else {
        base
    }
}

pub fn shaping_intensity_inputs(
    arbitration: &ModeArbitrationResult,
    opportunities: &[OpportunitySignal],
    receptiveness: ReceptivenessHint,
) -> ShapingIntensityInputs {
    let opportunity_count = opportunities.len();
    let uncertainty_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::ExpressedUncertainty))
        .count();
    let contradiction_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::IntroducedContradiction))
        .count();
    let open_goal_topic_count = opportunities
        .iter()
        .filter(|signal| matches!(signal.kind, OpportunitySignalKind::OpenGoalTopicMatch))
        .count();

    ShapingIntensityInputs {
        winner_goal_id: arbitration.winner.goal.id.clone(),
        winner_effective_tier: arbitration.winner_bias.effective_tier,
        winner_relevance_score: arbitration.winner.relevance_score,
        opportunity_count,
        uncertainty_count,
        contradiction_count,
        open_goal_topic_count,
        receptiveness,
        protected_winner: arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GroundingRef, Mode, OpportunitySignal, OpportunitySignalKind, VolitionFixture,
        VolitionState, arbitrate_with_mode, detect_opportunities, grounded_terms_from_text,
        realtime_seed_fixture, select_goals_ranked,
    };

    fn opportunity(kind: OpportunitySignalKind) -> OpportunitySignal {
        OpportunitySignal {
            kind,
            grounding_ref: GroundingRef::GoalId {
                goal_id: "goal".to_string(),
            },
        }
    }

    #[test]
    fn choose_shaping_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("how can you help me with this task", &state, &fixture);
        let arbitration =
            arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("maybe however help"),
            &state,
            &fixture,
        );

        assert_eq!(
            choose_shaping_intensity(&arbitration, &opportunities, ReceptivenessHint::Open),
            choose_shaping_intensity(&arbitration, &opportunities, ReceptivenessHint::Open)
        );
    }

    #[test]
    fn protected_winner_clamps_to_low_under_every_mode() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("how can you help me with this task", &state, &fixture);
        let arbitration =
            arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let opportunities = vec![
            opportunity(OpportunitySignalKind::ExpressedUncertainty),
            opportunity(OpportunitySignalKind::IntroducedContradiction),
            opportunity(OpportunitySignalKind::OpenGoalTopicMatch),
        ];

        for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
            let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, mode).unwrap();
            let intensity =
                choose_shaping_intensity(&arbitration, &opportunities, ReceptivenessHint::Open);
            if arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR {
                assert!(matches!(
                    intensity,
                    ShapingIntensity::Low | ShapingIntensity::None
                ));
            }
        }

        assert!(arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR);
    }

    #[test]
    fn high_requires_the_documented_conditions_and_skips_protected_winners() {
        let fixture = VolitionFixture {
            tensions: vec![crate::Tension {
                id: "topic".to_string(),
                title: "Topic".to_string(),
                summary: "topic".to_string(),
                priority_bias: crate::TensionPriority::Medium,
                arbitration_tier: 6,
                focused_bias: 0,
                exploratory_bias: 0,
            }],
            goals: vec![crate::Goal {
                id: "go-a".to_string(),
                title: "Goal A".to_string(),
                summary: "Goal summary".to_string(),
                tension_ids: vec!["topic".to_string()],
                status: crate::GoalStatus::Accepted,
                scope: crate::GoalScope::Session,
                base_priority: 100,
                activation_keywords: vec!["help".to_string(), "thread".to_string()],
                allowed_effects: vec![crate::AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![],
                estimated_tokens: 10,
                source_reference: "test".to_string(),
            }],
        };
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("help maybe however thread", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .expect("arbitration");
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("help maybe however thread"),
            &state,
            &fixture,
        );
        let intensity =
            choose_shaping_intensity(&arbitration, &opportunities, ReceptivenessHint::Open);
        assert!(matches!(
            intensity,
            ShapingIntensity::High | ShapingIntensity::Medium
        ));
        assert!(
            !(arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR
                && matches!(intensity, ShapingIntensity::High))
        );
    }
}
