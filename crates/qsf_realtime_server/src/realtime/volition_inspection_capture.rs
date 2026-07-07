use qsf_volition::{
    ActivationKeyword, ForcedSurfacing, FunctionalSignal, GoalVisibility, InitiativeOutput,
    ModeArbitrationOutcome, RankedSelectionResult, ShapingIntensity, VolitionFixture,
    VolitionState, VolitionStateInspection, VolitionSuppressionReason, derive_signals,
    forced_surfaced_goals, goal_visibility,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::realtime::volition_injection::{
    AmbientExposure, VolitionModeBiasOutcome, build_mode_bias_outcomes, compute_ambient_exposure,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnWinnerSummary {
    pub winner_goal_id: String,
    pub winner_goal_title: String,
    pub winner_effective_tier: u8,
    pub winner_biased_tier: u8,
    pub protected_tier_active: bool,
    /// The winner's narration visibility. The operator panel badges a subconscious winner and
    /// reads `ambient_exposure` to show how it was exposed this turn. `#[serde(default)]` =
    /// `Conscious` for captures serialized before this field.
    #[serde(default)]
    pub winner_visibility: GoalVisibility,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionBelowThresholdSummary {
    pub goal_id: String,
    pub goal_title: String,
    pub matched_keywords: Vec<ActivationKeyword>,
    pub match_strength: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnDecisionSummary {
    /// `None` on a no-qualifier turn — the dedicated no-winner turn-decision outcome.
    pub winner: Option<VolitionTurnWinnerSummary>,
    pub qualification_threshold: u32,
    pub below_threshold: Vec<VolitionBelowThresholdSummary>,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub selected_goal_ids: Vec<String>,
    pub omitted_or_suppressed_goal_ids: Vec<String>,
    pub shaping_intensity: ShapingIntensity,
    pub last_initiative_output_kind: Option<String>,
    pub last_initiative_surfaced: bool,
    pub last_initiative_suppression_reason: Option<VolitionSuppressionReason>,
    pub last_initiative_rendered_line_present: bool,
    /// How this turn's winner was exposed in the model-visible text: `ordinary` for a conscious
    /// winner, `reduced_subconscious` / `forced_surfaced_subconscious` for a subconscious winner.
    /// `#[serde(default)]` = `Ordinary` for captures serialized before this field.
    #[serde(default = "default_ambient_exposure_capture")]
    pub ambient_exposure: AmbientExposure,
    /// Count of selected goals that are subconscious dispositions this turn.
    #[serde(default)]
    pub subconscious_selected_count: usize,
}

fn default_ambient_exposure_capture() -> AmbientExposure {
    AmbientExposure::Ordinary
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
    /// Display-only functional signals derived from the live volition state via
    /// [`derive_signals`]. Operator-panel only: never model-visible (no context injection, no
    /// tool output). `#[serde(default)]` keeps previously captured JSON (no `signals` key)
    /// parseable.
    #[serde(default)]
    pub signals: Vec<FunctionalSignal>,
    /// Subconscious goals forced to surface this run, with the recorded condition forcing each
    /// (rendered initiative or coherence conflict). Derived via
    /// [`forced_surfaced_goals`](qsf_volition::forced_surfaced_goals) so the operator panel can
    /// badge which subconscious goals are surfaced and why, without hiding any. Operator-panel
    /// only. `#[serde(default)]` keeps previously captured JSON parseable.
    #[serde(default)]
    pub forced_surfaced: Vec<ForcedSurfacing>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_volition_turn_decision_summary(
    ranked: &RankedSelectionResult,
    outcome: &ModeArbitrationOutcome,
    initiative_output: Option<&InitiativeOutput>,
    surfaced: bool,
    suppression_reason: Option<VolitionSuppressionReason>,
    rendered_line_present: bool,
    shaping_intensity: ShapingIntensity,
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> VolitionTurnDecisionSummary {
    // The winner block and mode-bias outcomes derive from the qualified result; on a
    // no-qualifier turn there is no `ModeArbitrationResult` (below-threshold candidates never
    // entered the sort), so both are absent/empty and the winner is `None`.
    let winner = outcome
        .qualified
        .as_ref()
        .map(|arbitration| VolitionTurnWinnerSummary {
            winner_goal_id: arbitration.winner.goal.id.clone(),
            winner_goal_title: arbitration.winner.goal.title.clone(),
            winner_effective_tier: arbitration.winner_bias.effective_tier,
            winner_biased_tier: arbitration.winner_bias.biased_tier,
            protected_tier_active: arbitration.winner_bias.protected,
            winner_visibility: goal_visibility(&arbitration.winner.goal.id, state, fixture),
        });
    let winner_visibility = winner.as_ref().map(|w| w.winner_visibility);
    let ambient_exposure = compute_ambient_exposure(
        winner_visibility,
        winner.as_ref().map(|w| w.winner_goal_id.as_str()),
        rendered_line_present,
        &state.declined_candidates,
    );
    let subconscious_selected_count = ranked
        .selected
        .iter()
        .filter(|selection| {
            goal_visibility(&selection.goal.id, state, fixture) == GoalVisibility::Subconscious
        })
        .count();
    let mode_bias_outcomes = outcome
        .qualified
        .as_ref()
        .map(build_mode_bias_outcomes)
        .unwrap_or_default();
    let below_threshold = outcome
        .below_threshold
        .iter()
        .map(|candidate| VolitionBelowThresholdSummary {
            goal_id: candidate.selection.goal.id.clone(),
            goal_title: candidate.selection.goal.title.clone(),
            matched_keywords: candidate.selection.matched_keywords.clone(),
            match_strength: candidate.match_strength,
        })
        .collect();

    VolitionTurnDecisionSummary {
        winner,
        qualification_threshold: outcome.qualification_threshold,
        below_threshold,
        mode_bias_outcomes,
        selected_goal_ids: ranked
            .selected
            .iter()
            .map(|selection| selection.goal.id.clone())
            .collect(),
        omitted_or_suppressed_goal_ids: build_omitted_or_suppressed_goal_ids(ranked),
        shaping_intensity,
        last_initiative_output_kind: initiative_output
            .map(|output| initiative_output_kind(output).to_string()),
        last_initiative_surfaced: surfaced,
        last_initiative_suppression_reason: suppression_reason,
        last_initiative_rendered_line_present: rendered_line_present,
        ambient_exposure,
        subconscious_selected_count,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_volition_inspection_capture(
    qsf_session_id: String,
    exchange_index: usize,
    captured_at: OffsetDateTime,
    response_create_event_ref: String,
    state: &VolitionState,
    fixture: &VolitionFixture,
    inspection: VolitionStateInspection,
    decision: Option<VolitionTurnDecisionSummary>,
) -> VolitionInspectionCapture {
    // Signals and forced-surfacing are derived here, in the single capture-builder site, so the
    // operator panel is the only surface that ever sees them. Both read the live session
    // state/fixture. `forced_surfaced` lets the panel badge which subconscious goals are surfaced
    // and why, without hiding any (guardrail D2).
    let signals = derive_signals(state, fixture);
    let forced_surfaced = forced_surfaced_goals(state, fixture);
    VolitionInspectionCapture {
        qsf_session_id,
        exchange_index,
        captured_at,
        response_create_event_ref,
        inspection,
        decision,
        signals,
        forced_surfaced,
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
        ModeArbitrationOutcome,
        InitiativeOutput,
        ShapingIntensity,
    ) {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .expect("expected selection");
        let winner = outcome.qualified.as_ref().expect("qualified winner");
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("how can you help me"),
            &state,
            &fixture,
        );
        let intensity = qsf_volition::choose_shaping_intensity(
            winner,
            &opportunities,
            ReceptivenessHint::Neutral,
        );
        let output =
            qsf_volition::execute_initiative(&winner.winner.initiative, &winner.winner.goal);
        (ranked, outcome, output, intensity)
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
            &state,
            &fixture,
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
        let (ranked, outcome, output, intensity) = selection_decision();
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &outcome,
            Some(&output),
            false,
            Some(VolitionSuppressionReason::Intensity),
            false,
            intensity,
            &state,
            &fixture,
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            inspection,
            Some(decision.clone()),
        );

        assert_eq!(capture.qsf_session_id, "session-123");
        assert_eq!(capture.exchange_index, 7);
        assert_eq!(capture.response_create_event_ref, "request-ref");
        assert!(capture.decision.is_some());
        let decision = capture.decision.expect("decision");
        assert!(
            decision
                .winner
                .as_ref()
                .expect("winner")
                .protected_tier_active
        );
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
            &state,
            &fixture,
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
        let (ranked, outcome, output, intensity) = selection_decision();
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &outcome,
            Some(&output),
            false,
            Some(VolitionSuppressionReason::Intensity),
            false,
            intensity,
            &state,
            &fixture,
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
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
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .expect("expected selection");
        let winner = outcome.qualified.as_ref().expect("qualified winner");
        let output =
            qsf_volition::execute_initiative(&winner.winner.initiative, &winner.winner.goal);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            Some(build_volition_turn_decision_summary(
                &ranked,
                &outcome,
                Some(&output),
                false,
                Some(VolitionSuppressionReason::Intensity),
                false,
                ShapingIntensity::None,
                &state,
                &fixture,
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

    #[test]
    fn no_qualifier_turn_builds_no_winner_decision_with_reason_and_threshold() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
        let outcome =
            arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral).unwrap();
        assert!(outcome.qualified.is_none());
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &outcome,
            None,
            false,
            Some(VolitionSuppressionReason::BelowQualificationThreshold),
            false,
            ShapingIntensity::None,
            &state,
            &fixture,
        );
        assert!(decision.winner.is_none());
        assert_eq!(
            decision.qualification_threshold,
            fixture.arbitration_qualification_threshold
        );
        assert!(!decision.below_threshold.is_empty());
        for below in &decision.below_threshold {
            assert_eq!(
                below.match_strength,
                below
                    .matched_keywords
                    .iter()
                    .map(|k| k.weight())
                    .sum::<u32>()
            );
        }
        assert!(decision.mode_bias_outcomes.is_empty());
        assert_eq!(
            decision.last_initiative_suppression_reason,
            Some(VolitionSuppressionReason::BelowQualificationThreshold)
        );
        assert!(decision.last_initiative_output_kind.is_none());
    }

    #[test]
    fn below_qualification_threshold_reason_serializes_to_wire_string() {
        let json = serde_json::to_string(&VolitionSuppressionReason::BelowQualificationThreshold)
            .expect("json");
        assert_eq!(json, "\"below_qualification_threshold\"");
    }

    // ── functional signals surfaced on the capture ───────────────────────────

    /// Advance the clock past the boredom elapsed-guard so a fresh seed state emits at least one
    /// functional signal, without any live model call.
    fn state_with_signals() -> (VolitionFixture, VolitionState) {
        let fixture = realtime_seed_fixture();
        let state = qsf_volition::apply(
            VolitionState::from_fixture(&fixture),
            qsf_volition::VolitionEvent::TickAdvanced {
                tick: qsf_volition::BOREDOM_MIN_ELAPSED_TICKS,
            },
        );
        (fixture, state)
    }

    #[test]
    fn capture_signals_match_derive_signals_for_active_state() {
        let (fixture, state) = state_with_signals();
        let expected = derive_signals(&state, &fixture);
        assert!(
            !expected.is_empty(),
            "test setup must produce at least one signal"
        );

        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );

        assert_eq!(capture.signals, expected);
        assert!(
            capture
                .signals
                .iter()
                .any(|s| s.kind == qsf_volition::SignalKind::Boredom)
        );
    }

    #[test]
    fn capture_signals_empty_on_cold_start_state() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );
        assert!(
            capture.signals.is_empty(),
            "a cold-start state must surface no signals"
        );
    }

    #[test]
    fn serialized_capture_includes_top_level_signals_array() {
        let (fixture, state) = state_with_signals();
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );

        // The capture is flattened into the `volition_state` websocket message, so its own
        // serialization is what rides the wire under a top-level `signals` key.
        let value = serde_json::to_value(&capture).expect("json");
        assert!(
            value.get("signals").is_some_and(|s| s.is_array()),
            "capture must serialize a top-level `signals` array"
        );
        assert!(
            !value["signals"].as_array().unwrap().is_empty(),
            "signals array must carry the derived signals"
        );
    }

    // ── visibility on the capture (Phase 4, steps 4-6) ───────────────────────

    #[test]
    fn decision_reports_subconscious_winner_visibility_and_reduced_exposure() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        // "world history society politics" activates only the subconscious world-picture goal.
        let ranked = select_goals_ranked("world history society politics", &state, &fixture);
        let outcome =
            arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral).unwrap();
        assert_eq!(
            outcome.qualified.as_ref().unwrap().winner.goal.id,
            "assemble-world-picture"
        );
        let decision = build_volition_turn_decision_summary(
            &ranked,
            &outcome,
            None,
            false,
            None,
            false,
            ShapingIntensity::Low,
            &state,
            &fixture,
        );
        let winner = decision.winner.expect("winner");
        assert_eq!(winner.winner_visibility, GoalVisibility::Subconscious);
        assert_eq!(
            decision.ambient_exposure,
            AmbientExposure::ReducedSubconscious
        );
        assert_eq!(decision.subconscious_selected_count, 1);
    }

    #[test]
    fn capture_reports_forced_surfaced_subconscious_goal() {
        let fixture = realtime_seed_fixture();
        // The subconscious goal renders an initiative line → forced surfaced.
        let state = qsf_volition::apply(
            VolitionState::from_fixture(&fixture),
            qsf_volition::VolitionEvent::InitiativeExecuted {
                goal_id: "assemble-world-picture".to_string(),
                effect: qsf_volition::AllowedEffect::Reflect,
                output: InitiativeOutput::ReflectionRequested {
                    proposed_question: "How does this fit the larger picture?".to_string(),
                },
                rationale: "test".to_string(),
                tick: 2,
                rendered_ref: Some(
                    qsf_volition::EvidenceRef::try_new("exchange:1/diagnostic:x").unwrap(),
                ),
            },
        );
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );
        assert!(
            capture
                .forced_surfaced
                .iter()
                .any(|f| f.goal_id == "assemble-world-picture"),
            "the rendered subconscious goal must be reported forced surfaced on the capture"
        );
    }

    #[test]
    fn capture_json_without_forced_surfaced_still_deserializes() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );
        let mut value = serde_json::to_value(&capture).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .remove("forced_surfaced");
        let restored: VolitionInspectionCapture =
            serde_json::from_value(value).expect("back-compat deserialize without forced_surfaced");
        assert!(restored.forced_surfaced.is_empty());
    }

    #[test]
    fn capture_json_without_signals_still_deserializes() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let capture = build_volition_inspection_capture(
            "session-123".to_string(),
            7,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            "request-ref".to_string(),
            &state,
            &fixture,
            build_state_inspection(&state, &fixture),
            None,
        );

        // Emulate a previously captured artifact that predates the `signals` field.
        let mut value = serde_json::to_value(&capture).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .remove("signals")
            .expect("signals present before removal");
        assert!(value.get("signals").is_none());

        let restored: VolitionInspectionCapture =
            serde_json::from_value(value).expect("back-compat deserialize without signals");
        assert!(restored.signals.is_empty());
    }
}
