use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TraceRecord {
    pub trace_id: Uuid,
    pub experiment_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub operation: String,
    pub input_summary: String,
    pub output_summary: String,
    pub details: Value,
    pub latency_ms: Option<u64>,
    pub latency_ns: Option<u64>,
    pub error: Option<String>,
}

impl TraceRecord {
    pub fn new(
        experiment_id: impl Into<String>,
        operation: impl Into<String>,
        input_summary: impl Into<String>,
        output_summary: impl Into<String>,
    ) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            experiment_id: experiment_id.into(),
            timestamp: OffsetDateTime::now_utc(),
            operation: operation.into(),
            input_summary: input_summary.into(),
            output_summary: output_summary.into(),
            details: json!({}),
            latency_ms: None,
            latency_ns: None,
            error: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn with_latency_ms(mut self, latency_ms: u64) -> Self {
        self.latency_ms = Some(latency_ms);
        self.latency_ns = latency_ms.checked_mul(1_000_000);
        self
    }

    pub fn with_latency_ns(mut self, latency_ns: u64) -> Self {
        self.latency_ms = Some(latency_ns / 1_000_000);
        self.latency_ns = Some(latency_ns);
        self
    }
}

pub struct TraceLogWriter {
    writer: BufWriter<File>,
}

impl TraceLogWriter {
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

    pub fn append(&mut self, record: &TraceRecord) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.writer, record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::TraceRecord;

    #[test]
    fn trace_record_serializes_as_json_object() {
        let trace = TraceRecord::new("test-experiment", "placeholder-run", "input", "output")
            .with_details(json!({ "selected": 1 }))
            .with_latency_ms(7);

        let serialized = serde_json::to_value(trace).unwrap();

        assert_eq!(serialized["experiment_id"], "test-experiment");
        assert_eq!(serialized["operation"], "placeholder-run");
        assert_eq!(serialized["details"]["selected"], 1);
        assert_eq!(serialized["latency_ms"], 7);
        assert_eq!(serialized["latency_ns"], 7_000_000);
    }
}
