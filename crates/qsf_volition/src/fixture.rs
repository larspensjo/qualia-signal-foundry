use crate::{
    AllowedEffect, Goal, GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture,
};

/// Extended fixture for the realtime session seed. Includes all tensions and goals from
/// `static_fixture` plus the protected-tier tensions and goals that must be present in any
/// realtime session before behavioral influence can be activated.
///
/// Protected tiers (≤ `PROTECTED_TIER_FLOOR = 3`) are immune to mode bias and always win
/// arbitration over curiosity or exploration goals.
pub fn realtime_seed_fixture() -> VolitionFixture {
    let base = static_fixture();
    let mut tensions = base.tensions;
    let mut goals = base.goals;

    tensions.push(Tension {
        id: "explicit-user-intent".to_string(),
        title: "Explicit user intent".to_string(),
        summary: "Honor what the user is explicitly requesting in this turn.".to_string(),
        priority_bias: TensionPriority::Highest,
        arbitration_tier: 2,
        focused_bias: 0,
        exploratory_bias: 0,
    });
    tensions.push(Tension {
        id: "current-task-completion".to_string(),
        title: "Current task completion".to_string(),
        summary: "Keep focus on completing the task that is currently in progress.".to_string(),
        priority_bias: TensionPriority::High,
        arbitration_tier: 3,
        focused_bias: 0,
        exploratory_bias: 0,
    });

    goals.push(Goal {
        id: "honor-explicit-user-request".to_string(),
        title: "Honor explicit user request".to_string(),
        summary: "Respond directly to what the user is explicitly asking for in this turn."
            .to_string(),
        tension_ids: vec!["explicit-user-intent".to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Input,
        base_priority: 100,
        activation_keywords: vec![
            "what".to_string(),
            "how".to_string(),
            "can".to_string(),
            "please".to_string(),
            "help".to_string(),
            "want".to_string(),
            "need".to_string(),
            "do".to_string(),
            "tell".to_string(),
            "show".to_string(),
            "explain".to_string(),
            "make".to_string(),
        ],
        allowed_effects: vec![AllowedEffect::Reflect],
        satisfaction_condition_summary: "The user's explicit request has been addressed directly."
            .to_string(),
        evidence_refs: vec!["docs/Plans/Plan.RealtimeVolitionIntegration.md".to_string()],
        estimated_tokens: 15,
        source_reference: "docs/Plans/Plan.RealtimeVolitionIntegration.md".to_string(),
    });
    goals.push(Goal {
        id: "complete-current-task".to_string(),
        title: "Complete current task".to_string(),
        summary: "Stay focused on finishing the task in progress without introducing unrelated diversions.".to_string(),
        tension_ids: vec!["current-task-completion".to_string()],
        status: GoalStatus::Accepted,
        scope: GoalScope::Session,
        base_priority: 95,
        activation_keywords: vec![
            "this".to_string(),
            "work".to_string(),
            "done".to_string(),
            "finish".to_string(),
            "continue".to_string(),
            "task".to_string(),
            "working".to_string(),
            "still".to_string(),
            "trying".to_string(),
            "going".to_string(),
        ],
        allowed_effects: vec![AllowedEffect::Reflect, AllowedEffect::SurfaceOpenThread],
        satisfaction_condition_summary:
            "The current task is complete or the user has explicitly moved on.".to_string(),
        evidence_refs: vec!["docs/Plans/Plan.RealtimeVolitionIntegration.md".to_string()],
        estimated_tokens: 18,
        source_reference: "docs/Plans/Plan.RealtimeVolitionIntegration.md".to_string(),
    });

    VolitionFixture { tensions, goals }
}

pub fn static_fixture() -> VolitionFixture {
    VolitionFixture {
        tensions: vec![
            Tension {
                id: "research-curiosity".to_string(),
                title: "Research curiosity".to_string(),
                summary: "Keep unresolved technical questions visible long enough to compare candidate designs.".to_string(),
                priority_bias: TensionPriority::Medium,
                arbitration_tier: 7,
                focused_bias: 3,
                exploratory_bias: -2,
            },
            Tension {
                id: "coherence-maintenance".to_string(),
                title: "Coherence maintenance".to_string(),
                summary: "Avoid overstating implementation status or blending speculative ideas into current fact.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 4,
                focused_bias: 0,
                exploratory_bias: 0,
            },
            Tension {
                id: "continuity-preservation".to_string(),
                title: "Continuity preservation".to_string(),
                summary: "Keep open threads and unresolved context available across turns.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 5,
                focused_bias: -1,
                exploratory_bias: 1,
            },
            Tension {
                id: "boundary-preservation".to_string(),
                title: "Boundary preservation".to_string(),
                summary: "Protect the distinction between current code, future experiments, and out-of-scope ideas.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 1,
                focused_bias: 0,
                exploratory_bias: 0,
            },
        ],
        goals: vec![
            Goal {
                id: "clarify-weak-evidence-topic".to_string(),
                title: "Clarify weak evidence topic".to_string(),
                summary: "Surface a research question when the input points at uncertain or under-explained material.".to_string(),
                tension_ids: vec!["research-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 85,
                activation_keywords: vec![
                    "voice".to_string(),
                    "memory".to_string(),
                    "evidence".to_string(),
                    "unclear".to_string(),
                    "unsettled".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect, AllowedEffect::ProposeExperiment],
                satisfaction_condition_summary: "The uncertain topic has been named clearly enough to compare options or ask a narrower question.".to_string(),
                evidence_refs: vec![
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                    "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
                ],
                estimated_tokens: 20,
                source_reference: "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
            },
            Goal {
                id: "avoid-overstating-impl-status".to_string(),
                title: "Avoid overstating implementation status".to_string(),
                summary: "Keep status claims grounded when the input asks whether the volition work is actually done.".to_string(),
                tension_ids: vec![
                    "coherence-maintenance".to_string(),
                    "boundary-preservation".to_string(),
                ],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 95,
                activation_keywords: vec![
                    "implemented".to_string(),
                    "status".to_string(),
                    "complete".to_string(),
                    "done".to_string(),
                    "ready".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "The response avoids claiming completion that the current repository state does not support.".to_string(),
                evidence_refs: vec![
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                    "docs/DecisionLog.md".to_string(),
                ],
                estimated_tokens: 18,
                source_reference: "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
            },
            Goal {
                id: "resurface-open-thread".to_string(),
                title: "Resurface open thread".to_string(),
                summary: "Bring an unresolved continuity issue back into view when the input mentions continuity or an open thread.".to_string(),
                tension_ids: vec!["continuity-preservation".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 98,
                activation_keywords: vec![
                    "continuity".to_string(),
                    "thread".to_string(),
                    "revisit".to_string(),
                    "open".to_string(),
                    "unresolved".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::RetrieveContext, AllowedEffect::SurfaceOpenThread],
                satisfaction_condition_summary: "The unresolved thread is named well enough that the next turn can carry it forward.".to_string(),
                evidence_refs: vec![
                    "docs/Architecture/Architecture.ContextManagement.md".to_string(),
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                ],
                estimated_tokens: 24,
                source_reference: "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
            },
            Goal {
                id: "propose-followup-experiment".to_string(),
                title: "Propose follow-up experiment".to_string(),
                summary: "Suggest a bounded follow-up experiment when the conversation is already in research mode.".to_string(),
                tension_ids: vec!["research-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 90,
                activation_keywords: vec![
                    "experiment".to_string(),
                    "compare".to_string(),
                    "perturbation".to_string(),
                    "fixture".to_string(),
                    "prototype".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::ProposeExperiment],
                satisfaction_condition_summary: "A concrete follow-up experiment has been described in a way that can be run later.".to_string(),
                evidence_refs: vec![
                    "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
                    "docs/Plans/Plan.VolitionGoalSystem.md".to_string(),
                ],
                estimated_tokens: 22,
                source_reference: "docs/Experiments/Experiment.VolitionGoalFixture.md".to_string(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GoalSelection, InitiativeProposal, Mode, PROTECTED_TIER_FLOOR, VolitionState,
        arbitrate_with_mode,
    };

    #[test]
    fn static_fixture_loads_and_is_deterministic() {
        let f1 = static_fixture();
        let f2 = static_fixture();
        assert_eq!(f1, f2);
        assert!(!f1.tensions.is_empty());
        assert!(!f1.goals.is_empty());
    }

    #[test]
    fn realtime_seed_fixture_is_deterministic() {
        let f1 = realtime_seed_fixture();
        let f2 = realtime_seed_fixture();
        assert_eq!(f1, f2);
    }

    #[test]
    fn realtime_seed_fixture_includes_static_fixture_content() {
        let base = static_fixture();
        let seed = realtime_seed_fixture();
        for tension in &base.tensions {
            assert!(
                seed.tensions.iter().any(|t| t.id == tension.id),
                "static fixture tension '{}' missing from realtime seed",
                tension.id
            );
        }
        for goal in &base.goals {
            assert!(
                seed.goals.iter().any(|g| g.id == goal.id),
                "static fixture goal '{}' missing from realtime seed",
                goal.id
            );
        }
    }

    #[test]
    fn realtime_seed_fixture_has_protected_tier_tensions() {
        let fixture = realtime_seed_fixture();
        let explicit_user = fixture
            .tensions
            .iter()
            .find(|t| t.id == "explicit-user-intent")
            .expect("explicit-user-intent tension must be present");
        assert!(
            explicit_user.arbitration_tier <= PROTECTED_TIER_FLOOR,
            "explicit-user-intent must be at or below protected tier floor"
        );
        let task_completion = fixture
            .tensions
            .iter()
            .find(|t| t.id == "current-task-completion")
            .expect("current-task-completion tension must be present");
        assert!(
            task_completion.arbitration_tier <= PROTECTED_TIER_FLOOR,
            "current-task-completion must be at or below protected tier floor"
        );
    }

    #[test]
    fn realtime_seed_fixture_seeds_accepted_goals_for_protected_tensions() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        assert!(
            state.goals.contains_key("honor-explicit-user-request"),
            "honor-explicit-user-request must be seeded from realtime fixture"
        );
        assert!(
            state.goals.contains_key("complete-current-task"),
            "complete-current-task must be seeded from realtime fixture"
        );
    }

    fn make_goal_selection_for(goal_id: &str, fixture: &VolitionFixture) -> GoalSelection {
        let goal = fixture
            .goals
            .iter()
            .find(|g| g.id == goal_id)
            .unwrap_or_else(|| panic!("goal '{goal_id}' not found in fixture"))
            .clone();
        let scope = goal.scope;
        let effect = goal.allowed_effects[0];
        GoalSelection {
            relevance_score: goal.base_priority as f64,
            matched_terms: goal.activation_keywords[..1].to_vec(),
            initiative: InitiativeProposal {
                goal_id: goal.id.clone(),
                goal_title: goal.title.clone(),
                effect,
                rationale: "test".to_string(),
                matched_terms: goal.activation_keywords[..1].to_vec(),
                scope,
            },
            goal,
        }
    }

    #[test]
    fn tier2_goal_wins_over_tier7_curiosity_under_neutral_mode() {
        let fixture = realtime_seed_fixture();
        let protected = make_goal_selection_for("honor-explicit-user-request", &fixture);
        let curiosity = make_goal_selection_for("clarify-weak-evidence-topic", &fixture);
        let result =
            arbitrate_with_mode(vec![curiosity, protected], &fixture, Mode::Neutral).unwrap();
        assert_eq!(
            result.winner.goal.id, "honor-explicit-user-request",
            "tier-2 goal must win under Neutral"
        );
    }

    #[test]
    fn tier2_goal_wins_over_tier7_curiosity_under_focused_mode() {
        let fixture = realtime_seed_fixture();
        let protected = make_goal_selection_for("honor-explicit-user-request", &fixture);
        let curiosity = make_goal_selection_for("clarify-weak-evidence-topic", &fixture);
        let result =
            arbitrate_with_mode(vec![curiosity, protected], &fixture, Mode::Focused).unwrap();
        assert_eq!(
            result.winner.goal.id, "honor-explicit-user-request",
            "tier-2 goal must win under Focused"
        );
        assert!(
            result.winner_bias.protected,
            "tier-2 winner must be marked protected"
        );
    }

    #[test]
    fn tier2_goal_wins_over_tier7_curiosity_under_exploratory_mode() {
        let fixture = realtime_seed_fixture();
        let protected = make_goal_selection_for("honor-explicit-user-request", &fixture);
        let curiosity = make_goal_selection_for("clarify-weak-evidence-topic", &fixture);
        let result =
            arbitrate_with_mode(vec![curiosity, protected], &fixture, Mode::Exploratory).unwrap();
        assert_eq!(
            result.winner.goal.id, "honor-explicit-user-request",
            "tier-2 goal must win under Exploratory"
        );
    }

    #[test]
    fn tier3_goal_wins_over_tier7_curiosity_under_all_modes() {
        let fixture = realtime_seed_fixture();
        let protected = make_goal_selection_for("complete-current-task", &fixture);
        let curiosity = make_goal_selection_for("clarify-weak-evidence-topic", &fixture);
        for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
            let result =
                arbitrate_with_mode(vec![curiosity.clone(), protected.clone()], &fixture, mode)
                    .unwrap();
            assert_eq!(
                result.winner.goal.id, "complete-current-task",
                "tier-3 goal must win under {mode}"
            );
        }
    }
}
