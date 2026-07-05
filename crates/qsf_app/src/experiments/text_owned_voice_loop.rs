use std::time::{Instant, SystemTime};

use anyhow::Context;
use serde_json::json;

use crate::audio::{
    AudioRuntimeEntryPoint, AudioSafetyMarkers, SpeechOutputProvider, SpeechOutputRequest,
    TranscriptProvider, build_speech_output_provider, build_transcript_provider,
    requested_speech_output_provider_from_env, requested_transcript_provider_from_env,
};
use crate::conversation::prompt;
use crate::memory::RetrievalStrategy;
use crate::models::invoke_model_role;
use crate::observability::event_log::EventType;
use crate::observability::trace::elapsed_ms;
use crate::runtime::run_context::RunContext;
use crate::session::{
    ExchangeModelUse, ExchangeOutput, LiveSessionEvent, SessionBootRequest, SessionConfig,
    SessionEndReason, SessionEvent, StateDirectoryResolution, Turn,
};
use qsf_models::{ModelClient, build_client, requested_provider_from_env};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

mod memory_source;
mod report;
mod request_building;
#[cfg(test)]
mod tests;
mod trace_recording;
mod transcript_ingestion;

#[cfg(test)]
pub(crate) use memory_source::SharedVoiceMemorySource;
#[cfg(test)]
use memory_source::{FileVoiceMemorySource, PhaseFourVoiceMemorySource};
use memory_source::{VoiceLoopMemorySource, build_voice_memory_source_from_env};
use report::{
    VoiceLoopReport, VoiceLoopReportTiming, write_text_owned_voice_loop_report,
    write_voice_memory_source_snapshot,
};
use request_building::{
    assemble_voice_context, build_conversational_request, retrieved_memory_block_with_boot_brief,
};
use trace_recording::{
    record_context_assembly, record_context_events, record_latency_event, record_speech_events,
    record_speech_failure, record_speech_output, record_voice_loop_latency,
    record_voice_model_response, retrieve_voice_memories, speech_output_boundary,
};
use transcript_ingestion::ingest_transcript_to_input;

const VOICE_CONTEXT_ASSEMBLY_LATENCY_MS: u64 = 6;
const VOICE_MEMORY_RETRIEVAL_LIMIT: usize = 1;
const VOICE_MEMORY_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::KeywordTag;

pub struct TextOwnedVoiceLoopExperiment;

pub(crate) struct VoiceLoopSessionConfig {
    pub(crate) state_resolution: StateDirectoryResolution,
    pub(crate) config: SessionConfig,
}

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
        let state_resolution = crate::session::resolve_shared_state_directory_from_env();
        let memory_source = build_voice_memory_source_from_env(&state_resolution.resume_state_dir)?;

        self.run_with_components_and_memory_source_at_state_dirs(
            context,
            transcript_provider.as_ref(),
            model_client.as_ref(),
            speech_provider.as_ref(),
            memory_source.as_ref(),
            state_resolution,
        )
    }
}

impl TextOwnedVoiceLoopExperiment {
    #[cfg(test)]
    fn run_with_components(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
    ) -> anyhow::Result<ExperimentOutcome> {
        let state_dir = context.run_dir().join("state/session");
        let memory_source = SharedVoiceMemorySource::new(&state_dir);
        self.run_with_components_and_memory_source(
            context,
            transcript_provider,
            model_client,
            speech_provider,
            &memory_source,
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn run_with_components_and_memory_source(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
        memory_source: &dyn VoiceLoopMemorySource,
    ) -> anyhow::Result<ExperimentOutcome> {
        let state_dir = context.run_dir().join("state/session");
        self.run_with_components_and_memory_source_at_state_dirs(
            context,
            transcript_provider,
            model_client,
            speech_provider,
            memory_source,
            StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir,
                legacy_fallback_used: false,
            },
        )
    }

    pub(crate) fn run_with_components_and_memory_source_at_state_dirs(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
        memory_source: &dyn VoiceLoopMemorySource,
        state_resolution: StateDirectoryResolution,
    ) -> anyhow::Result<ExperimentOutcome> {
        let config = crate::session::config::session_config_from_env();
        self.run_with_components_and_memory_source_at_state_dirs_with_config(
            context,
            transcript_provider,
            model_client,
            speech_provider,
            memory_source,
            VoiceLoopSessionConfig {
                state_resolution,
                config,
            },
        )
    }

    pub(crate) fn run_with_components_and_memory_source_at_state_dirs_with_config(
        &self,
        context: &mut RunContext,
        transcript_provider: &dyn TranscriptProvider,
        model_client: &dyn ModelClient,
        speech_provider: &dyn SpeechOutputProvider,
        memory_source: &dyn VoiceLoopMemorySource,
        session_config: VoiceLoopSessionConfig,
    ) -> anyhow::Result<ExperimentOutcome> {
        let VoiceLoopSessionConfig {
            state_resolution,
            config,
        } = session_config;
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
        let boot_brief_fragment = if state.turns.is_empty() {
            boot.pending_boot_brief
                .as_ref()
                .map(crate::session::format_boot_brief_for_context)
        } else {
            None
        };

        let transcript_ingestion::TranscriptIngestionOutcome {
            transcript_session,
            final_transcript,
            exchange_index,
        } = ingest_transcript_to_input(context, &mut state, transcript_provider)?;

        let memory_snapshot = memory_source.load()?;
        write_voice_memory_source_snapshot(context, &memory_snapshot)?;
        let memory_retrieval = retrieve_voice_memories(
            context,
            &state.session_id,
            &final_transcript,
            &memory_snapshot,
        )?;

        let context_assembly =
            assemble_voice_context(&final_transcript, &memory_retrieval.selected);
        let context_trace_id =
            record_context_assembly(context, &state.session_id, &context_assembly)?;
        record_context_events(
            context,
            &state.session_id,
            &context_assembly,
            context_trace_id,
        )?;
        let retrieved_memory_block = retrieved_memory_block_with_boot_brief(
            &context_assembly,
            boot_brief_fragment.as_deref(),
        );
        crate::session::apply_live_session_event(
            &mut state,
            LiveSessionEvent::MemoryContextRecorded {
                exchange_index,
                context_assembly: context_assembly.clone(),
                retrieved_memory_block: retrieved_memory_block.clone(),
                recalled_items: vec![],
                live_capture: None,
            },
        );

        let model_request = build_conversational_request(
            &state.session_id,
            &final_transcript,
            &context_assembly,
            &retrieved_memory_block,
        );
        let model_started_at = Instant::now();
        let model_response = match invoke_model_role(context, model_client, &model_request) {
            Ok(response) => response,
            Err(error) => {
                crate::session::apply_session_event(
                    context,
                    &mut state,
                    SessionEvent::ModelRoleFailed {
                        error_summary: error.to_string(),
                    },
                )?;
                crate::session::apply_live_session_event(
                    &mut state,
                    LiveSessionEvent::ModelRoleFailed {
                        error_summary: error.to_string(),
                    },
                );
                return Err(error);
            }
        };
        let model_latency_ms = elapsed_ms(model_started_at);
        let model_trace_id = record_voice_model_response(
            context,
            &state.session_id,
            &model_request,
            &model_response,
            model_latency_ms,
        )?;
        let prompt_assembly = prompt::prompt_assembly_from_messages(model_request.messages.clone());
        crate::session::apply_session_event(
            context,
            &mut state,
            SessionEvent::ModelRoleCompleted {
                response: model_response.output_text.clone(),
                latency_ms: model_latency_ms,
                input_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.input_tokens)
                    .unwrap_or(0),
                cached_input_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.cached_input_tokens)
                    .unwrap_or(0),
                output_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.output_tokens)
                    .unwrap_or(0),
            },
        )?;
        crate::session::apply_live_session_event(
            &mut state,
            LiveSessionEvent::ModelRoleCompleted(ExchangeModelUse {
                provider_name: Some(model_response.provider_name.clone()),
                model_id: model_response.model_name.clone(),
                latency_ms: model_latency_ms,
                input_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.input_tokens)
                    .unwrap_or(0),
                cached_input_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.cached_input_tokens)
                    .unwrap_or(0),
                output_tokens: model_response
                    .usage
                    .as_ref()
                    .map(|usage| usage.output_tokens)
                    .unwrap_or(0),
                full_request_hash: prompt_assembly.full_request_hash,
                message_count: prompt_assembly.message_count,
            }),
        );

        context.record_event(
            EventType::OutputProduced,
            json!({
                "session_id": &state.session_id,
                "exchange_index": exchange_index,
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
        engine_logging::engine_info!(
            "text-owned voice response produced: experiment_id={} run_id={} session_id={}",
            context.experiment_id(),
            context.run_id(),
            state.session_id
        );
        crate::session::apply_live_session_event(
            &mut state,
            LiveSessionEvent::OutputProduced(ExchangeOutput {
                response_id: Some(format!("voice-response-{exchange_index}")),
                text: model_response.output_text.clone(),
                produced_at: SystemTime::now(),
                provider_name: Some(model_response.provider_name.clone()),
                target: Some("speech-output-provider".to_string()),
                audio_marker: None,
            }),
        );
        crate::session::apply_live_memory_reinforcement(
            context,
            &state,
            &state_resolution.persist_state_dir,
            &memory_retrieval,
        )?;
        crate::session::apply_live_memory_capture(
            context,
            &state,
            &state_resolution.persist_state_dir,
            &final_transcript,
            &model_response.output_text,
        )?;

        let speech_request =
            SpeechOutputRequest::from_env(&state.session_id, &model_response.output_text);
        context.record_event(
            EventType::SpeechPlaybackRequested,
            json!({
                "session_id": &state.session_id,
                "exchange_index": exchange_index,
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
                record_speech_failure(context, &state.session_id, &error)?;
                return Err(error).context("speech output provider failed");
            }
        };
        let speech_trace_id = record_speech_output(context, &speech_request, &speech_session)?;
        record_speech_events(context, &speech_session, speech_trace_id)?;

        let latency_trace_id = record_voice_loop_latency(
            context,
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            context_assembly.used_estimated_tokens,
            model_latency_ms,
        )?;
        record_latency_event(
            context,
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            model_latency_ms,
            latency_trace_id,
        )?;

        let exchange_index = state
            .live
            .active_exchange
            .as_ref()
            .map(|exchange| exchange.index)
            .context("active exchange missing before ExchangeCompleted")?;
        crate::session::apply_live_session_event(
            &mut state,
            LiveSessionEvent::ExchangeCompleted {
                exchange_index,
                completed_at: SystemTime::now(),
            },
        );
        let completed_exchange = state
            .live
            .completed_exchanges
            .last()
            .cloned()
            .context("completed voice exchange missing after ExchangeCompleted")?;
        let turn = Turn::try_from(&completed_exchange).with_context(|| {
            format!(
                "failed to convert completed voice exchange {} into a turn",
                completed_exchange.index
            )
        })?;
        crate::session::apply_session_event(
            context,
            &mut state,
            SessionEvent::TurnCompleted(turn),
        )?;
        crate::session::ageing::age_out_warm_turns(
            context,
            &mut state,
            &state_resolution.persist_state_dir,
            model_client,
        )?;
        crate::session::apply_live_session_event(
            &mut state,
            LiveSessionEvent::SessionEnded {
                reason: SessionEndReason::Eof,
            },
        );
        crate::session::apply_session_event(
            context,
            &mut state,
            SessionEvent::SessionEnded {
                reason: SessionEndReason::Eof,
            },
        )?;
        crate::session::persist_continuity_state_from_dirs(
            &state,
            &state_resolution.resume_state_dir,
            &state_resolution.persist_state_dir,
            &resume_manifest,
        )?;

        let report_timing = VoiceLoopReportTiming::new(
            &transcript_session,
            &memory_retrieval,
            &speech_session,
            model_latency_ms,
        );
        write_text_owned_voice_loop_report(
            context,
            VoiceLoopReport {
                transcript_session: &transcript_session,
                context_assembly: &context_assembly,
                memory_snapshot: &memory_snapshot,
                model_response: &model_response,
                speech_request: &speech_request,
                speech_session: &speech_session,
                timing: report_timing,
            },
        )?;

        Ok(ExperimentOutcome {
            summary: "The deterministic text-owned voice loop now turns simulated speech into AudioFinalTranscript, bridges it to InputReceived, assembles QSF context, invokes the ConversationalResponder model role, emits OutputProduced, and hands that exact text to a simulated speech output provider.".to_string(),
            observations: vec![
                "The answer text is produced by the QSF model-role path before speech output receives it.".to_string(),
                "One retrieved memory fragment now participates in the selected context passed to ConversationalResponder.".to_string(),
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
            extra_artifacts: vec![
                "text-owned-voice-loop.md".to_string(),
                "voice-memory-source.json".to_string(),
                state_resolution
                    .persist_state_dir
                    .join("session-state.json")
                    .display()
                    .to_string(),
            ],
        })
    }
}
