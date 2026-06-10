use std::collections::HashSet;

use time::OffsetDateTime;

use crate::memory::co_retrieval::{
    CROSS_TURN_ASSOCIATION_WINDOW, CoRetrievalDelta, CrossTurnAnchorRange,
    generate_cross_turn_deltas_for_anchor_ranges,
};
use crate::memory::processed_ranges::{contiguous_ranges, uncovered_turn_indices};
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::proposer::{AssociationProposer, ProposedAssociation};

pub struct SafetyNetCoRetrievalProposer;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SafetyNetCoRetrievalOutput {
    pub proposals: Vec<ProposedAssociation>,
    pub strengthened_associations: Vec<(String, String, f64)>,
    pub processed_ranges: Vec<qsf_memory::ProcessedRange>,
}

impl SafetyNetCoRetrievalProposer {
    pub fn propose_with_bookkeeping(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> SafetyNetCoRetrievalOutput {
        let sleep_records = session.sleep_records();
        if sleep_records.is_empty() {
            return SafetyNetCoRetrievalOutput::default();
        }

        let coverage_ranges = coverage_ranges_for_sleep_records(&store.processed_ranges, session);
        let uncovered = uncovered_turn_indices(
            &coverage_ranges,
            &session.session_id,
            0,
            sleep_records.len() - 1,
        );
        if uncovered.is_empty() {
            return SafetyNetCoRetrievalOutput::default();
        }

        let ranges = contiguous_ranges(&uncovered);
        let anchor_ranges = ranges
            .iter()
            .map(|(first, last)| CrossTurnAnchorRange {
                first_turn: *first,
                last_turn: *last,
            })
            .collect::<Vec<_>>();
        let retrievals = sleep_records
            .iter()
            .map(|record| record.retrieval_source_ids())
            .collect::<Vec<_>>();
        let known_record_ids: HashSet<String> = store
            .records
            .iter()
            .map(|record| record.id.clone())
            .collect();
        let deltas = generate_cross_turn_deltas_for_anchor_ranges(
            &retrievals,
            &store.associations,
            &known_record_ids,
            CROSS_TURN_ASSOCIATION_WINDOW,
            &session.session_id,
            as_of,
            &anchor_ranges,
        );

        let mut proposals = Vec::new();
        let mut strengthened_associations = Vec::new();
        for delta in deltas {
            match delta {
                CoRetrievalDelta::Create {
                    from,
                    to,
                    weight,
                    reason,
                    ..
                } => proposals.push(ProposedAssociation {
                    from_id: from,
                    to_id: to,
                    weight,
                    reason,
                    proposer_name: self.name().to_string(),
                }),
                CoRetrievalDelta::Strengthen {
                    from,
                    to,
                    new_weight,
                    ..
                } => strengthened_associations.push((from, to, new_weight)),
            }
        }

        let processed_ranges = ranges
            .into_iter()
            .map(|(first, last)| qsf_memory::ProcessedRange {
                session_id: session.session_id.clone(),
                first_turn_index: first,
                last_turn_index: last,
                kind: qsf_memory::ProcessedRangeKind::SleepSafetyNet,
                at: as_of,
            })
            .collect();

        SafetyNetCoRetrievalOutput {
            proposals,
            strengthened_associations,
            processed_ranges,
        }
    }
}

fn coverage_ranges_for_sleep_records(
    ranges: &[qsf_memory::ProcessedRange],
    session: &SessionState,
) -> Vec<qsf_memory::ProcessedRange> {
    if session.exchanges.is_empty() {
        return ranges.to_vec();
    }

    ranges
        .iter()
        .filter(|range| range.kind == qsf_memory::ProcessedRangeKind::SleepSafetyNet)
        .cloned()
        .collect()
}

impl AssociationProposer for SafetyNetCoRetrievalProposer {
    fn name(&self) -> &str {
        "safety-net-co-retrieval"
    }

    fn priority(&self) -> u8 {
        30
    }

    fn propose(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation> {
        self.propose_with_bookkeeping(store, session, as_of)
            .proposals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextAssembly, ContextBudget, ContextFragment, ContextSelection};
    use crate::memory::association::Association;
    use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
    use crate::memory::store::MemoryStoreContents;
    use crate::session::{
        Exchange, ExchangeInput, ExchangeOutput, MemorySourceConfig, SessionConfig, SessionState,
        Turn, UtteranceRecord,
    };
    use crate::sleep::proposer::AssociationProposer;

    fn session_with_turns(turn_count: usize, retrievals: &[&[&str]]) -> SessionState {
        let mut session = SessionState::new_with_id(
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
        );
        for index in 0..turn_count {
            let ids = retrievals.get(index).copied().unwrap_or(&[]);
            session.turns.push(Turn {
                index,
                started_at: std::time::SystemTime::UNIX_EPOCH,
                completed_at: std::time::SystemTime::UNIX_EPOCH,
                user_input: format!("turn-{index}"),
                context_assembly: ContextAssembly {
                    budget: ContextBudget::new(4, 600),
                    selected: ids
                        .iter()
                        .map(|id| ContextSelection {
                            fragment: ContextFragment {
                                fragment_id: (*id).to_string(),
                                source_kind: crate::context::ContextSourceKind::Memory,
                                summary: format!("summary {id}"),
                                tags: vec![],
                                score: 1.0,
                                estimated_tokens: 10,
                                source_reference: "tests".to_string(),
                                selection_reason: "tests".to_string(),
                            },
                            cumulative_estimated_tokens: 10,
                        })
                        .collect(),
                    omitted: vec![],
                    used_estimated_tokens: ids.len() * 10,
                },
                retrieved_memory_block: String::new(),
                assistant_response: format!("response-{index}"),
                recalled_turns: vec![],
                tool_requests: vec![],
                tool_executions: vec![],
                model_id: "mock".to_string(),
                model_latency_ms: 0,
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                full_request_hash: crate::conversation::ContentHash([index as u8; 32]),
                message_count: 0,
            });
        }
        session
    }

    fn store_with_records(ids: &[&str]) -> MemoryStoreContents {
        MemoryStoreContents {
            records: ids
                .iter()
                .map(|id| {
                    MemoryRecord::new(
                        *id,
                        MemoryRecordKind::Observation,
                        *id,
                        *id,
                        vec![],
                        time::OffsetDateTime::UNIX_EPOCH,
                        0.5,
                        0,
                        "tests",
                        10,
                    )
                })
                .collect(),
            associations: vec![Association::new(
                "memory.a",
                "memory.c",
                0.4,
                "existing",
                time::OffsetDateTime::UNIX_EPOCH,
            )],
            ..MemoryStoreContents::default()
        }
    }

    fn voice_exchange_with_memory(
        index: usize,
        started_at: std::time::SystemTime,
        memory_id: &str,
    ) -> Exchange {
        Exchange {
            index,
            started_at,
            completed_at: Some(started_at),
            input: ExchangeInput::Voice {
                final_transcript: "Please keep the voice path in sleep.".to_string(),
                utterances: vec![UtteranceRecord {
                    utterance_id: "utt-1".to_string(),
                    revision_index: 0,
                    transcript: "Please keep the voice path in sleep.".to_string(),
                    received_at: started_at,
                    provider_id: Some("provider".to_string()),
                    source_chunk_index: None,
                }],
            },
            output: Some(ExchangeOutput {
                response_id: Some("response-1".to_string()),
                text: "Understood.".to_string(),
                produced_at: started_at,
                provider_name: Some("provider".to_string()),
                target: Some("speech-output-provider".to_string()),
                audio_marker: None,
            }),
            context_assembly: Some(ContextAssembly {
                budget: ContextBudget::new(4, 600),
                selected: vec![ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: memory_id.to_string(),
                        source_kind: crate::context::ContextSourceKind::Memory,
                        summary: format!("summary {memory_id}"),
                        tags: vec![],
                        score: 1.0,
                        estimated_tokens: 10,
                        source_reference: "tests".to_string(),
                        selection_reason: "tests".to_string(),
                    },
                    cumulative_estimated_tokens: 10,
                }],
                omitted: vec![],
                used_estimated_tokens: 10,
            }),
            retrieved_memory_block: String::new(),
            recalled_items: vec![],
            model: None,
            interruptions: vec![],
            provider_events: vec![],
            tool_requests: vec![],
            tool_executions: vec![],
            status: crate::session::ExchangeStatus::Completed,
        }
    }

    #[test]
    fn safety_net_skips_already_processed_ranges() {
        let session =
            session_with_turns(2, &[&["memory.a", "memory.b"], &["memory.b", "memory.c"]]);
        let mut store = store_with_records(&["memory.a", "memory.b", "memory.c"]);
        store.processed_ranges.push(qsf_memory::ProcessedRange {
            session_id: "s-test".to_string(),
            first_turn_index: 0,
            last_turn_index: 1,
            kind: qsf_memory::ProcessedRangeKind::SleepSafetyNet,
            at: time::OffsetDateTime::UNIX_EPOCH,
        });
        let proposer = SafetyNetCoRetrievalProposer;

        let proposals = proposer.propose(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert!(proposals.is_empty());
    }

    #[test]
    fn safety_net_proposes_for_uncovered_ranges() {
        let session =
            session_with_turns(2, &[&["memory.a", "memory.b"], &["memory.b", "memory.c"]]);
        let store = store_with_records(&["memory.a", "memory.b", "memory.c"]);
        let proposer = SafetyNetCoRetrievalProposer;

        let proposals = proposer.propose(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert!(!proposals.is_empty());
    }

    #[test]
    fn safety_net_bookkeeping_tracks_strengthens_and_ranges() {
        let session =
            session_with_turns(2, &[&["memory.a", "memory.b"], &["memory.b", "memory.c"]]);
        let store = store_with_records(&["memory.a", "memory.b", "memory.c"]);
        let proposer = SafetyNetCoRetrievalProposer;

        let output =
            proposer.propose_with_bookkeeping(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert_eq!(
            output.strengthened_associations,
            vec![("memory.a".to_string(), "memory.c".to_string(), 0.45)]
        );
        assert_eq!(output.processed_ranges.len(), 1);
        assert_eq!(
            output.processed_ranges[0].kind,
            qsf_memory::ProcessedRangeKind::SleepSafetyNet
        );
    }

    #[test]
    fn safety_net_includes_voice_exchange_retrievals_in_mixed_sessions() {
        let mut session = session_with_turns(1, &[&["memory.a"]]);
        session.exchanges.push(voice_exchange_with_memory(
            0,
            std::time::SystemTime::UNIX_EPOCH,
            "memory.b",
        ));

        let store = store_with_records(&["memory.a", "memory.b"]);
        let proposer = SafetyNetCoRetrievalProposer;

        let output =
            proposer.propose_with_bookkeeping(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert!(!output.processed_ranges.is_empty());
        assert!(
            output
                .proposals
                .iter()
                .any(|proposal| proposal.from_id == "memory.a" && proposal.to_id == "memory.b")
        );
    }

    #[test]
    fn mixed_sessions_ignore_text_live_ranges_when_covering_voice_sleep_records() {
        let mut session = session_with_turns(1, &[&["memory.a"]]);
        let later = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        session.turns[0].started_at = later;
        session.turns[0].completed_at = later;
        session.exchanges.push(voice_exchange_with_memory(
            0,
            std::time::SystemTime::UNIX_EPOCH,
            "memory.b",
        ));
        let mut store = store_with_records(&["memory.a", "memory.b"]);
        store.processed_ranges.push(qsf_memory::ProcessedRange {
            session_id: session.session_id.clone(),
            first_turn_index: 0,
            last_turn_index: 0,
            kind: qsf_memory::ProcessedRangeKind::LiveBatch,
            at: time::OffsetDateTime::UNIX_EPOCH,
        });
        let proposer = SafetyNetCoRetrievalProposer;

        let output =
            proposer.propose_with_bookkeeping(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert!(
            output
                .proposals
                .iter()
                .any(|proposal| proposal.from_id == "memory.a" && proposal.to_id == "memory.b")
        );
        assert_eq!(output.processed_ranges.len(), 1);
        assert_eq!(output.processed_ranges[0].first_turn_index, 0);
        assert_eq!(output.processed_ranges[0].last_turn_index, 1);
        assert_eq!(
            output.processed_ranges[0].kind,
            qsf_memory::ProcessedRangeKind::SleepSafetyNet
        );
    }
}
