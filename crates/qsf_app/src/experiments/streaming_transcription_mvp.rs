use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::audio::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, TranscriptEventEmission,
    TranscriptEventTraceIds, TranscriptProvider, TranscriptProviderError,
    TranscriptProviderRequest, TranscriptProviderSession, build_transcript_provider,
    record_transcript_runtime_events, requested_transcript_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct StreamingTranscriptionMvpExperiment;

impl Experiment for StreamingTranscriptionMvpExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::StreamingTranscriptionMvp
    }

    fn description(&self) -> &'static str {
        "Stream transcript deltas as audio-derived events before committing final text to runtime input"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let provider = build_transcript_provider(requested_transcript_provider_from_env());
        self.run_with_provider(context, provider.as_ref())
    }
}

impl StreamingTranscriptionMvpExperiment {
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        provider: &dyn TranscriptProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        let request =
            TranscriptProviderRequest::from_env(format!("{}-transcription", context.run_id()));
        let session = match provider.transcribe(&request) {
            Ok(session) => session,
            Err(error) => {
                record_provider_failure(context, &error)?;
                return Err(error).context("transcript provider failed");
            }
        };

        let provider_trace = TraceRecord::new(
            context.experiment_id(),
            "transcript-provider-session",
            format!(
                "provider={} input_source={}",
                session.provider_name,
                session.input_source.label()
            ),
            format!(
                "emitted {} partial revisions and one final transcript",
                session.partial_revision_count()
            ),
        )
        .with_details(json!({
            "provider": &session.provider_name,
            "model": &session.model,
            "input_source": &session.input_source,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            "partials": &session.partials,
            "final_transcript": &session.final_transcript,
        }))
        .with_latency_context("audio", "transcript-provider-session")
        .with_latency_ms(session.final_transcript_latency_ms());
        let provider_trace_id = provider_trace.trace_id;
        context.record_trace(provider_trace)?;

        let latency_trace = TraceRecord::new(
            context.experiment_id(),
            "transcript-latency",
            "audio input timing and transcript revision timing",
            "first partial and final transcript latency measurements recorded",
        )
        .with_details(json!({
            "session_id": &session.session_id,
            "measurements": session.latency_measurements(),
            "first_partial_latency_ms": session.first_partial_latency_ms(),
            "final_transcript_latency_ms": session.final_transcript_latency_ms(),
            "partial_revision_count": session.partial_revision_count(),
        }))
        .with_latency_context("audio", "streaming-transcription")
        .with_latency_ms(session.final_transcript_latency_ms());
        let latency_trace_id = latency_trace.trace_id;
        context.record_trace(latency_trace)?;

        let boundary = transcription_boundary();
        let runtime_bridge_trace = TraceRecord::new(
            context.experiment_id(),
            "transcript-runtime-bridge",
            "AudioFinalTranscript produced by transcript provider",
            "InputReceived emitted through the normal runtime input boundary",
        )
        .with_details(json!({
            "session_id": &session.session_id,
            "boundary": boundary,
            "final_transcript_length": session.final_transcript_text_length(),
        }))
        .with_latency_context("audio", "runtime-input-dispatch")
        .with_latency_ms(
            session
                .completed_at_ms
                .saturating_sub(session.final_transcript.received_at_ms),
        );
        let runtime_bridge_trace_id = runtime_bridge_trace.trace_id;
        context.record_trace(runtime_bridge_trace)?;

        record_transcript_runtime_events(
            context,
            &session,
            TranscriptEventTraceIds {
                provider_trace_id,
                latency_trace_id,
                runtime_bridge_trace_id,
            },
            TranscriptEventEmission::new(transcription_boundary(), "streaming-transcription"),
        )?;
        write_streaming_transcription_report(context, &session)?;

        Ok(ExperimentOutcome {
            summary: "Phase 9 streaming transcription now supports simulated, prerecorded WAV, and live microphone request paths behind the transcript provider boundary, then bridges finalized speech text back into the existing InputReceived runtime event.".to_string(),
            observations: vec![
                "The transcript provider emits partial revisions before one final transcript.".to_string(),
                "Partial transcripts are logged as AudioPartialTranscript events and do not create InputReceived events.".to_string(),
                "The final transcript is explicitly bridged through AudioFinalTranscript before becoming normal runtime input.".to_string(),
                "Latency traces capture first partial, final transcript, and runtime dispatch timing without storing raw audio.".to_string(),
            ],
            failure_modes: vec![
                "The default experiment path remains simulated unless QSF_TRANSCRIPT_PROVIDER and QSF_TRANSCRIPT_INPUT_SOURCE select a real provider path.".to_string(),
                "Real microphone evaluation depends on local device support, permission prompts, and ambient recording conditions.".to_string(),
            ],
            follow_up_questions: vec![
                "Should prerecorded WAV fixtures be added before live microphone testing?".to_string(),
                "What latency target should first partial transcript events meet for presence experiments?".to_string(),
                "Should provider errors fall back to typed input or only fail the audio experiment?".to_string(),
                "Should the AudioFinalTranscript to InputReceived bridge move behind a dedicated reducer or dispatcher before Phase 10?".to_string(),
            ],
            decision_candidates: vec![
                "Partial transcript events remain observability-only unless a later experiment explicitly allows live-state updates.".to_string(),
                "Finalized transcript events enter the runtime loop as ordinary InputReceived events.".to_string(),
                "Provider adapters should report transcript metadata and timing, not raw audio or secrets.".to_string(),
                "A reducer or dispatcher should own producer-event to runtime-event translation before full realtime voice sessions.".to_string(),
            ],
            extra_artifacts: vec!["streaming-transcription.md".to_string()],
        })
    }
}

fn transcription_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::TranscriptProvider,
        producer_event: EventType::AudioFinalTranscript,
        runtime_event: EventType::InputReceived,
        description: "Final transcript text becomes committed runtime input only after the provider emits AudioFinalTranscript.".to_string(),
    }
}

fn record_provider_failure(
    context: &mut RunContext,
    error: &TranscriptProviderError,
) -> anyhow::Result<()> {
    engine_logging::engine_error!(
        "transcript provider failed: experiment_id={} run_id={} provider={} error={}",
        context.experiment_id(),
        context.run_id(),
        error.provider(),
        error.sanitized_message()
    );
    let sanitized_message = error.sanitized_message();
    let trace = TraceRecord::new(
        context.experiment_id(),
        "transcript-provider-session",
        format!("provider={}", error.provider()),
        "transcription failed before final transcript",
    )
    .with_latency_context("audio", "streaming-transcription")
    .with_error(&sanitized_message);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    context.record_event(
        EventType::AudioTranscriptionFailed,
        json!({
            "provider": error.provider(),
            "error_category": error.category(),
            "sanitized_error": sanitized_message,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn write_streaming_transcription_report(
    context: &RunContext,
    session: &TranscriptProviderSession,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Phase 9 Streaming Transcription MVP\n\n");
    markdown.push_str("## Provider\n\n");
    markdown.push_str(&format!("- Session id: `{}`\n", session.session_id));
    markdown.push_str(&format!("- Provider: `{}`\n", session.provider_name));
    markdown.push_str(&format!(
        "- Input source: `{}`\n",
        session.input_source.label()
    ));
    markdown.push_str("- Raw audio logged: `false`\n\n");

    markdown.push_str("## Transcript revisions\n\n");
    for partial in &session.partials {
        markdown.push_str(&format!(
            "- Partial {} at {} ms: {}\n",
            partial.revision_index, partial.received_at_ms, partial.transcript
        ));
    }
    markdown.push_str(&format!(
        "- Final at {} ms: {}\n\n",
        session.final_transcript.received_at_ms, session.final_transcript.transcript
    ));

    markdown.push_str("## Latency\n\n");
    markdown.push_str(&format!(
        "- First partial latency: {} ms\n",
        session.first_partial_latency_ms().unwrap_or(0)
    ));
    markdown.push_str(&format!(
        "- Final transcript latency: {} ms\n",
        session.final_transcript_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Runtime input dispatch latency: {} ms\n",
        session
            .completed_at_ms
            .saturating_sub(session.final_transcript.received_at_ms)
    ));

    fs::write(
        context.run_dir().join("streaming-transcription.md"),
        markdown,
    )
    .with_context(|| {
        format!(
            "failed to write streaming transcription report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::StreamingTranscriptionMvpExperiment;
    use crate::audio::test_support::{
        assert_events_have_safety_markers, assert_payloads_do_not_contain_raw_audio_fields,
        is_audio_input_or_transcript_event, parse_event_records,
    };
    use crate::audio::{
        SimulatedTranscriptProvider, TranscriptProvider, TranscriptProviderError,
        TranscriptProviderRequest, TranscriptProviderSession,
    };
    use crate::experiments::registry::{Experiment, ExperimentName};
    use crate::observability::event_log::EventType;
    use crate::runtime::run_context::RunContext;
    use uuid::Uuid;

    struct FailingProvider;

    impl TranscriptProvider for FailingProvider {
        fn provider_name(&self) -> &str {
            "failing-transcript-provider"
        }

        fn transcribe(
            &self,
            _request: &TranscriptProviderRequest,
        ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
            Err(TranscriptProviderError::TranscriptionFailed {
                provider: self.provider_name().to_string(),
                message: "upstream returned Authorization: Bearer sk-test-secret".to_string(),
            })
        }
    }

    #[test]
    fn streaming_transcription_experiment_writes_transcript_events_and_traces() {
        let base_dir = std::env::temp_dir().join(format!("qsf-streaming-test-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "streaming-transcription-mvp").unwrap();
        let experiment = StreamingTranscriptionMvpExperiment;

        experiment
            .run_with_provider(&mut context, &SimulatedTranscriptProvider)
            .unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("streaming-transcription.md")).unwrap();
        let event_records = parse_event_records(&events);

        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::AudioPartialTranscript)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::AudioFinalTranscript)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::InputReceived)
        );
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::AudioPartialTranscript
                && record.payload["committed_to_runtime"] == false
        }));
        assert_events_have_safety_markers(&event_records, is_audio_input_or_transcript_event);
        assert_payloads_do_not_contain_raw_audio_fields(&event_records);
        assert!(traces.contains("transcript-provider-session"));
        assert!(traces.contains("transcript-latency"));
        assert!(traces.contains("transcript-runtime-bridge"));
        assert!(traces.contains("\"latency_domain\":\"audio\""));
        assert!(traces.contains("\"latency_ms\":86"));
        assert!(report.contains("Phase 9 Streaming Transcription MVP"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn streaming_transcription_provider_failure_records_sanitized_failure_event() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-streaming-failure-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "streaming-transcription-mvp").unwrap();
        let experiment = StreamingTranscriptionMvpExperiment;

        let error = experiment
            .run_with_provider(&mut context, &FailingProvider)
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert_eq!(error.to_string(), "transcript provider failed");
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::AudioTranscriptionFailed
                && record.payload["error_category"] == "transcription_failed"
                && record.payload["sanitized_error"]
                    == "provider error redacted because it may contain credential-like content"
        }));
        assert!(!events.contains("sk-test-secret"));
        assert!(!traces.contains("sk-test-secret"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn experiment_name_matches_registry_id() {
        let experiment = StreamingTranscriptionMvpExperiment;

        assert_eq!(experiment.name(), ExperimentName::StreamingTranscriptionMvp);
        assert_eq!(experiment.id(), "streaming-transcription-mvp");
    }
}
