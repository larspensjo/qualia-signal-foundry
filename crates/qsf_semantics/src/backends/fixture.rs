//! Deterministic, explicitly non-evidentiary pair scoring.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use engine_logging::engine_info;
use sha2::{Digest, Sha256};

use crate::{
    pair_scoring::{PairScore, PairScoreRequest, PairScorer, ScoreKind},
    trace::{
        BackendKind, ModelIdentity, ModelIdentityKind, SemanticFailure, SemanticOperation,
        SemanticTraceRecord, Traced, invocation_prefix,
    },
};

/// Configuration for the deterministic fixture backend.
#[derive(Clone, Debug)]
pub struct FixtureRelevanceJudgeConfig {
    /// Explicit scores indexed by candidate content hash.
    pub verdict_table: BTreeMap<String, u16>,
    /// Artificial blocking latency used by timing tests and listening checks.
    pub synthetic_latency: Duration,
    /// Synthetic failure returned after any configured latency.
    pub synthetic_failure: Option<SemanticFailure>,
}

impl Default for FixtureRelevanceJudgeConfig {
    fn default() -> Self {
        Self {
            verdict_table: BTreeMap::new(),
            synthetic_latency: Duration::ZERO,
            synthetic_failure: None,
        }
    }
}

/// No-cost deterministic backend. Its scores are never relevance evidence.
#[derive(Clone, Debug)]
pub struct FixtureRelevanceJudge {
    config: FixtureRelevanceJudgeConfig,
    invocation_prefix: String,
    invocation_sequence: Arc<AtomicU64>,
}

impl FixtureRelevanceJudge {
    /// Creates a fixture backend and logs its non-evidentiary status prominently.
    pub fn new(config: FixtureRelevanceJudgeConfig) -> Self {
        engine_info!(
            "relevance judge backend=fixture selected; fixture verdicts are not relevance evidence"
        );
        Self {
            config,
            invocation_prefix: invocation_prefix("fixture"),
            invocation_sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Derives a stable probability from the two input content hashes.
    pub fn deterministic_basis_points(
        utterance_content_hash: &str,
        candidate_content_hash: &str,
    ) -> u16 {
        let mut hasher = Sha256::new();
        hasher.update(b"qsf_fixture_relevance_v1\0");
        hasher.update(utterance_content_hash.as_bytes());
        hasher.update([0]);
        hasher.update(candidate_content_hash.as_bytes());
        let digest = hasher.finalize();
        let value = u16::from_be_bytes([digest[0], digest[1]]) as u32;
        (value % 10_001) as u16
    }

    fn trace(
        &self,
        request: &PairScoreRequest,
        latency_micros: u64,
        failure: Option<SemanticFailure>,
    ) -> SemanticTraceRecord {
        SemanticTraceRecord {
            invocation_id: format!(
                "{}-{}",
                self.invocation_prefix,
                self.invocation_sequence.fetch_add(1, Ordering::Relaxed)
            ),
            task: request.task,
            operation: SemanticOperation::PairScore,
            backend_kind: BackendKind::Fixture,
            model_identity: ModelIdentity {
                backend: BackendKind::Fixture,
                model_id: "fixture".to_owned(),
                identity_kind: ModelIdentityKind::Fixture,
                identity_value: "fixture_relevance_v1".to_owned(),
            },
            shaping: request.options.shaping,
            question_wording_version: request.options.question_wording_version.clone(),
            score_kind: ScoreKind::Probability,
            injection_deadline_ms: 0,
            request_timeout_ms: None,
            latency_micros,
            failure,
            service: None,
            local_encoder: None,
        }
    }
}

impl PairScorer for FixtureRelevanceJudge {
    fn score_pairs(&self, request: PairScoreRequest) -> Traced<Vec<PairScore>> {
        let started = std::time::Instant::now();
        if !self.config.synthetic_latency.is_zero() {
            thread::sleep(self.config.synthetic_latency);
        }
        let latency_micros = started.elapsed().as_micros() as u64;
        if let Some(failure) = self.config.synthetic_failure.clone() {
            let trace = self.trace(&request, latency_micros, Some(failure.clone()));
            return Traced::failure_with_trace(trace, failure);
        }
        let scores = request
            .candidates
            .iter()
            .map(|candidate| PairScore {
                candidate_id: candidate.candidate_id.clone(),
                candidate_content_hash: candidate.candidate_content_hash.clone(),
                score_basis_points: self
                    .config
                    .verdict_table
                    .get(&candidate.candidate_content_hash)
                    .copied()
                    .unwrap_or_else(|| {
                        Self::deterministic_basis_points(
                            &request.utterance_content_hash,
                            &candidate.candidate_content_hash,
                        )
                    }),
                score_kind: ScoreKind::Probability,
                abstained: false,
                abstain_reason: None,
            })
            .collect();
        Traced::success(self.trace(&request, latency_micros, None), scores)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        backends::remote_http::QUESTION_WORDING_VERSION,
        pair_scoring::{
            Candidate, CandidateKind, PairScoringOptions, QuestionShaping, RelevanceTask,
        },
        trace::BackendKind,
    };

    use super::*;

    fn request() -> PairScoreRequest {
        PairScoreRequest {
            task: RelevanceTask::MemoryRelevance,
            utterance_text: "I need help with my garden".to_owned(),
            utterance_content_hash: "utterance-hash".to_owned(),
            candidates: vec![Candidate {
                candidate_id: "candidate-a".to_owned(),
                candidate_text: "The person grows tomatoes.".to_owned(),
                candidate_content_hash: "candidate-hash".to_owned(),
                candidate_kind: CandidateKind("memory".to_owned()),
            }],
            options: PairScoringOptions {
                question_wording_version: QUESTION_WORDING_VERSION.to_owned(),
                shaping: QuestionShaping::SharedStateQuestions,
                score_kind_expected: ScoreKind::Probability,
            },
        }
    }

    #[test]
    fn verdicts_are_deterministic_but_invocation_ids_are_unique() {
        let judge = FixtureRelevanceJudge::new(FixtureRelevanceJudgeConfig::default());
        let first = judge.score_pairs(request());
        let second = judge.score_pairs(request());
        assert_eq!(first.outcome, second.outcome);
        assert_ne!(first.trace.invocation_id, second.trace.invocation_id);
        assert_eq!(first.trace.backend_kind, BackendKind::Fixture);
    }
}
