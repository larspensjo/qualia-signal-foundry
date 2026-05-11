use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum EventType {
    ExperimentStarted,
    ExperimentCompleted,
    InputReceived,
    MemoryRetrievalRequested,
    MemoryRetrieved,
    ContextAssemblyRequested,
    ContextAssembled,
    ToolRequested,
    ToolCompleted,
    ToolFailed,
    ModelRoleRequested,
    ModelRoleCompleted,
    ModelRoleFailed,
    OutputProduced,
    ErrorOccurred,
    TraceRecorded,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventRecord {
    pub event_id: Uuid,
    pub experiment_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub event_type: EventType,
    pub payload: Value,
    pub trace_id: Option<Uuid>,
}

impl EventRecord {
    pub fn new(
        experiment_id: impl Into<String>,
        event_type: EventType,
        payload: Value,
        trace_id: Option<Uuid>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            experiment_id: experiment_id.into(),
            timestamp: OffsetDateTime::now_utc(),
            event_type,
            payload,
            trace_id,
        }
    }
}

pub struct EventLogWriter {
    writer: BufWriter<File>,
}

impl EventLogWriter {
    pub fn create(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;

        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    pub fn append(&mut self, record: &EventRecord) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{EventRecord, EventType};

    #[test]
    fn event_record_serializes_as_json_object() {
        let event = EventRecord::new(
            "test-experiment",
            EventType::ExperimentStarted,
            json!({ "run_id": "test-run" }),
            None,
        );

        let serialized = serde_json::to_value(event).unwrap();

        assert_eq!(serialized["experiment_id"], "test-experiment");
        assert_eq!(serialized["event_type"], "ExperimentStarted");
        assert_eq!(serialized["payload"]["run_id"], "test-run");
    }
}
