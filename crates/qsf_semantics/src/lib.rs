#![deny(missing_docs)]
//! Backend-independent pair scoring for semantic relevance work.
//!
//! This crate returns trace values to its callers. It deliberately owns neither
//! domain policy nor artifact persistence.

/// Pair-scoring backend implementations.
pub mod backends;
/// Budget constants and timeout relationships.
pub mod budgets;
/// Configured HTTP backend selection and validation.
#[cfg(feature = "remote-http")]
pub mod config;
/// Pair-scoring contract and its synchronous adapter.
pub mod pair_scoring;
/// Trace values and relevance-judgment lifecycle records.
pub mod trace;

#[cfg(feature = "remote-http")]
pub use pair_scoring::BlockingPairScorerService;
pub use pair_scoring::{
    Candidate, CandidateKind, InjectionDeadline, PairScore, PairScoreRequest, PairScorer,
    PairScoringOptions, PairScoringService, QuestionShaping, RelevanceTask, ScoreKind,
};
pub use trace::{SemanticFailure, SemanticTraceRecord, Traced};
