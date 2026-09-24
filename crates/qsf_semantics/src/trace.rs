//! Serializable traces for pair scoring and relevance-judgment lifecycle facts.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::pair_scoring::{CandidateKind, PairScore, QuestionShaping, RelevanceTask, ScoreKind};

static BACKEND_INSTANCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn invocation_prefix(backend: &str) -> String {
    let started_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let instance = BACKEND_INSTANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{backend}-{}-{started_nanos}-{instance}",
        std::process::id()
    )
}

/// The backend that produced a semantic trace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Deterministic, non-evidentiary fixture backend.
    Fixture,
    /// Hosted System One HTTP service.
    RemoteHttp,
    /// Future local encoder backend.
    LocalEncoder,
}

/// How a model identity value should be interpreted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelIdentityKind {
    /// A configured remote model version.
    PinnedRemoteVersion,
    /// An immutable local artifact digest.
    LocalArtifactDigest,
    /// Deterministic fixture implementation.
    Fixture,
}

/// Model identity recorded with each invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelIdentity {
    /// Backend name.
    pub backend: BackendKind,
    /// Human-readable model identifier.
    pub model_id: String,
    /// Identity representation.
    pub identity_kind: ModelIdentityKind,
    /// Resolved identity value supplied by the backend.
    pub identity_value: String,
}

/// Parsed token counts alongside an unmodified provider usage object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedUsage {
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
}

/// Service-specific payload fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceTracePayload {
    /// Number of HTTP attempts.
    pub attempt_count: u32,
    /// Retryable failure names observed before completion.
    pub retry_reasons: Vec<String>,
    /// Original provider usage object, never normalized or reconstructed.
    pub usage_raw: Option<Value>,
    /// Parsed token counts when provider usage was present.
    pub usage_parsed: Option<ParsedUsage>,
    /// Model requested by configuration, retained if the response resolved differently.
    pub requested_model_id: String,
    /// Every distinct model identity resolved across the invocation's hosted calls.
    pub resolved_model_ids: Vec<String>,
}

/// Semantic operation performed by an invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticOperation {
    /// Embed one input.
    Embed,
    /// Embed a batch of inputs.
    EmbedBatch,
    /// Score an utterance/candidate pair.
    PairScore,
    /// Apply a semantic classifier.
    Classify,
}

/// Local encoder-specific payload fields, reserved for local backends.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalEncoderTracePayload {
    /// Signed similarity basis points; cosine similarity can be negative.
    pub embedding_score_basis_points: Option<i32>,
    /// Source of the candidate embedding.
    pub candidate_embedding_source: Option<String>,
}

/// One backend invocation record, present on both success and failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTraceRecord {
    /// Stable invocation correlation id.
    pub invocation_id: String,
    /// Semantic task whose operating point applies.
    pub task: RelevanceTask,
    /// Operation performed by the backend.
    pub operation: SemanticOperation,
    /// Backend discriminator.
    pub backend_kind: BackendKind,
    /// Model identity observed by the backend.
    pub model_identity: ModelIdentity,
    /// Request shaping used.
    pub shaping: QuestionShaping,
    /// Version of the question wording.
    pub question_wording_version: String,
    /// Expected score kind.
    pub score_kind: ScoreKind,
    /// Injection deadline supplied by the caller.
    pub injection_deadline_ms: u64,
    /// Request timeout used by an I/O backend.
    pub request_timeout_ms: Option<u64>,
    /// End-to-end operation latency.
    pub latency_micros: u64,
    /// Typed failure, present exactly when the outcome failed.
    pub failure: Option<SemanticFailure>,
    /// Service-only payload.
    pub service: Option<ServiceTracePayload>,
    /// Local encoder-only payload.
    pub local_encoder: Option<LocalEncoderTracePayload>,
}

impl SemanticTraceRecord {
    /// Builds a minimal trace for an adapter failure before a backend can build one.
    pub fn unavailable(
        task: RelevanceTask,
        operation: SemanticOperation,
        failure: SemanticFailure,
    ) -> Self {
        Self {
            invocation_id: "unavailable".to_owned(),
            task,
            operation,
            backend_kind: BackendKind::Fixture,
            model_identity: ModelIdentity {
                backend: BackendKind::Fixture,
                model_id: "unavailable".to_owned(),
                identity_kind: ModelIdentityKind::Fixture,
                identity_value: "unavailable".to_owned(),
            },
            shaping: QuestionShaping::SharedStateQuestions,
            question_wording_version: "unavailable".to_owned(),
            score_kind: ScoreKind::Probability,
            injection_deadline_ms: 0,
            request_timeout_ms: None,
            latency_micros: 0,
            failure: Some(failure),
            service: None,
            local_encoder: None,
        }
    }
}

/// A traced outcome. A failed semantic call is never traceless.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Traced<T> {
    /// Complete record of the backend invocation.
    pub trace: SemanticTraceRecord,
    /// Successful output or typed backend failure.
    pub outcome: Result<T, SemanticFailure>,
}

impl<T> Traced<T> {
    /// Pairs a trace with a successful output.
    pub fn success(trace: SemanticTraceRecord, value: T) -> Self {
        Self {
            trace,
            outcome: Ok(value),
        }
    }

    /// Pairs a complete trace with a typed failure.
    pub fn failure_with_trace(trace: SemanticTraceRecord, failure: SemanticFailure) -> Self {
        Self {
            trace,
            outcome: Err(failure),
        }
    }

    /// Creates a minimal trace for a failure outside a concrete backend invocation.
    pub fn failure(
        task: RelevanceTask,
        operation: SemanticOperation,
        failure: SemanticFailure,
    ) -> Self {
        Self::failure_with_trace(
            SemanticTraceRecord::unavailable(task, operation, failure.clone()),
            failure,
        )
    }
}

/// Every failure the semantic seam can report.
///
/// Local backends can emit the asset and inference variants; service backends can
/// emit the HTTP, timeout, transport, decode, availability, and cancellation variants.
#[derive(Clone, Debug, Deserialize, Error, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticFailure {
    /// Required local assets are absent.
    #[error("semantic assets are missing: {detail}")]
    AssetsMissing {
        /// Detail identifying the missing asset.
        detail: String,
    },
    /// A local asset hash differs from its manifest.
    #[error("semantic asset hash mismatch: {detail}")]
    HashMismatch {
        /// Detail identifying the mismatch.
        detail: String,
    },
    /// A local model could not be loaded.
    #[error("semantic model load failed: {detail}")]
    LoadFailed {
        /// Load failure detail.
        detail: String,
    },
    /// Input tokenization failed.
    #[error("semantic tokenization failed: {detail}")]
    TokenizationFailed {
        /// Tokenization failure detail.
        detail: String,
    },
    /// Local inference failed.
    #[error("semantic inference failed: {detail}")]
    InferenceFailed {
        /// Inference failure detail.
        detail: String,
    },
    /// A local classifier head does not fit the runtime.
    #[error("semantic head is incompatible: {detail}")]
    IncompatibleHead {
        /// Compatibility detail.
        detail: String,
    },
    /// Hosted credentials were rejected.
    #[error("semantic service unauthorized")]
    Unauthorized,
    /// The hosted request was invalid.
    #[error("semantic service rejected the request: {detail}")]
    InvalidRequest {
        /// Provider response detail.
        detail: String,
    },
    /// The hosted service rate limited the request.
    #[error("semantic service rate limited the request")]
    RateLimited,
    /// The hosted service was overloaded.
    #[error("semantic service is overloaded")]
    Overloaded,
    /// An individual service request exceeded its request timeout.
    #[error("semantic service request timed out")]
    Timeout,
    /// Transport failed before a usable response arrived.
    #[error("semantic transport failed: {detail}")]
    Transport {
        /// Transport failure detail.
        detail: String,
    },
    /// The hosted response did not match its documented shape.
    #[error("semantic service response could not be decoded: {detail}")]
    Decode {
        /// Decode failure detail.
        detail: String,
    },
    /// A selected backend cannot currently operate.
    #[error("semantic backend unavailable: {detail}")]
    BackendUnavailable {
        /// Availability failure detail.
        detail: String,
    },
    /// The caller deliberately cancelled the invocation.
    #[error("semantic invocation cancelled: {detail}")]
    Cancelled {
        /// Cancellation reason.
        detail: String,
    },
}

/// Identity fields for correlating multiple invocations around a turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationIdentity {
    /// Caller-owned session id.
    pub qsf_session_id: String,
    /// Attachment generation.
    pub attachment_epoch: u64,
    /// Exchange number.
    pub exchange_index: u64,
    /// Revision of input within the exchange.
    pub input_revision: u64,
    /// Invocation sequence number.
    pub invocation_seq: u64,
}

/// Candidate descriptor recorded without candidate text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedCandidate {
    /// Caller-selected candidate id.
    pub candidate_id: String,
    /// Content hash of the text sent to the judge.
    pub candidate_content_hash: String,
    /// Candidate kind.
    pub candidate_kind: CandidateKind,
}

/// Requested lifecycle record emitted before a backend effect starts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelevanceJudgmentRequestedRecord {
    /// Schema version for this lifecycle record.
    pub schema_version: u32,
    /// Relevance task family.
    pub task: RelevanceTask,
    /// Selected backend.
    pub backend_kind: BackendKind,
    /// Invocation id.
    pub invocation_id: String,
    /// Invocation identity.
    pub identity: InvocationIdentity,
    /// Hash of the entire request.
    pub request_hash: String,
    /// Hash of utterance input.
    pub utterance_content_hash: String,
    /// Input classification, such as `final_transcript`.
    pub input_kind: String,
    /// Fraction of transcript available.
    pub transcript_fraction_milli: u16,
    /// Versioned context specification.
    pub context_spec: String,
    /// Retrieval-query hash.
    pub retrieval_query_hash: String,
    /// Whether a volition hint was consumed.
    pub volition_hint_consumed: bool,
    /// Number of candidates.
    pub candidate_count: usize,
    /// Candidate-selection description.
    pub candidate_selection: String,
    /// Number of source store records.
    pub store_record_count: usize,
    /// Candidates represented by id and hash only.
    pub candidates: Vec<RequestedCandidate>,
    /// Request shaping.
    pub shaping: QuestionShaping,
    /// Question wording version.
    pub question_wording_version: String,
    /// Model identity expected at the operating point.
    pub model_identity: ModelIdentity,
    /// Caller injection deadline.
    pub injection_deadline_ms: u64,
    /// Backend request timeout.
    pub request_timeout_ms: u64,
}

/// A candidate completion value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedCandidate {
    /// Candidate id.
    pub candidate_id: String,
    /// Probability in basis points, if judged.
    pub probability_basis_points: Option<u16>,
    /// Whether this candidate got a verdict.
    pub judged: bool,
    /// Reason a candidate was unjudged.
    pub unjudged_reason: Option<String>,
}

/// How a completed invocation was handled by its caller.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionDisposition {
    /// Used before the current turn's injection deadline.
    UsedSameTurn,
    /// Kept to carry into a later turn.
    LateCarriedOver,
    /// Finished late but was not retained.
    LateDropped,
    /// Identity did not match current runtime state.
    StaleRejected,
    /// Caller cancelled it.
    Cancelled,
    /// A newer input revision superseded it.
    SupersededByRevision,
}

/// Completion lifecycle record emitted for both success and failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelevanceJudgmentCompletedRecord {
    /// Invocation id.
    pub invocation_id: String,
    /// Invocation identity.
    pub identity: InvocationIdentity,
    /// Per-candidate outcomes.
    pub candidates: Vec<CompletedCandidate>,
    /// Invocation latency.
    pub latency_micros: u64,
    /// Number of service attempts.
    pub attempt_count: u32,
    /// Retry reason names.
    pub retry_reasons: Vec<String>,
    /// Original provider usage object.
    pub usage_raw: Option<Value>,
    /// Parsed provider token counts.
    pub usage_parsed: Option<ParsedUsage>,
    /// Typed failure on failed calls.
    pub failure_reason: Option<SemanticFailure>,
    /// Caller disposition.
    pub completion_disposition: CompletionDisposition,
}

/// A consulted judgment invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsultedInvocation {
    /// Invocation id.
    pub invocation_id: String,
    /// Whether it was current or carried.
    pub role: String,
    /// Source exchange when carried.
    pub carried_from_exchange_index: Option<u64>,
    /// Age in turns when carried.
    pub carry_over_age_turns: Option<u64>,
}

/// Candidate selection outcome recorded by a caller after injection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionCandidate {
    /// Candidate id.
    pub candidate_id: String,
    /// Whether the candidate passed the relevance gate, even if a selection limit cut it.
    pub admitted: bool,
    /// Caller-owned gate-passage explanation; absent when the gate rejected the candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_basis: Option<String>,
    /// Whether durable structure may consume it.
    pub associable: bool,
    /// Omission reason when not selected.
    pub skip_reason: Option<String>,
}

/// Budget in force for one injection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionBudget {
    /// Fragment limit.
    pub fragments: u32,
    /// Token limit.
    pub tokens: u32,
}

/// Goal-specific facts recorded only when a goal-family selection is evaluated.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalFamilyExtra {
    /// Goal qualification threshold in integer basis points.
    pub qualification_threshold_in_force: u16,
    /// Goal ids qualified by the judge.
    pub judge_qualified_goal_ids: Vec<String>,
    /// Whether judge input changed the arbitration winner.
    pub arbitration_winner_changed_by_judge: bool,
    /// Caller-owned description of the allowed durable mutation scope.
    pub durable_mutation_scope: String,
}

/// Selection lifecycle record, exactly one per injected turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelevanceSelectionRecordedRecord {
    /// Exchange number.
    pub exchange_index: u64,
    /// Request hash.
    pub request_hash: String,
    /// Current and carried invocations consulted.
    pub consulted_invocations: Vec<ConsultedInvocation>,
    /// Caller deadline outcome.
    pub deadline_outcome: String,
    /// Whether deterministic fallback ran.
    pub fallback_executed: bool,
    /// Fallback reason when it ran.
    pub fallback_reason: Option<String>,
    /// Candidate admission results.
    pub candidates: Vec<SelectionCandidate>,
    /// Lexical-only selected ids.
    pub lexical_only_selected_ids: Vec<String>,
    /// Threshold represented in basis points.
    pub judge_admission_threshold_basis_points: u16,
    /// Caller-chosen combination policy name.
    pub combination_policy_in_force: String,
    /// Injection budget.
    pub injection_budget_in_force: InjectionBudget,
    /// Injected fragment ids.
    pub injected_fragment_ids: Vec<String>,
    /// Used estimated token count.
    pub used_estimated_tokens: u32,
    /// Candidate ids omitted due to budget.
    pub omitted_by_budget: Vec<String>,
    /// Explicit evaluation time string owned by the caller.
    pub evaluation_time: String,
    /// Goal-only selection facts; absent for memory-family selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_family_extra: Option<GoalFamilyExtra>,
}

/// A JSONL line emitted by the semantic artifact writer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "record_type", content = "record", rename_all = "snake_case")]
pub enum SemanticLifecycleRecord {
    /// Request started.
    RelevanceJudgmentRequested(RelevanceJudgmentRequestedRecord),
    /// Request completed.
    RelevanceJudgmentCompleted(RelevanceJudgmentCompletedRecord),
    /// Selection was recorded.
    RelevanceSelectionRecorded(RelevanceSelectionRecordedRecord),
}

/// Converts a traced scoring result into completed-candidate lifecycle values.
pub fn completed_candidates(
    request: &crate::pair_scoring::PairScoreRequest,
    traced: &Traced<Vec<PairScore>>,
) -> Vec<CompletedCandidate> {
    match &traced.outcome {
        Ok(scores) => scores
            .iter()
            .map(|score| CompletedCandidate {
                candidate_id: score.candidate_id.clone(),
                probability_basis_points: (!score.abstained).then_some(score.score_basis_points),
                judged: !score.abstained,
                unjudged_reason: score.abstain_reason.clone(),
            })
            .collect(),
        Err(failure) => request
            .candidates
            .iter()
            .map(|candidate| CompletedCandidate {
                candidate_id: candidate.candidate_id.clone(),
                probability_basis_points: None,
                judged: false,
                unjudged_reason: Some(failure.to_string()),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn identity() -> InvocationIdentity {
        InvocationIdentity {
            qsf_session_id: "session".to_owned(),
            attachment_epoch: 1,
            exchange_index: 2,
            input_revision: 0,
            invocation_seq: 1,
        }
    }

    fn model_identity() -> ModelIdentity {
        ModelIdentity {
            backend: BackendKind::Fixture,
            model_id: "fixture".to_owned(),
            identity_kind: ModelIdentityKind::Fixture,
            identity_value: "fixture_relevance_v1".to_owned(),
        }
    }

    fn requested() -> RelevanceJudgmentRequestedRecord {
        RelevanceJudgmentRequestedRecord {
            schema_version: 1,
            task: RelevanceTask::MemoryRelevance,
            backend_kind: BackendKind::Fixture,
            invocation_id: "invocation".to_owned(),
            identity: identity(),
            request_hash: "request".to_owned(),
            utterance_content_hash: "utterance".to_owned(),
            input_kind: "final_transcript".to_owned(),
            transcript_fraction_milli: 1000,
            context_spec: "context_v1".to_owned(),
            retrieval_query_hash: "query".to_owned(),
            volition_hint_consumed: false,
            candidate_count: 1,
            candidate_selection: "all_records".to_owned(),
            store_record_count: 1,
            candidates: vec![RequestedCandidate {
                candidate_id: "candidate".to_owned(),
                candidate_content_hash: "candidate-hash".to_owned(),
                candidate_kind: CandidateKind("memory".to_owned()),
            }],
            shaping: QuestionShaping::SharedStateQuestions,
            question_wording_version: "relevance_noul_v1".to_owned(),
            model_identity: model_identity(),
            injection_deadline_ms: 10,
            request_timeout_ms: 20,
        }
    }

    fn completed() -> RelevanceJudgmentCompletedRecord {
        RelevanceJudgmentCompletedRecord {
            invocation_id: "invocation".to_owned(),
            identity: identity(),
            candidates: vec![CompletedCandidate {
                candidate_id: "candidate".to_owned(),
                probability_basis_points: None,
                judged: false,
                unjudged_reason: Some("timeout".to_owned()),
            }],
            latency_micros: 20_000,
            attempt_count: 1,
            retry_reasons: Vec::new(),
            usage_raw: None,
            usage_parsed: None,
            failure_reason: Some(SemanticFailure::Timeout),
            completion_disposition: CompletionDisposition::LateCarriedOver,
        }
    }

    fn selection(goal_family_extra: Option<GoalFamilyExtra>) -> RelevanceSelectionRecordedRecord {
        RelevanceSelectionRecordedRecord {
            exchange_index: 2,
            request_hash: "request".to_owned(),
            consulted_invocations: vec![ConsultedInvocation {
                invocation_id: "invocation".to_owned(),
                role: "current".to_owned(),
                carried_from_exchange_index: None,
                carry_over_age_turns: None,
            }],
            deadline_outcome: "failed".to_owned(),
            fallback_executed: true,
            fallback_reason: Some("timeout".to_owned()),
            candidates: vec![SelectionCandidate {
                candidate_id: "candidate".to_owned(),
                admitted: false,
                admission_basis: None,
                associable: false,
                skip_reason: Some("timeout".to_owned()),
            }],
            lexical_only_selected_ids: Vec::new(),
            judge_admission_threshold_basis_points: 5000,
            combination_policy_in_force: "bounded_additive".to_owned(),
            injection_budget_in_force: InjectionBudget {
                fragments: 4,
                tokens: 600,
            },
            injected_fragment_ids: Vec::new(),
            used_estimated_tokens: 0,
            omitted_by_budget: Vec::new(),
            evaluation_time: "2026-09-21T00:00:00Z".to_owned(),
            goal_family_extra,
        }
    }

    #[test]
    fn lifecycle_records_reject_unknown_fields_and_round_trip() {
        let records = [
            SemanticLifecycleRecord::RelevanceJudgmentRequested(requested()),
            SemanticLifecycleRecord::RelevanceJudgmentCompleted(completed()),
            SemanticLifecycleRecord::RelevanceSelectionRecorded(selection(None)),
            SemanticLifecycleRecord::RelevanceSelectionRecorded(selection(Some(GoalFamilyExtra {
                qualification_threshold_in_force: 6_000,
                judge_qualified_goal_ids: vec!["goal-a".to_owned()],
                arbitration_winner_changed_by_judge: true,
                durable_mutation_scope: "qualified_winner_only".to_owned(),
            }))),
        ];
        for record in records {
            let serialized = serde_json::to_string(&record).expect("serialize lifecycle record");
            let parsed: SemanticLifecycleRecord =
                serde_json::from_str(&serialized).expect("round trip lifecycle record");
            assert_eq!(parsed, record);
            let mut value: Value = serde_json::from_str(&serialized).expect("json value");
            value["record"]["unexpected"] = json!(true);
            assert!(serde_json::from_value::<SemanticLifecycleRecord>(value).is_err());
        }
    }

    #[test]
    fn absent_goal_family_extra_is_omitted_and_defaults_on_read() {
        let record = SemanticLifecycleRecord::RelevanceSelectionRecorded(selection(None));
        let serialized = serde_json::to_value(&record).expect("serialize lifecycle record");
        assert!(serialized["record"].get("goal_family_extra").is_none());
        let parsed: SemanticLifecycleRecord =
            serde_json::from_value(serialized).expect("deserialize without goal extra");
        assert_eq!(parsed, record);
    }
}
