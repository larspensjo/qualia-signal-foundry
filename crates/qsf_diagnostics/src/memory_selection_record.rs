use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Retrieval score components persisted with one live memory selection candidate.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySelectionScore {
    pub total: f64,
    pub recency: f64,
    pub keyword: f64,
    pub tag: f64,
    pub association: f64,
    pub importance: f64,
    pub reinforcement: f64,
    pub judge: f64,
}

/// One edge in the association path that contributed to a candidate's retrieval score.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySelectionAssociationPath {
    pub from_memory_id: String,
    pub to_memory_id: String,
    pub weight: f64,
    pub reason: String,
}

/// A selected or omitted memory in the live retrieval result.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySelectionCandidate {
    pub candidate_id: String,
    /// Passed the relevance gate, including candidates cut by the retrieval limit.
    pub admitted: bool,
    /// Gate-passage explanation; absent for a gate-rejected candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_basis: Option<String>,
    pub associable: bool,
    pub selection_eligibility: String,
    pub skip_reason: Option<String>,
    pub score: MemorySelectionScore,
    pub matched_terms: Vec<String>,
    pub association_paths: Vec<MemorySelectionAssociationPath>,
    pub judge_verdict_basis_points: Option<u16>,
}

/// Fragment and token limits that were in force for context injection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemorySelectionInjectionBudget {
    pub fragments: usize,
    pub tokens: usize,
}

/// Persisted selection provenance for a trusted realtime turn without an active judge.
///
/// The overlapping fields deliberately follow `RelevanceSelectionRecordedRecord` names and
/// meanings so the lifecycle record can subsume this record when the live judge is introduced.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemorySelectionRecordedRecord {
    pub qsf_session_id: String,
    pub exchange_index: usize,
    pub request_hash: String,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    pub candidates: Vec<MemorySelectionCandidate>,
    pub lexical_only_selected_ids: Vec<String>,
    pub combination_policy_in_force: String,
    pub injection_budget_in_force: MemorySelectionInjectionBudget,
    pub injected_fragment_ids: Vec<String>,
    pub used_estimated_tokens: usize,
    pub omitted_by_budget: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub evaluation_time: OffsetDateTime,
    pub strategy: String,
    pub retrieval_limit_in_force: usize,
    /// Store load and retrieval, including the blocking-executor handoff.
    pub retrieval_latency_micros: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_error: Option<String>,
}
