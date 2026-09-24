use serde::{Deserialize, Serialize};

pub use qsf_memory::AdmissionBasis;
use qsf_memory::{RetrievalResult, RetrievedMemory};

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
    #[serde(default)]
    pub admission_basis: AdmissionBasis,
    #[serde(default = "default_associable")]
    pub associable: bool,
}

fn default_associable() -> bool {
    true
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

    /// Returns only selected memory sources eligible to shape durable associations.
    pub fn associable_retrieval_source_ids(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter(|selection| {
                selection.fragment.source_kind == ContextSourceKind::Memory
                    && selection.fragment.associable
            })
            .map(|selection| selection.fragment.fragment_id.clone())
            .collect()
    }
}

pub fn assemble_context(fragments: Vec<ContextFragment>, budget: ContextBudget) -> ContextAssembly {
    assemble_context_with_ordering(fragments, budget, None)
}

/// Assembles context with an optional explicit per-fragment ordering preference.
///
/// Source-kind priority remains primary. Within a source kind, ids supplied in
/// `ordering` precede unranked fragments and keep the supplied relative order.
pub fn assemble_context_with_ordering(
    fragments: Vec<ContextFragment>,
    budget: ContextBudget,
    ordering: Option<&[String]>,
) -> ContextAssembly {
    let mut sorted = fragments;
    let ordering_ranks = ordering.map(|ids| {
        let mut ranks = std::collections::HashMap::new();
        for (rank, id) in ids.iter().enumerate() {
            ranks.entry(id.as_str()).or_insert(rank);
        }
        ranks
    });
    sorted.sort_by(|left, right| {
        let ordering = ordering_ranks.as_ref().map(|ranks| {
            let left_rank = ranks.get(left.fragment_id.as_str()).copied();
            let right_rank = ranks.get(right.fragment_id.as_str()).copied();
            match (left_rank, right_rank) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        right
            .source_kind
            .source_priority()
            .cmp(&left.source_kind.source_priority())
            .then_with(|| ordering.unwrap_or(std::cmp::Ordering::Equal))
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

/// Maps a retrieval result to fragments and applies reserved-slot ordering when required.
pub fn assemble_retrieval_context(
    retrieval: &RetrievalResult,
    budget: ContextBudget,
) -> ContextAssembly {
    let fragments = retrieval
        .selected
        .iter()
        .map(ContextFragment::from)
        .collect::<Vec<_>>();
    let ordering = retrieval.context_ordering();
    assemble_context_with_ordering(fragments, budget, ordering.as_deref())
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

        if matches!(
            retrieved.admission_basis,
            AdmissionBasis::Judge | AdmissionBasis::LexicalAndJudge
        ) {
            if let Some(points) = retrieved.judge_verdict_basis_points {
                reasons.push(format!("judge verdict: {points} basis points"));
            }
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
            admission_basis: retrieved.admission_basis,
            associable: retrieved.selection_eligibility.is_associable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_memory::{
        AdmissionCombinationPolicy, JudgeVerdict, MemoryRecord, MemoryRecordKind, RetrievalRequest,
        RetrievalStrategy, retrieve_memories,
    };
    use qsf_semantics::trace::{BackendKind, ModelIdentity, ModelIdentityKind};
    use time::OffsetDateTime;

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
                        admission_basis: AdmissionBasis::Lexical,
                        associable: true,
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
                        admission_basis: AdmissionBasis::Lexical,
                        associable: true,
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

    #[test]
    fn older_context_fragment_json_defaults_to_lexical_and_associable() {
        let json = serde_json::json!({
            "fragment_id": "memory.legacy",
            "source_kind": "memory",
            "summary": "Persisted before judge metadata existed.",
            "tags": [],
            "score": 1.0,
            "estimated_tokens": 12,
            "source_reference": "session-state",
            "selection_reason": "selected by retrieval score"
        });

        let fragment: ContextFragment = serde_json::from_value(json).unwrap();
        assert_eq!(fragment.admission_basis, AdmissionBasis::Lexical);
        assert!(fragment.associable);
    }

    #[test]
    fn associable_retrieval_source_ids_excludes_judge_influenced_memory_fragments() {
        let assembly = assemble_context(
            vec![
                ContextFragment {
                    fragment_id: "lexical".to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: "lexical selection".to_string(),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 10,
                    source_reference: "tests".to_string(),
                    selection_reason: "lexical".to_string(),
                    admission_basis: AdmissionBasis::Lexical,
                    associable: true,
                },
                ContextFragment {
                    fragment_id: "judge-influenced".to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: "judge-only selection".to_string(),
                    tags: vec![],
                    score: 0.5,
                    estimated_tokens: 10,
                    source_reference: "tests".to_string(),
                    selection_reason: "judge".to_string(),
                    admission_basis: AdmissionBasis::Judge,
                    associable: false,
                },
            ],
            ContextBudget::new(4, 600),
        );

        assert_eq!(
            assembly.retrieved_memory_ids(),
            vec!["lexical", "judge-influenced"]
        );
        assert_eq!(assembly.associable_retrieval_source_ids(), vec!["lexical"]);
    }

    #[test]
    fn retrieval_to_context_keeps_each_combination_policies_selected_ids() {
        let records = judged_records();
        for policy in [
            AdmissionCombinationPolicy::BoundedAdditive,
            AdmissionCombinationPolicy::ReservedSlots,
        ] {
            let retrieval = retrieve_memories(
                &RetrievalRequest::new(
                    &records,
                    &[],
                    "orchid",
                    RetrievalStrategy::KeywordTag,
                    2,
                    OffsetDateTime::UNIX_EPOCH + time::Duration::days(2_000),
                )
                .with_judge_verdicts(judged_verdicts())
                .with_combination_policy(policy),
            )
            .unwrap();
            let assembly = assemble_retrieval_context(&retrieval, ContextBudget::new(2, 200));
            let injected_ids = assembly.retrieved_memory_ids();
            let weak = assembly
                .selected
                .iter()
                .find(|selection| selection.fragment.fragment_id == "memory.orchid-weak")
                .unwrap();
            let top = assembly
                .selected
                .iter()
                .find(|selection| selection.fragment.fragment_id == "memory.orchid-top")
                .unwrap();

            let expected = match policy {
                AdmissionCombinationPolicy::BoundedAdditive => {
                    vec!["memory.orchid-top", "memory.orchid-weak"]
                }
                AdmissionCombinationPolicy::ReservedSlots => {
                    vec!["memory.orchid-top", "memory.orchid-weak"]
                }
            };
            assert_eq!(
                injected_ids,
                expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
            );
            assert!(!weak.fragment.associable);
            assert!(top.fragment.associable);
            assert_eq!(
                top.fragment.admission_basis,
                AdmissionBasis::LexicalAndJudge
            );
            assert!(
                top.fragment
                    .selection_reason
                    .contains("judge verdict: 4000 basis points")
            );
            assert!(weak.fragment.selection_reason.contains("judge verdict:"));
        }
    }

    #[test]
    fn reserved_slot_order_overrides_context_score_sort_under_budget_pressure() {
        let evaluation_time = OffsetDateTime::UNIX_EPOCH + time::Duration::days(2_000);
        let mut records = (b'a'..=b'f')
            .map(|letter| {
                MemoryRecord::new(
                    format!("memory.{}", char::from(letter)),
                    MemoryRecordKind::Observation,
                    "orchid",
                    "orchid note",
                    vec![],
                    evaluation_time,
                    0.0,
                    0,
                    "tests",
                    10,
                )
            })
            .collect::<Vec<_>>();
        for id in ["memory.y", "memory.z"] {
            records.push(MemoryRecord::new(
                id,
                MemoryRecordKind::Observation,
                "unrelated",
                "other note",
                vec![],
                evaluation_time,
                0.0,
                0,
                "tests",
                10,
            ));
        }
        let mut verdicts = judged_verdicts();
        let exemplar = verdicts.values().next().unwrap().clone();
        verdicts.clear();
        verdicts.insert(
            "memory.y".to_owned(),
            qsf_memory::JudgeVerdict {
                score_basis_points: 8_000,
                ..exemplar.clone()
            },
        );
        verdicts.insert(
            "memory.z".to_owned(),
            qsf_memory::JudgeVerdict {
                score_basis_points: 9_000,
                ..exemplar
            },
        );
        let retrieval = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[],
                "orchid",
                RetrievalStrategy::KeywordTag,
                8,
                evaluation_time,
            )
            .with_judge_verdicts(verdicts)
            .with_combination_policy(AdmissionCombinationPolicy::ReservedSlots),
        )
        .unwrap();
        let fragments = retrieval
            .selected
            .iter()
            .map(ContextFragment::from)
            .collect::<Vec<_>>();
        let legacy_score_order = assemble_context(fragments.clone(), ContextBudget::new(8, 200));
        let ordered = assemble_retrieval_context(&retrieval, ContextBudget::new(8, 200));

        assert_eq!(
            legacy_score_order.retrieved_memory_ids(),
            [
                "memory.a", "memory.b", "memory.c", "memory.d", "memory.e", "memory.f", "memory.y",
                "memory.z"
            ]
        );
        assert_eq!(
            ordered.retrieved_memory_ids(),
            [
                "memory.a", "memory.b", "memory.c", "memory.d", "memory.e", "memory.f", "memory.z",
                "memory.y"
            ]
        );
    }

    fn judged_records() -> Vec<MemoryRecord> {
        let evaluation_time = OffsetDateTime::UNIX_EPOCH + time::Duration::days(2_000);
        let top = MemoryRecord::new(
            "memory.orchid-top",
            MemoryRecordKind::Observation,
            "orchid archive",
            "A strong lexical orchid match.",
            vec!["orchid"],
            evaluation_time,
            0.8,
            0,
            "context-test",
            10,
        );
        let second = MemoryRecord::new(
            "memory.orchid-second",
            MemoryRecordKind::Observation,
            "orchid note",
            "A second lexical orchid match.",
            vec![],
            evaluation_time,
            0.5,
            0,
            "context-test",
            10,
        );
        let weak = MemoryRecord::new(
            "memory.orchid-weak",
            MemoryRecordKind::Observation,
            "orchid trace",
            "An old weak lexical match.",
            vec![],
            evaluation_time - time::Duration::days(3_650),
            0.0,
            0,
            "context-test",
            10,
        );
        let zero_signal = MemoryRecord::new(
            "memory.zero-signal",
            MemoryRecordKind::Observation,
            "botanical practice",
            "Notes about tending plants.",
            vec![],
            evaluation_time,
            0.8,
            0,
            "context-test",
            10,
        );
        vec![top, second, weak, zero_signal]
    }

    fn judged_verdicts() -> std::collections::BTreeMap<String, JudgeVerdict> {
        [
            ("memory.orchid-top", 4_000),
            ("memory.orchid-weak", 10_000),
            ("memory.zero-signal", 9_000),
        ]
        .into_iter()
        .map(|(memory_id, score_basis_points)| {
            (
                memory_id.to_owned(),
                JudgeVerdict {
                    score_basis_points,
                    model_identity: ModelIdentity {
                        backend: BackendKind::Fixture,
                        model_id: "fixture-memory-judge".to_owned(),
                        identity_kind: ModelIdentityKind::Fixture,
                        identity_value: "fixture-v1".to_owned(),
                    },
                    question_wording_version: "memory-relevance-v1".to_owned(),
                    backend_kind: BackendKind::Fixture,
                },
            )
        })
        .collect()
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
            admission_basis: AdmissionBasis::Lexical,
            associable: true,
        }
    }
}
