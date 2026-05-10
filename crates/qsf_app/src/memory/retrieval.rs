use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use serde::Serialize;

use super::association::{Association, ensure_current_association_schema};
use super::memory_record::{MemoryRecord, ensure_current_memory_schema};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalStrategy {
    RecencyOnly,
    KeywordTag,
    AssociationWeighted,
}

impl fmt::Display for RetrievalStrategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecencyOnly => formatter.write_str("recency-only"),
            Self::KeywordTag => formatter.write_str("keyword-tag"),
            Self::AssociationWeighted => formatter.write_str("association-weighted"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievalScore {
    pub total: f64,
    pub recency: f64,
    pub keyword: f64,
    pub tag: f64,
    pub association: f64,
    pub importance: f64,
    pub reinforcement: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssociationPath {
    pub from_memory_id: String,
    pub to_memory_id: String,
    pub weight: f64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievedMemory {
    pub memory: MemoryRecord,
    pub strategy: RetrievalStrategy,
    pub score: RetrievalScore,
    pub matched_terms: Vec<String>,
    pub association_paths: Vec<AssociationPath>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RetrievalResult {
    pub query: String,
    pub strategy: RetrievalStrategy,
    pub selected: Vec<RetrievedMemory>,
    pub omitted: Vec<RetrievedMemory>,
    pub latency_ms: u64,
    pub latency_ns: u64,
}

pub fn retrieve_memories(
    records: &[MemoryRecord],
    associations: &[Association],
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
) -> anyhow::Result<RetrievalResult> {
    ensure_current_memory_schema(records)?;
    ensure_current_association_schema(associations)?;

    let started_at = Instant::now();
    let query_terms = tokenize(query);
    let recency_by_id = recency_scores(records);
    let seed_ids = keyword_seed_ids(records, &query_terms);
    let association_paths = association_paths_by_target(associations, &seed_ids);

    let mut candidates = records
        .iter()
        .map(|record| {
            let matched_terms = matched_terms(record, &query_terms);
            let paths = association_paths
                .get(&record.id)
                .cloned()
                .unwrap_or_default();
            let score = score_record(
                record,
                strategy,
                *recency_by_id.get(&record.id).unwrap_or(&0.0),
                &matched_terms,
                &paths,
            );

            RetrievedMemory {
                memory: record.clone(),
                strategy,
                score,
                matched_terms,
                association_paths: paths,
            }
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score
            .total
            .total_cmp(&left.score.total)
            .then_with(|| right.memory.created_at.cmp(&left.memory.created_at))
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });

    let selected_count = limit.min(candidates.len());
    let omitted = candidates.split_off(selected_count);
    let selected = candidates;

    let elapsed = started_at.elapsed();

    Ok(RetrievalResult {
        query: query.to_string(),
        strategy,
        selected,
        omitted,
        latency_ms: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
        latency_ns: elapsed.as_nanos().try_into().unwrap_or(u64::MAX),
    })
}

fn score_record(
    record: &MemoryRecord,
    strategy: RetrievalStrategy,
    recency: f64,
    matched_terms: &[String],
    association_paths: &[AssociationPath],
) -> RetrievalScore {
    let keyword = matched_terms_in_text(record, matched_terms) as f64;
    let tag = matched_terms_in_tags(record, matched_terms) as f64;
    let association = association_paths
        .iter()
        .map(|path| path.weight)
        .sum::<f64>()
        .min(2.0);
    let importance = record.importance;
    // Cap reinforcement so repeated retrieval/use can help without dominating direct relevance.
    let reinforcement = f64::from(record.reinforcement_count).min(5.0) / 5.0;

    let total = match strategy {
        RetrievalStrategy::RecencyOnly => recency,
        RetrievalStrategy::KeywordTag => {
            // Curated tags outweigh free-text matches; importance is a light nudge, not a replacement for direct relevance.
            (keyword * 0.8) + (tag * 1.4) + (importance * 0.35) + (recency * 0.2)
        }
        RetrievalStrategy::AssociationWeighted => {
            // Association weight is strongest after direct tag matches so linked memories can surface without overwhelming explicit query matches.
            (keyword * 0.65)
                + (tag * 1.1)
                + (association * 1.35)
                + (importance * 0.35)
                + (recency * 0.2)
                + (reinforcement * 0.25)
        }
    };

    RetrievalScore {
        total,
        recency,
        keyword,
        tag,
        association,
        importance,
        reinforcement,
    }
}

fn tokenize(input: &str) -> HashSet<String> {
    input
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let normalized = term.trim().to_ascii_lowercase();
            // Keep the MVP signal low-noise; two-letter acronyms like AI/UI need an explicit future policy.
            if normalized.len() < 3 {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn memory_terms(record: &MemoryRecord) -> HashSet<String> {
    let mut terms = tokenize(&record.title);
    terms.extend(tokenize(&record.summary));
    terms.extend(record.tags.iter().map(|tag| tag.to_ascii_lowercase()));
    terms
}

fn matched_terms(record: &MemoryRecord, query_terms: &HashSet<String>) -> Vec<String> {
    let memory_terms = memory_terms(record);
    let mut matches = query_terms
        .iter()
        .filter(|term| memory_terms.contains(*term))
        .cloned()
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

fn matched_terms_in_text(record: &MemoryRecord, terms: &[String]) -> usize {
    let text_terms = tokenize(&format!("{} {}", record.title, record.summary));
    terms
        .iter()
        .filter(|term| text_terms.contains(term.as_str()))
        .count()
}

fn matched_terms_in_tags(record: &MemoryRecord, terms: &[String]) -> usize {
    let tags = record
        .tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    terms
        .iter()
        .filter(|term| tags.contains(term.as_str()))
        .count()
}

fn keyword_seed_ids(records: &[MemoryRecord], query_terms: &HashSet<String>) -> HashSet<String> {
    records
        .iter()
        .filter(|record| !matched_terms(record, query_terms).is_empty())
        .map(|record| record.id.clone())
        .collect()
}

fn association_paths_by_target(
    associations: &[Association],
    seed_ids: &HashSet<String>,
) -> HashMap<String, Vec<AssociationPath>> {
    let mut paths: HashMap<String, Vec<AssociationPath>> = HashMap::new();

    for association in associations {
        if seed_ids.contains(&association.from_memory_id) {
            paths
                .entry(association.to_memory_id.clone())
                .or_default()
                .push(AssociationPath {
                    from_memory_id: association.from_memory_id.clone(),
                    to_memory_id: association.to_memory_id.clone(),
                    weight: association.weight,
                    reason: association.reason.clone(),
                });
        }
    }

    for target_paths in paths.values_mut() {
        target_paths.sort_by(|left, right| right.weight.total_cmp(&left.weight));
    }

    paths
}

fn recency_scores(records: &[MemoryRecord]) -> HashMap<String, f64> {
    let mut sorted = records.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    let total = sorted.len().max(1) as f64;

    sorted
        .into_iter()
        .enumerate()
        .map(|(index, record)| {
            let score = (total - index as f64) / total;
            (record.id.clone(), score)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{RetrievalStrategy, retrieve_memories};
    use crate::memory::phase_four_fixture;

    #[test]
    fn recency_only_prefers_newest_records() {
        let fixture = phase_four_fixture();
        let result = retrieve_memories(
            &fixture.records,
            &fixture.associations,
            "memory context association",
            RetrievalStrategy::RecencyOnly,
            2,
        )
        .unwrap();

        assert_eq!(result.selected[0].memory.id, "memory.non-goals");
        assert_eq!(result.selected[1].memory.id, "memory.external-inputs");
    }

    #[test]
    fn keyword_tag_retrieval_prefers_direct_matches() {
        let fixture = phase_four_fixture();
        let result = retrieve_memories(
            &fixture.records,
            &fixture.associations,
            "context budget retrieval",
            RetrievalStrategy::KeywordTag,
            2,
        )
        .unwrap();

        assert_eq!(result.selected[0].memory.id, "memory.context-budget");
        assert!(
            result.selected[0]
                .matched_terms
                .contains(&"context".to_string())
        );
    }

    #[test]
    fn association_weighted_retrieval_includes_linked_memories() {
        let fixture = phase_four_fixture();
        let result = retrieve_memories(
            &fixture.records,
            &fixture.associations,
            "associative memory",
            RetrievalStrategy::AssociationWeighted,
            3,
        )
        .unwrap();

        assert!(
            result
                .selected
                .iter()
                .any(|memory| memory.memory.id == "memory.context-budget")
        );
        assert!(
            result
                .selected
                .iter()
                .any(|memory| !memory.association_paths.is_empty())
        );
    }

    #[test]
    fn tokenize_drops_short_terms_for_mvp_noise_control() {
        let terms = super::tokenize("AI UI context id");

        assert!(terms.contains("context"));
        assert!(!terms.contains("ai"));
        assert!(!terms.contains("ui"));
        assert!(!terms.contains("id"));
    }
}
