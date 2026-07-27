use serde::{Deserialize, Serialize};

use qsf_volition::{
    AllowedEffect, InitiativeOutput, VolitionStateInspection, VolitionSuppressionReason,
};

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
