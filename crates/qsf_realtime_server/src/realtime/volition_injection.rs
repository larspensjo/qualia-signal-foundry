use qsf_realtime_protocol::build_openai_realtime_conversation_item_create;
use qsf_volition::{
    DeclineReason, DeclinedCandidate, Mode, ModeArbitrationResult, OpportunitySignal,
    RankedSelectionResult, ReceptivenessHint, ShapingIntensity, ShapingIntensityInputs,
    VolitionFixture, render_volition_stance, stable_baseline_hash as volition_stable_baseline_hash,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::realtime::tools::VolitionStateSnapshot;

/// A declined candidate as it appears in an injection trace: just enough to reconstruct which
/// coherence rejection was model-visible for this turn, without duplicating the full record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DeclinedCandidateInjectionRef {
    pub candidate_id: String,
    pub conflict: DeclineReason,
}

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
    /// `None` on a coherence-only turn (no arbitration winner selected this turn) — the
    /// `declined_candidates` layer can still be injected on such a turn (see A7).
    pub arbitration_result: Option<VolitionArbitrationSummary>,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub protected_tier_active: bool,
    pub shaping_intensity: ShapingIntensity,
    pub shaping_intensity_inputs: Option<ShapingIntensityInputs>,
    pub initiative_line: Option<String>,
    pub stable_baseline_hash: String,
    pub context_packet_hash: String,
    pub context_packet_token_estimate: usize,
    pub rationale: String,
    /// Coherence rejections recorded so far this session, carried in the `coherence` injection
    /// layer from the turn after each rejection onward. Evidence-backed (names the conflicting
    /// goal + the judge's rationale), never a scripted line.
    pub declined_candidates: Vec<DeclinedCandidate>,
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
    pub arbitration_result: Option<VolitionArbitrationSummary>,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub protected_tier_active: bool,
    pub shaping_intensity: ShapingIntensity,
    pub shaping_intensity_inputs: Option<ShapingIntensityInputs>,
    pub context_packet_hash: String,
    pub context_packet_token_estimate: usize,
    pub response_create_event_ref: String,
    pub declined_candidates_injected: Vec<DeclinedCandidateInjectionRef>,
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

/// Builds the turn-context packet. Returns `None` only when there is nothing at all to inject:
/// no arbitration winner (or an empty selection) *and* no declined candidates. A coherence-only
/// turn — no goal selected this turn, but a prior rejection is still model-visible — still
/// produces a packet carrying just the `coherence` layer (see A7: the declined-candidate layer
/// must not disappear on turns with no selected goal).
#[allow(clippy::too_many_arguments)]
pub fn build_volition_turn_context_packet(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: Option<ModeArbitrationResult>,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    stable_baseline_hash: String,
    initiative_line: Option<&str>,
    declined_candidates: &[DeclinedCandidate],
) -> Option<VolitionTurnPacket> {
    let has_selection = arbitration.is_some() && !ranked.selected.is_empty();
    if !has_selection && declined_candidates.is_empty() {
        return None;
    }
    let arbitration = arbitration.filter(|_| !ranked.selected.is_empty());

    let summary = build_turn_packet_summary(
        snapshot,
        ranked,
        arbitration.as_ref(),
        opportunities,
        intensity,
        initiative_line,
        stable_baseline_hash,
        declined_candidates,
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
        declined_candidates_injected: packet
            .summary
            .declined_candidates
            .iter()
            .map(|declined| DeclinedCandidateInjectionRef {
                candidate_id: declined.candidate_id.clone(),
                conflict: declined.conflict.clone(),
            })
            .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_turn_packet_summary(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: Option<&ModeArbitrationResult>,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    initiative_line: Option<&str>,
    stable_baseline_hash: String,
    declined_candidates: &[DeclinedCandidate],
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
    let arbitration_result = arbitration.map(|arbitration| VolitionArbitrationSummary {
        mode: snapshot.state.mode,
        winner_goal_id: arbitration.winner.goal.id.clone(),
        winner_goal_title: arbitration.winner.goal.title.clone(),
        winner_goal_summary: arbitration.winner.goal.summary.clone(),
        winner_effective_tier: arbitration.winner_bias.effective_tier,
        winner_biased_tier: arbitration.winner_bias.biased_tier,
        loser_count: arbitration.losers.len(),
    });
    let mode_bias_outcomes = arbitration
        .map(build_mode_bias_outcomes)
        .unwrap_or_default();
    let protected_tier_active = arbitration
        .map(|arbitration| {
            arbitration.winner_bias.effective_tier <= qsf_volition::PROTECTED_TIER_FLOOR
        })
        .unwrap_or(false);
    let shaping_intensity_inputs = arbitration.map(|arbitration| {
        qsf_volition::shaping_intensity_inputs(
            arbitration,
            opportunities,
            ReceptivenessHint::Neutral,
        )
    });
    let rationale = build_rationale(
        opportunities,
        intensity,
        selector_output.selected_count,
        omitted_or_suppressed_candidates.len(),
    );

    // Layers are modeled as data derived from what actually got rendered, so `injected_layers`
    // can never claim a section the text lacks (A6): "dynamic volition turn packet" is declared
    // only when there is an arbitration winner to render, and "coherence" only when there is a
    // non-empty declined-candidates section — honestly described as rendered inline within the
    // same item (or standalone on a coherence-only turn with no goal selected, see A7), never a
    // separate `conversation.item.create` the caller doesn't actually send.
    let has_core_section = arbitration_result.is_some();
    let has_coherence_section = !declined_candidates.is_empty();
    let mut injected_layers = vec![VolitionInjectionLayer {
        name: "stable baseline".to_string(),
        carrier: "session.update instructions".to_string(),
        injection_point: "initial session.update and each per-turn session.update".to_string(),
    }];
    if has_core_section {
        injected_layers.push(VolitionInjectionLayer {
            name: "dynamic volition turn packet".to_string(),
            carrier: "conversation.item.create".to_string(),
            injection_point: "after memory item and before response.create".to_string(),
        });
    }
    if has_coherence_section {
        injected_layers.push(VolitionInjectionLayer {
            name: "coherence".to_string(),
            carrier: "conversation.item.create".to_string(),
            injection_point: if has_core_section {
                "inline within the dynamic volition turn packet item, before response.create"
                    .to_string()
            } else {
                "standalone conversation.item.create (no goal selected this turn), before \
                 response.create"
                    .to_string()
            },
        });
    }

    let text = render_turn_packet_text_from_parts(
        arbitration_result.as_ref(),
        opportunities,
        intensity,
        initiative_line,
        shaping_intensity_inputs.as_ref(),
        omitted_or_suppressed_candidates.len(),
        &omitted_or_suppressed_candidates,
        &rationale,
        declined_candidates,
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
        declined_candidates: declined_candidates.to_vec(),
    }
}

fn build_candidate_summaries(
    ranked: &RankedSelectionResult,
    arbitration: Option<&ModeArbitrationResult>,
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
    if let Some(arbitration) = arbitration {
        for loser in &arbitration.losers {
            summaries.push(VolitionCandidateSummary {
                goal_id: loser.selection.goal.id.clone(),
                goal_title: loser.selection.goal.title.clone(),
                reason_category: "lower_arbitration_rank".to_string(),
                reason: loser.reason.clone(),
            });
        }
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
    arbitration: Option<&VolitionArbitrationSummary>,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    initiative_line: Option<&str>,
    inputs: Option<&ShapingIntensityInputs>,
    suppressed_or_omitted_count: usize,
    candidates: &[VolitionCandidateSummary],
    rationale: &str,
    declined_candidates: &[DeclinedCandidate],
) -> String {
    let coherence_section = render_declined_candidates_section(declined_candidates);
    let Some(arbitration) = arbitration else {
        // Coherence-only turn (A7): no goal was selected, so there is nothing to say about
        // arbitration, opportunities, or shaping — only the declined-candidate context, which
        // the caller guarantees is non-empty whenever arbitration is None. It still gets the
        // same simulation framing and no-external-action guardrail as the full turn packet.
        return format!(
            "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\n{coherence_section}Guidance: You may let this gently shape framing at the internal-context level only. Do not state these goals as literal desires and do not take any external action."
        );
    };
    let inputs =
        inputs.expect("shaping_intensity_inputs must be Some whenever arbitration is Some");

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
        "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nActive goal: {title} ({goal_id}) — {summary}\nArbitration: {arbitration_status}; mode {mode}; protected winner: {protected}.\nOpportunities: {opportunities}.\nShaping intensity: {intensity} (from {inputs}).\nOther candidates: {suppressed_or_omitted_count} not selected ({reason_categories}).\n{initiative_section}Rationale: {rationale}.\n{coherence_section}Guidance: You may let this gently shape framing at the {intensity} level only. Do not state these goals as literal desires and do not take any external action.",
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
        coherence_section = coherence_section,
    )
}

/// Renders the `coherence` layer: goal candidates declined this session because they'd
/// contradict a more-fundamental goal, each grounded in the conflicting goal id and the judge's
/// rationale (guardrail D4 - evidence-backed, never confabulated). Empty when nothing has been
/// declined yet.
fn render_declined_candidates_section(declined_candidates: &[DeclinedCandidate]) -> String {
    if declined_candidates.is_empty() {
        return String::new();
    }

    let lines = declined_candidates
        .iter()
        .map(|declined| {
            // `ProtectedFloor` names no goal — rendering it as "conflicts with protected_floor"
            // would fabricate a goal id no trace consumer could resolve (A8).
            let conflict_description = match &declined.conflict {
                DeclineReason::ConflictingGoal { goal_id } => format!("conflicts with {goal_id}"),
                DeclineReason::ProtectedFloor => "is below the protected floor tier".to_string(),
            };
            format!(
                "- \"{}\" declined: {} ({})",
                declined.title, conflict_description, declined.rationale
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Declined goal candidates this session (you may voice these if relevant, at your discretion):\n{lines}\n"
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
        summary.arbitration_result.as_ref(),
        &summary.opportunity_signals,
        summary.shaping_intensity,
        summary.initiative_line.as_deref(),
        summary.shaping_intensity_inputs.as_ref(),
        summary.omitted_or_suppressed_candidates.len(),
        &summary.omitted_or_suppressed_candidates,
        &summary.rationale,
        &summary.declined_candidates,
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
                &[],
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
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .and_then(|outcome| outcome.qualified);
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
            &[],
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

    #[test]
    fn turn_packet_omits_the_coherence_layer_when_nothing_declined() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .and_then(|outcome| outcome.qualified);
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
            None,
            &[],
        )
        .expect("packet");

        assert!(
            !packet
                .summary
                .injected_layers
                .iter()
                .any(|layer| layer.name == "coherence"),
            "the coherence layer must not be declared when the text carries no declined-candidate section"
        );
        assert!(!packet.text.contains("Declined goal candidates"));
    }

    #[test]
    fn turn_packet_renders_declined_candidates_grounded_in_the_conflicting_goal() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .and_then(|outcome| outcome.qualified);
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("how can you help me"),
            &state,
            &fixture,
        );
        let declined = vec![DeclinedCandidate {
            candidate_id: "candidate-1".to_string(),
            title: "pursue an unrelated tangent".to_string(),
            conflict: DeclineReason::ConflictingGoal {
                goal_id: "keep-theses-distinct-from-fact".to_string(),
            },
            rationale: "would derail the current task".to_string(),
            tick: 3,
        }];
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            arbitration,
            &opportunities,
            ShapingIntensity::Low,
            "stable-baseline-hash".to_string(),
            None,
            &declined,
        )
        .expect("packet");

        assert!(packet.text.contains("Declined goal candidates"));
        assert!(packet.text.contains("pursue an unrelated tangent"));
        assert!(packet.text.contains("keep-theses-distinct-from-fact"));
        assert!(packet.text.contains("would derail the current task"));
        assert_eq!(packet.summary.declined_candidates, declined);
        assert!(
            packet
                .summary
                .injected_layers
                .iter()
                .any(|layer| layer.name == "coherence"),
            "the coherence layer must be declared when a declined candidate is present"
        );
    }

    #[test]
    fn turn_packet_renders_protected_floor_decline_without_fabricating_a_goal_id() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral)
            .and_then(|outcome| outcome.qualified);
        let opportunities = detect_opportunities(
            &grounded_terms_from_text("how can you help me"),
            &state,
            &fixture,
        );
        let declined = vec![DeclinedCandidate {
            candidate_id: "candidate-2".to_string(),
            title: "override a protected priority".to_string(),
            conflict: DeclineReason::ProtectedFloor,
            rationale: "the candidate's own effective tier is at or below the protected floor"
                .to_string(),
            tick: 4,
        }];
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            arbitration,
            &opportunities,
            ShapingIntensity::Low,
            "stable-baseline-hash".to_string(),
            None,
            &declined,
        )
        .expect("packet");

        assert!(!packet.text.contains("conflicts with protected_floor"));
        assert!(packet.text.contains("is below the protected floor tier"));
    }

    #[test]
    fn coherence_only_turn_injects_declined_candidates_with_no_goal_selected() {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        // No arbitration winner and an empty selection - previously this made
        // build_volition_turn_context_packet return None unconditionally, dropping the
        // declined-candidate layer from this turn's context entirely (A7).
        let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
        let opportunities =
            detect_opportunities(&grounded_terms_from_text("xyzzy"), &state, &fixture);
        let declined = vec![DeclinedCandidate {
            candidate_id: "candidate-3".to_string(),
            title: "pursue an unrelated tangent".to_string(),
            conflict: DeclineReason::ConflictingGoal {
                goal_id: "keep-theses-distinct-from-fact".to_string(),
            },
            rationale: "would derail the current task".to_string(),
            tick: 5,
        }];

        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            None,
            &opportunities,
            ShapingIntensity::None,
            "stable-baseline-hash".to_string(),
            None,
            &declined,
        )
        .expect("a coherence-only packet must still be produced");

        assert!(packet.text.contains("Declined goal candidates"));
        assert!(packet.text.contains("pursue an unrelated tangent"));
        assert!(packet.text.contains("Simulated volition context"));
        assert!(
            packet
                .text
                .contains("Do not state these goals as literal desires")
        );
        assert!(packet.text.contains("do not take any external action"));
        assert!(!packet.text.contains("Active goal:"));
        assert!(packet.summary.arbitration_result.is_none());
        assert!(
            !packet
                .summary
                .injected_layers
                .iter()
                .any(|layer| layer.name == "dynamic volition turn packet"),
            "the core turn-packet layer must not be declared when there is no arbitration winner"
        );
        assert!(
            packet
                .summary
                .injected_layers
                .iter()
                .any(|layer| layer.name == "coherence")
        );
    }
}
