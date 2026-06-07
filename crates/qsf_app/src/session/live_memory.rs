use std::collections::HashSet;
use std::path::Path;

use serde_json::json;

use crate::memory::{
    Association, LiveCaptureInput, LiveMemoryCandidate, MemoryRecord, MemoryRecordKind,
    RetrievalResult, capture_live_memory_candidates, estimated_tokens, remember_this_skip_reason,
};
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;

use super::SessionState;

pub(crate) fn apply_live_memory_reinforcement(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    retrieval: &RetrievalResult,
) -> anyhow::Result<()> {
    let turn_index = completed_turn_count(state);
    let memory_store_path = state_dir.join("memory-store.json");
    let retrieved_pairs = retrieval
        .selected
        .iter()
        .map(|memory| (memory.memory.id.clone(), memory.score.total))
        .collect::<Vec<_>>();
    let retrieved_ids = retrieved_pairs
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let mut relevance_skipped_ids = Vec::new();
    let mut over_limit_skipped_ids = Vec::new();
    for memory in &retrieval.omitted {
        match memory.skip_reason.as_deref() {
            Some(crate::memory::retrieval::RELEVANCE_GATE_SKIP_REASON) => {
                relevance_skipped_ids.push(memory.memory.id.clone());
            }
            Some(crate::memory::retrieval::RETRIEVAL_LIMIT_SKIP_REASON) => {
                over_limit_skipped_ids.push(memory.memory.id.clone());
            }
            _ => {}
        }
    }
    let relevance_skipped_count = relevance_skipped_ids.len();
    let over_limit_skipped_count = over_limit_skipped_ids.len();
    let no_store_skipped_count = retrieved_ids.len();

    if !memory_store_path.exists() {
        context.record_event(
            EventType::MemoryReinforced,
            json!({
                "turn_index": turn_index,
                "ids": Vec::<String>::new(),
                "requested_ids": retrieved_ids.clone(),
                "skipped_relevance_ids": relevance_skipped_ids,
                "skipped_over_limit_ids": over_limit_skipped_ids,
                "skipped_no_store_ids": retrieved_ids,
                "count": 0,
                "skipped_relevance_count": relevance_skipped_count,
                "skipped_over_limit_count": over_limit_skipped_count,
                "skipped_no_store_count": no_store_skipped_count,
                "timestamp_source": "live_now",
                "skipped_reason": "no persistent memory store on cold start",
            }),
            None,
        )?;
        return Ok(());
    }

    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path)?;
    let now = time::OffsetDateTime::now_utc();
    let deltas = crate::memory::co_retrieval::generate_deltas(
        &retrieved_pairs,
        &store.contents().associations,
        turn_index,
        &state.session_id,
        now,
    );

    let mut created_count = 0;
    let mut strengthened_count = 0;
    for delta in &deltas {
        match delta {
            crate::memory::co_retrieval::CoRetrievalDelta::Create {
                from,
                to,
                weight,
                reason,
                at,
            } => {
                store.contents_mut().associations.push(Association::new(
                    from.clone(),
                    to.clone(),
                    *weight,
                    reason.clone(),
                    *at,
                ));
                created_count += 1;
            }
            crate::memory::co_retrieval::CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight,
                at,
            } => {
                if let Some(existing) =
                    store
                        .contents_mut()
                        .associations
                        .iter_mut()
                        .find(|association| {
                            (association.from_memory_id == *from && association.to_memory_id == *to)
                                || (association.from_memory_id == *to
                                    && association.to_memory_id == *from)
                        })
                {
                    existing.weight = *new_weight;
                    existing.last_reinforced_at = *at;
                    strengthened_count += 1;
                }
            }
        }
    }

    let retrieved_id_set = retrieved_ids.iter().cloned().collect::<HashSet<_>>();
    let mut reinforced_ids = Vec::new();
    for record in &mut store.contents_mut().records {
        if retrieved_id_set.contains(&record.id) {
            record.reinforcement_count = record.reinforcement_count.saturating_add(1);
            record.last_reinforced_at = Some(now);
            reinforced_ids.push(record.id.clone());
        }
    }
    reinforced_ids.sort();
    let reinforced_count = reinforced_ids.len();

    let dropped_count = candidate_pair_count(&retrieved_pairs)
        .saturating_sub(created_count)
        .saturating_sub(strengthened_count);

    context.record_event(
        EventType::CoRetrievalAssociationsProposed,
        json!({
            "turn_index": turn_index,
            "proposed_count": deltas.len(),
            "created_count": created_count,
            "strengthened_count": strengthened_count,
            "dropped_count": dropped_count,
        }),
        None,
    )?;
    context.record_event(
        EventType::MemoryReinforced,
        json!({
            "turn_index": turn_index,
            "ids": reinforced_ids.clone(),
            "requested_ids": retrieved_ids,
            "skipped_relevance_ids": relevance_skipped_ids,
            "skipped_over_limit_ids": over_limit_skipped_ids,
            "skipped_no_store_ids": Vec::<String>::new(),
            "count": reinforced_count,
            "skipped_relevance_count": relevance_skipped_count,
            "skipped_over_limit_count": over_limit_skipped_count,
            "skipped_no_store_count": 0,
            "timestamp_source": "live_now",
        }),
        None,
    )?;

    if !deltas.is_empty() || !reinforced_ids.is_empty() {
        store.persist()?;
        context.record_event(
            EventType::MemoryStorePersisted,
            json!({
                "turn_index": turn_index,
                "path": memory_store_path.display().to_string(),
                "records_count": store.contents().records.len(),
                "associations_count": store.contents().associations.len(),
            }),
            None,
        )?;
    }

    Ok(())
}

pub(crate) fn apply_live_memory_capture(
    context: &mut RunContext,
    state: &SessionState,
    state_dir: &Path,
    user_input: &str,
    assistant_response: &str,
) -> anyhow::Result<()> {
    let previous_turn = state.turns.last();
    let capture_input = LiveCaptureInput {
        user_input,
        assistant_response,
        previous_turn_index: previous_turn.map(|turn| turn.index),
        previous_user_input: previous_turn.map(|turn| turn.user_input.as_str()),
        previous_assistant_response: previous_turn.map(|turn| turn.assistant_response.as_str()),
    };
    let remember_skip_reason = remember_this_skip_reason(&capture_input);
    let candidates = capture_live_memory_candidates(&capture_input);
    let turn_index = completed_turn_count(state);

    if candidates.is_empty() {
        if let Some(reason) = remember_skip_reason {
            let trace = TraceRecord::new(
                context.experiment_id(),
                "live-memory-capture",
                format!("turn={} user_input={}", turn_index, user_input),
                "skipped remember-this capture",
            )
            .with_details(json!({
                "session_id": state.session_id,
                "turn_index": turn_index,
                "stage": "remember-this",
                "reason": reason,
                "previous_turn_index": capture_input.previous_turn_index,
                "previous_user_input": capture_input.previous_user_input,
            }))
            .with_latency_context("runtime", "live-memory-capture");
            context.record_trace(trace)?;
        }
        return Ok(());
    }

    let memory_store_path = state_dir.join("memory-store.json");
    let mut store = crate::memory::MemoryStore::load_or_empty(&memory_store_path)?;
    let now = time::OffsetDateTime::now_utc();
    let mut persisted_records = Vec::new();
    let mut record_ids = Vec::new();
    let mut candidate_kinds = Vec::new();
    let mut duplicate_ids = Vec::new();
    for candidate in candidates {
        if store
            .contents()
            .records
            .iter()
            .any(|record| live_memory_duplicate(record, &candidate))
            || persisted_records
                .iter()
                .any(|record: &MemoryRecord| live_memory_duplicate(record, &candidate))
        {
            duplicate_ids.push(candidate.candidate_kind.as_str().to_string());
            continue;
        }

        let record_id = format!(
            "memory.live.{}.turn-{:03}.{}",
            memory_id_segment(&state.session_id),
            turn_index,
            candidate.id_suffix
        );
        let source_turn_index = candidate.source_turn_index.unwrap_or(turn_index);
        let record = MemoryRecord::new(
            record_id.clone(),
            MemoryRecordKind::Observation,
            candidate.title.clone(),
            candidate.summary.clone(),
            candidate
                .tags
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            now,
            candidate.importance,
            0,
            format!(
                "session:{}#turn-{:03}:live_memory_capture:source-turn-{:03}",
                state.session_id, turn_index, source_turn_index
            ),
            estimated_tokens(&candidate.summary),
        )
        .with_last_reinforced_at(now);

        candidate_kinds.push(candidate.candidate_kind.as_str().to_string());
        record_ids.push(record_id);
        persisted_records.push(record);
    }

    if persisted_records.is_empty() {
        if remember_skip_reason.is_none() {
            let duplicate_intent = if duplicate_ids.iter().any(|kind| kind == "remembered-topic") {
                "remember-this"
            } else {
                "live-memory-capture"
            };
            let trace = TraceRecord::new(
                context.experiment_id(),
                "live-memory-capture",
                format!("turn={} user_input={}", turn_index, user_input),
                "live memory capture matched only duplicates",
            )
            .with_details(json!({
                "session_id": state.session_id,
                "turn_index": turn_index,
                "stage": "live_memory_capture",
                "intent": duplicate_intent,
                "duplicate_kinds": duplicate_ids,
                "previous_turn_index": capture_input.previous_turn_index,
                "previous_user_input": capture_input.previous_user_input,
            }))
            .with_latency_context("runtime", "live-memory-capture");
            context.record_trace(trace)?;
        }
        return Ok(());
    }

    let persisted_count = persisted_records.len();
    store.append_records(persisted_records);
    store.persist()?;
    context.record_event(
        EventType::MemoryStorePersisted,
        json!({
            "session_id": state.session_id,
            "turn_index": turn_index,
            "stage": "live_memory_capture",
            "path": memory_store_path.display().to_string(),
            "candidate_count": record_ids.len(),
            "candidate_kinds": candidate_kinds.clone(),
            "record_ids": record_ids.clone(),
            "source_turn_index": capture_input.previous_turn_index,
            "records_count": store.contents().records.len(),
            "associations_count": store.contents().associations.len(),
        }),
        None,
    )?;
    let trace = TraceRecord::new(
        context.experiment_id(),
        "live-memory-capture",
        format!("turn={} user_input={}", turn_index, user_input),
        format!("captured {} live memory candidate(s)", persisted_count),
    )
    .with_details(json!({
        "session_id": state.session_id,
        "turn_index": turn_index,
        "stage": "live_memory_capture",
        "intent": if candidate_kinds.iter().any(|kind| kind == "remembered-topic") {
            "remember-this"
        } else {
            "live-memory-capture"
        },
        "candidate_count": persisted_count,
        "candidate_kinds": candidate_kinds,
        "record_ids": record_ids,
        "source_turn_index": capture_input.previous_turn_index,
        "remember_skip_reason": remember_skip_reason,
        "duplicate_kinds": duplicate_ids,
    }))
    .with_latency_context("runtime", "live-memory-capture");
    context.record_trace(trace)?;

    Ok(())
}

fn completed_turn_count(state: &SessionState) -> usize {
    state.turns.len()
}

fn live_memory_duplicate(record: &MemoryRecord, candidate: &LiveMemoryCandidate) -> bool {
    normalize_memory_text(&record.title) == normalize_memory_text(&candidate.title)
        && normalize_memory_text(&record.summary) == normalize_memory_text(&candidate.summary)
}

fn normalize_memory_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn memory_id_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        "session".to_string()
    } else {
        sanitized
    }
}

fn candidate_pair_count(retrieved: &[(String, f64)]) -> usize {
    let mut pairs = HashSet::new();
    for first_index in 0..retrieved.len() {
        for second_index in (first_index + 1)..retrieved.len() {
            let first_id = &retrieved[first_index].0;
            let second_id = &retrieved[second_index].0;
            if first_id == second_id {
                continue;
            }
            let pair = if first_id <= second_id {
                (first_id.clone(), second_id.clone())
            } else {
                (second_id.clone(), first_id.clone())
            };
            pairs.insert(pair);
        }
    }
    pairs.len()
}
