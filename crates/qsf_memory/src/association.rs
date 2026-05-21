use anyhow::bail;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Bump when a persisted association removes, renames, or changes the
/// semantics of a field. Pure optional additions do not require a bump.
pub const ASSOCIATION_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Association {
    pub schema_version: u16,
    pub from_memory_id: String,
    pub to_memory_id: String,
    pub weight: f64,
    pub reason: String,
    #[serde(with = "time::serde::rfc3339")]
    pub last_reinforced_at: OffsetDateTime,
}

impl Association {
    pub fn new(
        from_memory_id: impl Into<String>,
        to_memory_id: impl Into<String>,
        weight: f64,
        reason: impl Into<String>,
        last_reinforced_at: OffsetDateTime,
    ) -> Self {
        Self {
            schema_version: ASSOCIATION_SCHEMA_VERSION,
            from_memory_id: from_memory_id.into(),
            to_memory_id: to_memory_id.into(),
            weight,
            reason: reason.into(),
            last_reinforced_at,
        }
    }

    pub fn ensure_current_schema(&self) -> anyhow::Result<()> {
        if self.schema_version != ASSOCIATION_SCHEMA_VERSION {
            bail!(
                "unsupported association schema version: from_memory_id={} to_memory_id={} found={} expected={}",
                self.from_memory_id,
                self.to_memory_id,
                self.schema_version,
                ASSOCIATION_SCHEMA_VERSION
            );
        }

        Ok(())
    }
}

pub fn ensure_current_association_schema(associations: &[Association]) -> anyhow::Result<()> {
    for association in associations {
        association.ensure_current_schema()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::{ASSOCIATION_SCHEMA_VERSION, Association};

    #[test]
    fn new_association_uses_current_schema_version() {
        let association = Association::new(
            "memory.source",
            "memory.target",
            0.8,
            "related",
            timestamp(),
        );

        assert_eq!(association.schema_version, ASSOCIATION_SCHEMA_VERSION);
        assert!(association.ensure_current_schema().is_ok());
    }

    fn timestamp() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-09T12:00:00Z", &Rfc3339).unwrap()
    }
}
