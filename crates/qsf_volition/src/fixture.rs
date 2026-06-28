use crate::{
    AllowedEffect, Goal, GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture,
};

pub fn static_fixture() -> VolitionFixture {
    VolitionFixture {
        tensions: vec![
            Tension {
                id: "research-curiosity".to_string(),
                title: "Research curiosity".to_string(),
                summary: "Keep unresolved technical questions visible long enough to compare candidate designs.".to_string(),
                priority_bias: TensionPriority::Medium,
                arbitration_tier: 7,
            },
            Tension {
                id: "coherence-maintenance".to_string(),
                title: "Coherence maintenance".to_string(),
                summary: "Avoid overstating implementation status or blending speculative ideas into current fact.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 4,
            },
            Tension {
                id: "continuity-preservation".to_string(),
                title: "Continuity preservation".to_string(),
                summary: "Keep open threads and unresolved context available across turns.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 5,
            },
            Tension {
                id: "boundary-preservation".to_string(),
                title: "Boundary preservation".to_string(),
                summary: "Protect the distinction between current code, future experiments, and out-of-scope ideas.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 1,
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

    #[test]
    fn static_fixture_loads_and_is_deterministic() {
        let f1 = static_fixture();
        let f2 = static_fixture();
        assert_eq!(f1, f2);
        assert!(!f1.tensions.is_empty());
        assert!(!f1.goals.is_empty());
    }
}
