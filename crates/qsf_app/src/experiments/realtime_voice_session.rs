use std::fs;
use std::time::SystemTime;

use anyhow::Context;
use serde_json::json;

use crate::audio::{
    AudioRuntimeEntryPoint, AudioSafetyMarkers, RealtimeInterruptionAction,
    RealtimeSessionProvider, RealtimeSessionProviderError, RealtimeSessionRequest,
    VoiceProviderSession, build_realtime_session_provider, relative_audio_event_time,
    requested_realtime_session_provider_from_env, transcript_provider_to_input_boundary,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::session::{
    Exchange, ExchangeOutput, InterruptionAction, InterruptionRecord, InterruptionStopOutcome,
    LiveSessionEvent, ProviderEventKind, ProviderEventRecord, SessionBootRequest, SessionConfig,
    SessionEndReason, SessionEvent, StateDirectoryResolution, ToolRequestRecord, UtteranceRecord,
};

use super::failure::{SanitizedFailure, record_sanitized_failure};
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
        let state_resolution = crate::session::resolve_shared_state_directory_from_env();
        self.run_with_provider_at_state_dirs(context, provider.as_ref(), state_resolution)
    }
}

impl RealtimeVoiceSessionExperiment {
    #[cfg(test)]
    fn run_with_provider(
        &self,
        context: &mut RunContext,
        provider: &dyn RealtimeSessionProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        let state_dir = context.run_dir().join("state/session");
        self.run_with_provider_at_state_dirs(
            context,
            provider,
            StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir,
                legacy_fallback_used: false,
            },
        )
    }

    fn run_with_provider_at_state_dirs(
        &self,
        context: &mut RunContext,
        provider: &dyn RealtimeSessionProvider,
        state_resolution: StateDirectoryResolution,
    ) -> anyhow::Result<ExperimentOutcome> {
        let config = SessionConfig::from_env();
        let boot = crate::session::boot_session(
            context,
            SessionBootRequest {
                resume_state_dir: state_resolution.resume_state_dir.clone(),
                persist_state_dir: state_resolution.persist_state_dir.clone(),
                config,
                legacy_fallback_used: state_resolution.legacy_fallback_used,
            },
        )?;
        let mut state = boot.state;
        let resume_manifest = boot.resume_inputs.manifest.clone();
        let request = RealtimeSessionRequest::from_env(state.session_id.clone());
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
        bridge_realtime_session_into_shared_state(
            context,
            &mut state,
            &session,
            &state_resolution,
            &resume_manifest,
        )?;
        write_realtime_voice_report(context, &session)?;

        Ok(ExperimentOutcome {
            summary: "Realtime voice sessions run behind a RealtimeSessionProvider boundary, map provider lifecycle data into QSF events and traces, and keep transcript, state, output, and tool handling on the framework side of the boundary.".to_string(),
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
                "Should local speaker playback be added after the provider event boundary is stable?".to_string(),
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

fn bridge_realtime_session_into_shared_state(
    context: &mut RunContext,
    state: &mut crate::session::SessionState,
    session: &VoiceProviderSession,
    state_resolution: &StateDirectoryResolution,
    resume_manifest: &crate::session::manifest::ContinuityManifest,
) -> anyhow::Result<()> {
    let exchange_index = state.turns.len() + state.exchanges.len();
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::ExchangeStarted(Box::new(Exchange::new_voice_pending(
            exchange_index,
            SystemTime::now(),
        ))),
    );
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::AudioFinalTranscriptCommitted {
            exchange_index,
            utterance: UtteranceRecord {
                utterance_id: format!(
                    "realtime-utterance-{}",
                    session.final_transcript.utterance_index
                ),
                revision_index: session.final_transcript.utterance_index,
                transcript: session.final_transcript.transcript.clone(),
                received_at: relative_audio_event_time(session.final_transcript.received_at_ms),
                provider_id: Some(session.provider_name.clone()),
                source_chunk_index: session
                    .chunks
                    .last()
                    .map(|chunk| chunk.chunk_index as usize),
            },
            final_transcript: session.final_transcript.transcript.clone(),
        },
    );
    crate::session::apply_session_event(
        context,
        state,
        SessionEvent::InputReceived {
            input: session.final_transcript.transcript.clone(),
        },
    )?;

    if let Some(preamble) = &session.preamble {
        crate::session::apply_live_session_event(
            state,
            LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
                exchange_index,
                event_kind: ProviderEventKind::Preamble,
                provider_id: session.provider_name.clone(),
                received_at: relative_audio_event_time(session.response.started_at_ms),
                response_id: Some(session.response.response_id.clone()),
                text: Some(preamble.clone()),
                status: None,
                audio_marker: None,
            }),
        );
    }
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
            exchange_index,
            event_kind: ProviderEventKind::ResponseStarted,
            provider_id: session.provider_name.clone(),
            received_at: relative_audio_event_time(session.response.started_at_ms),
            response_id: Some(session.response.response_id.clone()),
            text: None,
            status: None,
            audio_marker: Some("speech-playback".to_string()),
        }),
    );

    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::OutputProduced(ExchangeOutput {
            response_id: Some(session.response.response_id.clone()),
            text: session.response.text.clone(),
            produced_at: SystemTime::now(),
            provider_name: Some(session.provider_name.clone()),
            target: Some("speech-playback".to_string()),
            audio_marker: Some(format!(
                "audio_output_bytes={}",
                session.response.audio_output_bytes
            )),
        }),
    );

    for tool_call in &session.tool_calls {
        crate::session::apply_live_session_event(
            state,
            LiveSessionEvent::ToolRequested(ToolRequestRecord {
                exchange_index,
                call_id: tool_call.call_id.clone(),
                tool_name: tool_call.name.clone(),
                arguments_summary: tool_call.arguments_summary.clone(),
                requested_at: relative_audio_event_time(tool_call.requested_at_ms),
                source: "realtime_provider".to_string(),
                routed_to: Some("qsf_tool_permission_boundary".to_string()),
                auto_executed: false,
            }),
        );
    }

    for interruption in &session.interruptions {
        crate::session::apply_live_session_event(
            state,
            LiveSessionEvent::UserInterrupted(InterruptionRecord {
                exchange_index,
                response_id: interruption.response_id.clone(),
                detected_at: relative_audio_event_time(interruption.detected_at_ms),
                source: interruption.source.clone(),
                action: realtime_interruption_action(interruption.action),
                stop_outcome: realtime_interruption_outcome(interruption.action),
                partial_response_text: None,
            }),
        );
    }

    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::ProviderEventRecorded(ProviderEventRecord {
            exchange_index,
            event_kind: ProviderEventKind::ResponseCompleted,
            provider_id: session.provider_name.clone(),
            received_at: relative_audio_event_time(session.response.completed_at_ms),
            response_id: Some(session.response.response_id.clone()),
            text: Some(session.response.text.clone()),
            status: Some(session.response.status.clone()),
            audio_marker: Some(format!(
                "audio_output_bytes={}",
                session.response.audio_output_bytes
            )),
        }),
    );
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::ExchangeCompleted {
            completed_at: SystemTime::now(),
        },
    );
    let completed_exchange = state
        .live
        .completed_exchanges
        .last()
        .cloned()
        .context("completed realtime voice exchange missing after ExchangeCompleted")?;
    crate::session::apply_session_event(
        context,
        state,
        SessionEvent::ExchangeRecorded {
            session_id: state.session_id.clone(),
            exchange: Box::new(completed_exchange),
        },
    )?;
    crate::session::apply_live_session_event(
        state,
        LiveSessionEvent::SessionEnded {
            reason: SessionEndReason::Eof,
        },
    );
    crate::session::apply_session_event(
        context,
        state,
        SessionEvent::SessionEnded {
            reason: SessionEndReason::Eof,
        },
    )?;
    crate::session::persist_continuity_state_from_dirs(
        state,
        &state_resolution.resume_state_dir,
        &state_resolution.persist_state_dir,
        resume_manifest,
    )?;

    engine_logging::engine_info!(
        "realtime voice session bridged into shared state: experiment_id={} run_id={} session_id={} exchange_index={} state_dir={} provider={} response_id={} tool_requests={} interruptions={}",
        context.experiment_id(),
        context.run_id(),
        state.session_id,
        exchange_index,
        state_resolution.persist_state_dir.display(),
        session.provider_name,
        session.response.response_id,
        session.tool_calls.len(),
        session.interruptions.len()
    );

    Ok(())
}

fn realtime_interruption_action(action: RealtimeInterruptionAction) -> InterruptionAction {
    match action {
        RealtimeInterruptionAction::RecordedWithoutBypassingRuntime => {
            InterruptionAction::MarkInterrupted
        }
    }
}

fn realtime_interruption_outcome(action: RealtimeInterruptionAction) -> InterruptionStopOutcome {
    match action {
        RealtimeInterruptionAction::RecordedWithoutBypassingRuntime => {
            InterruptionStopOutcome::Ignored
        }
    }
}

fn voice_runtime_boundary() -> crate::audio::AudioRuntimeBoundary {
    transcript_provider_to_input_boundary(
        "Realtime session providers emit transcript and response lifecycle facts, but finalized user text still enters QSF as InputReceived.",
    )
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
    record_sanitized_failure(
        context,
        SanitizedFailure {
            event_type: EventType::RealtimeSessionFailed,
            operation: "realtime-session-provider",
            output_summary: "realtime voice session failed before completion",
            latency_stage: "realtime-voice-session",
            provider: error.provider(),
            error_category: error.category(),
            sanitized_error: &sanitized_message,
            extra_payload: serde_json::Map::new(),
        },
    )
}

fn write_realtime_voice_report(
    context: &RunContext,
    session: &VoiceProviderSession,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Realtime Voice Session MVP\n\n");
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
        let persisted_state = crate::session::persistence::load_session_state(
            context.run_dir().join("state/session/session-state.json"),
        )
        .unwrap();
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
        assert!(report.contains("Realtime Voice Session MVP"));
        assert!(report.contains("Provider tool calls auto-executed: `false`"));
        assert!(persisted_state.turns.is_empty());
        assert_eq!(persisted_state.exchanges.len(), 1);
        let exchange = &persisted_state.exchanges[0];
        assert_eq!(
            exchange.final_user_input(),
            "Can the voice session stay inside the QSF runtime boundaries?"
        );
        assert_eq!(exchange.interruptions.len(), 1);
        assert_eq!(exchange.status, crate::session::ExchangeStatus::Completed);
        assert!(
            exchange
                .started_at
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                > 1_700_000_000
        );
        assert_eq!(exchange.tool_requests.len(), 1);
        assert!(!exchange.tool_requests[0].auto_executed);
        assert_eq!(
            exchange.interruptions[0].action,
            crate::session::InterruptionAction::MarkInterrupted
        );
        assert_eq!(
            exchange.interruptions[0].stop_outcome,
            crate::session::InterruptionStopOutcome::Ignored
        );
        assert!(exchange.interruptions[0].partial_response_text.is_none());
        assert!(
            exchange
                .provider_events
                .iter()
                .any(|event| event.event_kind == crate::session::ProviderEventKind::Preamble)
        );
        assert!(
            exchange
                .provider_events
                .iter()
                .any(|event| event.event_kind
                    == crate::session::ProviderEventKind::ResponseCompleted)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn provider_tool_request_is_persisted_without_runtime_side_effects() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-voice-tool-boundary-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "realtime-voice-session").unwrap();
        let experiment = RealtimeVoiceSessionExperiment;

        experiment
            .run_with_provider(&mut context, &SimulatedRealtimeSessionProvider)
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let event_records = parse_event_records(&events);
        let persisted_state = crate::session::persistence::load_session_state(
            context.run_dir().join("state/session/session-state.json"),
        )
        .unwrap();

        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::ToolRequested
                && record.payload["source"] == "realtime_provider"
                && record.payload["auto_executed"] == false
        }));
        assert!(!event_records.iter().any(|record| matches!(
            record.event_type,
            EventType::ToolCompleted | EventType::ToolFailed
        )));
        assert!(persisted_state.turns.is_empty());
        assert_eq!(persisted_state.exchanges.len(), 1);
        assert_eq!(persisted_state.exchanges[0].tool_requests.len(), 1);
        assert!(!persisted_state.exchanges[0].tool_requests[0].auto_executed);

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
