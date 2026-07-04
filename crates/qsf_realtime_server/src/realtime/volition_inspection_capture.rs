use qsf_volition::{
    InitiativeOutput, ModeArbitrationResult, RankedSelectionResult, ShapingIntensity,
    VolitionStateInspection, VolitionSuppressionReason,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::realtime::volition_injection::{VolitionModeBiasOutcome, build_mode_bias_outcomes};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnDecisionSummary {
    pub winner_goal_id: String,
    pub winner_goal_title: String,
    pub winner_effective_tier: u8,
    pub winner_biased_tier: u8,
    pub protected_tier_active: bool,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub selected_goal_ids: Vec<String>,
    pub omitted_or_suppressed_goal_ids: Vec<String>,
    pub shaping_intensity: ShapingIntensity,
    pub last_initiative_output_kind: Option<String>,
    pub last_initiative_surfaced: bool,
    pub last_initiative_suppression_reason: Option<VolitionSuppressionReason>,
    pub last_initiative_rendered_line_present: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionInspectionCapture {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    #[serde(with = "time::serde::rfc3339")]
    pub captured_at: OffsetDateTime,
    pub response_create_event_ref: String,
    pub inspection: VolitionStateInspection,
    pub decision: Option<VolitionTurnDecisionSummary>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_volition_turn_decision_summary(
    ranked: &RankedSelectionResult,
    arbitration: &ModeArbitrationResult,
    initiative_output: &InitiativeOutput,
    surfaced: bool,
    suppression_reason: Option<VolitionSuppressionReason>,
    rendered_line_present: bool,
    shaping_intensity: ShapingIntensity,
) -> VolitionTurnDecisionSummary {
    VolitionTurnDecisionSummary {
        winner_goal_id: arbitration.winner.goal.id.clone(),
        winner_goal_title: arbitration.winner.goal.title.clone(),
        winner_effective_tier: arbitration.winner_bias.effective_tier,
        winner_biased_tier: arbitration.winner_bias.biased_tier,
        protected_tier_active: arbitration.winner_bias.protected,
        mode_bias_outcomes: build_mode_bias_outcomes(arbitration),
        selected_goal_ids: ranked
            .selected
            .iter()
            .map(|selection| selection.goal.id.clone())
            .collect(),
        omitted_or_suppressed_goal_ids: build_omitted_or_suppressed_goal_ids(ranked),
        shaping_intensity,
        last_initiative_output_kind: Some(initiative_output_kind(initiative_output).to_string()),
        last_initiative_surfaced: surfaced,
        last_initiative_suppression_reason: suppression_reason,
        last_initiative_rendered_line_present: rendered_line_present,
    }
}

pub fn build_volition_inspection_capture(
    qsf_session_id: String,
    exchange_index: usize,
    captured_at: OffsetDateTime,
    response_create_event_ref: String,
    inspection: VolitionStateInspection,
    decision: Option<VolitionTurnDecisionSummary>,
) -> VolitionInspectionCapture {
    VolitionInspectionCapture {
        qsf_session_id,
        exchange_index,
        captured_at,
        response_create_event_ref,
        inspection,
        decision,
    }
}

fn build_omitted_or_suppressed_goal_ids(ranked: &RankedSelectionResult) -> Vec<String> {
    ranked
        .omitted
        .iter()
        .chain(ranked.suppressed_cooldown.iter())
        .chain(ranked.visible_blocked.iter())
        .map(|goal| goal.goal.id.clone())
        .collect()
}

fn initiative_output_kind(output: &InitiativeOutput) -> &'static str {
    match output {
        InitiativeOutput::ReflectionRequested { .. } => "reflection_requested",
        InitiativeOutput::ContextRetrievalRequested { .. } => "context_retrieval_requested",
        InitiativeOutput::ExperimentProposed { .. } => "experiment_proposed",
        InitiativeOutput::OpenThreadSurfaced { .. } => "open_thread_surfaced",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_volition::InitiativeOutput;
    use qsf_volition::{
        Mode, ReceptivenessHint, ShapingIntensity, VolitionState, arbitrate_with_mode,
        build_state_inspection, detect_opportunities, grounded_terms_from_text,
        realtime_seed_fixture, select_goals_ranked,
    };

    fn selection_decision() -> (
        qsf_volition::RankedSelectionResult,
        ModeArbitrationResult,
        InitiativeOutput,
        ShapingIntensity,
    ) {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .expect("expected selection")
            .qualified
            .expect("qualified winner");
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("how can you help me"),
            &state,
            &fixture,
        );
        let intensity = qsf_volition::choose_shaping_intensity(
            &arbitration,
            &opportunities,
            ReceptivenessHint::Neutral,
        );
        let output = qsf_volition::execute_initiative(
            &arbitration.winner.initiative,
            &arbitration.winner.goal,
        );
        (ranked, arbitration, output, intensity)
    }

    #[test]
    fn captured_at_serializes_as_string() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            inspection,
            None,
        );

        let value = serde_json::to_value(&capture).expect("json");
        assert!(value["captured_at"].is_string());
    }

    #[test]
    fn builder_preserves_decision_and_inspection_fields() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let (ranked, arbitration, output, intensity) = selection_decision();
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &arbitration,
            &output,
            false,
            Some(VolitionSuppressionReason::Intensity),
            false,
            intensity,
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            inspection,
            Some(decision.clone()),
        );

        assert_eq!(capture.qsf_session_id, "session-123");
        assert_eq!(capture.exchange_index, 7);
        assert_eq!(capture.response_create_event_ref, "request-ref");
        assert!(capture.decision.is_some());
        let decision = capture.decision.expect("decision");
        assert!(decision.protected_tier_active);
        assert!(!decision.mode_bias_outcomes.is_empty());
        assert!(
            decision
                .selected_goal_ids
                .contains(&"serve-the-present-person".to_string())
        );
        assert!(!decision.omitted_or_suppressed_goal_ids.is_empty());
        assert_eq!(
            decision.last_initiative_output_kind.as_deref(),
            Some("reflection_requested")
        );
        assert!(!decision.last_initiative_surfaced);
        assert_eq!(
            decision.last_initiative_suppression_reason,
            Some(VolitionSuppressionReason::Intensity)
        );
        assert!(!decision.last_initiative_rendered_line_present);
        assert_eq!(capture.inspection.mode, state.mode);
        assert_eq!(capture.inspection.tick, state.tick);
    }

    #[test]
    fn builder_preserves_no_decision_and_inspection_fields() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            inspection.clone(),
            None,
        );

        assert!(capture.decision.is_none());
        assert_eq!(capture.inspection, inspection);
    }

    #[test]
    fn serialized_capture_does_not_leak_provider_payload_or_instructions() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let inspection = build_state_inspection(&state, &fixture);
        let (ranked, arbitration, output, intensity) = selection_decision();
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &arbitration,
            &output,
            false,
            Some(VolitionSuppressionReason::Intensity),
            false,
            intensity,
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            inspection,
            Some(decision),
        );
        let serialized = serde_json::to_string(&capture).expect("json");

        assert!(!serialized.contains("OPENAI_API_KEY"));
        assert!(!serialized.contains("Bearer "));
        assert!(!serialized.contains("The following describes your simulated volition stance"));
        assert!(!serialized.contains("conversation.item.create"));
        assert!(!serialized.contains("response.create"));
        assert!(!serialized.contains("\"messages\""));
    }

    #[test]
    fn builder_handles_selection_without_surface() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .expect("expected selection")
            .qualified
            .expect("qualified winner");
        let output = qsf_volition::execute_initiative(
            &arbitration.winner.initiative,
            &arbitration.winner.goal,
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            build_state_inspection(&state, &fixture),
            Some(build_volition_turn_decision_summary(
                &ranked,
                &arbitration,
                &output,
                false,
                Some(VolitionSuppressionReason::Intensity),
                false,
                ShapingIntensity::None,
            )),
        );

        let decision = capture.decision.expect("decision");
        assert_eq!(
            decision.last_initiative_suppression_reason,
            Some(VolitionSuppressionReason::Intensity)
        );
        assert!(!decision.last_initiative_surfaced);
        assert!(!decision.last_initiative_rendered_line_present);
    }
}
