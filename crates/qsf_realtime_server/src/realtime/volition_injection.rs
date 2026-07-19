use qsf_realtime_protocol::build_openai_realtime_conversation_item_create;
use qsf_volition::{
    ActivationKeyword, DeclineReason, DeclinedCandidate, GoalVisibility, Mode,
    ModeArbitrationOutcome, OpportunitySignal, RankedSelectionResult, ShapingIntensity,
    ShapingIntensityInputs, VolitionFixture, render_volition_stance,
    stable_baseline_hash as volition_stable_baseline_hash,
};
use serde::{Deserialize, Serialize};

use crate::realtime::tools::VolitionStateSnapshot;
pub(crate) use crate::realtime::volition_injection_summary::build_mode_bias_outcomes;
use crate::realtime::volition_injection_summary::build_turn_packet_summary;
use crate::realtime::volition_injection_text::render_turn_packet_text;
pub(crate) use crate::realtime::volition_injection_text::{
    AmbientExposure, compute_ambient_exposure,
};

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
    /// Matched keywords with weight classes (empty for status-filtered candidates that never
    /// matched a keyword). Carried so the trace can recompute `match_strength`.
    #[serde(default)]
    pub matched_keywords: Vec<ActivationKeyword>,
    /// Summed weight of `matched_keywords` (0 when nothing matched).
    #[serde(default)]
    pub match_strength: u32,
}

/// Per-selected-goal matched keywords with weight classes and strength, for the trace's
/// `selector_output` (the arbitration-losing / below-threshold detail lives on the candidate
/// summaries).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionSelectedMatchDetail {
    pub goal_id: String,
    pub matched_keywords: Vec<ActivationKeyword>,
    pub match_strength: u32,
    /// Per-goal narration visibility, so an operator can reconstruct which selected goals were
    /// subconscious dispositions and which were conscious. `#[serde(default)]` = `Conscious` for
    /// traces serialized before this field.
    #[serde(default)]
    pub visibility: GoalVisibility,
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
    #[serde(default)]
    pub selected_match_details: Vec<VolitionSelectedMatchDetail>,
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
    /// The qualification threshold in force this turn (fixture-level). Present whether or not a
    /// goal qualified so the no-winner outcome stays auditable.
    #[serde(default)]
    pub qualification_threshold: u32,
    /// Selections that activated but fell below the qualification threshold — categorized
    /// `below_qualification_threshold`, never `lower_arbitration_rank`. Also folded into
    /// `omitted_or_suppressed_candidates`.
    #[serde(default)]
    pub below_threshold_candidates: Vec<VolitionCandidateSummary>,
    /// `None` on a coherence-only or no-qualifier turn (no arbitration winner selected this
    /// turn) — the `declined_candidates` layer can still be injected on such a turn (see A7).
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
    /// The arbitration winner's narration visibility (`None` on a no-winner turn). The trace keeps
    /// the full winner identity in `arbitration_result` whatever the exposure treatment.
    #[serde(default)]
    pub winner_visibility: Option<GoalVisibility>,
    /// How the winner was exposed in the model-visible text this turn. `ordinary` for a conscious
    /// winner or no winner; `reduced_subconscious` / `forced_surfaced_subconscious` for a
    /// subconscious winner. `#[serde(default)]` = `Ordinary` for back-compat.
    #[serde(default = "default_ambient_exposure")]
    pub ambient_exposure: AmbientExposure,
    /// Number of selected goals that are subconscious dispositions this turn. Lets an operator
    /// reconstruct how much subconscious biasing shaped the turn without diffing visibilities.
    #[serde(default)]
    pub subconscious_selected_count: usize,
}

fn default_ambient_exposure() -> AmbientExposure {
    AmbientExposure::Ordinary
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
    #[serde(default)]
    pub qualification_threshold: u32,
    #[serde(default)]
    pub below_threshold_candidates: Vec<VolitionCandidateSummary>,
    pub arbitration_result: Option<VolitionArbitrationSummary>,
    pub mode_bias_outcomes: Vec<VolitionModeBiasOutcome>,
    pub protected_tier_active: bool,
    pub shaping_intensity: ShapingIntensity,
    pub shaping_intensity_inputs: Option<ShapingIntensityInputs>,
    pub context_packet_hash: String,
    pub context_packet_token_estimate: usize,
    pub response_create_event_ref: String,
    pub declined_candidates_injected: Vec<DeclinedCandidateInjectionRef>,
    #[serde(default)]
    pub winner_visibility: Option<GoalVisibility>,
    #[serde(default = "default_ambient_exposure")]
    pub ambient_exposure: AmbientExposure,
    #[serde(default)]
    pub subconscious_selected_count: usize,
}

pub fn build_stable_baseline_instructions(fixture: &VolitionFixture, mode: Mode) -> String {
    format!(
        "The following describes your own volition stance — part of your inner life. It weights your\nattention and framing in this conversation. It never authorizes any action outside this\nconversation or the QSF trust boundary. Do not read it aloud or enumerate it unless the user\nasks about your goals or internal state.\n{}",
        render_volition_stance(fixture, mode)
    )
}

pub fn stable_baseline_hash(instructions: &str) -> String {
    volition_stable_baseline_hash(instructions)
}

/// Builds the turn-context packet. Emits whenever the turn has something to trace: a qualified
/// winner (with a non-empty selection), any below-threshold candidate (a no-qualifier turn —
/// volition stays quiet but the suppression is recorded), or any declined candidate (a
/// coherence-only turn). Returns `None` only when none of those hold — a trusted turn with no
/// lexical activation at all injects nothing and is outside the trace contract.
#[allow(clippy::too_many_arguments)]
pub fn build_volition_turn_context_packet(
    snapshot: &VolitionStateSnapshot,
    ranked: &RankedSelectionResult,
    arbitration: Option<ModeArbitrationOutcome>,
    opportunities: &[OpportunitySignal],
    intensity: ShapingIntensity,
    stable_baseline_hash: String,
    initiative_line: Option<&str>,
    declined_candidates: &[DeclinedCandidate],
) -> Option<VolitionTurnPacket> {
    // A qualified winner (with a non-empty selection), any below-threshold candidate, or any
    // declined candidate each warrants a packet so the turn's outcome — including a no-qualifier
    // suppression — stays traced. A trusted turn with no lexical activation at all emits nothing.
    let qualified = arbitration
        .as_ref()
        .and_then(|outcome| outcome.qualified.clone())
        .filter(|_| !ranked.selected.is_empty());
    let below_threshold = arbitration
        .as_ref()
        .map(|outcome| outcome.below_threshold.clone())
        .unwrap_or_default();
    let qualification_threshold = arbitration
        .as_ref()
        .map(|outcome| outcome.qualification_threshold)
        .unwrap_or(snapshot.fixture.arbitration_qualification_threshold);
    if qualified.is_none() && below_threshold.is_empty() && declined_candidates.is_empty() {
        return None;
    }

    let summary = build_turn_packet_summary(
        snapshot,
        ranked,
        qualified.as_ref(),
        &below_threshold,
        qualification_threshold,
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
        qualification_threshold: packet.summary.qualification_threshold,
        below_threshold_candidates: packet.summary.below_threshold_candidates.clone(),
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
        winner_visibility: packet.summary.winner_visibility,
        ambient_exposure: packet.summary.ambient_exposure,
        subconscious_selected_count: packet.summary.subconscious_selected_count,
    }
}
