//! Memory-owned adaptation of pair scores into admission verdicts.

use std::collections::{BTreeMap, BTreeSet};

use qsf_semantics::trace::{BackendKind, ModelIdentity, SemanticOperation};
use qsf_semantics::{PairScore, RelevanceTask, ScoreKind, Traced};
use serde::{Deserialize, Serialize};

/// Maximum probability score in the integer basis-point representation.
pub const MAX_JUDGE_VERDICT_BASIS_POINTS: u16 = 10_000;

/// The existing retrieval signal, the judge signal, or both admitted a memory.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionBasis {
    /// Existing keyword, tag, association, profile, or recency policy admitted it.
    #[default]
    Lexical,
    /// The judge admitted a candidate without an existing retrieval signal.
    Judge,
    /// Both the existing retrieval policy and the judge admitted the candidate.
    LexicalAndJudge,
}

/// Whether a selected memory would have appeared without judge input.
/// Read this only from `RetrievalResult.selected`; omitted entries are not eligible.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionEligibility {
    /// The memory is part of the lexical-only selection and may shape durable structure.
    Associable,
    /// The judged selection contains the memory but the lexical-only selection does not.
    JudgeInfluenced,
    /// Placeholder on omitted entries; eligibility is meaningful only for selected memories.
    NotSelected,
}

impl SelectionEligibility {
    /// Whether lexical-only retrieval selected this memory.
    pub const fn is_associable(self) -> bool {
        matches!(self, Self::Associable)
    }
}

/// A memory verdict, including the operating-point identity used to produce it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeVerdict {
    /// Probability in integer basis points, inclusive range `0..=10000`.
    pub score_basis_points: u16,
    /// Model identity reported by the scoring backend.
    pub model_identity: ModelIdentity,
    /// Versioned question wording reported by the scoring backend.
    pub question_wording_version: String,
    /// Backend discriminator reported by the scoring backend.
    pub backend_kind: BackendKind,
}

/// Creates memory-keyed verdicts from domain-neutral candidate scores.
///
/// Abstentions are omitted. Failed invocations, non-probability scores, and malformed probabilities are
/// rejected because they do not share the memory admission threshold's operating point.
pub fn adapt_pair_scores_to_judge_verdicts(
    scored: &Traced<Vec<PairScore>>,
    candidate_id_to_memory_id: &BTreeMap<String, String>,
) -> anyhow::Result<BTreeMap<String, JudgeVerdict>> {
    let trace = &scored.trace;
    anyhow::ensure!(
        trace.failure.is_none(),
        "memory judge verdicts require a successful trace"
    );
    let pair_scores = scored.outcome.as_ref().map_err(|failure| {
        anyhow::anyhow!("memory judge verdicts require a successful outcome: {failure}")
    })?;
    anyhow::ensure!(
        trace.operation == SemanticOperation::PairScore,
        "memory judge verdicts require a pair-score trace"
    );
    anyhow::ensure!(
        trace.task == RelevanceTask::MemoryRelevance,
        "memory judge verdicts require a memory-relevance trace"
    );
    anyhow::ensure!(
        trace.score_kind == ScoreKind::Probability,
        "memory judge verdicts require probability scores"
    );

    let mut verdicts = BTreeMap::new();
    let mut candidate_ids = BTreeSet::new();
    for score in pair_scores {
        if score.abstained {
            continue;
        }
        anyhow::ensure!(
            candidate_ids.insert(score.candidate_id.as_str()),
            "duplicate pair score for candidate `{}`",
            score.candidate_id
        );
        anyhow::ensure!(
            score.score_kind == ScoreKind::Probability,
            "candidate `{}` has a non-probability score",
            score.candidate_id
        );
        anyhow::ensure!(
            score.score_basis_points <= MAX_JUDGE_VERDICT_BASIS_POINTS,
            "candidate `{}` score is outside 0..=10000 basis points",
            score.candidate_id
        );
        let memory_id = candidate_id_to_memory_id
            .get(&score.candidate_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "pair score candidate `{}` has no memory mapping",
                    score.candidate_id
                )
            })?;
        anyhow::ensure!(
            !verdicts.contains_key(memory_id),
            "multiple pair scores map to memory `{memory_id}`"
        );
        verdicts.insert(
            memory_id.clone(),
            JudgeVerdict {
                score_basis_points: score.score_basis_points,
                model_identity: trace.model_identity.clone(),
                question_wording_version: trace.question_wording_version.clone(),
                backend_kind: trace.backend_kind,
            },
        );
    }

    Ok(verdicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_semantics::RelevanceTask;
    use qsf_semantics::trace::{SemanticFailure, SemanticOperation, SemanticTraceRecord};

    fn trace() -> SemanticTraceRecord {
        let mut trace = SemanticTraceRecord::unavailable(
            RelevanceTask::MemoryRelevance,
            SemanticOperation::PairScore,
            SemanticFailure::BackendUnavailable {
                detail: "adapter fixture".to_owned(),
            },
        );
        trace.invocation_id = "fixture-success".to_owned();
        trace.model_identity.model_id = "fixture-judge".to_owned();
        trace.model_identity.identity_value = "fixture-v1".to_owned();
        trace.question_wording_version = "memory-relevance-v1".to_owned();
        trace.failure = None;
        trace
    }

    fn pair_score(candidate_id: &str, score_basis_points: u16, abstained: bool) -> PairScore {
        PairScore {
            candidate_id: candidate_id.to_owned(),
            candidate_content_hash: "candidate-hash".to_owned(),
            score_basis_points,
            score_kind: ScoreKind::Probability,
            abstained,
            abstain_reason: abstained.then(|| "uncertain".to_owned()),
        }
    }

    #[test]
    fn adapter_maps_candidate_ids_and_preserves_judge_identity() {
        let mapping = BTreeMap::from([("candidate-1".to_owned(), "memory-1".to_owned())]);
        let trace = trace();
        let verdicts = adapt_pair_scores_to_judge_verdicts(
            &Traced::success(trace.clone(), vec![pair_score("candidate-1", 7_500, false)]),
            &mapping,
        )
        .unwrap();

        let verdict = &verdicts["memory-1"];
        assert_eq!(verdict.score_basis_points, 7_500);
        assert_eq!(verdict.model_identity, trace.model_identity);
        assert_eq!(
            verdict.question_wording_version,
            trace.question_wording_version
        );
        assert_eq!(verdict.backend_kind, trace.backend_kind);
    }

    #[test]
    fn adapter_does_not_turn_abstentions_into_verdicts() {
        let mapping = BTreeMap::from([("candidate-1".to_owned(), "memory-1".to_owned())]);
        let verdicts = adapt_pair_scores_to_judge_verdicts(
            &Traced::success(trace(), vec![pair_score("candidate-1", 10_000, true)]),
            &mapping,
        )
        .unwrap();

        assert!(verdicts.is_empty());
    }

    #[test]
    fn adapter_rejects_non_probability_scores_and_invalid_basis_points() {
        let mapping = BTreeMap::from([("candidate-1".to_owned(), "memory-1".to_owned())]);
        let mut similarity = pair_score("candidate-1", 7_500, false);
        similarity.score_kind = ScoreKind::Similarity;
        assert!(
            adapt_pair_scores_to_judge_verdicts(
                &Traced::success(trace(), vec![similarity]),
                &mapping
            )
            .is_err()
        );

        let invalid = pair_score("candidate-1", 10_001, false);
        assert!(
            adapt_pair_scores_to_judge_verdicts(&Traced::success(trace(), vec![invalid]), &mapping)
                .is_err()
        );
    }

    #[test]
    fn adapter_rejects_failed_trace_and_outcome() {
        let mapping = BTreeMap::from([("candidate-1".to_owned(), "memory-1".to_owned())]);
        let failure = SemanticFailure::BackendUnavailable {
            detail: "failed".to_owned(),
        };
        let failed = Traced::<Vec<PairScore>>::failure(
            RelevanceTask::MemoryRelevance,
            SemanticOperation::PairScore,
            failure.clone(),
        );
        assert!(adapt_pair_scores_to_judge_verdicts(&failed, &mapping).is_err());
        let mut failed_trace = trace();
        failed_trace.failure = Some(failure);
        let inconsistent =
            Traced::success(failed_trace, vec![pair_score("candidate-1", 7_500, false)]);
        assert!(adapt_pair_scores_to_judge_verdicts(&inconsistent, &mapping).is_err());
    }
}
