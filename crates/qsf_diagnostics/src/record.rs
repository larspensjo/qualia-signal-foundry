use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use qsf_session::Exchange;

use crate::{
    LiveGoalFormationTrace, RealtimeBoundedInitiativeTrace, TurnPhase,
    VolitionContextInjectionTrace, WorldConsultationTrace,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticTrust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiagnosticRecord {
    SessionAllocated {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    NoSecretEvidence {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
        note: String,
    },
    CallBound {
        qsf_session_id: String,
        call_id: String,
        #[serde(with = "time::serde::rfc3339")]
        bound_at: OffsetDateTime,
    },
    CallInvalidated {
        qsf_session_id: String,
        call_id: String,
        #[serde(with = "time::serde::rfc3339")]
        invalidated_at: OffsetDateTime,
        reason: String,
    },
    RelayEventReceived {
        qsf_session_id: String,
        event_id: String,
        event_kind: String,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    LatencyObservation {
        qsf_session_id: String,
        label: String,
        #[serde(with = "time::serde::rfc3339")]
        started_at: OffsetDateTime,
        #[serde(with = "time::serde::rfc3339")]
        finished_at: OffsetDateTime,
        latency_ms: i64,
    },
    VolitionContextInjected {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: VolitionContextInjectionTrace,
    },
    RealtimeBoundedInitiative {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: RealtimeBoundedInitiativeTrace,
    },
    /// Authoritative external-effect boundary for a successful or deferred world-corpus read.
    WorldConsultationPerformed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: WorldConsultationTrace,
    },
    LiveGoalFormationPerformed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        trace: LiveGoalFormationTrace,
    },
    /// Persistent record of a formation attempt that errored (provider failure, non-JSON
    /// output, or judge-output validation failure) — without this, "formation failed" was
    /// indistinguishable in the diagnostics from "formation never attempted".
    LiveGoalFormationFailed {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        error: String,
    },
    /// Persistent record of a formation attempt that did not apply because it was skipped or
    /// discarded by runtime guards.
    LiveGoalFormationSkipped {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        reason: String,
    },
    VolitionContinuityNote {
        qsf_session_id: String,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        note: String,
        artifact_reference: String,
    },
    /// Verbatim model-visible request sequence for one trusted turn, persisted so
    /// experiments can verify what was actually sent to the provider.
    TurnContextCaptured {
        qsf_session_id: String,
        exchange_index: usize,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        request_hash: String,
        messages: Vec<serde_json::Value>,
    },
    DiagnosticExchangeRecorded {
        qsf_session_id: String,
        source: String,
        trust: DiagnosticTrust,
        #[serde(with = "time::serde::rfc3339")]
        recorded_at: OffsetDateTime,
        exchange: Exchange,
    },
    IgnoredContinuationTranscript {
        qsf_session_id: String,
        transcript: String,
        turn_phase: TurnPhase,
        response_id: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    StaleProviderEvent {
        qsf_session_id: String,
        response_id: Option<String>,
        status: Option<String>,
        exchange_index: Option<usize>,
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
}

/// The fields every reader can rely on without deserializing a whole record. Used to dispatch on
/// `kind` before committing to a variant, and to attribute an undecodable line to a run and turn.
/// Every field is optional: this must succeed on any syntactically valid JSON object, including
/// records written by a build that this one does not know.
#[derive(Clone, Debug, Deserialize)]
pub struct RecordEnvelope {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub qsf_session_id: Option<String>,
    #[serde(default)]
    pub exchange_index: Option<usize>,
}

/// The `kind` tag of `DiagnosticRecord::RealtimeBoundedInitiative`, as serde writes it. Named so
/// readers dispatching on the tag cannot drift from the enum's `rename_all = "snake_case"`.
pub const REALTIME_BOUNDED_INITIATIVE_KIND: &str = "realtime_bounded_initiative";

/// Decodes only the envelope. `None` when the line is not a JSON object at all.
pub fn decode_envelope(line: &str) -> Option<RecordEnvelope> {
    serde_json::from_str::<RecordEnvelope>(line).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_decodes_a_record_kind_this_build_does_not_know() {
        let envelope = decode_envelope(
            r#"{"kind":"from_a_future_build","qsf_session_id":"s","exchange_index":4,"extra":{}}"#,
        )
        .expect("an unknown kind still decodes as an envelope");

        assert_eq!(envelope.kind.as_deref(), Some("from_a_future_build"));
        assert_eq!(envelope.qsf_session_id.as_deref(), Some("s"));
        assert_eq!(envelope.exchange_index, Some(4));
    }

    #[test]
    fn the_initiative_kind_constant_matches_what_serde_writes() {
        let record = DiagnosticRecord::LiveGoalFormationSkipped {
            qsf_session_id: "s".to_string(),
            exchange_index: 0,
            recorded_at: OffsetDateTime::UNIX_EPOCH,
            reason: "guard".to_string(),
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains(r#""kind":"live_goal_formation_skipped""#));
        assert_eq!(
            REALTIME_BOUNDED_INITIATIVE_KIND,
            "realtime_bounded_initiative"
        );
    }

    #[test]
    fn a_non_object_line_has_no_envelope() {
        assert!(decode_envelope("not json").is_none());
        assert!(decode_envelope("[1,2,3]").is_none());
    }

    #[test]
    fn session_allocated_round_trips_with_its_kind_tag() {
        let record = DiagnosticRecord::SessionAllocated {
            qsf_session_id: "default".to_string(),
            at: OffsetDateTime::UNIX_EPOCH,
        };

        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains(r#""kind":"session_allocated""#));

        let parsed: DiagnosticRecord = serde_json::from_str(&json).expect("deserialize");
        let reserialized = serde_json::to_string(&parsed).expect("reserialize");
        assert_eq!(reserialized, json);
    }

    #[test]
    fn unknown_kind_is_rejected_rather_than_silently_accepted() {
        let result = serde_json::from_str::<DiagnosticRecord>(r#"{"kind":"not_a_record"}"#);
        assert!(result.is_err(), "unknown record kinds must fail to parse");
    }
}
