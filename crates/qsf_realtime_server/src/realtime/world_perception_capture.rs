use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::realtime::world_consultation::WorldConsultationTrace;

/// Latest-only, browser-facing observation of this trusted turn's world perception.
///
/// `consultation: None` is intentional: it lets the diagnostic panel distinguish a trusted turn
/// that did not consult the world from a consultation that found no eligible facts.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldPerceptionCapture {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    #[serde(with = "time::serde::rfc3339")]
    pub captured_at: OffsetDateTime,
    pub consultation: Option<WorldConsultationTrace>,
}

#[derive(Debug, Serialize)]
pub struct WorldPerceptionMessage {
    kind: &'static str,
    #[serde(flatten)]
    capture: WorldPerceptionCapture,
}

pub fn build_world_perception_capture(
    qsf_session_id: String,
    exchange_index: usize,
    captured_at: OffsetDateTime,
    consultation: Option<WorldConsultationTrace>,
) -> WorldPerceptionCapture {
    WorldPerceptionCapture {
        qsf_session_id,
        exchange_index,
        captured_at,
        consultation,
    }
}

pub fn serialize_world_perception_message(
    capture: &WorldPerceptionCapture,
) -> serde_json::Result<String> {
    serde_json::to_string(&WorldPerceptionMessage {
        kind: "world_perception",
        capture: capture.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_consultation_capture_is_explicit() {
        let capture = build_world_perception_capture(
            "session-1".to_string(),
            4,
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp"),
            None,
        );

        assert!(capture.consultation.is_none());
        assert_eq!(capture.exchange_index, 4);
    }
}
