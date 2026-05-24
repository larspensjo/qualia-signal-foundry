use serde::{Deserialize, Serialize};

use crate::memory::RetrievedMemory;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    Memory,
    MemoryHint,
    ToolObservation,
    RuntimeState,
    ProjectFrame,
}

impl ContextSourceKind {
    /// Higher priority kinds win when the assembler must choose under budget pressure.
    pub fn source_priority(&self) -> u8 {
        match self {
            ContextSourceKind::Memory => 100,
            ContextSourceKind::ToolObservation => 90,
            ContextSourceKind::RuntimeState => 80,
            ContextSourceKind::ProjectFrame => 70,
            ContextSourceKind::MemoryHint => 50,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextFragment {
    pub fragment_id: String,
    pub source_kind: ContextSourceKind,
    pub summary: String,
    pub tags: Vec<String>,
    pub score: f64,
    pub estimated_tokens: usize,
    pub source_reference: String,
    pub selection_reason: String,
}

impl From<&RetrievedMemory> for ContextFragment {
    fn from(retrieved: &RetrievedMemory) -> Self {
        let mut reasons = Vec::new();

        if !retrieved.matched_terms.is_empty() {
            reasons.push(format!(
                "matched terms: {}",
                retrieved.matched_terms.join(", ")
            ));
        }

        if !retrieved.association_paths.is_empty() {
            let associations = retrieved
                .association_paths
                .iter()
                .map(|path| {
                    format!(
                        "{} -> {} ({:.2})",
                        path.from_memory_id, path.to_memory_id, path.weight
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            reasons.push(format!("association paths: {associations}"));
        }

        if reasons.is_empty() {
            reasons.push("selected by retrieval score".to_string());
        }

        Self {
            fragment_id: retrieved.memory.id.clone(),
            source_kind: ContextSourceKind::Memory,
            summary: retrieved.memory.summary.clone(),
            tags: retrieved.memory.tags.clone(),
            score: retrieved.score.total,
            estimated_tokens: retrieved.memory.estimated_tokens,
            source_reference: retrieved.memory.source_reference.clone(),
            selection_reason: reasons.join("; "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_outranks_memory_hint_in_priority() {
        assert!(
            ContextSourceKind::Memory.source_priority()
                > ContextSourceKind::MemoryHint.source_priority()
        );
    }

    #[test]
    fn memory_hint_serializes_in_snake_case() {
        let kind = ContextSourceKind::MemoryHint;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"memory_hint\"");
    }
}
