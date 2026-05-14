use serde_json::Value;

use crate::observability::event_log::{EventRecord, EventType};

pub fn parse_event_records(events: &str) -> Vec<EventRecord> {
    events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub fn assert_events_have_safety_markers(
    records: &[EventRecord],
    mut include_event: impl FnMut(&EventType) -> bool,
) {
    for record in records
        .iter()
        .filter(|record| include_event(&record.event_type))
    {
        assert_eq!(record.payload["safety"]["raw_audio_logged"], false);
        assert_eq!(record.payload["safety"]["authorization_logged"], false);
        assert_eq!(record.payload["safety"]["api_key_logged"], false);
    }
}

pub fn assert_payloads_do_not_contain_raw_audio_fields(records: &[EventRecord]) {
    for record in records {
        assert_value_does_not_contain_raw_audio_fields(&record.payload);
    }
}

pub fn is_audio_input_or_transcript_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::AudioInputStarted
            | EventType::AudioInputChunkCaptured
            | EventType::AudioPartialTranscript
            | EventType::AudioFinalTranscript
            | EventType::AudioInputEnded
            | EventType::AudioTranscriptionFailed
            | EventType::LatencyMeasurementRecorded
    )
}

pub fn is_audio_or_speech_event(event_type: &EventType) -> bool {
    is_audio_input_or_transcript_event(event_type)
        || matches!(
            event_type,
            EventType::SpeechPlaybackRequested
                | EventType::SpeechPlaybackStarted
                | EventType::SpeechPlaybackCompleted
                | EventType::ErrorOccurred
        )
}

fn assert_value_does_not_contain_raw_audio_fields(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let normalized_key = key.to_ascii_lowercase();
                assert!(
                    !matches!(
                        normalized_key.as_str(),
                        "pcm" | "audio_bytes" | "audio_blob" | "wav" | "raw_audio" | "audio_data"
                    ),
                    "payload contains raw-audio-like field `{key}`"
                );
                assert_value_does_not_contain_raw_audio_fields(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                assert_value_does_not_contain_raw_audio_fields(value);
            }
        }
        _ => {}
    }
}
