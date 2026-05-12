use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::audio::SimulatedAudioSession;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct AudioPreparationLayerExperiment;

impl Experiment for AudioPreparationLayerExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::AudioPreparationLayer
    }

    fn description(&self) -> &'static str {
        "Simulate transcript and speech playback boundaries before real audio providers are added"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let session = SimulatedAudioSession::default();

        for prepared_event in session.input_observation_events() {
            context.record_event(prepared_event.event_type, prepared_event.payload, None)?;
        }

        let transcription_trace = TraceRecord::new(
            context.experiment_id(),
            "audio-transcription-boundary",
            format!(
                "provider={} partial transcript observed",
                session.transcript_provider
            ),
            "final transcript re-entered runtime as InputReceived",
        )
        .with_details(json!({
            "session": session,
            "boundary": session.transcription_boundary(),
            "measurements": session.transcription_latency_measurements(),
        }))
        .with_latency_context("audio", "final-transcription")
        .with_latency_ms(session.transcription_total_latency_ms());
        let transcription_trace_id = transcription_trace.trace_id;
        context.record_trace(transcription_trace)?;

        for prepared_event in session.runtime_input_events() {
            context.record_event(
                prepared_event.event_type,
                prepared_event.payload,
                Some(transcription_trace_id),
            )?;
        }

        let playback_trace = TraceRecord::new(
            context.experiment_id(),
            "audio-playback-boundary",
            "runtime output produced text destined for speech playback",
            "speech playback adapter emitted lifecycle events without bypassing runtime output",
        )
        .with_details(json!({
            "session": session,
            "boundary": session.playback_boundary(),
            "measurements": session.playback_latency_measurements(),
        }))
        .with_latency_context("audio", "speech-playback")
        .with_latency_ms(session.playback_total_latency_ms());
        let playback_trace_id = playback_trace.trace_id;
        context.record_trace(playback_trace)?;

        for prepared_event in session.playback_events() {
            context.record_event(
                prepared_event.event_type,
                prepared_event.payload,
                Some(playback_trace_id),
            )?;
        }

        write_audio_preparation_report(context, &session)?;

        Ok(ExperimentOutcome {
            summary: "Phase 8 audio preparation landed as a simulated transcript and playback boundary that emits typed audio events, records audio-specific latency metadata in traces, and shows exactly where future transcription and TTS adapters plug into the existing event flow.".to_string(),
            observations: vec![
                "Final transcript text re-enters the framework as the existing InputReceived event rather than a parallel audio-only input path.".to_string(),
                "Runtime output remains observable as OutputProduced before any speech adapter receives a SpeechPlaybackRequested event.".to_string(),
                "Audio latency now has explicit domain and stage fields in traces so capture, transcription, and playback timing can remain first-class without introducing real audio I/O yet.".to_string(),
            ],
            failure_modes: vec![
                "The current audio session is simulated and does not capture real microphone data or synthesize real speech.".to_string(),
                "Interruption handling remains deferred until a realtime provider experiment needs it.".to_string(),
            ],
            follow_up_questions: vec![
                "Should partial transcript events ever participate in context assembly, or remain observability-only by default?".to_string(),
                "Should future playback requests carry voice configuration in the event payload or behind adapter configuration?".to_string(),
            ],
            decision_candidates: vec![
                "Realtime transcript providers should emit AudioFinalTranscript before finalized text becomes InputReceived.".to_string(),
                "Speech playback adapters should accept text from OutputProduced through an explicit SpeechPlaybackRequested boundary rather than owning response generation.".to_string(),
            ],
            extra_artifacts: vec!["audio-preparation.md".to_string()],
        })
    }
}

fn write_audio_preparation_report(
    context: &RunContext,
    session: &SimulatedAudioSession,
) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Phase 8 Audio Preparation\n\n");
    markdown.push_str("## Simulated session\n\n");
    markdown.push_str(&format!("- Session id: `{}`\n", session.session_id));
    markdown.push_str(&format!(
        "- Transcript provider: `{}`\n",
        session.transcript_provider
    ));
    markdown.push_str(&format!(
        "- Playback adapter: `{}`\n",
        session.playback_adapter
    ));
    markdown.push_str(&format!(
        "- Final transcript: {}\n",
        session.final_transcript
    ));
    markdown.push_str(&format!(
        "- Runtime reply text: {}\n\n",
        session.response_text
    ));

    let transcription_boundary = session.transcription_boundary();
    let playback_boundary = session.playback_boundary();
    markdown.push_str("## Runtime boundaries\n\n");
    markdown.push_str(&format!(
        "- Transcription boundary: `{:?}` -> `{:?}`. {}\n",
        transcription_boundary.producer_event,
        transcription_boundary.runtime_event,
        transcription_boundary.description
    ));
    markdown.push_str(&format!(
        "- Playback boundary: `{:?}` -> `{:?}`. {}\n\n",
        playback_boundary.producer_event,
        playback_boundary.runtime_event,
        playback_boundary.description
    ));

    markdown.push_str("## Latency totals\n\n");
    markdown.push_str(&format!(
        "- Transcription path: {} ms\n",
        session.transcription_total_latency_ms()
    ));
    markdown.push_str(&format!(
        "- Playback path: {} ms\n",
        session.playback_total_latency_ms()
    ));

    fs::write(context.run_dir().join("audio-preparation.md"), markdown).with_context(|| {
        format!(
            "failed to write audio preparation report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::AudioPreparationLayerExperiment;
    use crate::experiments::registry::{Experiment, ExperimentName, run_experiment_in};
    use uuid::Uuid;

    #[test]
    fn audio_preparation_experiment_writes_audio_events_and_traces() {
        let base_dir = std::env::temp_dir().join(format!("qsf-audio-test-{}", Uuid::new_v4()));

        let summary = run_experiment_in(&base_dir, ExperimentName::AudioPreparationLayer).unwrap();
        let events = fs::read_to_string(summary.run_dir.join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();
        let report = fs::read_to_string(summary.run_dir.join("audio-preparation.md")).unwrap();

        assert!(events.contains("AudioFinalTranscript"));
        assert!(events.contains("SpeechPlaybackRequested"));
        assert!(events.contains("LatencyMeasurementRecorded"));
        assert!(traces.contains("\"latency_domain\":\"audio\""));
        assert!(traces.contains("\"latency_stage\":\"final-transcription\""));
        assert!(traces.contains("\"latency_stage\":\"speech-playback\""));
        assert!(report.contains("Phase 8 Audio Preparation"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn experiment_name_matches_registry_id() {
        let experiment = AudioPreparationLayerExperiment;

        assert_eq!(experiment.name(), ExperimentName::AudioPreparationLayer);
        assert_eq!(experiment.id(), "audio-preparation-layer");
    }
}
