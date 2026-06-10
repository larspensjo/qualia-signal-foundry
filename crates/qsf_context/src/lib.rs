use serde::{Deserialize, Serialize};

use qsf_memory::RetrievedMemory;

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

pub fn assemble_context(fragments: Vec<ContextFragment>, budget: ContextBudget) -> ContextAssembly {
    let mut sorted = fragments;
    sorted.sort_by(|left, right| {
        right
            .source_kind
            .source_priority()
            .cmp(&left.source_kind.source_priority())
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.estimated_tokens.cmp(&right.estimated_tokens))
            .then_with(|| left.fragment_id.cmp(&right.fragment_id))
    });

    let mut selected = Vec::new();
    let mut omitted = Vec::new();
    let mut used_estimated_tokens = 0;

    for fragment in sorted {
        if selected.len() >= budget.max_fragments {
            omitted.push(ContextOmission {
                fragment,
                reason: "fragment limit reached".to_string(),
            });
            continue;
        }

        let next_token_total = used_estimated_tokens + fragment.estimated_tokens;
        if next_token_total > budget.max_estimated_tokens {
            omitted.push(ContextOmission {
                fragment,
                reason: format!(
                    "token budget exceeded: would use {} of {}",
                    next_token_total, budget.max_estimated_tokens
                ),
            });
            continue;
        }

        used_estimated_tokens = next_token_total;
        selected.push(ContextSelection {
            fragment,
            cumulative_estimated_tokens: used_estimated_tokens,
        });
    }

    ContextAssembly {
        budget,
        selected,
        omitted,
        used_estimated_tokens,
    }
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

    #[test]
    fn assembler_respects_fragment_and_token_budget() {
        let fragments = vec![
            fragment("a", 10.0, 40),
            fragment("b", 9.0, 35),
            fragment("c", 8.0, 50),
        ];
        let assembly = assemble_context(fragments, ContextBudget::new(2, 75));

        assert_eq!(assembly.selected.len(), 2);
        assert_eq!(assembly.omitted.len(), 1);
        assert_eq!(assembly.used_estimated_tokens, 75);
        assert_eq!(assembly.omitted[0].reason, "fragment limit reached");
    }

    #[test]
    fn assembler_logs_token_budget_omissions() {
        let fragments = vec![fragment("a", 10.0, 70), fragment("b", 9.0, 40)];
        let assembly = assemble_context(fragments, ContextBudget::new(3, 80));

        assert_eq!(assembly.selected.len(), 1);
        assert_eq!(assembly.omitted.len(), 1);
        assert!(assembly.omitted[0].reason.contains("token budget exceeded"));
    }

    #[test]
    fn hint_cannot_evict_direct_under_budget_pressure() {
        let direct = fragment_with_kind("direct.a", 5.0, 60, ContextSourceKind::Memory);
        let hint = fragment_with_kind("hint.b", 10.0, 60, ContextSourceKind::MemoryHint);

        let assembly = assemble_context(vec![hint, direct], ContextBudget::new(2, 60));

        assert_eq!(assembly.selected.len(), 1);
        assert_eq!(assembly.selected[0].fragment.fragment_id, "direct.a");
        assert_eq!(assembly.omitted.len(), 1);
        assert_eq!(assembly.omitted[0].fragment.fragment_id, "hint.b");
    }

    fn fragment(id: &str, score: f64, estimated_tokens: usize) -> ContextFragment {
        fragment_with_kind(id, score, estimated_tokens, ContextSourceKind::Memory)
    }

    fn fragment_with_kind(
        id: &str,
        score: f64,
        estimated_tokens: usize,
        source_kind: ContextSourceKind,
    ) -> ContextFragment {
        ContextFragment {
            fragment_id: id.to_string(),
            source_kind,
            summary: format!("Fragment {id}"),
            tags: vec![],
            score,
            estimated_tokens,
            source_reference: "test".to_string(),
            selection_reason: "test".to_string(),
        }
    }
}
