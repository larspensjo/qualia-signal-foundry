use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::audio::{
    AudioRuntimeBoundary, AudioRuntimeEntryPoint, AudioSafetyMarkers, SpeechOutputProvider,
    SpeechOutputProviderError, SpeechOutputRequest, SpeechOutputSession, TranscriptEventEmission,
    TranscriptEventTraceIds, TranscriptProvider, TranscriptProviderError,
    TranscriptProviderRequest, TranscriptProviderSession, build_speech_output_provider,
    build_transcript_provider, record_transcript_runtime_events,
    requested_speech_output_provider_from_env, requested_transcript_provider_from_env,
};
use crate::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextSourceKind, assemble_context,
};
use crate::models::{
    ModelClient, ModelMessage, ModelRequest, ModelResponse, ModelRole, ModelRoleId, build_client,
    invoke_model_role, requested_provider_from_env,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct TextOwnedVoiceLoopExperiment;

impl Experiment for TextOwnedVoiceLoopExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::TextOwnedVoiceLoop
    }

    fn description(&self) -> &'static str {
        "Capture or simulate speech, route finalized text through QSF-owned model behavior, then synthesize speech output from the QSF text response"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let transcript_provider =
            build_transcript_provider(requested_transcript_provider_from_env());
        let model_client = build_client(requested_provider_from_env())?;
        let speech_provider =
            build_speech_output_provider(requested_speech_output_provider_from_env());

        self.run_with_components(
            context,
            transcript_provider.as_ref(),
            model_client.as_ref(),
            speech_provider.as_ref(),
        )
    }
}

impl TextOwnedVoiceLoopExperiment {
    fn run_with_components(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        let session_id = format!("{}-text-owned-voice-loop", context.run_id());
        let transcript_request = TranscriptProviderRequest::from_env(&session_id);
        let transcript_session = match transcript_provider.transcribe(&transcript_request) {
            Ok(session) => session,
            Err(error) => {
                record_transcript_failure(context, &error)?;
                return Err(error).context("transcript provider failed");
            }
        };

        let provider_trace_id = record_transcript_provider_trace(context, &transcript_session)?;
        let input_bridge_trace_id = record_input_bridge_trace(context, &transcript_session)?;
        record_transcript_runtime_events(
            context,
            &transcript_session,
            TranscriptEventTraceIds {
                provider_trace_id,
                latency_trace_id: provider_trace_id,
                runtime_bridge_trace_id: input_bridge_trace_id,
            },
            TranscriptEventEmission::new(transcript_to_input_boundary(), "transcription"),
        )?;

        let context_assembly =
            assemble_voice_context(&transcript_session.final_transcript.transcript);
        let context_trace_id =
            record_context_assembly(context, &transcript_session.session_id, &context_assembly)?;
        record_context_events(
            context,
            &transcript_session.session_id,
            &context_assembly,
            context_trace_id,
        )?;

        let model_request = build_conversational_request(
            &transcript_session.session_id,
            &transcript_session.final_transcript.transcript,
            &context_assembly,
        );
        let model_response = invoke_model_role(context, model_client, &model_request)?;
        let model_trace_id = record_voice_model_response(
            context,
            &transcript_session.session_id,
            &model_request,
            &model_response,
        )?;

        context.record_event(
            EventType::OutputProduced,
            json!({
                "session_id": &transcript_session.session_id,
                "source": "qsf_model_role",
                "role_id": model_response.role_id,
                "provider_name": &model_response.provider_name,
                "model_name": &model_response.model_name,
                "message": &model_response.output_text,
                "target": "speech-output-provider",
                "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
            }),
            Some(model_trace_id),
        )?;

        let speech_request = SpeechOutputRequest::from_env(
            &transcript_session.session_id,
            &model_response.output_text,
        );
        context.record_event(
            EventType::SpeechPlaybackRequested,
            json!({
                "session_id": &transcript_session.session_id,
                "provider": speech_provider.provider_name(),
                "model": &speech_request.model,
                "voice": &speech_request.voice,
                "output_mode": speech_request.output_mode,
                "text": &speech_request.text,
                "entry_point": AudioRuntimeEntryPoint::RuntimeOutput,
                "entry_point_description": AudioRuntimeEntryPoint::RuntimeOutput.description(),
                "boundary": speech_output_boundary(),
                "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
            }),
            Some(model_trace_id),
        )?;

        let speech_session = match speech_provider.synthesize(&speech_request) {
            Ok(session) => session,
            Err(error) => {
                record_speech_failure(context, &transcript_session.session_id, &error)?;
                return Err(error).context("speech output provider failed");
            }
        };
        let speech_trace_id = record_speech_output(context, &speech_request, &speech_session)?;
        record_speech_events(context, &speech_session, speech_trace_id)?;

        let latency_trace_id = record_voice_loop_latency(
            context,
            &transcript_session,
            &speech_session,
            context_assembly.used_estimated_tokens,
        )?;
        record_latency_event(
            context,
            &transcript_session,
            &speech_session,
            latency_trace_id,
        )?;

        write_text_owned_voice_loop_report(
            context,
            &transcript_session,
            &context_assembly,
            &model_response,
            &speech_session,
        )?;

        Ok(ExperimentOutcome {
            summary: "The deterministic text-owned voice loop now turns simulated speech into AudioFinalTranscript, bridges it to InputReceived, assembles QSF context, invokes the ConversationalResponder model role, emits OutputProduced, and hands that exact text to a simulated speech output provider.".to_string(),
            observations: vec![
                "The answer text is produced by the QSF model-role path before speech output receives it.".to_string(),
                "The default speech output provider records metadata-only playback lifecycle events without persisting raw audio.".to_string(),
                "One voice-loop session id correlates transcript, context, model, speech output, and latency records.".to_string(),
            ],
            failure_modes: vec![
                "OpenAI speech output remains unavailable by design until the simulated exact-text boundary is proven.".to_string(),
                "Live microphone input remains an explicit transcript-provider configuration path and is not the default.".to_string(),
            ],
            follow_up_questions: vec![
                "Should the AudioFinalTranscript to InputReceived bridge become a shared dispatcher before more audio experiments reuse it?".to_string(),
                "What latency budget should ConversationalResponder target for live microphone turns?".to_string(),
            ],
            decision_candidates: vec![
                "Voice interfaces should adapt around QSF-owned text turns unless an experiment is explicitly provider-owned.".to_string(),
                "Speech output providers receive exactly OutputProduced text and do not alter response ownership.".to_string(),
            ],
            extra_artifacts: vec!["text-owned-voice-loop.md".to_string()],
        })
    }
}

fn record_transcript_provider_trace(
    context: &mut RunContext,
    session: &TranscriptProviderSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
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
        "session_id": &session.session_id,
        "provider": &session.provider_name,
        "model": &session.model,
        "input_source": &session.input_source,
        "partial_count": session.partial_revision_count(),
        "final_transcript_received_at_ms": session.final_transcript.received_at_ms,
        "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
    }))
    .with_latency_context("audio", "transcript-provider-session")
    .with_latency_ms(session.final_transcript_latency_ms());
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_input_bridge_trace(
    context: &mut RunContext,
    session: &TranscriptProviderSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-runtime-input-bridge",
        "AudioFinalTranscript emitted by transcript provider",
        "InputReceived emitted as the QSF-owned runtime input",
    )
    .with_details(json!({
        "session_id": &session.session_id,
        "boundary": transcript_to_input_boundary(),
        "final_transcript_length": session.final_transcript_text_length(),
    }))
    .with_latency_context("runtime", "voice-runtime-input-bridge")
    .with_latency_ms(
        session
            .completed_at_ms
            .saturating_sub(session.final_transcript.received_at_ms),
    );
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn assemble_voice_context(final_transcript: &str) -> ContextAssembly {
    let fragments = vec![
        ContextFragment {
            fragment_id: "voice-loop-runtime-boundary".to_string(),
            source_kind: ContextSourceKind::RuntimeState,
            summary: "AudioFinalTranscript is the commit point; only finalized speech becomes InputReceived.".to_string(),
            tags: vec!["audio".to_string(), "runtime".to_string()],
            score: 1.0,
            estimated_tokens: 70,
            source_reference: "runtime/audio-loop".to_string(),
            selection_reason: "required to answer through the QSF-owned runtime boundary"
                .to_string(),
        },
        ContextFragment {
            fragment_id: "voice-loop-output-boundary".to_string(),
            source_kind: ContextSourceKind::RuntimeState,
            summary: "OutputProduced must exist before speech output providers receive text.".to_string(),
            tags: vec!["audio".to_string(), "speech-output".to_string()],
            score: 0.94,
            estimated_tokens: 58,
            source_reference: "runtime/audio-loop".to_string(),
            selection_reason: "keeps response ownership separate from speech rendering"
                .to_string(),
        },
        ContextFragment {
            fragment_id: "voice-loop-user-turn".to_string(),
            source_kind: ContextSourceKind::ProjectFrame,
            summary: format!("Current finalized spoken input: {final_transcript}"),
            tags: vec!["current-turn".to_string()],
            score: 0.9,
            estimated_tokens: 52,
            source_reference: "audio-final-transcript".to_string(),
            selection_reason: "current turn input anchors the spoken response".to_string(),
        },
    ];

    assemble_context(fragments, ContextBudget::new(4, 600))
}

fn record_context_assembly(
    context: &mut RunContext,
    session_id: &str,
    assembly: &ContextAssembly,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-context-assembly",
        "final transcript plus QSF voice-loop context candidates",
        format!(
            "selected {} fragments and omitted {}",
            assembly.selected.len(),
            assembly.omitted.len()
        ),
    )
    .with_details(json!({
        "session_id": session_id,
        "assembly": assembly,
    }))
    .with_latency_context("runtime", "voice-context-assembly")
    .with_latency_ms(6);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_context_events(
    context: &mut RunContext,
    session_id: &str,
    assembly: &ContextAssembly,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::ContextAssemblyRequested,
        json!({
            "session_id": session_id,
            "source_event": EventType::InputReceived,
            "budget": &assembly.budget,
        }),
        Some(trace_id),
    )?;

    context.record_event(
        EventType::ContextAssembled,
        json!({
            "session_id": session_id,
            "selected_count": assembly.selected.len(),
            "omitted_count": assembly.omitted.len(),
            "used_estimated_tokens": assembly.used_estimated_tokens,
            "selected": &assembly.selected,
            "omitted": &assembly.omitted,
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn build_conversational_request(
    session_id: &str,
    final_transcript: &str,
    assembly: &ContextAssembly,
) -> ModelRequest {
    let context_summary = assembly
        .selected
        .iter()
        .map(|selection| format!("- {}", selection.fragment.summary))
        .collect::<Vec<_>>()
        .join("\n");

    ModelRequest::new(
        ModelRole::predefined(ModelRoleId::ConversationalResponder),
        vec![
            ModelMessage::system(
                "Answer as a short spoken QSF-owned response. Do not claim that the speech provider generated the answer.",
            ),
            ModelMessage::user(format!(
                "Final transcript:\n{final_transcript}\n\nSelected context:\n{context_summary}"
            )),
        ],
    )
    .with_session_id(session_id)
    .with_temperature(0.0)
    .with_max_output_tokens(120)
}

fn record_voice_model_response(
    context: &mut RunContext,
    session_id: &str,
    request: &ModelRequest,
    response: &ModelResponse,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-model-response-summary",
        format!(
            "role={} provider={}",
            request.role.role_id, response.provider_name
        ),
        response.output_summary(),
    )
    .with_details(json!({
        "session_id": session_id,
        "role_id": request.role.role_id,
        "provider": &response.provider_name,
        "model": &response.model_name,
        "response_length": response.output_text.chars().count(),
        "usage": &response.usage,
        "finish_reason": &response.finish_reason,
    }))
    .with_latency_context("runtime", "voice-model-response-summary");
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_speech_output(
    context: &mut RunContext,
    request: &SpeechOutputRequest,
    session: &SpeechOutputSession,
) -> anyhow::Result<uuid::Uuid> {
    let trace = TraceRecord::new(
        context.experiment_id(),
        "speech-output-provider",
        format!(
            "provider={} voice={} output_mode={}",
            session.provider_name, session.voice, session.output_mode
        ),
        format!(
            "metadata recorded for {} chars of QSF-owned text",
            session.text_length
        ),
    )
    .with_details(json!({
        "session_id": &session.session_id,
        "speech_provider": &session.provider_name,
        "model": &session.model,
        "voice": &session.voice,
        "output_mode": session.output_mode,
        "audio_byte_count": session.audio_output_bytes,
        "playback_adapter": &session.playback_adapter,
        "playback_timing": session,
        "exact_text_received": request.text,
    }))
    .with_latency_context("audio", "speech-output-provider")
    .with_latency_ms(session.total_latency_ms());
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_speech_events(
    context: &mut RunContext,
    session: &SpeechOutputSession,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::SpeechPlaybackStarted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "adapter": &session.playback_adapter,
            "voice": &session.voice,
            "output_mode": session.output_mode,
            "started_at_ms": session.first_audio_at_ms,
            "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    context.record_event(
        EventType::SpeechPlaybackCompleted,
        json!({
            "session_id": &session.session_id,
            "provider": &session.provider_name,
            "adapter": &session.playback_adapter,
            "voice": &session.voice,
            "output_mode": session.output_mode,
            "completed_at_ms": session.completed_at_ms,
            "audio_output_bytes": session.audio_output_bytes,
            "entry_point": AudioRuntimeEntryPoint::SpeechPlaybackAdapter,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn record_voice_loop_latency(
    context: &mut RunContext,
    transcript_session: &TranscriptProviderSession,
    speech_session: &SpeechOutputSession,
    context_tokens: usize,
) -> anyhow::Result<uuid::Uuid> {
    let capture_completed_ms = transcript_session
        .chunks
        .last()
        .map(|chunk| chunk.captured_at_ms + chunk.duration_ms)
        .unwrap_or(transcript_session.completed_at_ms);
    let input_received_ms = transcript_session.completed_at_ms;
    let context_started_ms = input_received_ms;
    let context_completed_ms = context_started_ms + 6;
    let model_started_ms = context_completed_ms;
    let model_completed_ms = model_started_ms;
    let output_produced_ms = model_completed_ms;
    let speech_requested_ms = output_produced_ms;
    let speech_provider_first_audio_offset_ms = speech_session
        .first_audio_at_ms
        .unwrap_or(speech_session.completed_at_ms)
        .saturating_sub(speech_session.started_at_ms);
    let speech_started_ms = speech_requested_ms + speech_provider_first_audio_offset_ms;
    let speech_completed_ms = speech_requested_ms + speech_session.total_latency_ms();

    let trace = TraceRecord::new(
        context.experiment_id(),
        "voice-loop-latency",
        "deterministic text-owned voice turn timing",
        "capture, transcription, runtime, model, and speech output timings recorded",
    )
    .with_details(json!({
        "session_id": &transcript_session.session_id,
        "capture_started_ms": transcript_session.started_at_ms,
        "capture_completed_ms": capture_completed_ms,
        "first_partial_transcript_ms": transcript_session.partials.first().map(|partial| partial.received_at_ms),
        "final_transcript_ms": transcript_session.final_transcript.received_at_ms,
        "input_received_ms": input_received_ms,
        "context_started_ms": context_started_ms,
        "context_completed_ms": context_completed_ms,
        "model_started_ms": model_started_ms,
        "model_completed_ms": model_completed_ms,
        "output_produced_ms": output_produced_ms,
        "speech_requested_ms": speech_requested_ms,
        "speech_started_ms": speech_started_ms,
        "speech_completed_ms": speech_completed_ms,
        "context_used_estimated_tokens": context_tokens,
        "transcript_measurements": transcript_session.latency_measurements(),
        "speech_measurements": speech_session.latency_measurements(),
        "total_turn_ms": speech_completed_ms.saturating_sub(transcript_session.started_at_ms),
    }))
    .with_latency_context("audio", "text-owned-voice-loop")
    .with_latency_ms(speech_completed_ms.saturating_sub(transcript_session.started_at_ms));
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    Ok(trace_id)
}

fn record_latency_event(
    context: &mut RunContext,
    transcript_session: &TranscriptProviderSession,
    speech_session: &SpeechOutputSession,
    trace_id: uuid::Uuid,
) -> anyhow::Result<()> {
    context.record_event(
        EventType::LatencyMeasurementRecorded,
        json!({
            "session_id": &transcript_session.session_id,
            "domain": "audio",
            "stage": "text-owned-voice-loop",
            "transcript_measurements": transcript_session.latency_measurements(),
            "speech_measurements": speech_session.latency_measurements(),
            "first_partial_latency_ms": transcript_session.first_partial_latency_ms(),
            "final_transcript_latency_ms": transcript_session.final_transcript_latency_ms(),
            "speech_output_latency_ms": speech_session.total_latency_ms(),
            "total_turn_ms": speech_session.completed_at_ms.saturating_sub(transcript_session.started_at_ms),
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;

    Ok(())
}

fn record_transcript_failure(
    context: &mut RunContext,
    error: &TranscriptProviderError,
) -> anyhow::Result<()> {
    let sanitized_message = error.sanitized_message();
    let trace = TraceRecord::new(
        context.experiment_id(),
        "transcript-provider-session",
        format!("provider={}", error.provider()),
        "transcription failed before text-owned voice loop input",
    )
    .with_latency_context("audio", "text-owned-voice-loop")
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

fn record_speech_failure(
    context: &mut RunContext,
    session_id: &str,
    error: &SpeechOutputProviderError,
) -> anyhow::Result<()> {
    let sanitized_message = error.sanitized_message();
    let trace = TraceRecord::new(
        context.experiment_id(),
        "speech-output-provider",
        format!("provider={}", error.provider()),
        "speech output failed after OutputProduced",
    )
    .with_latency_context("audio", "speech-output-provider")
    .with_error(&sanitized_message);
    let trace_id = trace.trace_id;
    context.record_trace(trace)?;
    context.record_event(
        EventType::ErrorOccurred,
        json!({
            "session_id": session_id,
            "operation": "speech-output-provider",
            "provider": error.provider(),
            "error_category": error.category(),
            "sanitized_error": sanitized_message,
            "safety": AudioSafetyMarkers::no_secret_or_raw_audio(),
        }),
        Some(trace_id),
    )?;
    Ok(())
}

fn transcript_to_input_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::TranscriptProvider,
        producer_event: EventType::AudioFinalTranscript,
        runtime_event: EventType::InputReceived,
        description:
            "Final transcript text becomes QSF-owned input only after AudioFinalTranscript."
                .to_string(),
    }
}

fn speech_output_boundary() -> AudioRuntimeBoundary {
    AudioRuntimeBoundary {
        entry_point: AudioRuntimeEntryPoint::RuntimeOutput,
        producer_event: EventType::OutputProduced,
        runtime_event: EventType::SpeechPlaybackRequested,
        description:
            "QSF-owned OutputProduced text is handed unchanged to the speech output provider."
                .to_string(),
    }
}

fn write_text_owned_voice_loop_report(
    context: &RunContext,
    transcript_session: &TranscriptProviderSession,
    context_assembly: &ContextAssembly,
    model_response: &ModelResponse,
    speech_session: &SpeechOutputSession,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Text-Owned Voice Loop\n\n");
    markdown.push_str("## Turn\n\n");
    markdown.push_str(&format!(
        "- Session id: `{}`\n",
        transcript_session.session_id
    ));
    markdown.push_str(&format!(
        "- Transcript provider: `{}`\n",
        transcript_session.provider_name
    ));
    markdown.push_str(&format!(
        "- Final transcript: {}\n",
        transcript_session.final_transcript.transcript
    ));
    markdown.push_str(&format!(
        "- Context fragments selected: `{}`\n",
        context_assembly.selected.len()
    ));
    markdown.push_str(&format!(
        "- Model role: `{}` via `{}`\n",
        model_response.role_id, model_response.provider_name
    ));
    markdown.push_str(&format!(
        "- OutputProduced text: {}\n",
        model_response.output_text
    ));
    markdown.push_str(&format!(
        "- Speech output provider: `{}`\n",
        speech_session.provider_name
    ));
    markdown.push_str(&format!(
        "- Speech output mode: `{}`\n",
        speech_session.output_mode
    ));
    markdown.push_str("- Raw audio logged: `false`\n\n");

    markdown.push_str("## Latency\n\n");
    markdown.push_str(&format!(
        "- Final transcript latency: {} ms\n",
        transcript_session.final_transcript_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Speech output latency: {} ms\n",
        speech_session.total_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Total deterministic turn latency: {} ms\n",
        speech_session
            .completed_at_ms
            .saturating_sub(transcript_session.started_at_ms)
    ));

    fs::write(context.run_dir().join("text-owned-voice-loop.md"), markdown).with_context(|| {
        format!(
            "failed to write text-owned voice loop report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::TextOwnedVoiceLoopExperiment;
    use crate::audio::test_support::{
        assert_events_have_safety_markers, assert_payloads_do_not_contain_raw_audio_fields,
        is_audio_or_speech_event, parse_event_records,
    };
    use crate::audio::{
        SimulatedSpeechOutputProvider, SimulatedTranscriptProvider, SpeechOutputProvider,
        SpeechOutputProviderError, SpeechOutputRequest, SpeechOutputSession,
    };
    use crate::experiments::registry::{Experiment, ExperimentName};
    use crate::models::{MockModelClient, ModelClient, ModelRequest, ModelResponse};
    use crate::observability::event_log::{EventRecord, EventType};
    use crate::runtime::run_context::RunContext;
    use anyhow::anyhow;
    use uuid::Uuid;

    struct FailingModelClient;

    impl ModelClient for FailingModelClient {
        fn client_name(&self) -> &str {
            "failing-model"
        }

        fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
            Err(anyhow!("model boom"))
        }
    }

    struct FailingSpeechOutputProvider;

    impl SpeechOutputProvider for FailingSpeechOutputProvider {
        fn provider_name(&self) -> &str {
            "failing-speech-output-provider"
        }

        fn synthesize(
            &self,
            _request: &SpeechOutputRequest,
        ) -> Result<SpeechOutputSession, SpeechOutputProviderError> {
            Err(SpeechOutputProviderError::SynthesisFailed {
                provider: self.provider_name().to_string(),
                message: "upstream returned Authorization: Bearer sk-test-secret".to_string(),
            })
        }
    }

    #[test]
    fn text_owned_voice_loop_writes_deterministic_turn_events_and_traces() {
        let base_dir = std::env::temp_dir().join(format!("qsf-text-owned-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &MockModelClient::default(),
                &SimulatedSpeechOutputProvider,
            )
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let report =
            fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
        let event_records = parse_event_records(&events);

        assert_event_order(
            &event_records,
            &[
                EventType::AudioInputStarted,
                EventType::AudioInputChunkCaptured,
                EventType::AudioPartialTranscript,
                EventType::AudioFinalTranscript,
                EventType::AudioInputEnded,
                EventType::LatencyMeasurementRecorded,
                EventType::InputReceived,
                EventType::ContextAssemblyRequested,
                EventType::ContextAssembled,
                EventType::ModelRoleRequested,
                EventType::ModelRoleCompleted,
                EventType::OutputProduced,
                EventType::SpeechPlaybackRequested,
                EventType::SpeechPlaybackStarted,
                EventType::SpeechPlaybackCompleted,
                EventType::LatencyMeasurementRecorded,
            ],
        );
        assert_only_final_transcript_commits_runtime_input(&event_records);
        assert_one_session_id_links_voice_loop_events(&event_records);
        assert_output_text_is_handed_exactly_to_speech_provider(&event_records);
        assert_events_have_safety_markers(&event_records, is_audio_or_speech_event);
        assert_payloads_do_not_contain_raw_audio_fields(&event_records);
        assert!(traces.contains("transcript-provider-session"));
        assert!(traces.contains("voice-runtime-input-bridge"));
        assert!(traces.contains("voice-context-assembly"));
        assert!(traces.contains("voice-model-response"));
        assert!(traces.contains("speech-output-provider"));
        assert!(traces.contains("voice-loop-latency"));
        assert!(report.contains("Text-Owned Voice Loop"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn model_failure_records_failure_and_does_not_emit_output() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-model-fail-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        let error = experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &FailingModelClient,
                &SimulatedSpeechOutputProvider,
            )
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert!(error.to_string().contains("conversational_responder"));
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::ModelRoleFailed)
        );
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::OutputProduced)
        );
        assert!(
            !event_records
                .iter()
                .any(|record| record.event_type == EventType::SpeechPlaybackRequested)
        );

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn speech_failure_is_sanitized_after_output_produced() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-text-owned-speech-fail-{}", Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
        let experiment = TextOwnedVoiceLoopExperiment;

        let error = experiment
            .run_with_components(
                &mut context,
                &SimulatedTranscriptProvider,
                &MockModelClient::default(),
                &FailingSpeechOutputProvider,
            )
            .unwrap_err();
        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();
        let event_records = parse_event_records(&events);

        assert_eq!(error.to_string(), "speech output provider failed");
        assert!(
            event_records
                .iter()
                .any(|record| record.event_type == EventType::OutputProduced)
        );
        assert!(event_records.iter().any(|record| {
            record.event_type == EventType::ErrorOccurred
                && record.payload["error_category"] == "synthesis_failed"
                && record.payload["sanitized_error"]
                    == "provider error redacted because it may contain credential-like content"
        }));
        assert!(!events.contains("sk-test-secret"));
        assert!(!traces.contains("sk-test-secret"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn experiment_name_matches_registry_id() {
        let experiment = TextOwnedVoiceLoopExperiment;

        assert_eq!(experiment.name(), ExperimentName::TextOwnedVoiceLoop);
        assert_eq!(experiment.id(), "text-owned-voice-loop");
    }

    fn assert_event_order(records: &[EventRecord], expected_order: &[EventType]) {
        let mut next_index = 0usize;

        for record in records {
            if next_index < expected_order.len() && record.event_type == expected_order[next_index]
            {
                next_index += 1;
            }
        }

        assert_eq!(next_index, expected_order.len());
    }

    fn assert_only_final_transcript_commits_runtime_input(records: &[EventRecord]) {
        assert!(records.iter().any(|record| {
            record.event_type == EventType::AudioPartialTranscript
                && record.payload["committed_to_runtime"] == false
        }));
        let input_count = records
            .iter()
            .filter(|record| record.event_type == EventType::InputReceived)
            .count();
        assert_eq!(input_count, 1);
    }

    fn assert_one_session_id_links_voice_loop_events(records: &[EventRecord]) {
        let session_ids = records
            .iter()
            .filter(|record| {
                matches!(
                    record.event_type,
                    EventType::AudioFinalTranscript
                        | EventType::InputReceived
                        | EventType::ModelRoleRequested
                        | EventType::ModelRoleCompleted
                        | EventType::OutputProduced
                        | EventType::SpeechPlaybackRequested
                        | EventType::SpeechPlaybackCompleted
                )
            })
            .map(|record| record.payload["session_id"].as_str().unwrap().to_string())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(session_ids.len(), 1);
    }

    fn assert_output_text_is_handed_exactly_to_speech_provider(records: &[EventRecord]) {
        let output_text = records
            .iter()
            .find(|record| record.event_type == EventType::OutputProduced)
            .unwrap()
            .payload["message"]
            .as_str()
            .unwrap();
        let speech_text = records
            .iter()
            .find(|record| record.event_type == EventType::SpeechPlaybackRequested)
            .unwrap()
            .payload["text"]
            .as_str()
            .unwrap();

        assert_eq!(output_text, speech_text);
    }
}
