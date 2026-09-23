//! The backend-independent contract for scoring an utterance against candidates.

use std::{future::Future, pin::Pin, time::Duration};

#[cfg(feature = "remote-http")]
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::trace::Traced;
#[cfg(feature = "remote-http")]
use crate::trace::{SemanticFailure, SemanticOperation};

/// The family of relevance question being asked.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelevanceTask {
    /// Whether a stored memory is about an utterance.
    MemoryRelevance,
    /// Whether a goal description is about an utterance.
    GoalRelevance,
}

/// The domain-neutral kind of a candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CandidateKind(pub String);

/// A domain adapter's rendered candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    /// Stable identifier selected by the caller.
    pub candidate_id: String,
    /// Text sent to the scoring backend.
    pub candidate_text: String,
    /// Content hash for the exact candidate text.
    pub candidate_content_hash: String,
    /// Domain-neutral candidate category.
    pub candidate_kind: CandidateKind,
}

/// The numerical interpretation expected from a scorer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreKind {
    /// Probability that the candidate is about the utterance.
    Probability,
    /// Similarity score from a local encoder.
    Similarity,
}

/// How candidates are expanded into hosted-judge requests.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionShaping {
    /// One request with one question per candidate and shared state.
    #[default]
    SharedStateQuestions,
    /// One hosted request per candidate.
    PerCandidateRequest,
}

/// Options which are part of a score's operating point.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairScoringOptions {
    /// Versioned wording identifier.
    pub question_wording_version: String,
    /// Request layout used by the backend.
    pub shaping: QuestionShaping,
    /// Numerical kind callers expect from the backend.
    pub score_kind_expected: ScoreKind,
}

/// A complete, domain-neutral scoring request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairScoreRequest {
    /// Task family.
    pub task: RelevanceTask,
    /// Text the candidate is judged against.
    pub utterance_text: String,
    /// Content hash for the exact utterance text.
    pub utterance_content_hash: String,
    /// Candidates supplied by the caller.
    pub candidates: Vec<Candidate>,
    /// Versioned request options.
    pub options: PairScoringOptions,
}

/// One candidate score. Probabilities are always integer basis points.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairScore {
    /// Candidate identifier copied from the request.
    pub candidate_id: String,
    /// Candidate content hash copied from the request.
    pub candidate_content_hash: String,
    /// Score in the inclusive range `0..=10000`.
    pub score_basis_points: u16,
    /// Interpretation of the score.
    pub score_kind: ScoreKind,
    /// Whether the scorer deliberately declined to provide a verdict.
    pub abstained: bool,
    /// Reason for abstaining, if any.
    pub abstain_reason: Option<String>,
}

/// A deadline at which a caller stops waiting to affect the current turn.
///
/// This is intentionally informational to a scoring service: it must not be used
/// to cancel the underlying request, because a late result is a carry-over input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InjectionDeadline(Duration);

impl InjectionDeadline {
    /// Constructs a deadline duration.
    pub const fn from_duration(duration: Duration) -> Self {
        Self(duration)
    }

    /// Returns the duration recorded for the caller's injection wait.
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Synchronous pair scorer for in-process backends.
pub trait PairScorer: Send + Sync + 'static {
    /// Scores every requested candidate and returns a trace on success or failure.
    fn score_pairs(&self, request: PairScoreRequest) -> Traced<Vec<PairScore>>;
}

/// Asynchronous pair-scoring service for effects that may block on I/O.
pub trait PairScoringService: Send + Sync {
    /// Scores candidates without allowing the injection deadline to cancel work.
    ///
    /// The returned future must be driven to completion. Callers on a deadline-sensitive
    /// path must spawn it and race the deadline against the resulting task handle; they
    /// must never race and drop this future directly. Dropping it can cancel the backend
    /// request and destroy the late verdict that the carry-over contract requires.
    fn score_pairs(
        &self,
        request: PairScoreRequest,
        injection_deadline: InjectionDeadline,
    ) -> Pin<Box<dyn Future<Output = Traced<Vec<PairScore>>> + Send + '_>>;
}

/// Adapts a synchronous scorer to the asynchronous service contract.
#[derive(Clone)]
#[cfg(feature = "remote-http")]
pub struct BlockingPairScorerService<S> {
    scorer: Arc<S>,
}

#[cfg(feature = "remote-http")]
impl<S> BlockingPairScorerService<S> {
    /// Wraps a synchronous scorer.
    pub fn new(scorer: S) -> Self {
        Self {
            scorer: Arc::new(scorer),
        }
    }
}

#[cfg(feature = "remote-http")]
impl<S> PairScoringService for BlockingPairScorerService<S>
where
    S: PairScorer,
{
    fn score_pairs(
        &self,
        request: PairScoreRequest,
        _injection_deadline: InjectionDeadline,
    ) -> Pin<Box<dyn Future<Output = Traced<Vec<PairScore>>> + Send + '_>> {
        let scorer = Arc::clone(&self.scorer);
        let task = request.task;
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || scorer.score_pairs(request)).await {
                Ok(traced) => traced,
                Err(error) => Traced::failure(
                    task,
                    SemanticOperation::PairScore,
                    SemanticFailure::BackendUnavailable {
                        detail: format!("blocking pair scorer task failed: {error}"),
                    },
                ),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        backends::{
            fixture::{FixtureRelevanceJudge, FixtureRelevanceJudgeConfig},
            remote_http::QUESTION_WORDING_VERSION,
        },
        pair_scoring::{
            Candidate, CandidateKind, PairScoreRequest, PairScoringOptions, QuestionShaping,
            RelevanceTask, ScoreKind,
        },
    };

    use super::*;

    #[tokio::test]
    async fn blocking_adapter_presents_a_sync_scorer_as_an_async_service() {
        let service = BlockingPairScorerService::new(FixtureRelevanceJudge::new(
            FixtureRelevanceJudgeConfig::default(),
        ));
        let result = service
            .score_pairs(
                PairScoreRequest {
                    task: RelevanceTask::MemoryRelevance,
                    utterance_text: "garden".to_owned(),
                    utterance_content_hash: "utterance".to_owned(),
                    candidates: vec![Candidate {
                        candidate_id: "candidate".to_owned(),
                        candidate_text: "tomatoes".to_owned(),
                        candidate_content_hash: "candidate-hash".to_owned(),
                        candidate_kind: CandidateKind("memory".to_owned()),
                    }],
                    options: PairScoringOptions {
                        question_wording_version: QUESTION_WORDING_VERSION.to_owned(),
                        shaping: QuestionShaping::SharedStateQuestions,
                        score_kind_expected: ScoreKind::Probability,
                    },
                },
                InjectionDeadline::from_duration(Duration::from_millis(1)),
            )
            .await;
        assert_eq!(result.outcome.expect("scores").len(), 1);
    }
}
