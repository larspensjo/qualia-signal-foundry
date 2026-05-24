use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessedRangeKind {
    LiveBatch,
    SessionEnd,
    SleepSafetyNet,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProcessedRange {
    pub session_id: String,
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub kind: ProcessedRangeKind,
    pub at: OffsetDateTime,
}
