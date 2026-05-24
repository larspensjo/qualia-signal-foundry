//! Single-hop neighbor expansion for retrieved memories.
//! Produces hint candidates from persisted `Association` edges, undirected.

use std::collections::{BTreeSet, HashMap};

use crate::memory::association::Association;
use crate::memory::memory_record::MemoryRecord;

#[derive(Clone, Debug, PartialEq)]
pub struct HintCandidate {
    pub memory: MemoryRecord,
    pub via_direct_id: String,
    pub association_reason: String,
    pub weight: f64,
}

pub const MAX_HINTS_PER_TURN: usize = 8;

/// Undirected single-hop expansion. Returns up to `max_hints` unique hint candidates,
/// ordered by descending association weight, then by `via_direct_id`, then by hint memory id.
/// A memory id already present in `direct_ids` is never returned as a hint.
pub fn expand_neighbors(
    direct_ids: &[String],
    records: &[MemoryRecord],
    associations: &[Association],
    max_hints: usize,
) -> Vec<HintCandidate> {
    let direct_set: BTreeSet<&str> = direct_ids.iter().map(String::as_str).collect();
    let record_by_id: HashMap<&str, &MemoryRecord> = records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect();
    let mut candidate_by_memory_id: HashMap<String, HintCandidate> = HashMap::new();

    for direct in direct_ids {
        for association in associations {
            let neighbor_id = if association.from_memory_id == *direct {
                association.to_memory_id.as_str()
            } else if association.to_memory_id == *direct {
                association.from_memory_id.as_str()
            } else {
                continue;
            };

            if direct_set.contains(neighbor_id) {
                continue;
            }

            let Some(memory) = record_by_id.get(neighbor_id) else {
                continue;
            };

            let candidate = HintCandidate {
                memory: (*memory).clone(),
                via_direct_id: direct.clone(),
                association_reason: association.reason.clone(),
                weight: association.weight,
            };
            match candidate_by_memory_id.get_mut(neighbor_id) {
                Some(existing) if candidate_sorts_before(&candidate, existing) => {
                    *existing = candidate;
                }
                Some(_) => {}
                None => {
                    candidate_by_memory_id.insert(neighbor_id.to_string(), candidate);
                }
            }
        }
    }

    let mut candidates = candidate_by_memory_id.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.via_direct_id.cmp(&right.via_direct_id))
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });
    candidates.truncate(max_hints);
    candidates
}

fn candidate_sorts_before(left: &HintCandidate, right: &HintCandidate) -> bool {
    right
        .weight
        .total_cmp(&left.weight)
        .then_with(|| left.via_direct_id.cmp(&right.via_direct_id))
        .then_with(|| left.memory.id.cmp(&right.memory.id))
        .is_lt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap()
    }

    fn record(id: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            "Title",
            "Summary text.",
            vec!["topic"],
            ts(),
            0.5,
            0,
            "tests",
            10,
        )
    }

    fn edge(from: &str, to: &str, weight: f64, reason: &str) -> Association {
        Association::new(from, to, weight, reason, ts())
    }

    #[test]
    fn outgoing_edge_produces_neighbor() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("a", "b", 0.5, "outgoing")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
        assert_eq!(hints[0].via_direct_id, "a");
    }

    #[test]
    fn incoming_edge_produces_neighbor() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("b", "a", 0.5, "incoming")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
    }

    #[test]
    fn reciprocal_pair_yields_single_unique_hint() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("a", "b", 0.4, "out"), edge("b", "a", 0.5, "in")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
        assert!((hints[0].weight - 0.5).abs() < 1e-9);
        assert_eq!(hints[0].association_reason, "in");
    }

    #[test]
    fn neighbor_already_in_directs_is_skipped() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("a", "b", 0.5, "r")];

        let hints = expand_neighbors(&["a".to_string(), "b".to_string()], &records, &edges, 8);

        assert!(hints.is_empty());
    }

    #[test]
    fn dangling_edge_is_dropped_silently() {
        let records = vec![record("a")];
        let edges = vec![edge("a", "b", 0.5, "r")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert!(hints.is_empty());
    }

    #[test]
    fn max_hints_cap_is_enforced_and_weight_ordered() {
        let records = (0..10)
            .map(|i| record(&format!("n{i}")))
            .collect::<Vec<_>>();
        let edges = (0..10)
            .map(|i| edge("a", &format!("n{i}"), 0.1 * i as f64, "r"))
            .collect::<Vec<_>>();
        let mut all_records = records.clone();
        all_records.push(record("a"));

        let hints = expand_neighbors(&["a".to_string()], &all_records, &edges, 3);

        assert_eq!(hints.len(), 3);
        assert_eq!(hints[0].memory.id, "n9");
        assert_eq!(hints[1].memory.id, "n8");
        assert_eq!(hints[2].memory.id, "n7");
    }
}
