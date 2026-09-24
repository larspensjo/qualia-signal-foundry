use std::time::Duration;

use qsf_context::{ContextAssembly, ContextBudget};
use qsf_diagnostics::{
    DiagnosticRecord, MemorySelectionAssociationPath, MemorySelectionCandidate,
    MemorySelectionInjectionBudget, MemorySelectionRecordedRecord, MemorySelectionScore,
};
use qsf_memory::{
    DEFAULT_ADMISSION_COMBINATION_POLICY, RETRIEVAL_LIMIT_SKIP_REASON, RetrievalResult,
    RetrievalStrategy, RetrievedMemory,
};
use serde::Serialize;
use time::OffsetDateTime;

pub(crate) struct MemorySelectionRecordInput<'a> {
    pub qsf_session_id: &'a str,
    pub exchange_index: usize,
    pub request_hash: &'a str,
    pub retrieval: Option<&'a RetrievalResult>,
    pub assembly: &'a ContextAssembly,
    pub retrieval_strategy: RetrievalStrategy,
    pub retrieval_limit: usize,
    pub evaluation_time: OffsetDateTime,
    pub recorded_at: OffsetDateTime,
    pub retrieval_latency: Duration,
    pub retrieval_error: Option<&'a str>,
}

/// Derives the durable per-turn selection record without performing I/O.
pub(crate) fn build_memory_selection_record(
    input: MemorySelectionRecordInput<'_>,
) -> DiagnosticRecord {
    let mut candidates = Vec::new();
    if let Some(retrieval) = input.retrieval {
        candidates.extend(
            retrieval
                .selected
                .iter()
                .map(|memory| memory_selection_candidate(memory, true)),
        );
        candidates.extend(
            retrieval
                .omitted
                .iter()
                .map(|memory| memory_selection_candidate(memory, false)),
        );
    }

    let (lexical_only_selected_ids, combination_policy_in_force) = input
        .retrieval
        .map(|retrieval| {
            (
                retrieval.lexical_only_selected_ids.clone(),
                serialized_enum_name(retrieval.combination_policy),
            )
        })
        .unwrap_or_else(|| {
            (
                Vec::new(),
                serialized_enum_name(DEFAULT_ADMISSION_COMBINATION_POLICY),
            )
        });
    let strategy = input
        .retrieval
        .map(|retrieval| retrieval.strategy)
        .unwrap_or(input.retrieval_strategy);

    DiagnosticRecord::MemorySelectionRecorded(MemorySelectionRecordedRecord {
        qsf_session_id: input.qsf_session_id.to_owned(),
        exchange_index: input.exchange_index,
        request_hash: input.request_hash.to_owned(),
        recorded_at: input.recorded_at,
        candidates,
        lexical_only_selected_ids,
        combination_policy_in_force,
        injection_budget_in_force: injection_budget(input.assembly.budget),
        injected_fragment_ids: input
            .assembly
            .selected
            .iter()
            .map(|selection| selection.fragment.fragment_id.clone())
            .collect(),
        used_estimated_tokens: input.assembly.used_estimated_tokens,
        omitted_by_budget: input
            .assembly
            .omitted
            .iter()
            .map(|omission| omission.fragment.fragment_id.clone())
            .collect(),
        evaluation_time: input.evaluation_time,
        strategy: serialized_enum_name(strategy),
        retrieval_limit_in_force: input.retrieval_limit,
        retrieval_latency_micros: u64::try_from(input.retrieval_latency.as_micros())
            .unwrap_or(u64::MAX),
        retrieval_error: input.retrieval_error.map(str::to_owned),
    })
}

fn memory_selection_candidate(
    memory: &RetrievedMemory,
    selected: bool,
) -> MemorySelectionCandidate {
    let admitted = memory.skip_reason.is_none()
        || memory.skip_reason.as_deref() == Some(RETRIEVAL_LIMIT_SKIP_REASON);
    let selection_eligibility = if selected {
        serialized_enum_name(memory.selection_eligibility)
    } else {
        serialized_enum_name(qsf_memory::SelectionEligibility::NotSelected)
    };
    MemorySelectionCandidate {
        candidate_id: memory.memory.id.clone(),
        admitted,
        admission_basis: admitted.then(|| serialized_enum_name(memory.admission_basis)),
        associable: selected && memory.selection_eligibility.is_associable(),
        selection_eligibility,
        skip_reason: memory.skip_reason.clone(),
        score: MemorySelectionScore {
            total: memory.score.total,
            recency: memory.score.recency,
            keyword: memory.score.keyword,
            tag: memory.score.tag,
            association: memory.score.association,
            importance: memory.score.importance,
            reinforcement: memory.score.reinforcement,
            judge: memory.score.judge,
        },
        matched_terms: memory.matched_terms.clone(),
        association_paths: memory
            .association_paths
            .iter()
            .map(|path| MemorySelectionAssociationPath {
                from_memory_id: path.from_memory_id.clone(),
                to_memory_id: path.to_memory_id.clone(),
                weight: path.weight,
                reason: path.reason.clone(),
            })
            .collect(),
        judge_verdict_basis_points: memory.judge_verdict_basis_points,
    }
}

fn injection_budget(budget: ContextBudget) -> MemorySelectionInjectionBudget {
    MemorySelectionInjectionBudget {
        fragments: budget.max_fragments,
        tokens: budget.max_estimated_tokens,
    }
}

fn serialized_enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("retrieval enum serializes")
        .as_str()
        .expect("retrieval enum serializes as a name")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use qsf_context::assemble_retrieval_context;
    use qsf_diagnostics::{DiagnosticRecord, MEMORY_SELECTION_RECORDED_KIND, decode_envelope};
    use qsf_memory::{
        AdmissionCombinationPolicy, Association, JudgeVerdict, MemoryRecord, MemoryRecordKind,
        RetrievalRequest, retrieve_memories,
    };
    use qsf_semantics::trace::{BackendKind, ModelIdentity, ModelIdentityKind};
    use tempfile::TempDir;
    use time::OffsetDateTime;

    use super::*;

    fn memory(id: &str, title: &str, tags: Vec<&str>) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Concept,
            title,
            format!("A detailed summary of {title}"),
            tags,
            OffsetDateTime::UNIX_EPOCH,
            0.5,
            0,
            "tests",
            16,
        )
    }

    #[test]
    fn generated_diagnostics_ledger_round_trips_live_memory_selection_provenance() {
        let records = vec![
            memory("lexical-hit", "alpha memory", vec!["alpha"]),
            memory("association-hit", "connected memory", vec!["connected"]),
            memory("judge-admitted", "thematic context", vec!["context"]),
            memory("irrelevant", "unrelated note", vec!["unrelated"]),
        ];
        let verdicts = BTreeMap::from([
            (
                "judge-admitted".to_owned(),
                JudgeVerdict {
                    score_basis_points: 8_000,
                    model_identity: ModelIdentity {
                        backend: BackendKind::Fixture,
                        model_id: "fixture".to_owned(),
                        identity_kind: ModelIdentityKind::Fixture,
                        identity_value: "selection-test".to_owned(),
                    },
                    question_wording_version: "memory-v1".to_owned(),
                    backend_kind: BackendKind::Fixture,
                },
            ),
            (
                "irrelevant".to_owned(),
                JudgeVerdict {
                    score_basis_points: 1_000,
                    model_identity: ModelIdentity {
                        backend: BackendKind::Fixture,
                        model_id: "fixture".to_owned(),
                        identity_kind: ModelIdentityKind::Fixture,
                        identity_value: "selection-test".to_owned(),
                    },
                    question_wording_version: "memory-v1".to_owned(),
                    backend_kind: BackendKind::Fixture,
                },
            ),
        ]);
        let evaluation_time = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let retrieval = retrieve_memories(
            &RetrievalRequest::new(
                &records,
                &[Association::new(
                    "lexical-hit",
                    "association-hit",
                    0.9,
                    "diagnostic path fixture",
                    OffsetDateTime::UNIX_EPOCH,
                )],
                "alpha",
                RetrievalStrategy::AssociationWeighted,
                3,
                evaluation_time,
            )
            .with_judge_verdicts(verdicts)
            .with_combination_policy(AdmissionCombinationPolicy::ReservedSlots),
        )
        .unwrap();
        let assembly = assemble_retrieval_context(&retrieval, ContextBudget::new(2, 32));
        let latency = Duration::from_millis(17);
        let diagnostic = build_memory_selection_record(MemorySelectionRecordInput {
            qsf_session_id: "session-ledger",
            exchange_index: 9,
            request_hash: "request-hash",
            retrieval: Some(&retrieval),
            assembly: &assembly,
            retrieval_strategy: RetrievalStrategy::AssociationWeighted,
            retrieval_limit: 3,
            evaluation_time,
            recorded_at: evaluation_time,
            retrieval_latency: latency,
            retrieval_error: None,
        });

        let tempdir = TempDir::new().unwrap();
        let writer = qsf_diagnostics::DiagnosticWriter::create(
            tempdir.path().join("diagnostics/session-ledger.jsonl"),
        )
        .unwrap();
        writer.write(&diagnostic).unwrap();
        let ledger = std::fs::read_to_string(writer.path()).unwrap();
        assert_eq!(
            decode_envelope(ledger.trim())
                .and_then(|envelope| envelope.kind)
                .as_deref(),
            Some(MEMORY_SELECTION_RECORDED_KIND)
        );
        let parsed: DiagnosticRecord = serde_json::from_str(ledger.trim()).unwrap();
        let DiagnosticRecord::MemorySelectionRecorded(record) = parsed else {
            panic!("writer emitted an unexpected diagnostics record kind")
        };

        assert_eq!(record.qsf_session_id, "session-ledger");
        assert_eq!(record.exchange_index, 9);
        assert_eq!(record.retrieval_limit_in_force, 3);
        assert_eq!(record.evaluation_time, evaluation_time);
        assert_eq!(record.recorded_at, evaluation_time);
        assert_eq!(record.retrieval_latency_micros, 17_000);
        assert_eq!(record.strategy, "association_weighted");
        assert_eq!(record.combination_policy_in_force, "reserved_slots");
        assert!(
            record
                .lexical_only_selected_ids
                .contains(&"lexical-hit".to_owned())
        );
        let lexical = record
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == "lexical-hit")
            .unwrap();
        assert!(lexical.matched_terms.iter().any(|term| term == "alpha"));
        assert!(lexical.score.keyword > 0.0);
        assert!(record.candidates.iter().any(|candidate| {
            candidate.candidate_id == "association-hit"
                && !candidate.association_paths.is_empty()
                && candidate.score.association > 0.0
        }));
        assert!(record.candidates.iter().any(|candidate| {
            candidate.candidate_id == "judge-admitted"
                && candidate.admitted
                && !candidate.associable
                && candidate.selection_eligibility == "judge_influenced"
        }));
        let omitted = record
            .candidates
            .iter()
            .filter(|candidate| !candidate.admitted)
            .collect::<Vec<_>>();
        assert!(!omitted.is_empty());
        assert!(
            omitted
                .iter()
                .all(|candidate| candidate.skip_reason.is_some())
        );
        assert_eq!(record.injection_budget_in_force.fragments, 2);
        assert_eq!(record.injection_budget_in_force.tokens, 32);
        assert_eq!(
            record.injected_fragment_ids,
            assembly
                .selected
                .iter()
                .map(|selection| selection.fragment.fragment_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(record.used_estimated_tokens, assembly.used_estimated_tokens);
        assert!(!record.omitted_by_budget.is_empty());
    }

    #[test]
    fn limit_cut_candidate_passed_gate_but_was_not_selected() {
        let records = vec![
            memory("first", "alpha first", vec!["alpha"]),
            memory("second", "alpha second", vec!["alpha"]),
            memory("rejected", "unrelated", vec!["unrelated"]),
        ];
        let evaluation_time = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let retrieval = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "alpha",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time,
        ))
        .unwrap();
        let assembly = assemble_retrieval_context(&retrieval, ContextBudget::new(1, 100));
        let DiagnosticRecord::MemorySelectionRecorded(record) =
            build_memory_selection_record(MemorySelectionRecordInput {
                qsf_session_id: "session",
                exchange_index: 0,
                request_hash: "hash",
                retrieval: Some(&retrieval),
                assembly: &assembly,
                retrieval_strategy: RetrievalStrategy::KeywordTag,
                retrieval_limit: 1,
                evaluation_time,
                recorded_at: evaluation_time,
                retrieval_latency: Duration::ZERO,
                retrieval_error: None,
            })
        else {
            panic!("wrong record kind")
        };
        let cut = record
            .candidates
            .iter()
            .find(|candidate| candidate.skip_reason.as_deref() == Some(RETRIEVAL_LIMIT_SKIP_REASON))
            .expect("limit-cut candidate");
        assert!(cut.admitted);
        assert_eq!(cut.admission_basis.as_deref(), Some("lexical"));
        assert!(!cut.associable);
        assert_eq!(cut.selection_eligibility, "not_selected");
        let rejected = record
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == "rejected")
            .unwrap();
        assert!(!rejected.admitted);
        assert!(rejected.admission_basis.is_none());
    }

    #[test]
    fn diagnostic_and_lifecycle_selection_share_serialized_field_names() {
        use qsf_semantics::trace::{
            InjectionBudget, RelevanceSelectionRecordedRecord, SelectionCandidate,
        };

        let lifecycle = RelevanceSelectionRecordedRecord {
            exchange_index: 0,
            request_hash: "hash".to_owned(),
            consulted_invocations: vec![],
            deadline_outcome: "not_used".to_owned(),
            fallback_executed: false,
            fallback_reason: None,
            candidates: vec![SelectionCandidate {
                candidate_id: "candidate".to_owned(),
                admitted: true,
                admission_basis: Some("lexical".to_owned()),
                associable: true,
                skip_reason: None,
            }],
            lexical_only_selected_ids: vec![],
            judge_admission_threshold_basis_points: 0,
            combination_policy_in_force: "bounded_additive".to_owned(),
            injection_budget_in_force: InjectionBudget {
                fragments: 1,
                tokens: 100,
            },
            injected_fragment_ids: vec![],
            used_estimated_tokens: 0,
            omitted_by_budget: vec![],
            evaluation_time: "2026-09-23T00:00:00Z".to_owned(),
            goal_family_extra: None,
        };
        let lifecycle = serde_json::to_value(lifecycle).unwrap();
        let records = vec![memory("candidate", "alpha", vec!["alpha"])];
        let evaluation_time = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let retrieval = retrieve_memories(&RetrievalRequest::new(
            &records,
            &[],
            "alpha",
            RetrievalStrategy::KeywordTag,
            1,
            evaluation_time,
        ))
        .unwrap();
        let assembly = assemble_retrieval_context(&retrieval, ContextBudget::new(1, 100));
        let diagnostic = build_memory_selection_record(MemorySelectionRecordInput {
            qsf_session_id: "session",
            exchange_index: 0,
            request_hash: "hash",
            retrieval: Some(&retrieval),
            assembly: &assembly,
            retrieval_strategy: RetrievalStrategy::KeywordTag,
            retrieval_limit: 1,
            evaluation_time,
            recorded_at: evaluation_time,
            retrieval_latency: Duration::ZERO,
            retrieval_error: None,
        });
        let diagnostic = serde_json::to_value(diagnostic).unwrap();
        let shared = [
            "exchange_index",
            "request_hash",
            "candidates",
            "lexical_only_selected_ids",
            "combination_policy_in_force",
            "injection_budget_in_force",
            "injected_fragment_ids",
            "used_estimated_tokens",
            "omitted_by_budget",
            "evaluation_time",
        ];
        for field in shared {
            assert!(lifecycle.get(field).is_some(), "missing lifecycle {field}");
            assert!(
                diagnostic.get(field).is_some(),
                "missing diagnostic {field}"
            );
        }
        let candidate = &lifecycle["candidates"][0];
        let diagnostic_candidate = &diagnostic["candidates"][0];
        for field in ["candidate_id", "admitted", "admission_basis", "associable"] {
            assert!(
                candidate.get(field).is_some(),
                "missing lifecycle candidate {field}"
            );
            assert!(
                diagnostic_candidate.get(field).is_some(),
                "missing diagnostic candidate {field}"
            );
        }
    }
}
