use serde::Serialize;
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
use crate::memory::processed_ranges::{contiguous_ranges, uncovered_turn_indices};
use crate::memory::store::MemoryStoreContents;
use crate::memory::token_estimate::estimated_tokens;
use crate::session::SessionState;
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

    let cross_turn = build_cross_turn_associations(session, current_store, as_of);
    let mut new_associations = cross_turn.new_associations;
    new_associations.extend(build_sleep_candidate_associations(
        report,
        &promoted_candidate_ids,
        current_store,
        as_of,
    ));

    PromotionPlan {
        new_records,
        new_associations,
        strengthened_associations: cross_turn.strengthened_associations,
        processed_ranges: cross_turn.processed_ranges,
        skipped_duplicates,
    }
}

fn build_cross_turn_associations(
    session: &SessionState,
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
) -> CrossTurnAssociationPlan {
    use crate::memory::co_retrieval::{
        CROSS_TURN_ASSOCIATION_WINDOW, CoRetrievalDelta, CrossTurnAnchorRange,
        generate_cross_turn_deltas_for_anchor_ranges,
    };

    if session.turns.is_empty() {
        return CrossTurnAssociationPlan::default();
    }

    let uncovered = uncovered_turn_indices(
        &current_store.processed_ranges,
        &session.session_id,
        0,
        session.turns.len() - 1,
    );
    if uncovered.is_empty() {
        return CrossTurnAssociationPlan::default();
    }

    let known = current_store
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect();
    let retrievals = session
        .turns
        .iter()
        .map(|turn| turn.context_assembly.retrieved_memory_ids())
        .collect::<Vec<_>>();

    let ranges = contiguous_ranges(&uncovered);
    let anchor_ranges = ranges
        .iter()
        .map(|(first, last)| CrossTurnAnchorRange {
            first_turn: *first,
            last_turn: *last,
        })
        .collect::<Vec<_>>();
    let deltas = generate_cross_turn_deltas_for_anchor_ranges(
        &retrievals,
        &current_store.associations,
        &known,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &session.session_id,
        as_of,
        &anchor_ranges,
    );
    let mut new_associations = Vec::new();
    let mut strengthened_associations = Vec::new();
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create {
                from,
                to,
                weight,
                reason,
                at,
            } => {
                new_associations.push(Association::new(from, to, weight, reason, at));
            }
            CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight,
                ..
            } => {
                strengthened_associations.push((from, to, new_weight));
            }
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

    CrossTurnAssociationPlan {
        new_associations,
        strengthened_associations,
        processed_ranges,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CrossTurnAssociationPlan {
    new_associations: Vec<Association>,
    strengthened_associations: Vec<(String, String, f64)>,
    processed_ranges: Vec<qsf_memory::ProcessedRange>,
}

fn build_sleep_candidate_associations(
    report: &SleepReport,
    promoted_candidate_ids: &[Option<String>],
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
) -> Vec<Association> {
    let mut associations = Vec::new();

    for candidate in &report.association_candidates {
        let Some(from_id) = candidate
            .from_memory_candidate_index
            .checked_sub(1)
            .and_then(|index| promoted_candidate_ids.get(index))
            .and_then(Option::as_ref)
        else {
            continue;
        };
        let Some(to_id) = candidate
            .to_memory_candidate_index
            .checked_sub(1)
            .and_then(|index| promoted_candidate_ids.get(index))
            .and_then(Option::as_ref)
        else {
            continue;
        };
        if from_id == to_id || association_exists(current_store, &associations, from_id, to_id) {
            continue;
        }

        let Some(reason) = candidate
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        else {
            continue;
        };
        let Some(weight) = candidate.weight else {
            continue;
        };

        associations.push(Association::new(
            from_id.clone(),
            to_id.clone(),
            weight.clamp(0.0, 1.0),
            reason.to_string(),
            as_of,
        ));
    }

    associations
}

fn association_exists(
    current_store: &MemoryStoreContents,
    pending_associations: &[Association],
    from_id: &str,
    to_id: &str,
) -> bool {
    current_store
        .associations
        .iter()
        .chain(pending_associations.iter())
        .any(|association| {
            association.from_memory_id == from_id && association.to_memory_id == to_id
        })
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
