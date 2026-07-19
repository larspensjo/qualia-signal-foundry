use qsf_volition::{
    BelowThresholdCandidate, DeclinedCandidate, GoalVisibility, ModeArbitrationResult,
    OpportunitySignal, RankedSelectionResult, ReceptivenessHint, ShapingIntensity, goal_visibility,
};

use crate::realtime::tools::VolitionStateSnapshot;
use crate::realtime::volition_injection::{
    VolitionArbitrationSummary, VolitionCandidateSummary, VolitionInjectionLayer,
    VolitionModeBiasOutcome, VolitionSelectedMatchDetail, VolitionSelectorSummary,
    VolitionTurnPacketSummary,
};
use crate::realtime::volition_injection_text::{
    build_rationale, categorize_reason, compute_ambient_exposure, estimate_tokens, hash_text,
    render_turn_packet_text_from_parts,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_turn_packet_summary(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: Option<&ModeArbitrationResult>,
    below_threshold: &[BelowThresholdCandidate],
    qualification_threshold: u32,
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
        selected_match_details: ranked
            .selected
            .iter()
            .map(|selection| VolitionSelectedMatchDetail {
                goal_id: selection.goal.id.clone(),
                matched_keywords: selection.matched_keywords.clone(),
                match_strength: selection.match_strength,
                visibility: goal_visibility(&selection.goal.id, &snapshot.state, &snapshot.fixture),
            })
            .collect(),
    };

    let subconscious_selected_count = ranked
        .selected
        .iter()
        .filter(|selection| {
            goal_visibility(&selection.goal.id, &snapshot.state, &snapshot.fixture)
                == GoalVisibility::Subconscious
        })
        .count();

    let below_threshold_candidates = build_below_threshold_summaries(below_threshold);
    let omitted_or_suppressed_candidates =
        build_candidate_summaries(ranked, arbitration, below_threshold);
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
    // when there is an Active-goal section OR a no-qualifier section to render, and "coherence"
    // only when there is a non-empty declined-candidates section — honestly described as rendered
    // inline within the same item (or standalone on a coherence-only turn with no goal selected,
    // see A7), never a separate `conversation.item.create` the caller doesn't actually send.
    let has_no_qualifier_section =
        arbitration_result.is_none() && !below_threshold_candidates.is_empty();
    let has_core_section = arbitration_result.is_some() || has_no_qualifier_section;
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

    let winner_visibility = arbitration.map(|arbitration| {
        goal_visibility(
            &arbitration.winner.goal.id,
            &snapshot.state,
            &snapshot.fixture,
        )
    });
    let ambient_exposure = compute_ambient_exposure(
        winner_visibility,
        arbitration_result
            .as_ref()
            .map(|r| r.winner_goal_id.as_str()),
        initiative_line.is_some(),
        declined_candidates,
    );

    let text = render_turn_packet_text_from_parts(
        arbitration_result.as_ref(),
        below_threshold_candidates.len(),
        qualification_threshold,
        opportunities,
        intensity,
        initiative_line,
        shaping_intensity_inputs.as_ref(),
        omitted_or_suppressed_candidates.len(),
        &omitted_or_suppressed_candidates,
        &rationale,
        declined_candidates,
        ambient_exposure,
    );
    let context_packet_hash = hash_text(&text);
    let context_packet_token_estimate = estimate_tokens(&text);

    VolitionTurnPacketSummary {
        injected_layers,
        opportunity_signals: opportunities.to_vec(),
        selector_output,
        omitted_or_suppressed_candidates,
        qualification_threshold,
        below_threshold_candidates,
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
        winner_visibility,
        ambient_exposure,
        subconscious_selected_count,
    }
}

/// Below-threshold candidates as candidate summaries, categorized
/// `below_qualification_threshold` (never `lower_arbitration_rank` — they never reached the
/// sort). Carries matched keywords + strength so the trace can recompute the outcome.
fn build_below_threshold_summaries(
    below_threshold: &[BelowThresholdCandidate],
) -> Vec<VolitionCandidateSummary> {
    below_threshold
        .iter()
        .map(|candidate| VolitionCandidateSummary {
            goal_id: candidate.selection.goal.id.clone(),
            goal_title: candidate.selection.goal.title.clone(),
            reason_category: "below_qualification_threshold".to_string(),
            reason: candidate.reason.clone(),
            matched_keywords: candidate.selection.matched_keywords.clone(),
            match_strength: candidate.match_strength,
        })
        .collect()
}

fn build_candidate_summaries(
    ranked: &RankedSelectionResult,
    arbitration: Option<&ModeArbitrationResult>,
    below_threshold: &[BelowThresholdCandidate],
) -> Vec<VolitionCandidateSummary> {
    let mut summaries = Vec::new();

    for goal in &ranked.omitted {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: categorize_reason(&goal.reason),
            reason: goal.reason.clone(),
            matched_keywords: Vec::new(),
            match_strength: 0,
        });
    }
    for goal in &ranked.suppressed_cooldown {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: "cooldown".to_string(),
            reason: goal.reason.clone(),
            matched_keywords: Vec::new(),
            match_strength: 0,
        });
    }
    for goal in &ranked.visible_blocked {
        summaries.push(VolitionCandidateSummary {
            goal_id: goal.goal.id.clone(),
            goal_title: goal.goal.title.clone(),
            reason_category: "blocked".to_string(),
            reason: goal.reason.clone(),
            matched_keywords: Vec::new(),
            match_strength: 0,
        });
    }
    // Below-threshold candidates are folded in here too, categorized as such — never
    // `lower_arbitration_rank`, which is reserved for goals that reached the arbitration sort.
    summaries.extend(build_below_threshold_summaries(below_threshold));
    if let Some(arbitration) = arbitration {
        for loser in &arbitration.losers {
            summaries.push(VolitionCandidateSummary {
                goal_id: loser.selection.goal.id.clone(),
                goal_title: loser.selection.goal.title.clone(),
                reason_category: "lower_arbitration_rank".to_string(),
                reason: loser.reason.clone(),
                matched_keywords: loser.selection.matched_keywords.clone(),
                match_strength: loser.selection.match_strength,
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
