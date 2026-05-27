use time::OffsetDateTime;

use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::proposer::{AssociationProposer, ProposedAssociation};
use crate::sleep::sleep_report::SleepReport;

pub struct LlmCandidateProposer<'a> {
    pub report: &'a SleepReport,
    pub promoted_candidate_ids: &'a [Option<String>],
}

impl<'a> AssociationProposer for LlmCandidateProposer<'a> {
    fn name(&self) -> &str {
        "llm-candidate"
    }

    fn priority(&self) -> u8 {
        100
    }

    fn propose(
        &self,
        _store: &MemoryStoreContents,
        _session: &SessionState,
        _as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation> {
        let mut proposals = Vec::new();

        for candidate in &self.report.association_candidates {
            let Some(from_id) = candidate
                .from_memory_candidate_index
                .checked_sub(1)
                .and_then(|index| self.promoted_candidate_ids.get(index))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(to_id) = candidate
                .to_memory_candidate_index
                .checked_sub(1)
                .and_then(|index| self.promoted_candidate_ids.get(index))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            if from_id == to_id {
                continue;
            }

            let reason = candidate
                .reason
                .as_deref()
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| self.name().to_string());

            proposals.push(ProposedAssociation {
                from_id: from_id.clone(),
                to_id: to_id.clone(),
                // Endpoint validation and final weight clamping happen in merge_and_dedupe.
                weight: candidate.weight.unwrap_or(0.35),
                reason,
                proposer_name: self.name().to_string(),
            });
        }

        proposals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::store::MemoryStoreContents;
    use crate::session::{MemorySourceConfig, SessionConfig, SessionState};
    use crate::sleep::proposer::AssociationProposer;
    use crate::sleep::sleep_report::{
        SleepAssociationCandidate, SleepMemoryCandidate, SleepReport,
    };

    fn report_with_one_candidate() -> SleepReport {
        SleepReport {
            session_summary: "summary".to_string(),
            memory_candidates: vec![
                SleepMemoryCandidate {
                    summary: "First association endpoint.".to_string(),
                    importance: Some(0.5),
                    source_reference: Some("source-1".to_string()),
                },
                SleepMemoryCandidate {
                    summary: "Second association endpoint.".to_string(),
                    importance: Some(0.4),
                    source_reference: Some("source-2".to_string()),
                },
            ],
            association_candidates: vec![SleepAssociationCandidate {
                from_memory_candidate_index: 1,
                to_memory_candidate_index: 2,
                weight: Some(0.4),
                reason: Some("test".to_string()),
            }],
            open_questions: vec![],
            decision_candidates: vec![],
            future_context_hints: vec![],
            review_notes: vec![],
        }
    }

    fn build_minimal_session_for_proposer_tests() -> SessionState {
        SessionState::new_with_id(
            "s-test".to_string(),
            SessionConfig {
                model_id: "mock".to_string(),
                max_turns: 10,
                warm_threshold: 2,
                allow_over_limit: false,
                memory_source: MemorySourceConfig {
                    source: "fixture".to_string(),
                    file: None,
                },
            },
        )
    }

    #[test]
    fn llm_candidate_proposer_returns_named_proposals() {
        let report = report_with_one_candidate();
        let promoted = vec![Some("memory.a".to_string()), Some("memory.b".to_string())];
        let store = MemoryStoreContents::default();
        let session = build_minimal_session_for_proposer_tests();
        let proposer = LlmCandidateProposer {
            report: &report,
            promoted_candidate_ids: &promoted,
        };

        let proposals = proposer.propose(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].proposer_name, "llm-candidate");
        assert_eq!(proposals[0].from_id, "memory.a");
        assert_eq!(proposals[0].to_id, "memory.b");
    }
}
