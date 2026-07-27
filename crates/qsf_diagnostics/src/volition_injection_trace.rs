use serde::{Deserialize, Serialize};

use qsf_volition::{
    ActivationKeyword, DeclineReason, GoalVisibility, Mode, OpportunitySignal, ShapingIntensity,
    ShapingIntensityInputs,
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

/// How the arbitration winner is exposed in this turn's model-visible text.
///
/// - `Ordinary` — a conscious winner rendered as the full `Active goal` line.
/// - `ReducedSubconscious` — a subconscious winner with no forced-surfacing condition this turn:
///   the model-visible text carries only a labeled background-guidance line (visibility, intensity,
///   safe guidance, artifact reference), never the winner's title, summary, or id. The trace keeps
///   the full winner identity.
/// - `ForcedSurfacedSubconscious` — a subconscious winner that rendered an initiative line or is
///   named in a coherence conflict this turn: full detail, labeled as a surfaced background goal
///   and backed by the forcing evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbientExposure {
    Ordinary,
    ReducedSubconscious,
    ForcedSurfacedSubconscious,
}

pub fn default_ambient_exposure() -> AmbientExposure {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_match_detail_round_trips_through_json() {
        let detail = VolitionSelectedMatchDetail {
            goal_id: "grow-the-library".to_string(),
            matched_keywords: vec![
                ActivationKeyword::normal("remember"),
                ActivationKeyword::weak("earlier"),
            ],
            match_strength: 5,
            visibility: GoalVisibility::Conscious,
        };

        let json = serde_json::to_string(&detail).expect("serialize");
        let parsed: VolitionSelectedMatchDetail = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed, detail);
    }

    #[test]
    fn selected_match_detail_defaults_visibility_for_older_traces() {
        let parsed: VolitionSelectedMatchDetail =
            serde_json::from_str(r#"{"goal_id":"g","matched_keywords":[],"match_strength":0}"#)
                .expect("deserialize without visibility");

        assert_eq!(parsed.visibility, GoalVisibility::Conscious);
    }
}
