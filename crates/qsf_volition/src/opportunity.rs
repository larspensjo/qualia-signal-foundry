use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{GoalStatus, GroundedTerm, GroundingRef, VolitionFixture, VolitionState};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunitySignalKind {
    ExpressedUncertainty,
    IntroducedContradiction,
    OpenGoalTopicMatch,
}

impl fmt::Display for OpportunitySignalKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExpressedUncertainty => "ExpressedUncertainty",
            Self::IntroducedContradiction => "IntroducedContradiction",
            Self::OpenGoalTopicMatch => "OpenGoalTopicMatch",
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OpportunitySignal {
    pub kind: OpportunitySignalKind,
    pub grounding_ref: GroundingRef,
}

const UNCERTAINTY_CUES: &[&str] = &[
    "maybe",
    "perhaps",
    "uncertain",
    "unsure",
    "unclear",
    "confused",
    "question",
    "questions",
    "wonder",
];

const CONTRADICTION_CUES: &[&str] = &[
    "but",
    "however",
    "though",
    "yet",
    "contradiction",
    "contradict",
    "inconsistent",
    "conflict",
    "instead",
];

/// Rule-based opportunity detection over grounded input terms and goal ids.
/// Every emitted signal cites a real grounding ref.
pub fn detect_opportunities(
    input_terms: &[GroundedTerm],
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> Vec<OpportunitySignal> {
    let mut signals = Vec::new();

    for term in input_terms {
        if UNCERTAINTY_CUES.contains(&term.normalized_text.as_str()) {
            signals.push(OpportunitySignal {
                kind: OpportunitySignalKind::ExpressedUncertainty,
                grounding_ref: term.grounding_ref(),
            });
        }

        if CONTRADICTION_CUES.contains(&term.normalized_text.as_str()) {
            signals.push(OpportunitySignal {
                kind: OpportunitySignalKind::IntroducedContradiction,
                grounding_ref: term.grounding_ref(),
            });
        }
    }

    let mut seen_goal_ids = Vec::<String>::new();
    for goal in fixture
        .goals
        .iter()
        .chain(state.accepted_candidates.values())
    {
        let Some(dynamic) = state.goals.get(&goal.id) else {
            continue;
        };
        if !matches!(dynamic.status, GoalStatus::Accepted | GoalStatus::Active) {
            continue;
        }
        if seen_goal_ids.iter().any(|goal_id| goal_id == &goal.id) {
            continue;
        }

        let matched = goal.activation_keywords.iter().any(|keyword| {
            input_terms
                .iter()
                .any(|term| term.normalized_text == keyword.term)
        });

        if matched {
            seen_goal_ids.push(goal.id.clone());
            signals.push(OpportunitySignal {
                kind: OpportunitySignalKind::OpenGoalTopicMatch,
                grounding_ref: GroundingRef::GoalId {
                    goal_id: goal.id.clone(),
                },
            });
        }
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{grounded_terms_from_text, realtime_seed_fixture};

    #[test]
    fn unrelated_input_emits_no_signals() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let grounded = grounded_terms_from_text("xyzzy frobnicator quux");
        assert!(detect_opportunities(&grounded, &state, &fixture).is_empty());
    }

    #[test]
    fn emitted_signals_cite_real_grounding_refs() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let grounded = grounded_terms_from_text("maybe however help");
        let signals = detect_opportunities(&grounded, &state, &fixture);
        assert!(!signals.is_empty());
        assert!(signals.iter().all(|signal| match &signal.grounding_ref {
            GroundingRef::InputTerm {
                normalized_text,
                original_text,
                span,
            } => !normalized_text.is_empty() && !original_text.is_empty() && span.end > span.start,
            GroundingRef::GoalId { goal_id } => !goal_id.is_empty(),
        }));
    }

    #[test]
    fn detection_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let grounded = grounded_terms_from_text("maybe however help");
        let first = detect_opportunities(&grounded, &state, &fixture);
        let second = detect_opportunities(&grounded, &state, &fixture);
        assert_eq!(first, second);
    }
}
