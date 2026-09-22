#![deny(missing_docs)]
//! Backend-independent pair scoring for semantic relevance work.
//!
//! This crate returns trace values to its callers. It deliberately owns neither
//! domain policy nor artifact persistence.

/// Pair-scoring backend implementations.
pub mod backends;
/// Budget constants and timeout relationships.
pub mod budgets;
/// Configured backend selection and validation.
pub mod config;
/// Pair-scoring contract and its synchronous adapter.
pub mod pair_scoring;
/// Trace values and relevance-judgment lifecycle records.
pub mod trace;

pub use pair_scoring::{
    BlockingPairScorerService, Candidate, CandidateKind, InjectionDeadline, PairScore,
    PairScoreRequest, PairScorer, PairScoringOptions, PairScoringService, QuestionShaping,
    RelevanceTask, ScoreKind,
};
pub use trace::{SemanticFailure, SemanticTraceRecord, Traced};
