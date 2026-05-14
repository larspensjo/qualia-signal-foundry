use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::audio::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, RealtimeSessionProvider,
    RealtimeSessionProviderError, RealtimeSessionRequest, VoiceProviderSession,
    build_realtime_session_provider, requested_realtime_session_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct RealtimeVoiceSessionExperiment;

impl Experiment for RealtimeVoiceSessionExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::RealtimeVoiceSession
    }

    fn description(&self) -> &'static str {
        "Run a realtime voice-session provider and map session events back into QSF event records"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let provider =
            build_realtime_session_provider(requested_realtime_session_provider_from_env());
        self.run_with_provider(context, provider.as_ref())
    }
}

impl RealtimeVoiceSessionExperiment {
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        provider: &dyn RealtimeSessionProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        let request =
            RealtimeSessionRequest::from_env(format!("{}-voice-session", context.run_id()));
        let session = match provider.run_session(&request) {
            Ok(session) => session,
            Err(error) => {
                record_provider_failure(context, &error)?;
                return Err(error).context("realtime session provider failed");
            }
        };

        let provider_trace = TraceRecord::new(
            context.experiment_id(),
            "realtime-session-provider",
            format!(
                "provider={} input_source={}",
                session.provider_name,
                session.input_source.label()
            ),
            format!(
                "response_status={} interruptions={} tool_calls={}",
                session.response.status,
                session.interruptions.len(),
                session.tool_calls.len()
            ),
        )
        .with_details(json!({
            "provider": &session.provider_name,
            "model": &session.model,
            "input_source": &session.input_source,
            "voice": &session.config.voice,
            "reasoning_effort": &session.config.reasoning_effort,
            "output_modalities": &session.config.output_modalities,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }))
        .with_latency_context("audio", "realtime-session-provider")
        .with_latency_ms(session.total_latency_ms());
        let provider_trace_id = provider_trace.trace_id;
        context.record_trace(provider_trace)?;

        let runtime_boundary = voice_runtime_boundary();
        let runtime_trace = TraceRecord::new(
            context.experiment_id(),
            "realtime-session-runtime-boundary",
            "AudioFinalTranscript emitted by realtime session provider",
            "InputReceived and OutputProduced remained QSF-owned event records",
        )
        .with_details(json!({
            "session_id": &session.session_id,
            "input_boundary": runtime_boundary,
            "final_transcript": &session.final_transcript,
            "response_id": &session.response.response_id,
            "provider_tool_calls": &session.tool_calls,
            "tool_calls_auto_executed": false,
        }))
        .with_latency_context("audio", "realtime-runtime-boundary")
        .with_latency_ms(
            session
                .response
                .started_at_ms
                .saturating_sub(session.final_transcript.received_at_ms),
        );
        let runtime_trace_id = runtime_trace.trace_id;
        context.record_trace(runtime_trace)?;

        let latency_trace = TraceRecord::new(
            context.experiment_id(),
            "realtime-session-latency",
            "voice session turn timing",
            "captured response start, first audio, completion, and interruption timing",
        )
        .with_details(json!({
            "session_id": &session.session_id,
            "measurements": session.latency_measurements(),
            "response_latency_ms": session.response_latency_ms(),
            "first_audio_latency_ms": session.first_audio_latency_ms(),
            "response_start_offset_from_final_transcript_ms": session.response_start_offset_from_final_transcript_ms(),
            "response_started_before_final_transcript": session.response_start_offset_from_final_transcript_ms() < 0,
            "interruption_count": session.interruptions.len(),
        }))
        .with_latency_context("audio", "realtime-voice-session")
        .with_latency_ms(session.total_latency_ms());
        let latency_trace_id = latency_trace.trace_id;
        context.record_trace(latency_trace)?;

        record_voice_session_events(
            context,
            &session,
            provider_trace_id,
            runtime_trace_id,
            latency_trace_id,
        )?;
        write_realtime_voice_report(context, &session)?;

        Ok(ExperimentOutcome {
            summary: "Phase 10 realtime voice sessions now run behind a RealtimeSessionProvider boundary, map provider lifecycle data into QSF events and traces, and keep transcript, state, output, and tool handling on the framework side of the boundary.".to_string(),
            observations: vec![
                "The default experiment path is deterministic simulation; OpenAI realtime voice is selected explicitly through QSF_REALTIME_SESSION_PROVIDER.".to_string(),
                "Provider preambles, response lifecycle, speech playback lifecycle, interruptions, and latency are visible as structured records.".to_string(),
                "Provider tool-call requests are recorded as ToolRequested events with auto execution disabled so QSF tool permission boundaries remain authoritative.".to_string(),
            ],
            failure_modes: vec![
                "The OpenAI path depends on OPENAI_API_KEY, network access, account realtime access, and valid local audio input when microphone or WAV sources are selected.".to_string(),
                "The MVP records output audio metadata but does not yet play synthesized audio through a local speaker device.".to_string(),
            ],
            follow_up_questions: vec![
                "Should Phase 10 add local speaker playback after the provider event boundary is stable?".to_string(),
                "Which interruption timing target best predicts perceived presence?".to_string(),
                "Should response preambles be promoted into a first-class runtime output category?".to_string(),
            ],
            decision_candidates: vec![
                "Realtime voice providers remain side-effect adapters and must not mutate runtime state directly.".to_string(),
                "Provider tool-call requests must be converted into QSF tool events before any tool can execute.".to_string(),
                "Realtime voice evaluation should stay opt-in rather than activating from OPENAI_API_KEY alone.".to_string(),
            ],
            extra_artifacts: vec!["realtime-voice-session.md".to_string()],
        })
    }
}

fn voice_runtime_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::TranscriptProvider,
        producer_event: EventType::AudioFinalTranscript,
        runtime_event: EventType::InputReceived,
        description: "Realtime session providers emit transcript and response lifecycle facts, but finalized user text still enters QSF as InputReceived.".to_string(),
    }
}

fn record_voice_session_events(
    context: &mut RunContext,
    session: &VoiceProviderSession,
    provider_trace_id: uuid::Uuid,
    runtime_trace_id: uuid::Uuid,
    latency_trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::RealtimeSessionStarted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "model": &session.model,
            "input_source": &session.input_source,
            "voice": &session.config.voice,
            "reasoning_effort": &session.config.reasoning_effort,
            "output_modalities": &session.config.output_modalities,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(provider_trace_id),
    )?;

    context.record_event(
        EventType::AudioInputStarted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "entry_point": AudioRuntimeEntryPoint::TranscriptProvider,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(provider_trace_id),
    )?;

    for chunk in &session.chunks {
        context.record_event(
            EventType::AudioInputChunkCaptured,
            json!({
                "session_id": &session.session_id,
                "provider": &session.provider_name,
                "chunk_index": chunk.chunk_index,
                "captured_chunk_count": session.chunks.len(),
                "captured_at_ms": chunk.captured_at_ms,
                "duration_ms": chunk.duration_ms,
                "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            }),
            Some(provider_trace_id),
        )?;
    }

    let boundary = voice_runtime_boundary();
    context.record_event(
        EventType::AudioFinalTranscript,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "utterance_index": session.final_transcript.utterance_index,
            "received_at_ms": session.final_transcript.received_at_ms,
            "transcript": &session.final_transcript.transcript,
            "boundary": boundary,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(runtime_trace_id),
    )?;

    context.record_event(
        EventType::AudioInputEnded,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "captured_chunk_count": session.chunks.len(),
            "completed_at_ms": session.final_transcript.received_at_ms,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(provider_trace_id),
    )?;

    context.record_event(
        EventType::InputReceived,
        json!({
            "session_id": &session.session_id,
            "source_event": EventType::AudioFinalTranscript,
            "input_text": &session.final_transcript.transcript,
            "entry_point": AudioRuntimeEntryPoint::RuntimeInput,
            "entry_point_description": AudioRuntimeEntryPoint::RuntimeInput.description(),
        }),
        Some(runtime_trace_id),
    )?;

    if let Some(preamble) = &session.preamble {
        context.record_event(
            EventType::RealtimePreambleProduced,
            json!({
                "session_id": &session.session_id,
                "response_id": &session.response.response_id,
                "preamble": preamble,
                "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
            }),
            Some(provider_trace_id),
        )?;
    }

    context.record_event(
        EventType::RealtimeResponseStarted,
        json!({
            "session_id": &session.session_id,
            "response_id": &session.response.response_id,
            "started_at_ms": session.response.started_at_ms,
            "target": "speech-playback",
            "provider": &session.provider_name,
        }),
        Some(provider_trace_id),
    )?;

    for tool_call in &session.tool_calls {
        context.record_event(
            EventType::ToolRequested,
            json!({
                "session_id": &session.session_id,
                "source": "realtime_provider",
                "call_id": &tool_call.call_id,
                "tool_name": &tool_call.name,
                "arguments_summary": &tool_call.arguments_summary,
                "requested_at_ms": tool_call.requested_at_ms,
                "routed_to": "qsf_tool_permission_boundary",
                "auto_executed": false,
            }),
            Some(runtime_trace_id),
        )?;
    }

    context.record_event(
        EventType::OutputProduced,
        json!({
            "session_id": &session.session_id,
            "response_id": &session.response.response_id,
            "message": &session.response.text,
            "source": "realtime_provider_response",
            "target": "speech-playback",
        }),
        Some(runtime_trace_id),
    )?;

    context.record_event(
        EventType::SpeechPlaybackRequested,
        json!({
            "session_id": &session.session_id,
            "response_id": &session.response.response_id,
            "adapter": &session.provider_name,
            "voice": &session.config.voice,
            "text": &session.response.text,
            "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
            "entry_point_description": AudioRuntimeEntryPoint::RuntimeOutput.description(),
        }),
        Some(provider_trace_id),
    )?;

    if session.response.first_audio_delta_at_ms.is_some() {
        context.record_event(
            EventType::SpeechPlaybackStarted,
            json!({
                "session_id": &session.session_id,
                "response_id": &session.response.response_id,
                "adapter": &session.provider_name,
                "started_at_ms": session.response.first_audio_delta_at_ms,
                "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
            }),
            Some(provider_trace_id),
        )?;
    }

    for interruption in &session.interruptions {
        context.record_event(
            EventType::UserInterrupted,
            json!({
                "session_id": &session.session_id,
                "response_id": &interruption.response_id,
                "detected_at_ms": interruption.detected_at_ms,
                "source": &interruption.source,
                "action": &interruption.action,
            }),
            Some(latency_trace_id),
        )?;
    }

    context.record_event(
        EventType::RealtimeResponseCompleted,
        json!({
            "session_id": &session.session_id,
            "response_id": &session.response.response_id,
            "completed_at_ms": session.response.completed_at_ms,
            "status": &session.response.status,
            "audio_output_bytes": session.response.audio_output_bytes,
        }),
        Some(provider_trace_id),
    )?;

    context.record_event(
        EventType::SpeechPlaybackCompleted,
        json!({
            "session_id": &session.session_id,
            "response_id": &session.response.response_id,
            "adapter": &session.provider_name,
            "completed_at_ms": session.response.completed_at_ms,
            "status": &session.response.status,
            "audio_output_bytes": session.response.audio_output_bytes,
            "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
        }),
        Some(provider_trace_id),
    )?;

    context.record_event(
        EventType::LatencyMeasurementRecorded,
        json!({
            "session_id": &session.session_id,
            "domain": "audio",
            "stage": "realtime-voice-session",
            "measurements": session.latency_measurements(),
            "response_latency_ms": session.response_latency_ms(),
            "first_audio_latency_ms": session.first_audio_latency_ms(),
            "response_start_offset_from_final_transcript_ms": session.response_start_offset_from_final_transcript_ms(),
            "response_started_before_final_transcript": session.response_start_offset_from_final_transcript_ms() < 0,
            "interruption_count": session.interruptions.len(),
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(latency_trace_id),
    )?;

    context.record_event(
        EventType::RealtimeSessionCompleted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "completed_at_ms": session.completed_at_ms,
            "response_status": &session.response.status,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(provider_trace_id),
    )?;

    Ok(())
}

fn record_provider_failure(
    context: &mut RunContext,
    error: &RealtimeSessionProviderError,
) -> anyhow::Result<()> {
    engine_logging::engine_error!(
        "realtime session provider failed: experiment_id={} run_id={} provider={} error={}",
        context.experiment_id(),
        context.run_id(),
        error.provider(),
        error.sanitized_message()
    );
    let sanitized_message = error.sanitized_message();
    let trace = TraceRecord::new(
        context.experiment_id(),
        "realtime-session-provider",
        format!("provider={}", error.provider()),
        "realtime voice session failed before completion",
    )
    .with_latency_context("audio", "realtime-voice-session")
    .with_error(&sanitized_message);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    context.record_event(
        EventType::RealtimeSessionFailed,
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

fn write_realtime_voice_report(
    context: &RunContext,
    session: &VoiceProviderSession,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Phase 10 Realtime Voice Session MVP\n\n");
    markdown.push_str("## Provider\n\n");
    markdown.push_str(&format!("- Session id: `{}`\n", session.session_id));
    markdown.push_str(&format!("- Provider: `{}`\n", session.provider_name));
    markdown.push_str(&format!("- Model: `{:?}`\n", session.model));
    markdown.push_str(&format!(
        "- Input source: `{}`\n",
        session.input_source.label()
    ));
    markdown.push_str(&format!("- Voice: `{}`\n", session.config.voice));
    markdown.push_str(&format!(
        "- Reasoning effort: `{}`\n",
        session.config.reasoning_effort
    ));
    markdown.push_str("- Raw audio logged: `false`\n\n");

    markdown.push_str("## Turn\n\n");
    markdown.push_str(&format!(
        "- Final transcript at {} ms: {}\n",
        session.final_transcript.received_at_ms, session.final_transcript.transcript
    ));
    if let Some(preamble) = &session.preamble {
        markdown.push_str(&format!("- Preamble: {}\n", preamble));
    }
    markdown.push_str(&format!(
        "- Response status: `{}` at {} ms\n",
        session.response.status, session.response.completed_at_ms
    ));
    markdown.push_str(&format!("- Response text: {}\n", session.response.text));
    markdown.push_str(&format!(
        "- Output audio bytes observed: `{}`\n\n",
        session.response.audio_output_bytes
    ));

    markdown.push_str("## Boundaries\n\n");
    markdown.push_str(&format!(
        "- Provider tool calls requested: `{}`\n",
        session.tool_calls.len()
    ));
    markdown.push_str("- Provider tool calls auto-executed: `false`\n");
    markdown.push_str(&format!(
        "- Interruptions recorded: `{}`\n\n",
        session.interruptions.len()
    ));

    markdown.push_str("## Latency\n\n");
    markdown.push_str(&format!(
        "- Total turn latency: {} ms\n",
        session.total_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Response latency: {} ms\n",
        session.response_latency_ms()
    ));
    markdown.push_str(&format!(
        "- First audio latency: {} ms\n",
        session.first_audio_latency_ms().unwrap_or(0)
    ));
    markdown.push_str(&format!(
        "- Response start offset from final transcript: {} ms\n",
        session.response_start_offset_from_final_transcript_ms()
    ));

    fs::write(
        context.run_dir().join("realtime-voice-session.md"),
        markdown,
    )
    .with_context(|| {
        format!(
            "failed to write realtime voice session report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::RealtimeVoiceSessionExperiment;
    use crate::audio::{
        RealtimeSessionProvider, RealtimeSessionProviderError, RealtimeSessionRequest,
        SimulatedRealtimeSessionProvider, VoiceProviderSession,
    };
    use crate::experiments::registry::{Experiment, ExperimentName};
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use serde_json::Value;
    use uuid::Uuid;

    struct FailingProvider;

    impl RealtimeSessionProvider for FailingProvider {
        fn provider_name(&self) -> &str {
            "failing-realtime-session-provider"
        }

        fn run_session(
            &self,
            _request: &RealtimeSessionRequest,
        ) -> Result<VoiceProviderSession, RealtimeSessionProviderError> {
            Err(RealtimeSessionProviderError::SessionFailed {
                provider: self.provider_name().to_string(),
                message: "upstream returned Authorization: Bearer sk-test-secret".to_string(),
            })
        }
    }

    #[test]
    fn realtime_voice_session_experiment_writes_voice_events_and_traces() {
        let base_dir = std::env::temp_dir().join(format!("qsf-voice-test-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "realtime-voice-session").unwrap();
        let experiment = RealtimeVoiceSessionExperiment;

        experiment
            .run_with_provider(&mut context, &SimulatedRealtimeSessionProvider)
            .unwrap();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("realtime-voice-session.md")).unwrap();
        let event_records = parse_event_records(&events);

        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::RealtimeSessionStarted)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::RealtimePreambleProduced)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::RealtimeResponseStarted)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::RealtimeResponseCompleted)
        );
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::UserInterrupted)
        );
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::ToolRequested
                && record.payload["auto_executed"] == false
        }));
        assert_audio_events_have_safety_markers(&event_records);
        assert_payloads_do_not_contain_raw_audio_fields(&event_records);
        assert!(traces.contains("realtime-session-provider"));
        assert!(traces.contains("realtime-session-runtime-boundary"));
        assert!(traces.contains("realtime-session-latency"));
        assert!(traces.contains("\"latency_domain\":\"audio\""));
        assert!(report.contains("Phase 10 Realtime Voice Session MVP"));
        assert!(report.contains("Provider tool calls auto-executed: `false`"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn realtime_session_provider_failure_records_sanitized_failure_event() {
        let base_dir = std::env::temp_dir().join(format!("qsf-voice-failure-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "realtime-voice-session").unwrap();
        let experiment = RealtimeVoiceSessionExperiment;

        let error = experiment
            .run_with_provider(&mut context, &FailingProvider)
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert_eq!(error.to_string(), "realtime session provider failed");
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::RealtimeSessionFailed
                && record.payload["error_category"] == "session_failed"
                && record.payload["sanitized_error"]
                    == "provider error redacted because it may contain credential-like content"
        }));
        assert!(!events.contains("sk-test-secret"));
        assert!(!traces.contains("sk-test-secret"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn experiment_name_matches_registry_id() {
        let experiment = RealtimeVoiceSessionExperiment;

        assert_eq!(experiment.name(), ExperimentName::RealtimeVoiceSession);
        assert_eq!(experiment.id(), "realtime-voice-session");
    }

    fn parse_event_records(events: &str) -> Vec<EventRecord> {
        events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn assert_audio_events_have_safety_markers(records: &[EventRecord]) {
        for record in records
            .iter()
            .filter(|record| is_audio_or_realtime_event(&record.event_type))
        {
            assert_eq!(record.payload["safety"]["raw_audio_logged"], false);
            assert_eq!(record.payload["safety"]["authorization_logged"], false);
            assert_eq!(record.payload["safety"]["api_key_logged"], false);
        }
    }

    fn is_audio_or_realtime_event(event_type: &EventType) -> bool {
        matches!(
            event_type,
            EventType::AudioInputStarted
                | EventType::AudioInputChunkCaptured
                | EventType::AudioFinalTranscript
                | EventType::AudioInputEnded
                | EventType::RealtimeSessionStarted
                | EventType::RealtimeSessionCompleted
                | EventType::RealtimeSessionFailed
                | EventType::LatencyMeasurementRecorded
        )
    }

    fn assert_payloads_do_not_contain_raw_audio_fields(records: &[EventRecord]) {
        for record in records {
            assert_value_does_not_contain_raw_audio_fields(&record.payload);
        }
    }

    fn assert_value_does_not_contain_raw_audio_fields(value: &Value) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let normalized_key = key.to_ascii_lowercase();
                    assert!(
                        !matches!(
                            normalized_key.as_str(),
                            "pcm" | "audio_bytes" | "wav" | "raw_audio" | "audio_data"
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
}
