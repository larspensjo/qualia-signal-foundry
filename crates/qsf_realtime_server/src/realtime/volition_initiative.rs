use qsf_volition::{
    AllowedEffect, GoalSelection, InitiativeOutput, ShapingIntensity, VolitionStateInspection,
    VolitionSuppressionReason,
};
use serde::{Deserialize, Serialize};

const MAX_RENDERED_INITIATIVE_LINE_CHARS: usize = 240;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RealtimeBoundedOrExternalOutput {
    pub initiative_output: InitiativeOutput,
    pub external_effect_executed: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RealtimeBoundedInitiativeTrace {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    pub winning_goal_id: String,
    pub initiative_proposal: qsf_volition::InitiativeProposal,
    pub allowed_effect: AllowedEffect,
    pub initiative_output: InitiativeOutput,
    pub bounded_or_external_output: RealtimeBoundedOrExternalOutput,
    pub surfaced: bool,
    pub suppression_reason: Option<VolitionSuppressionReason>,
    pub rendered_line_present: bool,
    pub context_retrieval_hint_terms: Option<Vec<String>>,
    pub hint_consumed_by_next_memory_injection: bool,
    pub rationale: String,
    pub state_snapshot_before: VolitionStateInspection,
    pub state_snapshot_after: VolitionStateInspection,
    pub response_create_event_ref: String,
    pub artifact_or_record_reference: String,
}

pub fn render_initiative_line(
    output: &InitiativeOutput,
    intensity: ShapingIntensity,
) -> Option<String> {
    if matches!(intensity, ShapingIntensity::None) {
        return None;
    }

    let line = match output {
        InitiativeOutput::ReflectionRequested { proposed_question } => format!(
            "Bounded initiative: reflect on {proposed_question}. Keep it simulated and internal; do not take external action."
        ),
        InitiativeOutput::ContextRetrievalRequested { .. } => return None,
        InitiativeOutput::ExperimentProposed { hypothesis, scope } => format!(
            "Bounded initiative: consider experiment {hypothesis} (scope: {scope}). Keep it simulated and internal; do not take external action."
        ),
        InitiativeOutput::OpenThreadSurfaced { thread_summary } => format!(
            "Bounded initiative: surface open thread {thread_summary}. Keep it simulated and internal; do not take external action."
        ),
    };

    Some(bounded_line(line))
}

#[allow(clippy::too_many_arguments)]
pub fn build_realtime_bounded_initiative_trace(
    qsf_session_id: &str,
    exchange_index: usize,
    winner: &GoalSelection,
    initiative_output: InitiativeOutput,
    surfaced: bool,
    suppression_reason: Option<VolitionSuppressionReason>,
    rendered_line_present: bool,
    state_snapshot_before: VolitionStateInspection,
    state_snapshot_after: VolitionStateInspection,
    context_retrieval_hint_terms: Option<Vec<String>>,
    hint_consumed_by_next_memory_injection: bool,
    response_create_event_ref: &str,
) -> RealtimeBoundedInitiativeTrace {
    let bounded_or_external_output = RealtimeBoundedOrExternalOutput {
        initiative_output: initiative_output.clone(),
        external_effect_executed: false,
    };

    RealtimeBoundedInitiativeTrace {
        qsf_session_id: qsf_session_id.to_string(),
        exchange_index,
        winning_goal_id: winner.goal.id.clone(),
        initiative_proposal: winner.initiative.clone(),
        allowed_effect: winner.initiative.effect,
        initiative_output,
        bounded_or_external_output,
        surfaced,
        suppression_reason,
        rendered_line_present,
        context_retrieval_hint_terms,
        hint_consumed_by_next_memory_injection,
        rationale: winner.initiative.rationale.clone(),
        state_snapshot_before,
        state_snapshot_after,
        response_create_event_ref: response_create_event_ref.to_string(),
        artifact_or_record_reference: format!(
            "exchange:{exchange_index}/diagnostic:realtime_bounded_initiative"
        ),
    }
}

fn bounded_line(line: String) -> String {
    if line.chars().count() <= MAX_RENDERED_INITIATIVE_LINE_CHARS {
        return line;
    }

    let mut truncated = line
        .chars()
        .take(MAX_RENDERED_INITIATIVE_LINE_CHARS.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_volition::{
        AllowedEffect, Goal, GoalScope, GoalSelection, GoalStatus, InitiativeOutput,
        InitiativeProposal, ShapingIntensity, Tension, TensionPriority, VolitionFixture,
        VolitionState, build_state_inspection,
    };

    fn goal_selection(effect: AllowedEffect) -> GoalSelection {
        let goal = Goal {
            id: "goal-1".to_string(),
            title: "Goal 1".to_string(),
            summary: "Goal summary".to_string(),
            tension_ids: vec!["tension-1".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority: 10,
            activation_keywords: vec!["goal".to_string()],
            allowed_effects: vec![effect],
            satisfaction_condition_summary: "done".to_string(),
            evidence_refs: vec![],
            estimated_tokens: 10,
            source_reference: "test".to_string(),
        };
        GoalSelection {
            goal: goal.clone(),
            relevance_score: 123.0,
            matched_terms: vec!["goal".to_string()],
            initiative: InitiativeProposal {
                goal_id: goal.id.clone(),
                goal_title: goal.title.clone(),
                effect,
                rationale: "test rationale".to_string(),
                matched_terms: vec!["goal".to_string()],
                scope: goal.scope,
            },
        }
    }

    #[test]
    fn render_initiative_line_reflection_is_bounded_and_exact() {
        let output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What should we check next?".to_string(),
        };

        let line = render_initiative_line(&output, ShapingIntensity::Low).expect("line");

        assert_eq!(
            line,
            "Bounded initiative: reflect on What should we check next?. Keep it simulated and internal; do not take external action."
        );
        assert!(line.chars().count() <= MAX_RENDERED_INITIATIVE_LINE_CHARS);
    }

    #[test]
    fn render_initiative_line_experiment_is_bounded_and_exact() {
        let output = InitiativeOutput::ExperimentProposed {
            hypothesis: "Try a smaller retrieval query".to_string(),
            scope: GoalScope::Session,
        };

        let line = render_initiative_line(&output, ShapingIntensity::Medium).expect("line");

        assert_eq!(
            line,
            "Bounded initiative: consider experiment Try a smaller retrieval query (scope: session). Keep it simulated and internal; do not take external action."
        );
        assert!(line.chars().count() <= MAX_RENDERED_INITIATIVE_LINE_CHARS);
    }

    #[test]
    fn render_initiative_line_open_thread_is_bounded_and_exact() {
        let output = InitiativeOutput::OpenThreadSurfaced {
            thread_summary: "Revisit the memory gap".to_string(),
        };

        let line = render_initiative_line(&output, ShapingIntensity::Low).expect("line");

        assert_eq!(
            line,
            "Bounded initiative: surface open thread Revisit the memory gap. Keep it simulated and internal; do not take external action."
        );
        assert!(line.chars().count() <= MAX_RENDERED_INITIATIVE_LINE_CHARS);
    }

    #[test]
    fn render_initiative_line_returns_none_for_context_retrieval_and_none_intensity() {
        let output = InitiativeOutput::ContextRetrievalRequested {
            query_terms: vec!["thread".to_string()],
        };

        assert_eq!(render_initiative_line(&output, ShapingIntensity::Low), None);
        let reflected = InitiativeOutput::ReflectionRequested {
            proposed_question: "What should we check next?".to_string(),
        };
        assert_eq!(
            render_initiative_line(&reflected, ShapingIntensity::None),
            None
        );
    }

    #[test]
    fn trace_marks_external_effect_as_not_executed_and_is_deterministic() {
        let selection = goal_selection(AllowedEffect::Reflect);
        let fixture = VolitionFixture {
            tensions: vec![Tension {
                id: "tension-1".to_string(),
                title: "Tension 1".to_string(),
                summary: "summary".to_string(),
                priority_bias: TensionPriority::Medium,
                arbitration_tier: 3,
            }],
            goals: vec![selection.goal.clone()],
        };
        let state = VolitionState::from_fixture(&fixture);
        let snapshot_before = build_state_inspection(&state, &fixture);
        let snapshot_after = snapshot_before.clone();
        let initiative_output = InitiativeOutput::ReflectionRequested {
            proposed_question: "What should we check next?".to_string(),
        };

        let first = build_realtime_bounded_initiative_trace(
            "session-1",
            7,
            &selection,
            initiative_output.clone(),
            true,
            None,
            true,
            snapshot_before.clone(),
            snapshot_after.clone(),
            Some(vec!["thread".to_string()]),
            true,
            "response-create-ref",
        );
        let second = build_realtime_bounded_initiative_trace(
            "session-1",
            7,
            &selection,
            initiative_output,
            true,
            None,
            true,
            snapshot_before,
            snapshot_after,
            Some(vec!["thread".to_string()]),
            true,
            "response-create-ref",
        );

        assert!(!first.bounded_or_external_output.external_effect_executed);
        assert_eq!(first, second);
    }
}
