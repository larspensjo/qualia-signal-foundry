use std::fmt;

use serde::{Deserialize, Serialize};

/// A non-empty, non-whitespace reference to an observable artifact or trace that
/// justifies a progress or satisfaction event. Cannot be constructed from empty or
/// whitespace-only input — use `EvidenceRef::try_new` or `TryFrom<String>`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRef(String);

impl EvidenceRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, EvidenceRefError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(EvidenceRefError::Empty);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EvidenceRef {
    type Error = EvidenceRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl fmt::Display for EvidenceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceRefError {
    Empty,
}

impl fmt::Display for EvidenceRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("evidence ref must not be empty or whitespace-only")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_ref_rejects_empty_string() {
        assert!(EvidenceRef::try_new("").is_err());
    }

    #[test]
    fn evidence_ref_rejects_whitespace_only() {
        assert!(EvidenceRef::try_new("   ").is_err());
        assert!(EvidenceRef::try_new("\t\n").is_err());
    }

    #[test]
    fn evidence_ref_accepts_non_empty() {
        let r = EvidenceRef::try_new("docs/Experiment.md").unwrap();
        assert_eq!(r.as_str(), "docs/Experiment.md");
    }

    #[test]
    fn evidence_ref_try_from_string_works() {
        let r = EvidenceRef::try_from("trace-42".to_string()).unwrap();
        assert_eq!(r.as_str(), "trace-42");
    }
}
