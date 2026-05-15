use serde_json::{Map, Value, json};

use crate::audio::AudioSafetyMarkers;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

pub(super) struct SanitizedFailure<'a> {
    pub event_type: EventType,
    pub operation: &'a str,
    pub output_summary: &'a str,
    pub latency_stage: &'a str,
    pub provider: &'a str,
    pub error_category: &'a str,
    pub sanitized_error: &'a str,
    pub extra_payload: Map<String, Value>,
}

pub(super) fn record_sanitized_failure(
    context: &mut RunContext,
    failure: SanitizedFailure<'_>,
) -> anyhow::Result<()> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        failure.operation,
        format!("provider={}", failure.provider),
        failure.output_summary,
    )
    .with_latency_context("audio", failure.latency_stage)
    .with_error(failure.sanitized_error);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;

    let mut payload = failure.extra_payload;
    payload.insert("provider".to_string(), json!(failure.provider));
    payload.insert("error_category".to_string(), json!(failure.error_category));
    payload.insert(
        "sanitized_error".to_string(),
        json!(failure.sanitized_error),
    );
    payload.insert(
        "safety".to_string(),
        json!(AudioSafetyMarkers::no_secret_or_raw_audio()),
    );

    context.record_event(failure.event_type, Value::Object(payload), Some(trace_id))?;
    Ok(())
}
