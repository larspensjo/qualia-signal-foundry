use std::time::Instant;

use serde_json::json;

use crate::observability::event_log::EventType;
use crate::observability::trace::{TraceRecord, elapsed_ns};
use crate::runtime::run_context::RunContext;

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

pub struct PlaceholderExperiment {
    name: ExperimentName,
}

impl PlaceholderExperiment {
    pub fn new(name: ExperimentName) -> Self {
        Self { name }
    }
}

impl Experiment for PlaceholderExperiment {
    fn name(&self) -> ExperimentName {
        self.name
    }

    fn description(&self) -> &'static str {
        self.name.description()
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let started_at = Instant::now();

        context.record_event(
            EventType::InputReceived,
            json!({
                "runner": "placeholder",
                "experiment_id": self.id(),
                "description": self.description(),
            }),
            None,
        )?;

        let trace = TraceRecord::new(
            context.experiment_id(),
            "placeholder-experiment-run",
            "registered placeholder experiment",
            "runner emitted first-class experiment artifacts",
        )
        .with_details(json!({
            "artifacts": ["engine.log", "events.jsonl", "traces.jsonl", "Report.md"],
        }))
        .with_latency_ns(elapsed_ns(started_at));
        let trace_id = trace.trace_id;

        context.record_trace(trace)?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "operation": "placeholder-experiment-run",
            }),
            Some(trace_id),
        )?;

        let output_message = format!("Experiment `{}` completed placeholder run.", self.id());
        context.record_event(
            EventType::OutputProduced,
            json!({
                "message": output_message,
            }),
            Some(trace_id),
        )?;

        Ok(ExperimentOutcome {
            summary: "Runner dispatch verified named experiment registration, per-run context creation, structured event logging, trace logging, and Markdown report generation.".to_string(),
            observations: vec![
                "Experiments are now first-class runner targets behind a shared trait.".to_string(),
                "The placeholder experiment keeps the artifact shape expected by future domain experiments.".to_string(),
            ],
            failure_modes: vec![
                "Placeholder experiment has no domain behavior yet.".to_string(),
            ],
            follow_up_questions: vec![
                "Which domain experiment should replace the placeholder implementation first?".to_string(),
            ],
            decision_candidates: vec![
                "Keep experiment registration explicit until dynamic discovery becomes necessary."
                    .to_string(),
            ],
            extra_artifacts: vec![],
        })
    }
}
