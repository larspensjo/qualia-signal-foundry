use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use qsf_volition::{AdmissionResolution, Contradiction, DeclinedCandidate, VolitionEvent};

/// Live analogue of the offline `live-goal-formation` trace record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LiveGoalFormationTrace {
    pub tick: u64,
    pub input_transcript_ref: String,
    pub cached_prefix_ref: String,
    pub prefix_cache_eligible: bool,
    pub judge_model_role: String,
    pub judge_prompt_version: String,
    pub proposed_candidate_id: Option<String>,
    pub proposed_candidate_title: Option<String>,
    pub contradictions: Vec<Contradiction>,
    pub hard_tier_floor_rejected: bool,
    pub resolution: Option<AdmissionResolution>,
    /// The candidate newly added to `VolitionState::declined_candidates` this turn, if any.
    /// `None` both when nothing was rejected and when a rejection was deduplicated against an
    /// already-declined candidate with the same title.
    pub declined_candidate: Option<DeclinedCandidate>,
    pub events_emitted: Vec<VolitionEvent>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub response_dispatched_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub formation_started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub formation_completed_at: OffsetDateTime,
}
