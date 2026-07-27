use qsf_volition::{
    DeclineReason, DeclinedCandidate, GoalVisibility, OpportunitySignal, ShapingIntensity,
    ShapingIntensityInputs,
};
use sha2::Digest;

pub use qsf_diagnostics::AmbientExposure;

use crate::realtime::volition_injection::{
    VolitionArbitrationSummary, VolitionCandidateSummary, VolitionTurnPacketSummary,
};

/// Classify how the arbitration winner is exposed in the model-visible text. A subconscious winner
/// that renders an initiative line or is named in a coherence conflict this turn is forced to
/// surface (full labeled detail); an ordinary subconscious winner is reduced; a conscious winner
/// (or no winner) is ordinary.
pub(crate) fn compute_ambient_exposure(
    winner_visibility: Option<GoalVisibility>,
    winner_id: Option<&str>,
    initiative_line_present: bool,
    declined_candidates: &[DeclinedCandidate],
) -> AmbientExposure {
    if winner_visibility != Some(GoalVisibility::Subconscious) {
        return AmbientExposure::Ordinary;
    }
    // A subconscious winner is forced to surface only when a concrete condition holds this turn: it
    // rendered an initiative line, or it is named as the conflicting goal in a decline. (Unlike
    // `winner_forcing_note`, which carries a display fallback, this must test the real conditions.)
    let forced = initiative_line_present
        || winner_id.is_some_and(|winner_id| {
            declined_candidates.iter().any(|declined| {
                matches!(&declined.conflict, DeclineReason::ConflictingGoal { goal_id } if goal_id == winner_id)
            })
        });
    if forced {
        AmbientExposure::ForcedSurfacedSubconscious
    } else {
        AmbientExposure::ReducedSubconscious
    }
}

/// The forcing evidence label for a forced-surfaced subconscious winner, or `None` when the winner
/// is not forced. `rendered initiative line` takes precedence over `named in a coherence conflict`.
fn winner_forcing_note(
    exposure: AmbientExposure,
    initiative_line_present: bool,
    winner_id: Option<&str>,
    declined_candidates: &[DeclinedCandidate],
) -> Option<String> {
    if exposure != AmbientExposure::ForcedSurfacedSubconscious {
        return None;
    }
    if initiative_line_present {
        return Some("rendered initiative line".to_string());
    }
    if let Some(winner_id) = winner_id {
        if declined_candidates.iter().any(|declined| {
            matches!(&declined.conflict, DeclineReason::ConflictingGoal { goal_id } if goal_id == winner_id)
        }) {
            return Some("named in a coherence conflict".to_string());
        }
    }
    Some("forced surfacing".to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_turn_packet_text_from_parts(
    arbitration: Option<&VolitionArbitrationSummary>,
    below_threshold_count: usize,
    qualification_threshold: u32,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    initiative_line: Option<&str>,
    inputs: Option<&ShapingIntensityInputs>,
    suppressed_or_omitted_count: usize,
    candidates: &[VolitionCandidateSummary],
    rationale: &str,
    declined_candidates: &[DeclinedCandidate],
    ambient_exposure: AmbientExposure,
) -> String {
    let coherence_section = render_declined_candidates_section(declined_candidates);
    let Some(arbitration) = arbitration else {
        if below_threshold_count > 0 {
            // No-qualifier turn: goals activated but none reached the qualification threshold,
            // so volition stays quiet. The suppression is stated (and fully traced) rather than
            // promoting a weak winner. Same first-person framing and no-external-action guardrail.
            return format!(
                "Your volition context for this turn (inner state; it shapes attention and framing only).\nNo goal qualified to lead this turn: {below_threshold_count} candidate(s) matched only below the qualification threshold ({qualification_threshold}). Volition stays quiet this turn.\n{coherence_section}Guidance: Respond naturally to the person and do not take any external action."
            );
        }
        // Coherence-only turn (A7): no goal was selected, so there is nothing to say about
        // arbitration, opportunities, or shaping — only the declined-candidate context, which
        // the caller guarantees is non-empty whenever arbitration is None. It still gets the
        // same first-person framing and no-external-action guardrail as the full turn packet.
        return format!(
            "Your volition context for this turn (inner state; it shapes attention and framing only).\n{coherence_section}Guidance: You may let this gently shape framing at the internal-context level only. These goals are your own; let them shape your framing rather than reciting them, and do not take any external action."
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
    let protected = if arbitration.winner_effective_tier <= qsf_volition::PROTECTED_TIER_FLOOR {
        "true"
    } else {
        "false"
    };
    let initiative_section = initiative_line
        .map(|line| format!("{line}\n"))
        .unwrap_or_default();

    // The goal-identifying headline (Active goal + Arbitration lines) depends on the winner's
    // ambient exposure. A `reduced_subconscious` winner withholds its title, summary, and id from
    // the model-visible text — only the trace keeps the full identity — while still carrying the
    // minimum shaping contract (background disposition + intensity + safe guidance). A
    // `forced_surfaced_subconscious` winner shows full detail, labeled and backed by its forcing
    // evidence. A conscious winner is the ordinary `Active goal` line.
    let headline = match ambient_exposure {
        AmbientExposure::ReducedSubconscious => format!(
            "Background disposition active (subconscious): a background goal is shaping framing this turn at {intensity} intensity. Its identity is withheld here; full winner detail is in the volition trace.\nArbitration: a subconscious winner leads at tier {tier}; mode {mode}; protected winner: {protected}.",
            intensity = intensity,
            tier = arbitration.winner_effective_tier,
            mode = arbitration.mode,
            protected = protected,
        ),
        AmbientExposure::ForcedSurfacedSubconscious => {
            let note = winner_forcing_note(
                ambient_exposure,
                initiative_line.is_some(),
                Some(arbitration.winner_goal_id.as_str()),
                declined_candidates,
            )
            .unwrap_or_else(|| "forced surfacing".to_string());
            format!(
                "Active goal (surfaced background/subconscious goal — {note}): {title} ({goal_id}) — {summary}\nArbitration: winner {goal_id} at tier {tier}; mode {mode}; protected winner: {protected}.",
                note = note,
                title = arbitration.winner_goal_title,
                goal_id = arbitration.winner_goal_id,
                summary = arbitration.winner_goal_summary,
                tier = arbitration.winner_effective_tier,
                mode = arbitration.mode,
                protected = protected,
            )
        }
        AmbientExposure::Ordinary => format!(
            "Active goal: {title} ({goal_id}) — {summary}\nArbitration: winner {goal_id} at tier {tier}; mode {mode}; protected winner: {protected}.",
            title = arbitration.winner_goal_title,
            goal_id = arbitration.winner_goal_id,
            summary = arbitration.winner_goal_summary,
            tier = arbitration.winner_effective_tier,
            mode = arbitration.mode,
            protected = protected,
        ),
    };

    // A reduced-subconscious winner also redacts the shaping-intensity inputs line, which would
    // otherwise name the winner goal id — keeping the winner's identity out of model-visible text.
    let shaping_line = match ambient_exposure {
        AmbientExposure::ReducedSubconscious => format!("Shaping intensity: {intensity}."),
        _ => format!(
            "Shaping intensity: {intensity} (from {inputs}).",
            inputs = render_shaping_inputs(inputs)
        ),
    };

    format!(
        "Your volition context for this turn (inner state; it shapes attention and framing only).\n{headline}\nOpportunities: {opportunities}.\n{shaping_line}\nOther candidates: {suppressed_or_omitted_count} not selected ({reason_categories}).\n{initiative_section}Rationale: {rationale}.\n{coherence_section}Guidance: You may let this gently shape framing at the {intensity} level only. These goals are your own; let them shape your framing rather than reciting them, and do not take any external action.",
        headline = headline,
        opportunities = opportunities_text,
        shaping_line = shaping_line,
        intensity = intensity,
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

pub(crate) fn build_rationale(
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

pub(crate) fn categorize_reason(reason: &str) -> String {
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

pub(crate) fn render_turn_packet_text(summary: &VolitionTurnPacketSummary) -> String {
    render_turn_packet_text_from_parts(
        summary.arbitration_result.as_ref(),
        summary.below_threshold_candidates.len(),
        summary.qualification_threshold,
        &summary.opportunity_signals,
        summary.shaping_intensity,
        summary.initiative_line.as_deref(),
        summary.shaping_intensity_inputs.as_ref(),
        summary.omitted_or_suppressed_candidates.len(),
        &summary.omitted_or_suppressed_candidates,
        &summary.rationale,
        &summary.declined_candidates,
        summary.ambient_exposure,
    )
}

pub(crate) fn hash_text(text: &str) -> String {
    let hash = sha2::Sha256::digest(text.as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    (text.chars().count().saturating_add(3)) / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::tools::VolitionStateSnapshot;
    use crate::realtime::volition_injection::{
        VolitionTurnPacket, build_volition_turn_context_packet,
    };
    use qsf_volition::{
        GoalVisibility, Mode, ShapingIntensity, VolitionFixture, VolitionState,
        arbitrate_with_mode, detect_opportunities, grounded_terms_from_text, realtime_seed_fixture,
        select_goals_ranked,
    };

    fn fixture_state() -> (VolitionFixture, VolitionState) {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        (fixture, state)
    }

    // ── ambient exposure for subconscious winners (Phase 4, step 6) ──────────

    const SUBCONSCIOUS_WINNER: &str = "assemble-world-picture";
    const SUBCONSCIOUS_TITLE: &str = "Assemble a world picture";
    // A query that activates only the subconscious world-picture goal, so it is the sole winner.
    const SUBCONSCIOUS_QUERY: &str = "world history society politics";

    fn packet_for(
        query: &str,
        initiative_line: Option<&str>,
        declined: &[DeclinedCandidate],
    ) -> VolitionTurnPacket {
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };
        let ranked = select_goals_ranked(query, &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
        // Empty opportunities keep the reduced-exposure text free of goal-id-grounded signals.
        build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            outcome,
            &[],
            ShapingIntensity::Low,
            "stable-baseline-hash".to_string(),
            initiative_line,
            declined,
        )
        .expect("query produces a packet")
    }

    #[test]
    fn conscious_winner_renders_ordinary_active_goal_line() {
        let packet = packet_for("how can you help me", None, &[]);
        assert_eq!(packet.summary.ambient_exposure, AmbientExposure::Ordinary);
        assert_eq!(
            packet.summary.winner_visibility,
            Some(GoalVisibility::Conscious)
        );
        assert!(packet.text.contains("Active goal:"));
        assert!(!packet.text.contains("Background disposition active"));
    }

    #[test]
    fn ordinary_subconscious_winner_renders_reduced_text_but_full_trace() {
        let packet = packet_for(SUBCONSCIOUS_QUERY, None, &[]);
        assert_eq!(
            packet.summary.winner_visibility,
            Some(GoalVisibility::Subconscious)
        );
        assert_eq!(
            packet.summary.ambient_exposure,
            AmbientExposure::ReducedSubconscious
        );

        // Model-visible text withholds the winner's identity.
        assert!(
            packet
                .text
                .contains("Background disposition active (subconscious)")
        );
        assert!(!packet.text.contains("Active goal:"));
        assert!(!packet.text.contains(SUBCONSCIOUS_WINNER));
        assert!(!packet.text.contains(SUBCONSCIOUS_TITLE));

        // The trace keeps the full winner identity and summary.
        let arb = packet
            .summary
            .arbitration_result
            .as_ref()
            .expect("winner recorded in trace");
        assert_eq!(arb.winner_goal_id, SUBCONSCIOUS_WINNER);
        assert_eq!(arb.winner_goal_title, SUBCONSCIOUS_TITLE);
        assert!(!arb.winner_goal_summary.is_empty());
        assert_eq!(packet.summary.subconscious_selected_count, 1);
    }

    #[test]
    fn forced_surfaced_subconscious_winner_by_rendered_initiative_shows_labeled_full_detail() {
        let initiative = "Bounded initiative: surface open thread the larger picture. Keep it internal to this conversation; do not take external action.";
        let packet = packet_for(SUBCONSCIOUS_QUERY, Some(initiative), &[]);
        assert_eq!(
            packet.summary.ambient_exposure,
            AmbientExposure::ForcedSurfacedSubconscious
        );
        assert!(
            packet
                .text
                .contains("surfaced background/subconscious goal — rendered initiative line")
        );
        // Full detail is shown when forced.
        assert!(packet.text.contains(SUBCONSCIOUS_WINNER));
        assert!(packet.text.contains(SUBCONSCIOUS_TITLE));
    }

    #[test]
    fn forced_surfaced_subconscious_winner_by_coherence_conflict_shows_labeled_full_detail() {
        let declined = vec![DeclinedCandidate {
            candidate_id: "cand-x".to_string(),
            title: "a distracting tangent".to_string(),
            conflict: DeclineReason::ConflictingGoal {
                goal_id: SUBCONSCIOUS_WINNER.to_string(),
            },
            rationale: "would derail the background world picture".to_string(),
            tick: 3,
        }];
        let packet = packet_for(SUBCONSCIOUS_QUERY, None, &declined);
        assert_eq!(
            packet.summary.ambient_exposure,
            AmbientExposure::ForcedSurfacedSubconscious
        );
        assert!(
            packet
                .text
                .contains("surfaced background/subconscious goal — named in a coherence conflict")
        );
        assert!(packet.text.contains(SUBCONSCIOUS_TITLE));
    }

    #[test]
    fn packet_text_starts_with_ui_locator_prefix() {
        // The realtime browser UI locates the injected volition item by this exact prefix — see
        // `VOLITION_INJECTED_TEXT_PREFIX` and `selectInjectedVolitionText` in
        // crates/qsf_realtime_server/ui/src/realtime.ts. If you reword the rendered packet, update
        // that constant and its tests too; this assertion exists so the reword fails CI here first.
        const UI_LOCATOR_PREFIX: &str = "Your volition context for this turn";
        let (fixture, state) = fixture_state();
        let snapshot = VolitionStateSnapshot {
            state: state.clone(),
            fixture: fixture.clone(),
        };

        // Qualified-winner path.
        let ranked = select_goals_ranked("how can you help me", &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            outcome,
            &[],
            ShapingIntensity::Low,
            "stable-baseline-hash".to_string(),
            None,
            &[],
        )
        .expect("qualified winner emits a packet");
        assert!(
            packet.text.starts_with(UI_LOCATOR_PREFIX),
            "qualified-path packet prefix drifted from the UI locator: {}",
            packet.text
        );

        // No-qualifier path (goals activate but none clear the bar).
        let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
        let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
        let packet = build_volition_turn_context_packet(
            &snapshot,
            &ranked,
            outcome,
            &[],
            ShapingIntensity::None,
            "stable-baseline-hash".to_string(),
            None,
            &[],
        )
        .expect("no-qualifier turn emits a packet");
        assert!(
            packet.text.starts_with(UI_LOCATOR_PREFIX),
            "no-qualifier-path packet prefix drifted from the UI locator: {}",
            packet.text
        );

        // Coherence-only path (no ranked selection or arbitration winner; declined candidates only).
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
        .expect("coherence-only turn emits a packet");
        assert!(
            packet.text.starts_with(UI_LOCATOR_PREFIX),
            "coherence-only-path packet prefix drifted from the UI locator: {}",
            packet.text
        );
    }
}
