use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ActivationKeyword, AllowedEffect, Goal, GoalSelection, GoalStatus, InitiativeProposal,
    KeywordWeightClass, OmittedGoal, VolitionFixture, VolitionState, normalize_terms,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankedSelectionResult {
    pub input_terms: Vec<String>,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub suppressed_cooldown: Vec<OmittedGoal>,
    pub visible_blocked: Vec<OmittedGoal>,
}

/// The activation keywords that fired for this input, deduplicated by term with fixture order
/// preserved. Carries each match's weight class so `match_strength` and the relevance bonus
/// derive from one quantity.
pub fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<ActivationKeyword> {
    let mut matched: Vec<ActivationKeyword> = Vec::new();
    for keyword in &goal.activation_keywords {
        if input_terms.iter().any(|term| term == &keyword.term)
            && !matched.iter().any(|k| k.term == keyword.term)
        {
            matched.push(keyword.clone());
        }
    }
    matched
}

/// Relevance points contributed per point of match strength. One Normal keyword (strength 4)
/// scores exactly what one matched term scored before this became the single scoring quantity.
pub const RELEVANCE_PER_STRENGTH_POINT: f64 = 25.0;

/// The single scoring quantity: the summed weights of the matched keywords. Both the ranked
/// relevance bonus and the arbitration qualification gate derive from this.
pub fn match_strength(matched: &[ActivationKeyword]) -> u32 {
    matched.iter().map(|keyword| keyword.weight()).sum()
}

pub fn compute_relevance(
    goal: &Goal,
    fixture: &VolitionFixture,
    matched: &[ActivationKeyword],
) -> f64 {
    let matched_bonus = match_strength(matched) as f64 * RELEVANCE_PER_STRENGTH_POINT;
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
    matched: &[ActivationKeyword],
    salience: i32,
) -> f64 {
    compute_relevance(goal, fixture, matched) + salience as f64
}

/// Match strength at which a goal that allows `ProposeExperiment` treats the match as a
/// strong thematic hit. Strength alone is not the contract — see the distinct-term rule.
pub const STRONG_MATCH_STRENGTH_THRESHOLD: u32 = 8;
/// A rich match also needs this many distinct non-Weak matched terms: a single Strong
/// keyword scores 8 but is not a rich match.
pub const STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS: usize = 2;

/// Choose which allowed effect a goal fires for this match. A goal that permits
/// `ProposeExperiment` on a rich match (match strength at or above
/// `STRONG_MATCH_STRENGTH_THRESHOLD` and at least `STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS`
/// distinct non-Weak matched terms) proposes; otherwise the goal takes its first allowed
/// effect (`Reflect` by convention). Generic infrastructure — no persona-specific terms in code.
pub fn select_effect_for_goal(goal: &Goal, matched: &[ActivationKeyword]) -> AllowedEffect {
    let allows_propose = goal
        .allowed_effects
        .contains(&AllowedEffect::ProposeExperiment);
    // Distinct by term: `matched_keywords` deduplicates fixture matches today, but this
    // function also takes manually constructed slices, so it must not trust its input.
    let distinct_non_weak = matched
        .iter()
        .filter(|keyword| keyword.weight_class != KeywordWeightClass::Weak)
        .map(|keyword| keyword.term.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    if allows_propose
        && match_strength(matched) >= STRONG_MATCH_STRENGTH_THRESHOLD
        && distinct_non_weak >= STRONG_MATCH_MIN_DISTINCT_NON_WEAK_TERMS
    {
        return AllowedEffect::ProposeExperiment;
    }
    goal.allowed_effects
        .first()
        .copied()
        .unwrap_or(AllowedEffect::Reflect)
}

pub fn initiative_for_goal(goal: &Goal, matched: &[ActivationKeyword]) -> InitiativeProposal {
    let effect = select_effect_for_goal(goal, matched);
    initiative_for_effect(goal, effect, matched)
}

pub fn initiative_for_effect(
    goal: &Goal,
    effect: AllowedEffect,
    matched: &[ActivationKeyword],
) -> InitiativeProposal {
    let matched_terms: Vec<String> = matched.iter().map(|keyword| keyword.term.clone()).collect();
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
        matched_terms,
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

        let matched = matched_keywords(&goal, &input_terms);

        if matches!(status, GoalStatus::Blocked) {
            visible_blocked.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms: matched.iter().map(|k| k.term.clone()).collect(),
                reason: "goal status is blocked (visible unresolved tension)".to_string(),
            });
            continue;
        }

        if matched.is_empty() {
            omitted.push(OmittedGoal {
                goal,
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let salience = dynamic.map(|entry| entry.salience).unwrap_or(0);
        let strength = match_strength(&matched);
        let relevance_score =
            compute_relevance_with_salience(&goal, fixture, &matched, salience);
        selected_candidates.push((goal, matched, strength, relevance_score));
    }

    selected_candidates.sort_by(|a, b| b.3.total_cmp(&a.3).then_with(|| a.0.id.cmp(&b.0.id)));

    let selected = selected_candidates
        .into_iter()
        .map(|(goal, matched, match_strength, relevance_score)| GoalSelection {
            initiative: initiative_for_goal(&goal, &matched),
            matched_keywords: matched,
            match_strength,
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
    fn relevance_and_match_strength_derive_from_the_same_quantity() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let matched = matched_keywords(goal, &normalize_terms("voice memory evidence"));
        let strength = match_strength(&matched);
        assert!(strength > 0);
        let with_match = compute_relevance(goal, &fixture, &matched);
        let without_match = compute_relevance(goal, &fixture, &[]);
        assert_eq!(
            with_match - without_match,
            strength as f64 * RELEVANCE_PER_STRENGTH_POINT,
            "ranked display and qualification must derive from one strength quantity"
        );
    }

    #[test]
    fn goal_selection_carries_matched_keywords_and_strength() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let result = select_goals_ranked("how can you help me", &state, &fixture);
        let selection = result
            .selected
            .iter()
            .find(|s| s.goal.id == "serve-the-present-person")
            .unwrap();
        assert_eq!(
            selection.match_strength,
            match_strength(&selection.matched_keywords)
        );
        assert!(!selection.matched_keywords.is_empty());
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
                .all(|kw| goal.activation_keywords.iter().any(|k| k.term == kw.term))
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
        let one_term = vec![ActivationKeyword::normal("memory")];
        let two_terms = vec![
            ActivationKeyword::normal("memory"),
            ActivationKeyword::normal("evidence"),
        ];
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
        let terms = vec![ActivationKeyword::normal("memory")];
        let base = compute_relevance(goal, &fixture, &terms);
        let with_salience = compute_relevance_with_salience(goal, &fixture, &terms, 50);
        assert_eq!(with_salience, base + 50.0);
    }

    #[test]
    fn initiative_for_goal_takes_first_effect_on_thin_match() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = vec![ActivationKeyword::normal("memory")];
        let proposal = initiative_for_goal(goal, &terms);
        assert_eq!(proposal.effect, goal.allowed_effects[0]);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.matched_terms, vec!["memory".to_string()]);
    }

    #[test]
    fn propose_experiment_requires_strength_and_two_distinct_non_weak_terms() {
        let fixture = realtime_seed_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "track-the-ai-transition")
            .unwrap();
        // Two Normal terms: strength 8, two non-Weak — fires.
        let two_normal = vec![
            ActivationKeyword::normal("job"),
            ActivationKeyword::normal("replace"),
        ];
        assert_eq!(
            select_effect_for_goal(goal, &two_normal),
            AllowedEffect::ProposeExperiment
        );
        // A single Strong term scores 8 but is not a rich match — reflects.
        let single_strong = vec![ActivationKeyword::strong("ai")];
        assert_eq!(
            select_effect_for_goal(goal, &single_strong),
            AllowedEffect::Reflect
        );
        // A duplicated Normal term scores 8 but is one distinct term — reflects.
        let duplicated_normal = vec![
            ActivationKeyword::normal("job"),
            ActivationKeyword::normal("job"),
        ];
        assert_eq!(
            select_effect_for_goal(goal, &duplicated_normal),
            AllowedEffect::Reflect
        );
        // Weak-word combinations never fire regardless of count.
        let weak_pile = vec![
            ActivationKeyword::weak("future"),
            ActivationKeyword::weak("power"),
            ActivationKeyword::weak("what"),
            ActivationKeyword::weak("do"),
        ];
        assert_eq!(
            select_effect_for_goal(goal, &weak_pile),
            AllowedEffect::Reflect
        );
    }

    #[test]
    fn track_ai_transition_proposes_experiment_on_rich_transition_match() {
        let fixture = realtime_seed_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "track-the-ai-transition")
            .unwrap();
        let rich = vec![
            ActivationKeyword::normal("automation"),
            ActivationKeyword::normal("job"),
            ActivationKeyword::normal("economy"),
        ];
        assert_eq!(
            select_effect_for_goal(goal, &rich),
            AllowedEffect::ProposeExperiment
        );

        let thin = vec![ActivationKeyword::normal("future")];
        assert_eq!(select_effect_for_goal(goal, &thin), AllowedEffect::Reflect);
    }

    #[test]
    fn reflect_only_goal_always_reflects() {
        let fixture = realtime_seed_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "serve-the-present-person")
            .unwrap();
        let terms = vec![
            ActivationKeyword::normal("how"),
            ActivationKeyword::normal("help"),
            ActivationKeyword::normal("explain"),
        ];
        assert_eq!(select_effect_for_goal(goal, &terms), AllowedEffect::Reflect);
    }

    #[test]
    fn static_fixture_clarify_goal_proposes_on_strong_match_reflects_on_thin() {
        // Deliberate behavior change: `clarify-weak-evidence-topic` (static_fixture) allows
        // [Reflect, ProposeExperiment], so the generic selector proposes on a
        // rich match (strength >= 8, two distinct non-Weak terms) and only reflects on a thin match.
        // Pinned here so the static-fixture change is explicit, not accidental.
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let strong = vec![
            ActivationKeyword::normal("voice"),
            ActivationKeyword::normal("memory"),
        ];
        assert_eq!(
            select_effect_for_goal(goal, &strong),
            AllowedEffect::ProposeExperiment
        );
        let thin = vec![ActivationKeyword::normal("memory")];
        assert_eq!(select_effect_for_goal(goal, &thin), AllowedEffect::Reflect);
    }

    #[test]
    fn initiative_for_effect_builds_proposal_with_correct_effect() {
        let fixture = static_fixture();
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == "clarify-weak-evidence-topic")
            .unwrap();
        let terms = vec![ActivationKeyword::normal("memory")];
        let proposal = initiative_for_effect(goal, AllowedEffect::Reflect, &terms);
        assert_eq!(proposal.effect, AllowedEffect::Reflect);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.scope, goal.scope);
    }
}
