use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use anyhow::Context;
use clap::ValueEnum;
use serde_json::json;

use crate::observability::event_log::EventType;
use crate::reports::markdown_report::{ExperimentReport, write_report};
use crate::runtime::run_context::RunContext;

use super::accept_reviewed_memory::AcceptReviewedMemoryExperiment;
use super::accept_reviewed_volition_seed::AcceptReviewedVolitionSeedExperiment;
use super::audio_preparation_layer::AudioPreparationLayerExperiment;
use super::live_memory_extraction::LiveMemoryExtractionExperiment;
use super::memory_and_context::{
    AssociativeMemoryToyModelExperiment, ContextBudgetRetrievalTestExperiment,
};
use super::model_role_smoke::ModelRoleSmokeExperiment;
use super::multi_turn_text_loop::MultiTurnTextLoopExperiment;
use super::placeholder::PlaceholderExperiment;
use super::realtime_voice_session::RealtimeVoiceSessionExperiment;
use super::reviewed_memory_draft::ReviewedMemoryDraftExperiment;
use super::sleep_phase_session_summary::SleepPhaseSessionSummaryExperiment;
use super::streaming_transcription_mvp::StreamingTranscriptionMvpExperiment;
use super::text_owned_voice_loop::TextOwnedVoiceLoopExperiment;
use super::tool_as_perception_calculator::ToolAsPerceptionCalculatorExperiment;
use super::voice_loop::{VOICE_LOOP_DESCRIPTION, VoiceLoopExperiment};
use super::volition_arbitration_conflict::VolitionArbitrationConflictExperiment;
use super::volition_bounded_initiative_execution::VolitionBoundedInitiativeExecutionExperiment;
use super::volition_continuity::VolitionContinuityExperiment;
use super::volition_goal_fixture::VolitionGoalFixtureExperiment;
use super::volition_mode_bias::VolitionModeBiasExperiment;
use super::volition_reflection_goal_candidates::VolitionReflectionGoalCandidatesExperiment;
use super::volition_salience_and_satisfaction::VolitionSalienceAndSatisfactionExperiment;
use super::volition_trace_backed_initiative::VolitionTraceBackedInitiativeExperiment;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ExperimentName {
    FrameworkSkeletonMvp,
    AudioPreparationLayer,
    AssociativeMemoryToyModel,
    ContextBudgetRetrievalTest,
    ModelRoleSmokeTest,
    MultiTurnTextLoop,
    RealtimeVoiceSession,
    LiveMemoryExtraction,
    AcceptReviewedMemory,
    AcceptReviewedVolitionSeed,
    ReviewedMemoryDraft,
    SleepPhaseSessionSummary,
    StreamingTranscriptionMvp,
    TextOwnedVoiceLoop,
    VoiceLoop,
    ToolAsPerceptionCalculator,
    VolitionGoalFixture,
    VolitionTraceBackedInitiative,
    VolitionSalienceAndSatisfaction,
    VolitionArbitrationConflict,
    VolitionReflectionGoalCandidates,
    VolitionBoundedInitiativeExecution,
    VolitionModeBias,
    VolitionContinuity,
}

impl ExperimentName {
    pub fn id(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "framework-skeleton-mvp",
            Self::AudioPreparationLayer => "audio-preparation-layer",
            Self::AssociativeMemoryToyModel => "associative-memory-toy-model",
            Self::ContextBudgetRetrievalTest => "context-budget-retrieval-test",
            Self::ModelRoleSmokeTest => "model-role-smoke-test",
            Self::MultiTurnTextLoop => "multi-turn-text-loop",
            Self::RealtimeVoiceSession => "realtime-voice-session",
            Self::LiveMemoryExtraction => "live-memory-extraction",
            Self::AcceptReviewedMemory => "accept-reviewed-memory",
            Self::AcceptReviewedVolitionSeed => "accept-reviewed-volition-seed",
            Self::ReviewedMemoryDraft => "reviewed-memory-draft",
            Self::SleepPhaseSessionSummary => "sleep-phase-session-summary",
            Self::StreamingTranscriptionMvp => "streaming-transcription-mvp",
            Self::TextOwnedVoiceLoop => "text-owned-voice-loop",
            Self::VoiceLoop => "voice-loop",
            Self::ToolAsPerceptionCalculator => "tool-as-perception-calculator",
            Self::VolitionGoalFixture => "volition-goal-fixture",
            Self::VolitionTraceBackedInitiative => "volition-trace-backed-initiative",
            Self::VolitionSalienceAndSatisfaction => "volition-salience-and-satisfaction",
            Self::VolitionArbitrationConflict => "volition-arbitration-conflict",
            Self::VolitionReflectionGoalCandidates => "volition-reflection-goal-candidates",
            Self::VolitionBoundedInitiativeExecution => "volition-bounded-initiative-execution",
            Self::VolitionModeBias => "volition-mode-bias",
            Self::VolitionContinuity => "volition-continuity",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::FrameworkSkeletonMvp => "Placeholder framework skeleton smoke experiment",
            Self::AudioPreparationLayer => {
                "Simulate transcript and speech playback boundaries before real audio providers are added"
            }
            Self::AssociativeMemoryToyModel => {
                "Compare recency, keyword/tag, and association-weighted memory retrieval"
            }
            Self::ContextBudgetRetrievalTest => {
                "Compare selected and omitted memory context under a deliberately small budget"
            }
            Self::ModelRoleSmokeTest => {
                "Invoke a model role through a deterministic mock client or the explicitly selected OpenAI adapter"
            }
            Self::MultiTurnTextLoop => {
                "Run a human-driven text conversation with append-only session state and cache-stable prompt assembly"
            }
            Self::RealtimeVoiceSession => {
                "Run a realtime voice-session provider and map session events back into QSF records"
            }
            Self::LiveMemoryExtraction => {
                "Extract reviewable memory candidates from trusted realtime continuity artifacts"
            }
            Self::AcceptReviewedMemory => {
                "Accept a reviewed memory draft and write it as the durable reviewed voice-memory fixture"
            }
            Self::AcceptReviewedVolitionSeed => {
                "Accept a reviewed volition seed draft and write it as the durable reviewed volition seed artifact"
            }
            Self::ReviewedMemoryDraft => {
                "Convert provisional sleep memory candidates into a reviewable file-backed memory draft"
            }
            Self::SleepPhaseSessionSummary => {
                "Summarize a session into reviewable sleep-phase outputs and artifacts"
            }
            Self::StreamingTranscriptionMvp => {
                "Stream transcript deltas as audio-derived events before committing final text to runtime input"
            }
            Self::TextOwnedVoiceLoop => {
                "Capture or simulate speech, route finalized text through QSF-owned model behavior, then synthesize speech output from the QSF text response"
            }
            Self::VoiceLoop => VOICE_LOOP_DESCRIPTION,
            Self::ToolAsPerceptionCalculator => {
                "Execute a compute-only calculator tool and treat the result as a context candidate"
            }
            Self::VolitionGoalFixture => {
                "Select budget-bounded volition goals from a static fixture and trace candidate initiatives"
            }
            Self::VolitionTraceBackedInitiative => {
                "Record pre-initiative traces for selected volition goals before any effect could change behavior"
            }
            Self::VolitionSalienceAndSatisfaction => {
                "Replay a scripted multi-turn sequence to exercise salience rise/decay, satisfaction cooldown, blocked visibility, and retirement"
            }
            Self::VolitionArbitrationConflict => {
                "Replay a scripted multi-turn sequence exercising no_selection, single_selection, and conflict_resolved arbitration outcomes — no effect is executed"
            }
            Self::VolitionReflectionGoalCandidates => {
                "Replay a scripted propose/accept/reject/inert sequence to exercise reflection-generated goal candidates — no effect is executed; accepted candidates validated pending/accepted storage before selector wiring"
            }
            Self::VolitionBoundedInitiativeExecution => {
                "Replay a 5-turn scripted sequence exercising accepted-candidate selector wiring, arbitration, and bounded initiative execution — executed_effects=0 on every turn"
            }
            Self::VolitionModeBias => {
                "Replay 4 scripted turns exercising mode-aware arbitration: neutral baseline, Exploratory flips winner, floor immunity, Focused suppresses tangent — executed_effects=0 on every turn"
            }
            Self::VolitionContinuity => {
                "Read realtime volition continuity artifacts and consolidate them into reviewable report artifacts"
            }
        }
    }
}

impl fmt::Display for ExperimentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

impl FromStr for ExperimentName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <Self as ValueEnum>::from_str(value, true).map_err(|_| {
            format!(
                "unknown experiment `{value}`; run `qsf_app list-experiments` for available names"
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExperimentInfo {
    pub id: &'static str,
    pub description: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentOutcome {
    pub summary: String,
    pub observations: Vec<String>,
    pub failure_modes: Vec<String>,
    pub follow_up_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
    pub extra_artifacts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentRunSummary {
    pub experiment_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub status: String,
    pub event_count: usize,
    pub trace_count: usize,
}

pub trait Experiment {
    fn name(&self) -> ExperimentName;

    fn description(&self) -> &'static str;

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome>;

    fn id(&self) -> &'static str {
        self.name().id()
    }
}

pub fn available_experiments() -> Vec<ExperimentInfo> {
    ExperimentName::value_variants()
        .iter()
        .copied()
        .map(|experiment| ExperimentInfo {
            id: experiment.id(),
            description: experiment.description(),
        })
        .collect()
}

pub fn run_experiment(name: ExperimentName) -> anyhow::Result<ExperimentRunSummary> {
    run_experiment_in_with_workspace_root("runs", name, None::<PathBuf>)
}

pub fn run_experiment_with_workspace_root(
    name: ExperimentName,
    workspace_root: Option<PathBuf>,
) -> anyhow::Result<ExperimentRunSummary> {
    run_experiment_in_with_workspace_root("runs", name, workspace_root)
}

pub fn run_experiment_in(
    base_dir: impl AsRef<Path>,
    name: ExperimentName,
) -> anyhow::Result<ExperimentRunSummary> {
    run_experiment_in_with_workspace_root(base_dir, name, None::<PathBuf>)
}

pub fn run_experiment_in_with_workspace_root(
    base_dir: impl AsRef<Path>,
    name: ExperimentName,
    workspace_root: Option<impl AsRef<Path>>,
) -> anyhow::Result<ExperimentRunSummary> {
    let experiment = experiment_for(name);
    let started_at = Instant::now();
    let mut context =
        RunContext::create_in_with_workspace_root(base_dir, experiment.id(), workspace_root)?;
    context.initialize_engine_logging();

    engine_logging::engine_info!(
        "starting experiment: experiment_id={} run_id={}",
        experiment.id(),
        context.run_id()
    );

    context.record_event(
        EventType::ExperimentStarted,
        json!({
            "run_id": context.run_id(),
            "description": experiment.description(),
        }),
        None,
    )?;

    let outcome = match experiment.run(&mut context) {
        Ok(outcome) => outcome,
        Err(error) => {
            let error_message = error.to_string();
            engine_logging::engine_error!(
                "experiment failed: experiment_id={} run_id={} error={}",
                experiment.id(),
                context.run_id(),
                error_message
            );
            context.record_event(
                EventType::ErrorOccurred,
                json!({
                    "error": error_message,
                }),
                None,
            )?;
            context.record_event(
                EventType::ExperimentCompleted,
                json!({
                    "status": "failed",
                    "elapsed_ms": started_at.elapsed().as_millis(),
                }),
                None,
            )?;
            write_report(
                context.report_path(),
                &ExperimentReport {
                    experiment_id: context.experiment_id().to_string(),
                    run_id: context.run_id().to_string(),
                    status: "failed".to_string(),
                    summary: "Experiment failed before producing a completed outcome.".to_string(),
                    configuration: report_configuration(),
                    event_count: context.event_count(),
                    trace_count: context.trace_count(),
                    observations: vec![],
                    failure_modes: vec![error_message],
                    follow_up_questions: vec![],
                    decision_candidates: vec![],
                    extra_artifacts: vec![],
                },
            )?;

            return Err(error).with_context(|| {
                format!(
                    "experiment `{}` failed; run artifacts: {}",
                    experiment.id(),
                    context.run_dir().display()
                )
            });
        }
    };

    context.record_event(
        EventType::ExperimentCompleted,
        json!({
            "status": "completed",
            "elapsed_ms": started_at.elapsed().as_millis(),
        }),
        None,
    )?;

    write_report(
        context.report_path(),
        &ExperimentReport {
            experiment_id: context.experiment_id().to_string(),
            run_id: context.run_id().to_string(),
            status: "completed".to_string(),
            summary: outcome.summary,
            configuration: report_configuration(),
            event_count: context.event_count(),
            trace_count: context.trace_count(),
            observations: outcome.observations,
            failure_modes: outcome.failure_modes,
            follow_up_questions: outcome.follow_up_questions,
            decision_candidates: outcome.decision_candidates,
            extra_artifacts: outcome.extra_artifacts,
        },
    )?;

    engine_logging::engine_info!(
        "completed experiment: experiment_id={} run_id={}",
        experiment.id(),
        context.run_id()
    );

    Ok(ExperimentRunSummary {
        experiment_id: context.experiment_id().to_string(),
        run_id: context.run_id().to_string(),
        run_dir: context.run_dir().to_path_buf(),
        status: "completed".to_string(),
        event_count: context.event_count(),
        trace_count: context.trace_count(),
    })
}

fn experiment_for(name: ExperimentName) -> Box<dyn Experiment> {
    match name {
        ExperimentName::FrameworkSkeletonMvp => Box::new(PlaceholderExperiment::new(name)),
        ExperimentName::AudioPreparationLayer => Box::new(AudioPreparationLayerExperiment),
        ExperimentName::AssociativeMemoryToyModel => Box::new(AssociativeMemoryToyModelExperiment),
        ExperimentName::ContextBudgetRetrievalTest => {
            Box::new(ContextBudgetRetrievalTestExperiment)
        }
        ExperimentName::ModelRoleSmokeTest => Box::new(ModelRoleSmokeExperiment),
        ExperimentName::MultiTurnTextLoop => Box::new(MultiTurnTextLoopExperiment),
        ExperimentName::RealtimeVoiceSession => Box::new(RealtimeVoiceSessionExperiment),
        ExperimentName::LiveMemoryExtraction => Box::new(LiveMemoryExtractionExperiment),
        ExperimentName::AcceptReviewedMemory => Box::new(AcceptReviewedMemoryExperiment),
        ExperimentName::AcceptReviewedVolitionSeed => {
            Box::new(AcceptReviewedVolitionSeedExperiment)
        }
        ExperimentName::ReviewedMemoryDraft => Box::new(ReviewedMemoryDraftExperiment),
        ExperimentName::SleepPhaseSessionSummary => Box::new(SleepPhaseSessionSummaryExperiment),
        ExperimentName::StreamingTranscriptionMvp => Box::new(StreamingTranscriptionMvpExperiment),
        ExperimentName::TextOwnedVoiceLoop => Box::new(TextOwnedVoiceLoopExperiment),
        ExperimentName::VoiceLoop => Box::new(VoiceLoopExperiment),
        ExperimentName::ToolAsPerceptionCalculator => {
            Box::new(ToolAsPerceptionCalculatorExperiment)
        }
        ExperimentName::VolitionGoalFixture => Box::new(VolitionGoalFixtureExperiment),
        ExperimentName::VolitionTraceBackedInitiative => {
            Box::new(VolitionTraceBackedInitiativeExperiment)
        }
        ExperimentName::VolitionSalienceAndSatisfaction => {
            Box::new(VolitionSalienceAndSatisfactionExperiment)
        }
        ExperimentName::VolitionArbitrationConflict => {
            Box::new(VolitionArbitrationConflictExperiment)
        }
        ExperimentName::VolitionReflectionGoalCandidates => {
            Box::new(VolitionReflectionGoalCandidatesExperiment)
        }
        ExperimentName::VolitionBoundedInitiativeExecution => {
            Box::new(VolitionBoundedInitiativeExecutionExperiment)
        }
        ExperimentName::VolitionModeBias => Box::new(VolitionModeBiasExperiment),
        ExperimentName::VolitionContinuity => Box::new(VolitionContinuityExperiment),
    }
}

fn report_configuration() -> Vec<String> {
    vec![
        "Runner: first-class experiment runner".to_string(),
        "Event log: `events.jsonl`".to_string(),
        "Trace log: `traces.jsonl`".to_string(),
        "Developer log: `engine.log`".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ExperimentName, available_experiments, run_experiment_in};

    #[test]
    fn placeholder_experiments_are_registered() {
        let experiments = available_experiments();

        assert!(
            experiments
                .iter()
                .any(|experiment| { experiment.id == ExperimentName::FrameworkSkeletonMvp.id() })
        );
        let associative = experiments
            .iter()
            .find(|experiment| experiment.id == ExperimentName::AssociativeMemoryToyModel.id())
            .unwrap();
        assert!(!associative.description.contains("Placeholder"));
    }

    #[test]
    fn experiment_id_round_trips() {
        let parsed: ExperimentName = "framework-skeleton-mvp".parse().unwrap();

        assert_eq!(parsed, ExperimentName::FrameworkSkeletonMvp);
    }

    #[test]
    fn voice_loop_experiment_is_registered() {
        let experiments = available_experiments();
        let voice_loop = experiments
            .iter()
            .find(|experiment| experiment.id == ExperimentName::VoiceLoop.id())
            .unwrap();

        assert_eq!(ExperimentName::VoiceLoop.to_string(), "voice-loop");
        assert!(voice_loop.description.contains("voice-loop"));
    }

    #[test]
    fn volition_goal_fixture_experiment_is_registered() {
        let experiments = available_experiments();
        let volition = experiments
            .iter()
            .find(|experiment| experiment.id == ExperimentName::VolitionGoalFixture.id())
            .unwrap();

        assert_eq!(
            ExperimentName::VolitionGoalFixture.to_string(),
            "volition-goal-fixture"
        );
        assert!(volition.description.contains("volition goals"));
    }

    #[test]
    fn named_experiment_dispatch_writes_run_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-runner-{}", uuid::Uuid::new_v4()));
        let summary =
            run_experiment_in(&base_dir, ExperimentName::AssociativeMemoryToyModel).unwrap();

        assert_eq!(summary.experiment_id, "associative-memory-toy-model");
        assert_eq!(summary.status, "completed");
        assert_eq!(summary.trace_count, 6);

        let events = fs::read_to_string(summary.run_dir.join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();
        let report = fs::read_to_string(summary.run_dir.join("Report.md")).unwrap();

        assert!(events.contains("ExperimentStarted"));
        assert!(events.contains("InputReceived"));
        assert!(events.contains("MemoryRetrieved"));
        assert!(events.contains("ContextAssembled"));
        assert!(events.contains("ExperimentCompleted"));
        assert!(traces.contains("memory-retrieval"));
        assert!(traces.contains("context-assembly"));
        assert!(report.contains("associative memory toy model"));
        assert!(report.contains("Require memory retrieval traces"));
        assert!(report.contains("memory-fixture.json"));
        assert!(report.contains("retrieval-comparison.md"));

        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn logging_smoke_path_uses_engine_logging_test_initializer() {
        engine_logging::initialize_for_tests();
        engine_logging::engine_info!("qsf_app logging smoke test");
    }
}
