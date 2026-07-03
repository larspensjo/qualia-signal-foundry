use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AllowedEffect, Goal, GoalSelection, GoalStatus, InitiativeProposal, OmittedGoal,
    VolitionFixture, VolitionState, normalize_terms,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankedSelectionResult {
    pub input_terms: Vec<String>,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub suppressed_cooldown: Vec<OmittedGoal>,
    pub visible_blocked: Vec<OmittedGoal>,
}

pub fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<String> {
    let mut matched = Vec::new();
    for keyword in &goal.activation_keywords {
        if input_terms.iter().any(|term| term == keyword)
            && !matched.iter().any(|term| term == keyword)
        {
            matched.push(keyword.clone());
        }
    }
    matched
}

pub fn compute_relevance(goal: &Goal, fixture: &VolitionFixture, terms: &[String]) -> f64 {
    let matched_bonus = terms.len() as f64 * 100.0;
    let base_priority = goal.base_priority as f64;
    let tension_bonus = goal
        .tension_ids
        .iter()
        .filter_map(|tension_id| {
            fixture
                .tensions
                .iter()
                .find(|tension| tension.id == *tension_id)
        })
        .map(|tension| tension.priority_bias.score_bonus())
        .fold(0.0, f64::max);

    matched_bonus + base_priority + tension_bonus
}

pub fn compute_relevance_with_salience(
    goal: &Goal,
    fixture: &VolitionFixture,
    terms: &[String],
    salience: i32,
) -> f64 {
    compute_relevance(goal, fixture, terms) + salience as f64
}

pub fn initiative_for_goal(goal: &Goal, matched_terms: &[String]) -> InitiativeProposal {
    let effect = goal
        .allowed_effects
        .first()
        .copied()
        .unwrap_or(AllowedEffect::Reflect);
    initiative_for_effect(goal, effect, matched_terms)
}

pub fn initiative_for_effect(
    goal: &Goal,
    effect: AllowedEffect,
    matched_terms: &[String],
) -> InitiativeProposal {
    InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect,
        rationale: format!(
            "goal {} matched [{}] under scope {}",
            goal.id,
            matched_terms.join(", "),
            goal.scope
        ),
        matched_terms: matched_terms.to_vec(),
        scope: goal.scope,
    }
}

pub fn select_goals_ranked(
    query: &str,
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> RankedSelectionResult {
    let input_terms = normalize_terms(query);
    let mut goals = BTreeMap::<String, Goal>::new();

    for goal in &fixture.goals {
        goals.insert(goal.id.clone(), goal.clone());
    }
    for goal in state.accepted_candidates.values() {
        goals.insert(goal.id.clone(), goal.clone());
    }

    let mut selected_candidates = Vec::new();
    let mut omitted = Vec::new();
    let mut suppressed_cooldown = Vec::new();
    let mut visible_blocked = Vec::new();

    for goal in goals.into_values() {
        let dynamic = state.goals.get(&goal.id);
        let status = dynamic.map(|entry| entry.status).unwrap_or(goal.status);

        if matches!(status, GoalStatus::Cooldown) {
            suppressed_cooldown.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {status} (cooldown suppressed)"),
            });
            continue;
        }

        if matches!(
            status,
            GoalStatus::Proposed | GoalStatus::Retired | GoalStatus::Satisfied
        ) {
            omitted.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {status}"),
            });
            continue;
        }

        let matched_terms = matched_keywords(&goal, &input_terms);

        if matches!(status, GoalStatus::Blocked) {
            visible_blocked.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms,
                reason: "goal status is blocked (visible unresolved tension)".to_string(),
            });
            continue;
        }

        if matched_terms.is_empty() {
            omitted.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let salience = dynamic.map(|entry| entry.salience).unwrap_or(0);
        let relevance_score =
            compute_relevance_with_salience(&goal, fixture, &matched_terms, salience);
        selected_candidates.push((goal, matched_terms, relevance_score));
    }

    selected_candidates.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.id.cmp(&b.0.id)));

    let selected = selected_candidates
        .into_iter()
        .map(|(goal, matched_terms, relevance_score)| GoalSelection {
            initiative: initiative_for_goal(&goal, &matched_terms),
            matched_terms,
            relevance_score,
            goal,
        })
        .collect();

    RankedSelectionResult {
        input_terms,
        selected,
        omitted,
        suppressed_cooldown,
        visible_blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvidenceRef, VolitionEvent, apply, realtime_seed_fixture, static_fixture};

    fn fresh_state(fixture: &VolitionFixture) -> VolitionState {
        VolitionState::from_fixture(fixture)
    }

    #[test]
    fn select_goals_ranked_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let q = "how can you help me";
        let r1 = select_goals_ranked(q, &state, &fixture);
        let r2 = select_goals_ranked(q, &state, &fixture);
        assert_eq!(r1.selected.len(), r2.selected.len());
        for (a, b) in r1.selected.iter().zip(r2.selected.iter()) {
            assert_eq!(a.goal.id, b.goal.id);
            assert_eq!(a.relevance_score, b.relevance_score);
        }
    }

    #[test]
    fn select_goals_ranked_selected_sorted_descending_by_relevance() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let result = select_goals_ranked("how can you help me with this task", &state, &fixture);
        let scores: Vec<f64> = result.selected.iter().map(|s| s.relevance_score).collect();
        for window in scores.windows(2) {
            assert!(
                window[0] >= window[1],
                "selected must be sorted descending by relevance; got {window:?}"
            );
        }
    }

    #[test]
    fn cooldown_goal_appears_in_suppressed_cooldown_not_selected() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let evidence = EvidenceRef::try_new("test").unwrap();
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "serve-the-present-person".to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalSatisfied {
                goal_id: "serve-the-present-person".to_string(),
                evidence,
                tick: 2,
            },
        );

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(
            result
                .selected
                .iter()
                .all(|s| s.goal.id != "serve-the-present-person")
        );
        assert!(
            result
                .suppressed_cooldown
                .iter()
                .any(|g| g.goal.id == "serve-the-present-person")
        );
    }

    #[test]
    fn blocked_goal_appears_in_visible_blocked_not_selected() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let state = apply(
            state,
            VolitionEvent::GoalActivated {
                goal_id: "serve-the-present-person".to_string(),
                tick: 1,
            },
        );
        let state = apply(
            state,
            VolitionEvent::GoalBlocked {
                goal_id: "serve-the-present-person".to_string(),
                tick: 2,
            },
        );

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(
            result
                .selected
                .iter()
                .all(|s| s.goal.id != "serve-the-present-person")
        );
        assert!(
            result
                .visible_blocked
                .iter()
                .any(|g| g.goal.id == "serve-the-present-person")
        );
    }

    #[test]
    fn no_keyword_match_goes_to_omitted() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let result = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
        assert!(result.selected.is_empty());
        assert!(!result.omitted.is_empty());
    }

    #[test]
    fn proposed_and_retired_goals_appear_in_omitted() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let state = apply(
            state,
            VolitionEvent::GoalRetired {
                goal_id: "serve-the-present-person".to_string(),
                tick: 1,
            },
        );

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(
            result
                .selected
                .iter()
                .all(|s| s.goal.id != "serve-the-present-person")
        );
        assert!(
            result
                .omitted
                .iter()
                .any(|g| g.goal.id == "serve-the-present-person")
        );
    }

    #[test]
    fn matched_keywords_returns_intersection_with_activation_keywords() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = normalize_terms("voice memory evidence");
        let matched = matched_keywords(goal, &terms);
        assert!(!matched.is_empty());
        assert!(
            matched
                .iter()
                .all(|kw| goal.activation_keywords.contains(kw))
        );
    }

    #[test]
    fn compute_relevance_increases_with_more_matched_terms() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let one_term = vec!["memory".to_string()];
        let two_terms = vec!["memory".to_string(), "evidence".to_string()];
        assert!(
            compute_relevance(goal, &fixture, &two_terms)
                > compute_relevance(goal, &fixture, &one_term),
            "more matched terms must increase relevance"
        );
    }

    #[test]
    fn compute_relevance_with_salience_adds_salience_to_base() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = vec!["memory".to_string()];
        let base = compute_relevance(goal, &fixture, &terms);
        let with_salience = compute_relevance_with_salience(goal, &fixture, &terms, 50);
        assert_eq!(with_salience, base + 50.0);
    }

    #[test]
    fn initiative_for_goal_uses_first_allowed_effect() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = vec!["memory".to_string()];
        let proposal = initiative_for_goal(goal, &terms);
        assert_eq!(proposal.effect, goal.allowed_effects[0]);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.matched_terms, terms);
    }

    #[test]
    fn initiative_for_effect_builds_proposal_with_correct_effect() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = vec!["memory".to_string()];
        let proposal = initiative_for_effect(goal, AllowedEffect::Reflect, &terms);
        assert_eq!(proposal.effect, AllowedEffect::Reflect);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.scope, goal.scope);
    }
}
