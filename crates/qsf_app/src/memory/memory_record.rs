use anyhow::bail;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Bump when a persisted memory record removes, renames, or changes the
/// semantics of a field. Pure optional additions do not require a bump.
pub const MEMORY_RECORD_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecordKind {
    Concept,
    ArchitectureNote,
    Experiment,
    Decision,
    Question,
    Observation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemoryRecord {
    pub schema_version: u16,
    pub id: String,
    pub kind: MemoryRecordKind,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub importance: f64,
    pub reinforcement_count: u32,
    pub source_reference: String,
    pub estimated_tokens: usize,
}

impl MemoryRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        kind: MemoryRecordKind,
        title: impl Into<String>,
        summary: impl Into<String>,
        tags: Vec<&str>,
        created_at: OffsetDateTime,
        importance: f64,
        reinforcement_count: u32,
        source_reference: impl Into<String>,
        estimated_tokens: usize,
    ) -> Self {
        Self {
            schema_version: MEMORY_RECORD_SCHEMA_VERSION,
            id: id.into(),
            kind,
            title: title.into(),
            summary: summary.into(),
            tags: tags.into_iter().map(str::to_string).collect(),
            created_at,
            importance,
            reinforcement_count,
            source_reference: source_reference.into(),
            estimated_tokens,
        }
    }

    pub fn ensure_current_schema(&self) -> anyhow::Result<()> {
        if self.schema_version != MEMORY_RECORD_SCHEMA_VERSION {
            bail!(
                "unsupported memory record schema version: memory_id={} found={} expected={}",
                self.id,
                self.schema_version,
                MEMORY_RECORD_SCHEMA_VERSION
            );
        }

        Ok(())
    }
}

pub fn ensure_current_memory_schema(records: &[MemoryRecord]) -> anyhow::Result<()> {
    for record in records {
        record.ensure_current_schema()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::{MEMORY_RECORD_SCHEMA_VERSION, MemoryRecord, MemoryRecordKind};

    #[test]
    fn new_memory_record_uses_current_schema_version() {
        let record = MemoryRecord::new(
            "memory.test",
            MemoryRecordKind::Concept,
            "Test",
            "A compact memory.",
            vec!["test"],
            timestamp(),
            0.5,
            0,
            "tests",
            10,
        );

        assert_eq!(record.schema_version, MEMORY_RECORD_SCHEMA_VERSION);
        assert!(record.ensure_current_schema().is_ok());
    }

    #[test]
    fn off_version_memory_record_errors() {
        let mut record = MemoryRecord::new(
            "memory.test",
            MemoryRecordKind::Concept,
            "Test",
            "A compact memory.",
            vec!["test"],
            timestamp(),
            0.5,
            0,
            "tests",
            10,
        );
        record.schema_version = MEMORY_RECORD_SCHEMA_VERSION + 1;

        assert!(record.ensure_current_schema().is_err());
    }

    fn timestamp() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-09T12:00:00Z", &Rfc3339).unwrap()
    }
}
