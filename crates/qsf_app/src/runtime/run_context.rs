use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::observability::event_log::{EventLogWriter, EventRecord, EventType};
use crate::observability::trace::{TraceLogWriter, TraceRecord};

pub struct RunContext {
    experiment_id: String,
    run_id: String,
    run_dir: PathBuf,
    event_log: EventLogWriter,
    trace_log: TraceLogWriter,
}

impl RunContext {
    pub fn create(experiment_id: impl Into<String>) -> anyhow::Result<Self> {
        Self::create_in("runs", experiment_id)
    }

    pub fn create_in(
        base_dir: impl AsRef<Path>,
        experiment_id: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let experiment_id = experiment_id.into();
        let run_id = format!(
            "{}-{}",
            timestamp_for_path(OffsetDateTime::now_utc()),
            experiment_id
        );
        let run_dir = base_dir.as_ref().join(&run_id);

        fs::create_dir_all(&run_dir)?;

        Ok(Self {
            experiment_id,
            run_id,
            event_log: EventLogWriter::create(run_dir.join("events.jsonl"))?,
            trace_log: TraceLogWriter::create(run_dir.join("traces.jsonl"))?,
            run_dir,
        })
    }

    pub fn initialize_engine_logging(&self) {
        engine_logging::initialize_to_path(&self.engine_log_path());
    }

    pub fn experiment_id(&self) -> &str {
        &self.experiment_id
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    pub fn engine_log_path(&self) -> PathBuf {
        self.run_dir.join("engine.log")
    }

    pub fn report_path(&self) -> PathBuf {
        self.run_dir.join("Report.md")
    }

    pub fn record_event(
        &mut self,
        event_type: EventType,
        payload: Value,
        trace_id: Option<Uuid>,
    ) -> anyhow::Result<EventRecord> {
        let record = EventRecord::new(self.experiment_id.clone(), event_type, payload, trace_id);
        self.event_log.append(&record)?;
        Ok(record)
    }

    pub fn record_trace(&mut self, trace: TraceRecord) -> anyhow::Result<TraceRecord> {
        self.trace_log.append(&trace)?;
        Ok(trace)
    }
}

fn timestamp_for_path(timestamp: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
        timestamp.year(),
        u8::from(timestamp.month()),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::RunContext;
    use crate::observability::event_log::EventType;
    use crate::observability::trace::TraceRecord;

    #[test]
    fn run_context_creates_jsonl_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-test-{}", uuid::Uuid::new_v4()));
        let mut context = RunContext::create_in(&base_dir, "test-experiment").unwrap();

        let trace = TraceRecord::new(
            context.experiment_id(),
            "test-operation",
            "test input",
            "test output",
        )
        .with_details(json!({ "ok": true }));
        let trace_id = trace.trace_id;

        context.record_trace(trace).unwrap();
        context
            .record_event(
                EventType::ExperimentStarted,
                json!({ "run_id": context.run_id() }),
                Some(trace_id),
            )
            .unwrap();

        let events = fs::read_to_string(context.run_dir().join("events.jsonl")).unwrap();
        let traces = fs::read_to_string(context.run_dir().join("traces.jsonl")).unwrap();

        assert!(events.contains("ExperimentStarted"));
        assert!(events.contains(&trace_id.to_string()));
        assert!(traces.contains("test-operation"));

        fs::remove_dir_all(base_dir).unwrap();
    }
}
