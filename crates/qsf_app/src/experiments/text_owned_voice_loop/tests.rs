use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use super::{
    FileVoiceMemorySource, PhaseFourVoiceMemorySource, SharedVoiceMemorySource,
    TextOwnedVoiceLoopExperiment, VOICE_CONTEXT_ASSEMBLY_LATENCY_MS, VoiceLoopSessionConfig,
};
use crate::audio::test_support::{
    assert_events_have_safety_markers, assert_payloads_do_not_contain_raw_audio_fields,
    is_audio_or_speech_event, parse_event_records,
};
use crate::audio::{
    FinalTranscript, SimulatedSpeechOutputProvider, SimulatedTranscriptProvider,
    SpeechOutputProvider, SpeechOutputProviderError, SpeechOutputRequest, SpeechOutputSession,
    TranscriptProvider, TranscriptProviderError, TranscriptProviderRequest,
    TranscriptProviderSession,
};
use crate::experiments::registry::{Experiment, ExperimentName};
use crate::memory::{Association, MemoryFixture, MemoryRecord, MemoryRecordKind, MemoryStore};
use crate::observability::event_log::{EventRecord, EventType};
use crate::runtime::run_context::RunContext;
use crate::session::{
    MemorySourceConfig, SessionConfig, SessionEndReason, StateDirectoryResolution,
};
use anyhow::anyhow;
use qsf_models::{MockModelClient, ModelClient, ModelRequest, ModelResponse, ModelUsage};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

#[test]
fn voice_retrieval_uses_keyword_tag_strategy() {
    assert_eq!(
        super::VOICE_MEMORY_RETRIEVAL_STRATEGY,
        crate::memory::RetrievalStrategy::KeywordTag,
    );
}

struct FailingModelClient;

impl ModelClient for FailingModelClient {
    fn client_name(&self) -> &str {
        "failing-model"
    }

    fn complete(&self, _request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        Err(anyhow!("model boom"))
    }
}

struct SlowModelClient {
    delay: Duration,
}

struct ReviewedDraftReflectingModelClient;

struct ConsolidatedBriefAssertingModelClient;

impl ModelClient for SlowModelClient {
    fn client_name(&self) -> &str {
        "slow-mock-model"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        std::thread::sleep(self.delay);
        MockModelClient::default().complete(request)
    }
}

impl ModelClient for ReviewedDraftReflectingModelClient {
    fn client_name(&self) -> &str {
        "reviewed-draft-reflecting-model"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let selected_context = request.last_user_message().unwrap_or_default();
        let reflected_memory = if selected_context.contains("reviewed and explicit") {
            "The selected reviewed draft memory says file-backed voice tests stay reviewed and explicit."
        } else {
            "The selected context did not include the expected reviewed draft memory."
        };

        Ok(ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            reflected_memory,
        )
        .with_usage(ModelUsage::new(42, 16))
        .with_finish_reason("stop"))
    }
}

impl ModelClient for ConsolidatedBriefAssertingModelClient {
    fn client_name(&self) -> &str {
        "consolidated-brief-asserting-model"
    }

    fn complete(&self, request: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let user_message = request.last_user_message().unwrap_or_default();
        assert!(user_message.contains("Previous session summary:\nVoice sleep summary."));
        assert!(user_message.contains("Future context hints:\n- Resume the user's voice project."));
        assert!(user_message.contains("Open questions:\n- Which voice path needs testing next?"));

        Ok(ModelResponse::from_text(
            request,
            self.client_name(),
            request.model_name.clone(),
            "The consolidated voice brief is in context.",
        )
        .with_usage(ModelUsage::new(42, 16))
        .with_finish_reason("stop"))
    }
}

struct FailingSpeechOutputProvider;

struct EmptyFinalTranscriptProvider;

impl TranscriptProvider for EmptyFinalTranscriptProvider {
    fn provider_name(&self) -> &str {
        "empty-final-transcript-provider"
    }

    fn transcribe(
        &self,
        request: &TranscriptProviderRequest,
    ) -> Result<TranscriptProviderSession, TranscriptProviderError> {
        let mut session = SimulatedTranscriptProvider.transcribe(request)?;
        session.provider_name = self.provider_name().to_string();
        session.final_transcript = FinalTranscript {
            utterance_index: 0,
            received_at_ms: 86,
            transcript: "   ".to_string(),
        };
        Ok(session)
    }
}

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
    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
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
            EventType::MemoryRetrievalRequested,
            EventType::MemoryRetrieved,
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
    assert_default_shared_memory_store_path_is_used(&event_records);
    assert_one_session_id_links_voice_loop_events(&event_records);
    assert_output_text_is_handed_exactly_to_speech_provider(&event_records);
    assert_events_have_safety_markers(&event_records, is_audio_or_speech_event);
    assert_payloads_do_not_contain_raw_audio_fields(&event_records);
    assert!(traces.contains("transcript-provider-session"));
    assert!(traces.contains("voice-runtime-input-bridge"));
    assert!(traces.contains("voice-memory-retrieval"));
    assert!(traces.contains("voice-context-assembly"));
    assert!(traces.contains("voice-model-response"));
    assert!(traces.contains("speech-output-provider"));
    assert!(traces.contains("voice-loop-latency"));
    assert!(report.contains("Text-Owned Voice Loop"));
    assert!(report.contains("Selected memory context: `none`"));
    assert!(report.contains("Model role latency:"));
    assert!(report.contains("Total observed turn latency:"));
    assert!(report.contains("## Diagnostics"));
    assert!(report.contains("Response owner: `qsf_model_role`"));
    assert!(report.contains("Memory source: `memory_store`"));
    assert!(report.contains("Memory records: `0`"));
    assert!(report.contains("Retrieval strategy: `keyword-tag`"));
    assert!(report.contains("Exact speech handoff: `true`"));
    assert!(report.contains("Raw audio logged: `false`"));
    assert!(context.run_dir().join("voice-memory-source.json").exists());

    let state_dir = context.run_dir().join("state/session");
    let persisted_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    assert_eq!(persisted_state.turns.len(), 1);
    assert_eq!(persisted_state.turns[0].index, 0);
    assert_eq!(persisted_state.ended_reason, Some(SessionEndReason::Eof));
    let manifest = crate::session::manifest::ContinuityManifest::load_or_default(
        state_dir.join("continuity-manifest.json"),
    )
    .unwrap();
    assert_eq!(
        manifest.current_session_id,
        Some(persisted_state.session_id)
    );
    assert_eq!(
        manifest.current_session_state_path,
        Some(PathBuf::from("session-state.json"))
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn default_shared_memory_store_drives_voice_retrieval_when_present() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-shared-memory-{}", Uuid::new_v4()));
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let state_dir = context.run_dir().join("state/session");
    let store_path = state_dir.join("memory-store.json");
    let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
    store.append_records(vec![MemoryRecord::new(
        "memory.voice.shared-store",
        MemoryRecordKind::Observation,
        "Shared voice memory",
        "Streaming transcription should enter the runtime as events.",
        vec!["streaming", "transcription", "runtime", "events"],
        timestamp("2026-05-15T06:00:00Z"),
        0.95,
        0,
        "tests/shared-memory-store.json",
        18,
    )]);
    store.persist().unwrap();
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
    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let event_records = parse_event_records(&events);
    let retrieval = event_records
        .iter()
        .find(|record| record.event_type == EventType::MemoryRetrieved)
        .unwrap();

    assert_eq!(retrieval.payload["memory_source"], "memory_store");
    assert_eq!(
        retrieval.payload["selected"][0],
        "memory.voice.shared-store"
    );
    assert!(report.contains("Memory source: `memory_store`"));
    assert!(report.contains("Selected memory context: `memory.voice.shared-store`"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn explicit_fixture_memory_source_remains_supported() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-fixture-memory-{}", Uuid::new_v4()));
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    experiment
        .run_with_components_and_memory_source(
            &mut context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &PhaseFourVoiceMemorySource,
        )
        .unwrap();

    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let source_snapshot =
        fs::read_to_string(context.run_dir().join("voice-memory-source.json")).unwrap();

    assert!(report.contains("Memory source: `phase_four_fixture`"));
    assert!(report.contains("Memory records: `10`"));
    assert!(source_snapshot.contains("phase_four_fixture"));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn second_voice_run_awake_resumes_same_session() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-voice-resume-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/session");
    let memory_source = SharedVoiceMemorySource::new(&state_dir);
    let experiment = TextOwnedVoiceLoopExperiment;

    let mut first_context =
        RunContext::create_in(base_dir.join("runs"), "text-owned-voice-loop").unwrap();
    experiment
        .run_with_components_and_memory_source_at_state_dirs(
            &mut first_context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &memory_source,
            StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir.clone(),
                legacy_fallback_used: false,
            },
        )
        .unwrap();
    let first_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    let mut second_context =
        RunContext::create_in(base_dir.join("runs"), "text-owned-voice-loop").unwrap();
    experiment
        .run_with_components_and_memory_source_at_state_dirs(
            &mut second_context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &memory_source,
            StateDirectoryResolution {
                resume_state_dir: state_dir.clone(),
                persist_state_dir: state_dir.clone(),
                legacy_fallback_used: false,
            },
        )
        .unwrap();
    let second_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();
    let events = fs::read_to_string(second_context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();

    assert_eq!(second_state.session_id, first_state.session_id);
    assert_eq!(second_state.turns.len(), 2);
    assert_eq!(second_state.turns[0].index, 0);
    assert_eq!(second_state.turns[1].index, 1);
    assert_eq!(resumed.payload["mode"], "awake_continuation");
    assert_eq!(
        resumed.payload["previous_session_id"].as_str(),
        Some(first_state.session_id.as_str())
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn voice_run_after_sleep_resumes_from_consolidated_brief() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-brief-resume-{}", Uuid::new_v4()));
    let state_dir = base_dir.join("state/session");
    let config = SessionConfig {
        model_id: "mock".to_string(),
        max_turns: 10,
        warm_threshold: 2,
        allow_over_limit: false,
        memory_source: MemorySourceConfig {
            source: "memory_store".to_string(),
            file: None,
        },
    };
    let previous = crate::session::SessionState::new_with_id(
        "slept-voice-session".to_string(),
        config.clone(),
    );
    crate::session::persistence::persist_session_state(&previous, &state_dir).unwrap();
    fs::write(
        state_dir.join("consolidated-brief.json"),
        serde_json::to_string_pretty(&crate::sleep::commit::ConsolidatedBrief {
            previous_session_summary: "Voice sleep summary.".to_string(),
            future_context_hints: vec!["Resume the user's voice project.".to_string()],
            open_questions: vec!["Which voice path needs testing next?".to_string()],
            promoted_count: 1,
            new_associations_count: 0,
        })
        .unwrap(),
    )
    .unwrap();
    crate::session::manifest::ContinuityManifest {
        schema_version: crate::session::manifest::CONTINUITY_MANIFEST_SCHEMA_VERSION,
        current_session_id: Some(previous.session_id.clone()),
        current_session_state_path: Some(PathBuf::from("session-state.json")),
        current_volition_snapshot_path: None,
        last_sleep_run_id: Some("sleep-voice-1".to_string()),
        last_sleep_brief_path: Some(PathBuf::from("consolidated-brief.json")),
        last_sleep_consumed_session_id: Some(previous.session_id.clone()),
        sleep_pending: false,
        resume_mode: crate::session::manifest::ResumeMode::ConsolidatedBrief,
    }
    .persist(state_dir.join("continuity-manifest.json"))
    .unwrap();
    let memory_source = SharedVoiceMemorySource::new(&state_dir);
    let experiment = TextOwnedVoiceLoopExperiment;
    let mut context =
        RunContext::create_in(base_dir.join("runs"), "text-owned-voice-loop").unwrap();

    experiment
        .run_with_components_and_memory_source_at_state_dirs_with_config(
            &mut context,
            &SimulatedTranscriptProvider,
            &ConsolidatedBriefAssertingModelClient,
            &SimulatedSpeechOutputProvider,
            &memory_source,
            VoiceLoopSessionConfig {
                state_resolution: StateDirectoryResolution {
                    resume_state_dir: state_dir.clone(),
                    persist_state_dir: state_dir.clone(),
                    legacy_fallback_used: false,
                },
                config,
            },
        )
        .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();
    let resumed_state =
        crate::session::persistence::load_session_state(state_dir.join("session-state.json"))
            .unwrap();

    assert_eq!(resumed.payload["mode"], "consolidated_brief");
    assert_eq!(
        resumed_state.previous_session_id.as_deref(),
        Some("slept-voice-session")
    );
    assert_ne!(resumed_state.session_id, previous.session_id);
    assert_eq!(resumed_state.turns.len(), 1);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn voice_reads_legacy_text_loop_state_and_persists_to_shared_session_dir() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-legacy-fallback-{}", Uuid::new_v4()));
    let legacy_state_dir = base_dir.join("state/text-loop");
    let shared_state_dir = base_dir.join("state/session");
    let config = crate::session::config::session_config_from_env();
    let mut previous =
        crate::session::SessionState::new_with_id("legacy-text-session".to_string(), config);
    previous.turns.push(crate::session::tests::fake_turn(0));
    crate::session::persistence::persist_session_state(&previous, &legacy_state_dir).unwrap();
    let legacy_memory_path = legacy_state_dir.join("memory-store.json");
    let mut legacy_store = MemoryStore::load_or_empty(&legacy_memory_path).unwrap();
    legacy_store.contents_mut().records.push(MemoryRecord::new(
        "memory.legacy.voice",
        MemoryRecordKind::Observation,
        "Legacy voice memory",
        "Legacy text-loop memory must survive a voice-first shared session migration.",
        vec!["legacy", "voice", "session"],
        timestamp("2026-06-03T08:00:00Z"),
        0.8,
        0,
        "tests/legacy-memory.json",
        16,
    ));
    legacy_store.persist().unwrap();
    let legacy_memory_before = fs::read_to_string(&legacy_memory_path).unwrap();
    crate::session::manifest::ContinuityManifest {
        current_session_id: Some(previous.session_id.clone()),
        current_session_state_path: Some(PathBuf::from("session-state.json")),
        sleep_pending: true,
        resume_mode: crate::session::manifest::ResumeMode::AwakeContinuation,
        ..crate::session::manifest::ContinuityManifest::default()
    }
    .persist(legacy_state_dir.join("continuity-manifest.json"))
    .unwrap();
    let memory_source = SharedVoiceMemorySource::new(&legacy_state_dir);
    let experiment = TextOwnedVoiceLoopExperiment;
    let mut context =
        RunContext::create_in(base_dir.join("runs"), "text-owned-voice-loop").unwrap();

    experiment
        .run_with_components_and_memory_source_at_state_dirs(
            &mut context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &memory_source,
            StateDirectoryResolution {
                resume_state_dir: legacy_state_dir.clone(),
                persist_state_dir: shared_state_dir.clone(),
                legacy_fallback_used: true,
            },
        )
        .unwrap();

    let shared_state = crate::session::persistence::load_session_state(
        shared_state_dir.join("session-state.json"),
    )
    .unwrap();
    let legacy_state = crate::session::persistence::load_session_state(
        legacy_state_dir.join("session-state.json"),
    )
    .unwrap();
    let legacy_memory_after = fs::read_to_string(&legacy_memory_path).unwrap();
    let shared_store =
        MemoryStore::load_or_empty(shared_state_dir.join("memory-store.json")).unwrap();
    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let records = parse_event_records(&events);
    let resumed = records
        .iter()
        .find(|record| record.event_type == EventType::SessionResumed)
        .unwrap();

    assert_eq!(legacy_state.turns.len(), 1);
    assert_eq!(shared_state.session_id, previous.session_id);
    assert_eq!(shared_state.turns.len(), 2);
    assert_eq!(
        shared_state.turns[0].user_input,
        previous.turns[0].user_input
    );
    assert_eq!(legacy_memory_after, legacy_memory_before);
    assert!(
        shared_store
            .contents()
            .records
            .iter()
            .any(|record| record.id == "memory.legacy.voice")
    );
    assert!(shared_state_dir.join("continuity-manifest.json").exists());
    assert_eq!(resumed.payload["mode"], "awake_continuation");
    assert_eq!(resumed.payload["legacy_fallback_used"], true);
    assert_eq!(
        resumed.payload["resume_state_dir"].as_str().unwrap(),
        legacy_state_dir.display().to_string()
    );

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn file_memory_source_drives_voice_retrieval_and_diagnostics() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-memory-file-{}", Uuid::new_v4()));
    fs::create_dir_all(&base_dir).unwrap();
    let source_path = base_dir.join("voice-memory.json");
    let fixture = MemoryFixture {
        records: vec![MemoryRecord::new(
            "memory.voice-session-input",
            MemoryRecordKind::Observation,
            "Voice session input",
            "Streaming transcription enters QSF as finalized input events.",
            vec!["streaming", "transcription", "events", "runtime"],
            timestamp("2026-05-15T06:00:00Z"),
            0.9,
            1,
            "tests/voice-memory.json",
            28,
        )],
        associations: vec![],
    };
    fs::write(
        &source_path,
        serde_json::to_string_pretty(&fixture).unwrap(),
    )
    .unwrap();
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    experiment
        .run_with_components_and_memory_source(
            &mut context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &FileVoiceMemorySource::new(&source_path),
        )
        .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let source_snapshot =
        fs::read_to_string(context.run_dir().join("voice-memory-source.json")).unwrap();
    let event_records = parse_event_records(&events);

    assert!(report.contains("Selected memory context: `memory.voice-session-input`"));
    assert!(report.contains("Memory source: `file`"));
    assert!(report.contains("Memory records: `1`"));
    assert!(source_snapshot.contains("memory.voice-session-input"));
    assert!(source_snapshot.contains("voice-memory.json"));
    assert!(event_records.iter().any(|record| {
        record.event_type == EventType::MemoryRetrievalRequested
            && record.payload["memory_source"] == "file"
            && record.payload["memory_records"] == 1
    }));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn reviewed_memory_draft_file_source_meets_voice_test_success_criteria() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-reviewed-draft-voice-{}", Uuid::new_v4()));
    fs::create_dir_all(&base_dir).unwrap();
    let draft_path = base_dir.join("reviewed-memory-draft.json");
    let reviewed_memory_id = "memory.sleep.reviewed-source.001";
    let reviewed_summary =
        "Reviewed draft memory keeps file-backed voice tests reviewed and explicit.";
    let fixture = MemoryFixture {
        records: vec![MemoryRecord::new(
            reviewed_memory_id,
            MemoryRecordKind::Observation,
            "Reviewed draft voice test",
            reviewed_summary,
            vec!["streaming", "transcription", "runtime", "events"],
            timestamp("2026-05-16T07:00:00Z"),
            0.95,
            0,
            "sleep-run:reviewed-source#memory_candidates[001]",
            18,
        )],
        associations: vec![],
    };
    fs::write(&draft_path, serde_json::to_string_pretty(&fixture).unwrap()).unwrap();
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    experiment
        .run_with_components_and_memory_source(
            &mut context,
            &SimulatedTranscriptProvider,
            &ReviewedDraftReflectingModelClient,
            &SimulatedSpeechOutputProvider,
            &FileVoiceMemorySource::new(&draft_path),
        )
        .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let source_snapshot =
        fs::read_to_string(context.run_dir().join("voice-memory-source.json")).unwrap();
    let event_records = parse_event_records(&events);

    assert!(report.contains("Memory source: `file`"));
    assert!(report.contains("Memory records: `1`"));
    assert!(report.contains(&format!("Selected memory context: `{reviewed_memory_id}`")));
    assert!(report.contains(
        "OutputProduced text: The selected reviewed draft memory says file-backed voice tests stay reviewed and explicit."
    ));
    assert!(report.contains("Exact speech handoff: `true`"));
    assert!(report.contains("Raw audio logged: `false`"));
    assert!(source_snapshot.contains(reviewed_memory_id));
    assert!(source_snapshot.contains("reviewed-memory-draft.json"));

    let retrieval = event_records
        .iter()
        .find(|record| record.event_type == EventType::MemoryRetrieved)
        .unwrap();
    assert_eq!(retrieval.payload["memory_source"], "file");
    assert_eq!(retrieval.payload["selected"][0], reviewed_memory_id);

    let output = event_records
        .iter()
        .find(|record| record.event_type == EventType::OutputProduced)
        .unwrap();
    assert!(
        output.payload["message"]
            .as_str()
            .unwrap()
            .contains("reviewed draft memory")
    );
    assert_output_text_is_handed_exactly_to_speech_provider(&event_records);

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn file_memory_source_keyword_retrieval_prefers_direct_match() {
    let base_dir =
        std::env::temp_dir().join(format!("qsf-text-owned-association-{}", Uuid::new_v4()));
    fs::create_dir_all(&base_dir).unwrap();
    let source_path = base_dir.join("reviewed-memory-draft.json");
    let seed_id = "memory.sleep.reviewed-source.001";
    let associated_id = "memory.sleep.reviewed-source.002";
    let fixture = MemoryFixture {
        records: vec![
            MemoryRecord::new(
                seed_id,
                MemoryRecordKind::Observation,
                "Streaming seed",
                "Streaming.",
                vec![],
                timestamp("2026-05-16T07:00:00Z"),
                0.0,
                0,
                "sleep-run:reviewed-source#memory_candidates[001]",
                3,
            ),
            MemoryRecord::new(
                associated_id,
                MemoryRecordKind::Observation,
                "Associated draft memory",
                "A linked reviewed memory can surface through a draft association.",
                vec![],
                timestamp("2026-05-16T07:01:00Z"),
                1.0,
                0,
                "sleep-run:reviewed-source#memory_candidates[002]",
                15,
            ),
        ],
        associations: vec![Association::new(
            seed_id,
            associated_id,
            1.0,
            "Streaming seed points to associated reviewed memory.",
            timestamp("2026-05-16T07:02:00Z"),
        )],
    };
    fs::write(
        &source_path,
        serde_json::to_string_pretty(&fixture).unwrap(),
    )
    .unwrap();
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    experiment
        .run_with_components_and_memory_source(
            &mut context,
            &SimulatedTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
            &FileVoiceMemorySource::new(&source_path),
        )
        .unwrap();

    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();

    assert!(report.contains(&format!("Selected memory context: `{seed_id}`")));
    assert!(traces.contains("\"association_paths\":[]"));
    assert!(traces.contains(seed_id));
    assert!(traces.contains(associated_id));

    fs::remove_dir_all(base_dir).unwrap();
}

#[test]
fn latency_summary_includes_model_runtime() {
    let base_dir = std::env::temp_dir().join(format!("qsf-text-owned-latency-{}", Uuid::new_v4()));
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    experiment
        .run_with_components(
            &mut context,
            &SimulatedTranscriptProvider,
            &SlowModelClient {
                delay: Duration::from_millis(25),
            },
            &SimulatedSpeechOutputProvider,
        )
        .unwrap();

    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let report = fs::read_to_string(context.run_dir().join("text-owned-voice-loop.md")).unwrap();
    let event_records = parse_event_records(&events);
    let latency = text_owned_loop_latency_event(&event_records);

    let final_transcript_latency_ms = latency.payload["final_transcript_latency_ms"]
        .as_u64()
        .unwrap();
    let memory_retrieval_latency_ms = latency.payload["memory_retrieval_latency_ms"]
        .as_u64()
        .unwrap();
    let context_assembly_latency_ms = latency.payload["context_assembly_latency_ms"]
        .as_u64()
        .unwrap();
    let model_role_latency_ms = latency.payload["model_role_latency_ms"].as_u64().unwrap();
    let speech_output_latency_ms = latency.payload["speech_output_latency_ms"]
        .as_u64()
        .unwrap();
    let total_turn_ms = latency.payload["total_turn_ms"].as_u64().unwrap();

    assert_eq!(
        context_assembly_latency_ms,
        VOICE_CONTEXT_ASSEMBLY_LATENCY_MS
    );
    assert!(model_role_latency_ms >= 10);
    assert!(
        total_turn_ms
            >= final_transcript_latency_ms
                + memory_retrieval_latency_ms
                + context_assembly_latency_ms
                + model_role_latency_ms
                + speech_output_latency_ms
    );
    assert!(report.contains(&format!("Model role latency: {model_role_latency_ms} ms")));
    assert!(report.contains(&format!("Total observed turn latency: {total_turn_ms} ms")));

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
fn empty_final_transcript_does_not_commit_runtime_input() {
    let base_dir = std::env::temp_dir().join(format!("qsf-text-owned-empty-{}", Uuid::new_v4()));
    let mut context = RunContext::create_in(&base_dir, "text-owned-voice-loop").unwrap();
    let experiment = TextOwnedVoiceLoopExperiment;

    let error = experiment
        .run_with_components(
            &mut context,
            &EmptyFinalTranscriptProvider,
            &MockModelClient::default(),
            &SimulatedSpeechOutputProvider,
        )
        .unwrap_err();
    let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
    let event_records = parse_event_records(&events);

    assert_eq!(error.to_string(), "transcript provider failed");
    assert!(event_records.iter().any(|record| {
        record.event_type == EventType::AudioTranscriptionFailed
            && record.payload["error_category"] == "transcription_failed"
    }));
    assert!(
        !event_records
            .iter()
            .any(|record| record.event_type == EventType::InputReceived)
    );
    assert!(
        !event_records
            .iter()
            .any(|record| record.event_type == EventType::OutputProduced)
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
        if next_index < expected_order.len() && record.event_type == expected_order[next_index] {
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

fn assert_default_shared_memory_store_path_is_used(records: &[EventRecord]) {
    let retrieval = records
        .iter()
        .find(|record| record.event_type == EventType::MemoryRetrieved)
        .unwrap();
    assert_eq!(retrieval.payload["strategy"], "keyword_tag");
    assert_eq!(retrieval.payload["memory_source"], "memory_store");
    let selected_memories = retrieval.payload["selected"].as_array().unwrap();
    assert!(selected_memories.is_empty());

    let context = records
        .iter()
        .find(|record| record.event_type == EventType::ContextAssembled)
        .unwrap();
    let selected_context = context.payload["selected"].as_array().unwrap();
    assert!(
        selected_context
            .iter()
            .all(|selection| { selection["fragment"]["source_kind"] != "memory" })
    );
}

fn assert_one_session_id_links_voice_loop_events(records: &[EventRecord]) {
    let session_ids = records
        .iter()
        .filter(|record| {
            matches!(
                record.event_type,
                EventType::AudioFinalTranscript
                    | EventType::InputReceived
                    | EventType::MemoryRetrievalRequested
                    | EventType::MemoryRetrieved
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

fn text_owned_loop_latency_event(records: &[EventRecord]) -> &EventRecord {
    records
        .iter()
        .rev()
        .find(|record| {
            record.event_type == EventType::LatencyMeasurementRecorded
                && record.payload["stage"] == "text-owned-voice-loop"
        })
        .unwrap()
}

fn timestamp(value: &str) -> OffsetDateTime {
    OffsetDateTime::parse(value, &Rfc3339).unwrap()
}
