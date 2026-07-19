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

/// Where a durable memory claim originated.
///
/// World observations are external claims. They remain distinct from internal
/// first-party memories so retrieval can apply the appropriate freshness and
/// supersession policies.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryProvenance {
    #[default]
    FirstPartyInternal,
    WorldObservationExternal,
}

/// The confidence boundary applied to a memory claim.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTrustTier {
    #[default]
    Trusted,
    UntrustedExternal,
}

/// Durable attribution for an external world observation.
///
/// This stays separate from the human-readable `source_reference` so recall surfaces can
/// preserve the exact source claim without having to parse an application-owned string format.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldObservationSource {
    /// SHA-256 hash from the corpus ledger.
    pub content_hash: String,
    /// Source article title at the time of observation.
    pub title: String,
    /// Canonical source URL.
    pub url: String,
    /// Source host derived by corpus ingestion.
    pub source_domain: String,
    /// Producer fetch time for the observed article.
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_utc: OffsetDateTime,
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
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub last_reinforced_at: Option<OffsetDateTime>,
    pub source_reference: String,
    pub estimated_tokens: usize,
    #[serde(default)]
    pub provenance: MemoryProvenance,
    #[serde(default)]
    pub trust_tier: MemoryTrustTier,
    /// An optional record-specific half-life for facts likely to become stale.
    #[serde(default)]
    pub time_sensitive_decay_half_life_days: Option<f64>,
    /// The successor that replaced this world observation with newer information.
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Full attribution retained for durable external world observations.
    #[serde(default)]
    pub world_observation_source: Option<WorldObservationSource>,
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
            last_reinforced_at: None,
            source_reference: source_reference.into(),
            estimated_tokens,
            provenance: MemoryProvenance::default(),
            trust_tier: MemoryTrustTier::default(),
            time_sensitive_decay_half_life_days: None,
            superseded_by: None,
            world_observation_source: None,
        }
    }

    pub fn with_last_reinforced_at(mut self, at: OffsetDateTime) -> Self {
        self.last_reinforced_at = Some(at);
        self
    }

    pub fn with_world_observation(mut self) -> Self {
        self.provenance = MemoryProvenance::WorldObservationExternal;
        self.trust_tier = MemoryTrustTier::UntrustedExternal;
        self
    }

    pub fn with_time_sensitive_decay_half_life_days(mut self, half_life_days: f64) -> Self {
        self.time_sensitive_decay_half_life_days = Some(half_life_days);
        self
    }

    pub fn with_superseded_by(mut self, successor_memory_id: impl Into<String>) -> Self {
        self.superseded_by = Some(successor_memory_id.into());
        self
    }

    pub fn with_world_observation_source(mut self, source: WorldObservationSource) -> Self {
        self.world_observation_source = Some(source);
        self
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

    use super::{
        MEMORY_RECORD_SCHEMA_VERSION, MemoryProvenance, MemoryRecord, MemoryRecordKind,
        MemoryTrustTier, WorldObservationSource,
    };

    #[test]
    fn new_memory_record_uses_current_schema_version() {
        assert_eq!(MEMORY_RECORD_SCHEMA_VERSION, 1);

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

    #[test]
    fn deserializes_v1_record_without_last_reinforced_at_with_none_fallback() {
        let v1_json = r#"{
            "schema_version": 1,
            "id": "memory.legacy",
            "kind": "observation",
            "title": "Legacy",
            "summary": "An old record predating last_reinforced_at.",
            "tags": [],
            "created_at": "2026-05-09T12:00:00Z",
            "importance": 0.5,
            "reinforcement_count": 0,
            "source_reference": "tests",
            "estimated_tokens": 10
        }"#;

        let record: MemoryRecord = serde_json::from_str(v1_json).unwrap();
        assert_eq!(record.last_reinforced_at, None);
        assert_eq!(record.provenance, MemoryProvenance::FirstPartyInternal);
        assert_eq!(record.trust_tier, MemoryTrustTier::Trusted);
        assert_eq!(record.time_sensitive_decay_half_life_days, None);
        assert_eq!(record.superseded_by, None);
        assert_eq!(record.world_observation_source, None);
        assert!(record.ensure_current_schema().is_ok());
    }

    #[test]
    fn world_observation_source_roundtrips_with_external_metadata() {
        let source = WorldObservationSource {
            content_hash: "hash".to_string(),
            title: "Source title".to_string(),
            url: "https://example.com/article".to_string(),
            source_domain: "example.com".to_string(),
            fetched_utc: timestamp(),
        };
        let record = MemoryRecord::new(
            "memory.world.hash",
            MemoryRecordKind::Observation,
            "Source title",
            "An external claim.",
            vec![],
            timestamp(),
            0.5,
            0,
            "world-corpus:hash",
            4,
        )
        .with_world_observation()
        .with_world_observation_source(source.clone());

        let restored: MemoryRecord =
            serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();

        assert_eq!(restored.world_observation_source, Some(source));
    }

    fn timestamp() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-09T12:00:00Z", &Rfc3339).unwrap()
    }
}
