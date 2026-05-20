use serde::Serialize;
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::sleep_report::SleepReport;

pub const CROSS_TURN_ASSOCIATION_WINDOW: usize = 3;
pub const SLEEP_ASSOCIATION_INITIAL_WEIGHT: f64 = 0.35;
pub const SLEEP_ASSOCIATION_STRENGTHEN_DELTA: f64 = 0.05;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PromotionPlan {
    pub new_records: Vec<MemoryRecord>,
    pub new_associations: Vec<Association>,
    pub strengthened_associations: Vec<(String, String, f64)>,
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

        let record = MemoryRecord::new(
            format!("memory.sleep.{}.{:03}", sanitize(sleep_run_id), index + 1),
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
        new_records.push(record);
    }

    let (new_associations, strengthened_associations) =
        build_cross_turn_associations(session, current_store, as_of);

    PromotionPlan {
        new_records,
        new_associations,
        strengthened_associations,
        skipped_duplicates,
    }
}

fn build_cross_turn_associations(
    session: &SessionState,
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
) -> (Vec<Association>, Vec<(String, String, f64)>) {
    let mut new_associations = Vec::new();
    let mut strengthened_associations = Vec::new();
    let retrievals = session
        .turns
        .iter()
        .map(|turn| turn.context_assembly.retrieved_memory_ids())
        .collect::<Vec<_>>();

    for from_turn in 0..retrievals.len() {
        let last_turn = (from_turn + CROSS_TURN_ASSOCIATION_WINDOW).min(retrievals.len() - 1);
        for to_turn in (from_turn + 1)..=last_turn {
            for from_id in &retrievals[from_turn] {
                for to_id in &retrievals[to_turn] {
                    if from_id == to_id {
                        continue;
                    }
                    let (left, right) = ordered_pair(from_id, to_id);
                    if new_associations.iter().any(|association: &Association| {
                        association.from_memory_id == left && association.to_memory_id == right
                    }) || strengthened_associations
                        .iter()
                        .any(|(from, to, _)| from == &left && to == &right)
                    {
                        continue;
                    }

                    if let Some(existing) = current_store.associations.iter().find(|association| {
                        association.from_memory_id == left && association.to_memory_id == right
                    }) {
                        strengthened_associations.push((
                            left,
                            right,
                            (existing.weight + SLEEP_ASSOCIATION_STRENGTHEN_DELTA).min(1.0),
                        ));
                    } else {
                        new_associations.push(Association::new(
                            left,
                            right,
                            SLEEP_ASSOCIATION_INITIAL_WEIGHT,
                            format!(
                                "co-retrieved within {} turns during session {}",
                                CROSS_TURN_ASSOCIATION_WINDOW, session.session_id
                            ),
                            as_of,
                        ));
                    }
                }
            }
        }
    }

    new_associations.sort_by(|left, right| {
        left.from_memory_id
            .cmp(&right.from_memory_id)
            .then_with(|| left.to_memory_id.cmp(&right.to_memory_id))
    });
    strengthened_associations
        .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    (new_associations, strengthened_associations)
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
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

fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use crate::context::{ContextAssembly, ContextBudget, ContextFragment, ContextSelection};
    use crate::session::{MemorySourceConfig, SessionConfig};
    use crate::sleep::sleep_report::{SleepMemoryCandidate, SleepReport};

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
            records: vec![],
            associations: vec![Association::new(
                "memory.a",
                "memory.c",
                0.4,
                "existing",
                ts(),
            )],
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
}
