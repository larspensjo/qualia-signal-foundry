use serde::Serialize;
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
use crate::memory::store::MemoryStoreContents;
use crate::memory::token_estimate::estimated_tokens;
use crate::session::SessionState;
use crate::sleep::proposer::{AssociationProposer, merge_and_dedupe, sort_by_priority_descending};
use crate::sleep::proposers::llm_candidate::LlmCandidateProposer;
use crate::sleep::proposers::safety_net_co_retrieval::SafetyNetCoRetrievalProposer;
use crate::sleep::sleep_report::SleepReport;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PromotionPlan {
    pub new_records: Vec<MemoryRecord>,
    pub new_associations: Vec<Association>,
    pub strengthened_associations: Vec<(String, String, f64)>,
    pub processed_ranges: Vec<qsf_memory::ProcessedRange>,
    pub skipped_duplicates: Vec<String>,
}

pub fn build_promotion_plan(
    report: &SleepReport,
    session: &SessionState,
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
    sleep_run_id: &str,
) -> PromotionPlan {
    let mut new_records = Vec::new();
    let mut skipped_duplicates = Vec::new();
    let mut promoted_candidate_ids = vec![None; report.memory_candidates.len()];

    for (index, candidate) in report.memory_candidates.iter().enumerate() {
        let summary = candidate.summary.trim().to_string();
        if summary.is_empty() {
            continue;
        }

        let title = first_sentence(&summary);
        let normalized = normalize_for_dedup(&title, &summary);
        let duplicate = current_store
            .records
            .iter()
            .any(|record| normalize_for_dedup(&record.title, &record.summary) == normalized)
            || new_records.iter().any(|record: &MemoryRecord| {
                normalize_for_dedup(&record.title, &record.summary) == normalized
            });

        if duplicate {
            skipped_duplicates.push(title);
            continue;
        }

        let record_id = format!("memory.sleep.{}.{:03}", sanitize(sleep_run_id), index + 1);
        let record = MemoryRecord::new(
            record_id.clone(),
            MemoryRecordKind::Observation,
            title,
            summary.clone(),
            vec![],
            as_of,
            candidate.importance.unwrap_or(0.3).clamp(0.0, 1.0),
            0,
            candidate.source_reference.clone().unwrap_or_else(|| {
                format!(
                    "sleep-run:{sleep_run_id}#memory_candidates[{:03}]",
                    index + 1
                )
            }),
            estimated_tokens(&summary),
        )
        .with_last_reinforced_at(as_of);
        promoted_candidate_ids[index] = Some(record_id);
        new_records.push(record);
    }

    let llm = LlmCandidateProposer {
        report,
        promoted_candidate_ids: &promoted_candidate_ids,
    };
    let safety = SafetyNetCoRetrievalProposer;
    let safety_output = safety.propose_with_bookkeeping(current_store, session, as_of);
    let mut tagged = Vec::new();
    for proposal in llm.propose(current_store, session, as_of) {
        tagged.push((llm.priority(), proposal));
    }
    for proposal in safety_output.proposals {
        tagged.push((safety.priority(), proposal));
    }
    sort_by_priority_descending(&mut tagged);

    let mut known_record_ids: std::collections::HashSet<String> = current_store
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect();
    known_record_ids.extend(new_records.iter().map(|record| record.id.clone()));
    let merged = merge_and_dedupe(
        tagged.into_iter().map(|(_, proposal)| proposal).collect(),
        &current_store.associations,
        &known_record_ids,
    );
    let new_associations = merged
        .into_iter()
        .map(|proposal| {
            Association::new(
                proposal.from_id,
                proposal.to_id,
                proposal.weight,
                proposal.reason,
                as_of,
            )
        })
        .collect();

    PromotionPlan {
        new_records,
        new_associations,
        strengthened_associations: safety_output.strengthened_associations,
        processed_ranges: safety_output.processed_ranges,
        skipped_duplicates,
    }
}

fn normalize_for_dedup(title: &str, summary: &str) -> String {
    let mut normalized = format!("{title}|{summary}").to_ascii_lowercase();
    normalized.retain(|character| !character.is_whitespace());
    normalized
}

fn sanitize(value: &str) -> String {
    // Memory id segments are normalized to lowercase to match the reviewed-memory draft path.
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "unknown-sleep-run".to_string()
    } else {
        sanitized
    }
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    let sentence = match trimmed
        .chars()
        .position(|character| matches!(character, '.' | '!' | '?' | '\n' | '\r'))
    {
        Some(index) => trimmed.chars().take(index + 1).collect::<String>(),
        None => trimmed.to_string(),
    };

    let title = sentence.chars().take(64).collect::<String>();
    if title.trim().is_empty() {
        trimmed.chars().take(64).collect::<String>()
    } else {
        title.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget, ContextFragment, ContextSelection};
    use crate::session::{MemorySourceConfig, SessionConfig};
    use crate::sleep::sleep_report::{
        SleepAssociationCandidate, SleepMemoryCandidate, SleepReport,
    };

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    fn empty_session() -> SessionState {
        SessionState::new_with_id("s-test".to_string(), config())
    }

    fn config() -> SessionConfig {
        SessionConfig {
            model_id: "mock".to_string(),
            max_turns: 10,
            warm_threshold: 2,
            allow_over_limit: false,
            memory_source: MemorySourceConfig {
                source: "fixture".to_string(),
                file: None,
            },
        }
    }

    fn report_with_candidates(candidates: Vec<&str>) -> SleepReport {
        SleepReport {
            session_summary: "summary".to_string(),
            memory_candidates: candidates
                .into_iter()
                .map(|summary| SleepMemoryCandidate {
                    summary: summary.to_string(),
                    importance: Some(0.5),
                    source_reference: None,
                })
                .collect(),
            association_candidates: vec![],
            open_questions: vec![],
            decision_candidates: vec![],
            future_context_hints: vec![],
            review_notes: vec![],
        }
    }

    fn memory_record(id: &str, title: &str, summary: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            title,
            summary,
            vec![],
            ts(),
            0.5,
            0,
            "tests",
            10,
        )
    }

    fn turn_with_memories(index: usize, ids: &[&str]) -> crate::session::Turn {
        let mut turn = crate::session::tests::fake_turn(index);
        turn.context_assembly = ContextAssembly {
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
        };
        turn
    }

    #[test]
    fn promotes_each_candidate_as_observation() {
        let report = report_with_candidates(vec![
            "Reducers stay pure.",
            "Tools are perception extensions.",
        ]);
        let plan = build_promotion_plan(
            &report,
            &empty_session(),
            &MemoryStoreContents::default(),
            ts(),
            "sleep-1",
        );

        assert_eq!(plan.new_records.len(), 2);
        assert!(
            plan.new_records
                .iter()
                .all(|record| record.kind == MemoryRecordKind::Observation)
        );
        assert!(
            plan.new_records
                .iter()
                .all(|record| record.last_reinforced_at == Some(ts()))
        );
    }

    #[test]
    fn skips_duplicates_of_existing_store_records() {
        let report = report_with_candidates(vec!["Reducers stay pure."]);
        let store = MemoryStoreContents {
            records: vec![memory_record(
                "memory.existing",
                "Reducers stay pure.",
                "Reducers stay pure.",
            )],
            associations: vec![],
            ..MemoryStoreContents::default()
        };

        let plan = build_promotion_plan(&report, &empty_session(), &store, ts(), "sleep-1");

        assert_eq!(plan.new_records.len(), 0);
        assert_eq!(plan.skipped_duplicates.len(), 1);
    }

    #[test]
    fn promotion_is_byte_idempotent_on_same_inputs() {
        let report = report_with_candidates(vec!["Reducers stay pure."]);
        let plan_a = build_promotion_plan(
            &report,
            &empty_session(),
            &MemoryStoreContents::default(),
            ts(),
            "sleep-1",
        );
        let plan_b = build_promotion_plan(
            &report,
            &empty_session(),
            &MemoryStoreContents::default(),
            ts(),
            "sleep-1",
        );

        assert_eq!(
            serde_json::to_string(&plan_a).unwrap(),
            serde_json::to_string(&plan_b).unwrap()
        );
    }

    #[test]
    fn first_sentence_is_utf8_boundary_safe_when_truncating() {
        let summary = format!("{}.", "€".repeat(30));

        let title = first_sentence(&summary);

        assert_eq!(title, summary);
    }

    #[test]
    fn cross_turn_retrievals_create_or_strengthen_associations() {
        let mut session = empty_session();
        session.turns.push(turn_with_memories(0, &["memory.a"]));
        session.turns.push(turn_with_memories(1, &["memory.b"]));
        let store = MemoryStoreContents {
            records: vec![
                memory_record("memory.a", "A.", "A."),
                memory_record("memory.b", "B.", "B."),
                memory_record("memory.c", "C.", "C."),
            ],
            associations: vec![Association::new(
                "memory.a",
                "memory.c",
                0.4,
                "existing",
                ts(),
            )],
            ..MemoryStoreContents::default()
        };

        session.turns.push(turn_with_memories(2, &["memory.c"]));
        let plan = build_promotion_plan(
            &report_with_candidates(vec![]),
            &session,
            &store,
            ts(),
            "sleep-1",
        );

        assert!(plan.new_associations.iter().any(|association| {
            association.from_memory_id == "memory.a" && association.to_memory_id == "memory.b"
        }));
        assert_eq!(
            plan.strengthened_associations,
            vec![("memory.a".to_string(), "memory.c".to_string(), 0.45)]
        );
    }

    #[test]
    fn cross_turn_retrievals_strengthen_reverse_existing_direction_for_sleep_apply() {
        let mut session = empty_session();
        session.turns.push(turn_with_memories(0, &["memory.a"]));
        session.turns.push(turn_with_memories(1, &["memory.b"]));
        let store = MemoryStoreContents {
            records: vec![
                memory_record("memory.a", "A.", "A."),
                memory_record("memory.b", "B.", "B."),
            ],
            associations: vec![Association::new(
                "memory.b",
                "memory.a",
                0.4,
                "existing reverse",
                ts(),
            )],
            ..MemoryStoreContents::default()
        };

        let plan = build_promotion_plan(
            &report_with_candidates(vec![]),
            &session,
            &store,
            ts(),
            "sleep-1",
        );

        assert!(plan.new_associations.is_empty());
        assert_eq!(
            plan.strengthened_associations,
            vec![("memory.b".to_string(), "memory.a".to_string(), 0.45)]
        );
    }

    #[test]
    fn cross_turn_retrievals_skip_ids_missing_from_current_store() {
        let mut session = empty_session();
        session.turns.push(turn_with_memories(0, &["memory.a"]));
        session.turns.push(turn_with_memories(1, &["memory.ghost"]));
        let store = MemoryStoreContents {
            records: vec![memory_record("memory.a", "A.", "A.")],
            associations: vec![],
            ..MemoryStoreContents::default()
        };

        let plan = build_promotion_plan(
            &report_with_candidates(vec![]),
            &session,
            &store,
            ts(),
            "sleep-1",
        );

        assert!(plan.new_associations.is_empty());
        assert!(plan.strengthened_associations.is_empty());
    }

    #[test]
    fn cross_turn_retrievals_skip_processed_anchor_ranges() {
        let mut session = empty_session();
        session.turns.push(turn_with_memories(0, &["memory.a"]));
        session.turns.push(turn_with_memories(1, &["memory.b"]));
        let store = MemoryStoreContents {
            records: vec![
                memory_record("memory.a", "A.", "A."),
                memory_record("memory.b", "B.", "B."),
            ],
            associations: vec![],
            processed_ranges: vec![qsf_memory::ProcessedRange {
                session_id: session.session_id.clone(),
                first_turn_index: 0,
                last_turn_index: 1,
                kind: qsf_memory::ProcessedRangeKind::SessionEnd,
                at: ts(),
            }],
        };

        let plan = build_promotion_plan(
            &report_with_candidates(vec![]),
            &session,
            &store,
            ts(),
            "sleep-1",
        );

        assert!(plan.new_associations.is_empty());
        assert!(plan.strengthened_associations.is_empty());
        assert!(plan.processed_ranges.is_empty());
    }

    #[test]
    fn cross_turn_retrievals_dedupe_pairs_across_uncovered_segments() {
        let mut session = empty_session();
        session.turns.push(turn_with_memories(0, &["memory.x"]));
        session.turns.push(turn_with_memories(1, &[]));
        session.turns.push(turn_with_memories(2, &["memory.x"]));
        session.turns.push(turn_with_memories(3, &["memory.y"]));
        let store = MemoryStoreContents {
            records: vec![
                memory_record("memory.x", "X.", "X."),
                memory_record("memory.y", "Y.", "Y."),
            ],
            associations: vec![],
            processed_ranges: vec![
                qsf_memory::ProcessedRange {
                    session_id: session.session_id.clone(),
                    first_turn_index: 1,
                    last_turn_index: 1,
                    kind: qsf_memory::ProcessedRangeKind::LiveBatch,
                    at: ts(),
                },
                qsf_memory::ProcessedRange {
                    session_id: session.session_id.clone(),
                    first_turn_index: 3,
                    last_turn_index: 3,
                    kind: qsf_memory::ProcessedRangeKind::LiveBatch,
                    at: ts(),
                },
            ],
        };

        let plan = build_promotion_plan(
            &report_with_candidates(vec![]),
            &session,
            &store,
            ts(),
            "sleep-1",
        );

        let matching = plan
            .new_associations
            .iter()
            .filter(|association| {
                association.from_memory_id == "memory.x" && association.to_memory_id == "memory.y"
            })
            .count();
        assert_eq!(matching, 1);
    }

    #[test]
    fn promotes_sleep_association_candidates_between_new_records() {
        let mut report = report_with_candidates(vec![
            "The assistant's name is Ari.",
            "The user wants assistant identity work.",
        ]);
        report
            .association_candidates
            .push(SleepAssociationCandidate {
                from_memory_candidate_index: 1,
                to_memory_candidate_index: 2,
                weight: Some(0.42),
                reason: Some("Both describe assistant identity context.".to_string()),
            });

        let plan = build_promotion_plan(
            &report,
            &empty_session(),
            &MemoryStoreContents::default(),
            ts(),
            "sleep-1",
        );

        assert_eq!(plan.new_records.len(), 2);
        assert_eq!(plan.new_associations.len(), 1);
        assert_eq!(
            plan.new_associations[0].from_memory_id,
            "memory.sleep.sleep-1.001"
        );
        assert_eq!(
            plan.new_associations[0].to_memory_id,
            "memory.sleep.sleep-1.002"
        );
        assert_eq!(plan.new_associations[0].weight, 0.42);
        assert_eq!(
            plan.new_associations[0].reason,
            "Both describe assistant identity context."
        );
    }

    #[test]
    fn skips_sleep_association_when_endpoint_candidate_was_not_promoted() {
        let mut report = report_with_candidates(vec![
            "Reducers stay pure.",
            "Tools are perception extensions.",
        ]);
        report
            .association_candidates
            .push(SleepAssociationCandidate {
                from_memory_candidate_index: 1,
                to_memory_candidate_index: 2,
                weight: Some(0.7),
                reason: Some("Both describe runtime architecture.".to_string()),
            });
        let store = MemoryStoreContents {
            records: vec![memory_record(
                "memory.existing",
                "Reducers stay pure.",
                "Reducers stay pure.",
            )],
            associations: vec![],
            ..MemoryStoreContents::default()
        };

        let plan = build_promotion_plan(&report, &empty_session(), &store, ts(), "sleep-1");

        assert_eq!(plan.new_records.len(), 1);
        assert!(plan.new_associations.is_empty());
    }
}
