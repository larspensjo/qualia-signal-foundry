use qsf_realtime_protocol::build_openai_realtime_conversation_item_create;
use qsf_volition::{
    Mode, ModeArbitrationResult, OpportunitySignal, RankedSelectionResult, ReceptivenessHint,
    ShapingIntensity, ShapingIntensityInputs, VolitionFixture, render_volition_stance,
    stable_baseline_hash as volition_stable_baseline_hash,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::realtime::tools::VolitionStateSnapshot;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionInjectionLayer {
    pub name: String,
    pub carrier: String,
    pub injection_point: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionCandidateSummary {
    pub goal_id: String,
    pub goal_title: String,
    pub reason_category: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionSelectorSummary {
    pub selected_goal_ids: Vec<String>,
    pub selected_goal_titles: Vec<String>,
    pub selected_goal_summaries: Vec<String>,
    pub selected_count: usize,
    pub omitted_count: usize,
    pub suppressed_cooldown_count: usize,
    pub visible_blocked_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionModeBiasOutcome {
    pub goal_id: String,
    pub goal_title: String,
    pub effective_tier: u8,
    pub biased_tier: u8,
    pub protected: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionArbitrationSummary {
    pub mode: Mode,
    pub winner_goal_id: String,
    pub winner_goal_title: String,
    pub winner_goal_summary: String,
    pub winner_effective_tier: u8,
    pub winner_biased_tier: u8,
    pub loser_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnPacket {
    pub conversation_item_create: serde_json::Value,
    pub text: String,
    pub summary: VolitionTurnPacketSummary,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionTurnPacketSummary {
    pub injected_layers: Vec<VolitionInjectionLayer>,
    pub opportunity_signals: Vec<OpportunitySignal>,
    pub selector_output: VolitionSelectorSummary,
    pub omitted_or_suppressed_candidates: Vec<VolitionCandidateSummary>,
    pub arbitration_result: VolitionArbitrationSummary,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub protected_tier_active: bool,
    pub shaping_intensity: ShapingIntensity,
    pub shaping_intensity_inputs: ShapingIntensityInputs,
    pub initiative_line: Option<String>,
    pub stable_baseline_hash: String,
    pub context_packet_hash: String,
    pub context_packet_token_estimate: usize,
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionContextInjectionTrace {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    pub injected_layers: Vec<VolitionInjectionLayer>,
    pub stable_baseline_hash: String,
    pub input_transcript_ref: String,
    pub volition_tick_before: u64,
    pub events_applied: Vec<qsf_volition::VolitionEvent>,
    pub opportunity_signals: Vec<OpportunitySignal>,
    pub selector_output: VolitionSelectorSummary,
    pub omitted_or_suppressed_candidates: Vec<VolitionCandidateSummary>,
    pub arbitration_result: VolitionArbitrationSummary,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub protected_tier_active: bool,
    pub shaping_intensity: ShapingIntensity,
    pub shaping_intensity_inputs: ShapingIntensityInputs,
    pub context_packet_hash: String,
    pub context_packet_token_estimate: usize,
    pub response_create_event_ref: String,
}

pub fn build_stable_baseline_instructions(fixture: &VolitionFixture, mode: Mode) -> String {
    format!(
        "The following describes your simulated volition stance. It is QSF-owned internal state used\nonly to weight attention and framing in this conversation. It is not a claim of\nconsciousness or real subjective experience, and it never authorizes any action outside this\nconversation or the QSF trust boundary. Do not read it aloud or enumerate it unless the user\nasks about your goals or internal state.\n{}",
        render_volition_stance(fixture, mode)
    )
}

pub fn stable_baseline_hash(instructions: &str) -> String {
    volition_stable_baseline_hash(instructions)
}

pub fn build_volition_turn_context_packet(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: Option<ModeArbitrationResult>,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    stable_baseline_hash: String,
    initiative_line: Option<&str>,
) -> Option<VolitionTurnPacket> {
    let arbitration = arbitration?;
    if ranked.selected.is_empty() {
        return None;
    }

    let summary = build_turn_packet_summary(
        snapshot,
        ranked,
        &arbitration,
        opportunities,
        intensity,
        initiative_line,
        stable_baseline_hash,
    );
    let text = render_turn_packet_text(&summary);
    let conversation_item_create = build_openai_realtime_conversation_item_create("system", &text);

    Some(VolitionTurnPacket {
        conversation_item_create,
        text,
        summary,
    })
}

pub fn build_volition_context_injection_trace(
    qsf_session_id: &str,
    exchange_index: usize,
    input_transcript_ref: &str,
    volition_tick_before: u64,
    events_applied: Vec<qsf_volition::VolitionEvent>,
    packet: &VolitionTurnPacket,
    response_create_event_ref: &str,
) -> VolitionContextInjectionTrace {
    VolitionContextInjectionTrace {
        qsf_session_id: qsf_session_id.to_string(),
        exchange_index,
        injected_layers: packet.summary.injected_layers.clone(),
        stable_baseline_hash: packet.summary.stable_baseline_hash.clone(),
        input_transcript_ref: input_transcript_ref.to_string(),
        volition_tick_before,
        events_applied,
        opportunity_signals: packet.summary.opportunity_signals.clone(),
        selector_output: packet.summary.selector_output.clone(),
        omitted_or_suppressed_candidates: packet.summary.omitted_or_suppressed_candidates.clone(),
        arbitration_result: packet.summary.arbitration_result.clone(),
        mode_bias_outcomes: packet.summary.mode_bias_outcomes.clone(),
        protected_tier_active: packet.summary.protected_tier_active,
        shaping_intensity: packet.summary.shaping_intensity,
        shaping_intensity_inputs: packet.summary.shaping_intensity_inputs.clone(),
        context_packet_hash: packet.summary.context_packet_hash.clone(),
        context_packet_token_estimate: packet.summary.context_packet_token_estimate,
        response_create_event_ref: response_create_event_ref.to_string(),
    }
}

fn build_turn_packet_summary(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: &ModeArbitrationResult,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    initiative_line: Option<&str>,
    stable_baseline_hash: String,
) -> VolitionTurnPacketSummary {
    let selector_output = VolitionSelectorSummary {
        selected_goal_ids: ranked
            .selected
            .iter()
            .map(|selection| selection.goal.id.clone())
            .collect(),
        selected_goal_titles: ranked
            .selected
            .iter()
            .map(|selection| selection.goal.title.clone())
            .collect(),
        selected_goal_summaries: ranked
            .selected
            .iter()
            .map(|selection| selection.goal.summary.clone())
            .collect(),
        selected_count: ranked.selected.len(),
        omitted_count: ranked.omitted.len(),
        suppressed_cooldown_count: ranked.suppressed_cooldown.len(),
        visible_blocked_count: ranked.visible_blocked.len(),
    };

    let omitted_or_suppressed_candidates = build_candidate_summaries(ranked, arbitration);
    let arbitration_result = VolitionArbitrationSummary {
        mode: snapshot.state.mode,
        winner_goal_id: arbitration.winner.goal.id.clone(),
        winner_goal_title: arbitration.winner.goal.title.clone(),
        winner_goal_summary: arbitration.winner.goal.summary.clone(),
        winner_effective_tier: arbitration.winner_bias.effective_tier,
        winner_biased_tier: arbitration.winner_bias.biased_tier,
        loser_count: arbitration.losers.len(),
    };
    let mode_bias_outcomes = build_mode_bias_outcomes(arbitration);
    let protected_tier_active =
        arbitration.winner_bias.effective_tier <= qsf_volition::PROTECTED_TIER_FLOOR;
    let shaping_intensity_inputs = qsf_volition::shaping_intensity_inputs(
        arbitration,
        opportunities,
        ReceptivenessHint::Neutral,
    );
    let rationale = build_rationale(
        opportunities,
        intensity,
        selector_output.selected_count,
        omitted_or_suppressed_candidates.len(),
    );
    let injected_layers = vec![
        VolitionInjectionLayer {
            name: "stable baseline".to_string(),
            carrier: "session.update instructions".to_string(),
            injection_point: "initial session.update and each per-turn session.update".to_string(),
        },
        VolitionInjectionLayer {
            name: "dynamic volition turn packet".to_string(),
            carrier: "conversation.item.create".to_string(),
            injection_point: "after memory item and before response.create".to_string(),
        },
    ];

    let text = render_turn_packet_text_from_parts(
        &arbitration_result,
        opportunities,
        intensity,
        initiative_line,
        &shaping_intensity_inputs,
        omitted_or_suppressed_candidates.len(),
        &omitted_or_suppressed_candidates,
        &rationale,
    );
    let context_packet_hash = hash_text(&text);
    let context_packet_token_estimate = estimate_tokens(&text);

    VolitionTurnPacketSummary {
        injected_layers,
        opportunity_signals: opportunities.to_vec(),
        selector_output,
        omitted_or_suppressed_candidates,
        arbitration_result,
        mode_bias_outcomes,
        protected_tier_active,
        shaping_intensity: intensity,
        shaping_intensity_inputs,
        initiative_line: initiative_line.map(str::to_string),
        stable_baseline_hash,
        context_packet_hash,
        context_packet_token_estimate,
        rationale,
    }
}

fn build_candidate_summaries(
    ranked: &RankedSelectionResult,
    arbitration: &ModeArbitrationResult,
) -> Vec<VolitionCandidateSummary> {
    let mut summaries = Vec::new();

    for goal in &ranked.omitted {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: categorize_reason(&goal.reason),
            reason: goal.reason.clone(),
        });
    }
    for goal in &ranked.suppressed_cooldown {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: "cooldown".to_string(),
            reason: goal.reason.clone(),
        });
    }
    for goal in &ranked.visible_blocked {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: "blocked".to_string(),
            reason: goal.reason.clone(),
        });
    }
    for loser in &arbitration.losers {
        summaries.push(VolitionCandidateSummary {
            goal_id: loser.selection.goal.id.clone(),
            goal_title: loser.selection.goal.title.clone(),
            reason_category: "lower_arbitration_rank".to_string(),
            reason: loser.reason.clone(),
        });
    }

    summaries
}

pub(crate) fn build_mode_bias_outcomes(
    arbitration: &ModeArbitrationResult,
) -> Vec<VolitionModeBiasOutcome> {
    let mut outcomes = vec![VolitionModeBiasOutcome {
        goal_id: arbitration.winner.goal.id.clone(),
        goal_title: arbitration.winner.goal.title.clone(),
        effective_tier: arbitration.winner_bias.effective_tier,
        biased_tier: arbitration.winner_bias.biased_tier,
        protected: arbitration.winner_bias.protected,
    }];

    outcomes.extend(
        arbitration
            .losers
            .iter()
            .map(|loser| VolitionModeBiasOutcome {
                goal_id: loser.selection.goal.id.clone(),
                goal_title: loser.selection.goal.title.clone(),
                effective_tier: loser.bias.effective_tier,
                biased_tier: loser.bias.biased_tier,
                protected: loser.bias.protected,
            }),
    );

    outcomes
}

#[allow(clippy::too_many_arguments)]
fn render_turn_packet_text_from_parts(
    arbitration: &VolitionArbitrationSummary,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    initiative_line: Option<&str>,
    inputs: &ShapingIntensityInputs,
    suppressed_or_omitted_count: usize,
    candidates: &[VolitionCandidateSummary],
    rationale: &str,
) -> String {
    let opportunities_text = if opportunities.is_empty() {
        "none".to_string()
    } else {
        opportunities
            .iter()
            .map(|signal| format!("{} grounded in {}", signal.kind, signal.grounding_ref))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let reason_categories = if candidates.is_empty() {
        "none".to_string()
    } else {
        unique_reason_categories(candidates).join(", ")
    };
    let arbitration_status = format!(
        "winner {} at tier {}",
        arbitration.winner_goal_id, arbitration.winner_effective_tier
    );
    let protected = if arbitration.winner_effective_tier <= qsf_volition::PROTECTED_TIER_FLOOR {
        "true"
    } else {
        "false"
    };
    let initiative_section = initiative_line
        .map(|line| format!("{line}\n"))
        .unwrap_or_default();

    format!(
        "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nActive goal: {title} ({goal_id}) — {summary}\nArbitration: {arbitration_status}; mode {mode}; protected winner: {protected}.\nOpportunities: {opportunities}.\nShaping intensity: {intensity} (from {inputs}).\nOther candidates: {suppressed_or_omitted_count} not selected ({reason_categories}).\n{initiative_section}Rationale: {rationale}.\nGuidance: You may let this gently shape framing at the {intensity} level only. Do not state these goals as literal desires and do not take any external action.",
        title = arbitration.winner_goal_title,
        goal_id = arbitration.winner_goal_id,
        summary = arbitration.winner_goal_summary,
        arbitration_status = arbitration_status,
        mode = arbitration.mode,
        protected = protected,
        opportunities = opportunities_text,
        intensity = intensity,
        inputs = render_shaping_inputs(inputs),
        suppressed_or_omitted_count = suppressed_or_omitted_count,
        reason_categories = reason_categories,
        rationale = rationale,
    )
}

fn render_shaping_inputs(inputs: &ShapingIntensityInputs) -> String {
    format!(
        "winner={}; tier={}; relevance={:.1}; opportunities={}; uncertainty={}; contradiction={}; open_goal_topic={}; receptiveness={}; protected={}",
        inputs.winner_goal_id,
        inputs.winner_effective_tier,
        inputs.winner_relevance_score,
        inputs.opportunity_count,
        inputs.uncertainty_count,
        inputs.contradiction_count,
        inputs.open_goal_topic_count,
        inputs.receptiveness,
        inputs.protected_winner,
    )
}

fn build_rationale(
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    selected_count: usize,
    omitted_count: usize,
) -> String {
    let opportunity_count = opportunities.len();
    format!(
        "{} selected goal(s); {} opportunity signal(s); {} omitted candidate(s); intensity {}",
        selected_count, opportunity_count, omitted_count, intensity
    )
}

fn unique_reason_categories(candidates: &[VolitionCandidateSummary]) -> Vec<String> {
    let mut categories = Vec::<String>::new();
    for candidate in candidates {
        if !categories
            .iter()
            .any(|category| category == &candidate.reason_category)
        {
            categories.push(candidate.reason_category.clone());
        }
    }
    categories
}

fn categorize_reason(reason: &str) -> String {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("no activation keywords matched") {
        "no_match".to_string()
    } else if reason.contains("goal status is blocked") {
        "blocked".to_string()
    } else if reason.contains("goal status is cooldown") {
        "cooldown".to_string()
    } else if reason.contains("goal status is") {
        "status_filtered".to_string()
    } else {
        "omitted".to_string()
    }
}

fn render_turn_packet_text(summary: &VolitionTurnPacketSummary) -> String {
    render_turn_packet_text_from_parts(
        &summary.arbitration_result,
        &summary.opportunity_signals,
        summary.shaping_intensity,
        summary.initiative_line.as_deref(),
        &summary.shaping_intensity_inputs,
        summary.omitted_or_suppressed_candidates.len(),
        &summary.omitted_or_suppressed_candidates,
        &summary.rationale,
    )
}

fn hash_text(text: &str) -> String {
    let hash = sha2::Sha256::digest(text.as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count().saturating_add(3)) / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_volition::{
        Mode, ShapingIntensity, VolitionFixture, VolitionState, arbitrate_with_mode,
        detect_opportunities, grounded_terms_from_text, realtime_seed_fixture, select_goals_ranked,
    };

    fn fixture_state() -> (VolitionFixture, VolitionState) {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        (fixture, state)
    }

    #[test]
    fn stable_baseline_wraps_rendered_stance() {
        let fixture = realtime_seed_fixture();
        let baseline = build_stable_baseline_instructions(&fixture, Mode::Neutral);
        assert!(baseline.starts_with(
            "The following describes your simulated volition stance. It is QSF-owned internal state used"
        ));
        assert!(baseline.contains("Simulated volition stance"));
    }

    #[test]
    fn turn_packet_builder_returns_none_for_empty_selection() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
        let opportunities =
            detect_opportunities(&grounded_terms_from_text("xyzzy"), &state, &fixture);
        assert!(
            build_volition_turn_context_packet(
                &snapshot,
                &ranked,
                None,
                &opportunities,
                ShapingIntensity::None,
                "stable-baseline-hash".to_string(),
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn turn_packet_builder_renders_single_selection() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("how can you help me"),
            &state,
            &fixture,
        );
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            arbitration,
            &opportunities,
            ShapingIntensity::Low,
            "stable-baseline-hash".to_string(),
            Some("Bounded initiative: reflect on a thing. Keep it simulated and internal; do not take external action."),
        )
        .expect("packet");
        assert!(packet.text.contains("Active goal:"));
        assert!(packet.text.contains("Guidance:"));
        assert!(packet.text.contains("Bounded initiative: reflect on a thing. Keep it simulated and internal; do not take external action.\nRationale:"));
        assert!(
            !packet
                .text
                .contains("Bounded initiative: Bounded initiative:")
        );
        assert!(packet.summary.context_packet_hash.len() == 64);
        assert!(packet.summary.context_packet_token_estimate > 0);
    }
}
