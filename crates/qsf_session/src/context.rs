use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextBudget {
    pub max_fragments: usize,
    pub max_estimated_tokens: usize,
}

impl ContextBudget {
    pub fn new(max_fragments: usize, max_estimated_tokens: usize) -> Self {
        Self {
            max_fragments,
            max_estimated_tokens,
        }
    }
}

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextSelection {
    pub fragment: ContextFragment,
    pub cumulative_estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextOmission {
    pub fragment: ContextFragment,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextAssembly {
    pub budget: ContextBudget,
    pub selected: Vec<ContextSelection>,
    pub omitted: Vec<ContextOmission>,
    pub used_estimated_tokens: usize,
}

impl ContextAssembly {
    pub fn retrieved_memory_ids(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter(|selection| selection.fragment.source_kind == ContextSourceKind::Memory)
            .map(|selection| selection.fragment.fragment_id.clone())
            .collect()
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

    #[test]
    fn retrieved_memory_ids_only_returns_memory_sources() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(4, 100),
            selected: vec![
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory-a".to_string(),
                        source_kind: ContextSourceKind::Memory,
                        summary: "A".to_string(),
                        tags: vec![],
                        score: 1.0,
                        estimated_tokens: 10,
                        source_reference: "fixture".to_string(),
                        selection_reason: "selected".to_string(),
                    },
                    cumulative_estimated_tokens: 10,
                },
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "hint-b".to_string(),
                        source_kind: ContextSourceKind::MemoryHint,
                        summary: "B".to_string(),
                        tags: vec![],
                        score: 0.5,
                        estimated_tokens: 8,
                        source_reference: "fixture".to_string(),
                        selection_reason: "selected".to_string(),
                    },
                    cumulative_estimated_tokens: 18,
                },
            ],
            omitted: vec![],
            used_estimated_tokens: 18,
        };

        assert_eq!(
            assembly.retrieved_memory_ids(),
            vec!["memory-a".to_string()]
        );
    }
}
