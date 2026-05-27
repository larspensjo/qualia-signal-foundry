//! Sleep-time pluggable association proposer interface.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposedAssociation {
    pub from_id: String,
    pub to_id: String,
    pub weight: f64,
    pub reason: String,
    pub proposer_name: String,
}

pub trait AssociationProposer {
    fn name(&self) -> &str;

    /// Higher priority wins ties when two proposers propose the same pair.
    fn priority(&self) -> u8 {
        50
    }

    fn propose(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation>;
}

/// Merge proposed associations across multiple proposers, dedupe by
/// unordered endpoint pair, and drop any pair where either endpoint is not in
/// `known_record_ids`.
pub fn merge_and_dedupe(
    proposals: Vec<ProposedAssociation>,
    existing: &[Association],
    known_record_ids: &HashSet<String>,
) -> Vec<ProposedAssociation> {
    let mut seen: HashSet<(String, String)> = existing
        .iter()
        .map(|association| ordered_pair(&association.from_memory_id, &association.to_memory_id))
        .collect();
    let mut merged = Vec::new();

    for proposal in proposals {
        let from_id = proposal.from_id.trim();
        let to_id = proposal.to_id.trim();
        if from_id.is_empty() || to_id.is_empty() || from_id == to_id {
            continue;
        }
        if !known_record_ids.contains(from_id) || !known_record_ids.contains(to_id) {
            continue;
        }

        let key = ordered_pair(from_id, to_id);
        if seen.insert(key) {
            merged.push(ProposedAssociation {
                from_id: proposal.from_id,
                to_id: proposal.to_id,
                weight: proposal.weight.clamp(0.0, 1.0),
                reason: proposal.reason,
                proposer_name: proposal.proposer_name,
            });
        }
    }

    merged
}

/// Sort tagged proposals by proposer priority descending.
pub fn sort_by_priority_descending(proposals: &mut [(u8, ProposedAssociation)]) {
    proposals.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.proposer_name.cmp(&right.1.proposer_name))
            .then_with(|| left.1.from_id.cmp(&right.1.from_id))
            .then_with(|| left.1.to_id.cmp(&right.1.to_id))
            .then_with(|| right.1.weight.total_cmp(&left.1.weight))
            .then_with(|| left.1.reason.cmp(&right.1.reason))
    });
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn dedupe_drops_pair_existing_in_store() {
        let existing = vec![Association::new(
            "a",
            "b",
            0.5,
            "r",
            time::OffsetDateTime::UNIX_EPOCH,
        )];
        let proposals = vec![ProposedAssociation {
            from_id: "a".into(),
            to_id: "b".into(),
            weight: 0.4,
            reason: "p".into(),
            proposer_name: "x".into(),
        }];

        let merged = merge_and_dedupe(proposals, &existing, &known(&["a", "b"]));

        assert!(merged.is_empty());
    }

    #[test]
    fn dedupe_drops_missing_endpoints() {
        let proposals = vec![ProposedAssociation {
            from_id: "a".into(),
            to_id: "ghost".into(),
            weight: 0.4,
            reason: "p".into(),
            proposer_name: "x".into(),
        }];

        let merged = merge_and_dedupe(proposals, &[], &known(&["a"]));

        assert!(merged.is_empty());
    }

    #[test]
    fn dedupe_dedupes_across_proposers() {
        let proposals = vec![
            ProposedAssociation {
                from_id: "a".into(),
                to_id: "b".into(),
                weight: 0.4,
                reason: "p1".into(),
                proposer_name: "x".into(),
            },
            ProposedAssociation {
                from_id: "b".into(),
                to_id: "a".into(),
                weight: 0.5,
                reason: "p2".into(),
                proposer_name: "y".into(),
            },
        ];

        let merged = merge_and_dedupe(proposals, &[], &known(&["a", "b"]));

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].from_id, "a");
        assert_eq!(merged[0].to_id, "b");
    }

    #[test]
    fn priority_sort_keeps_high_priority_proposer_on_collision() {
        let mut tagged: Vec<(u8, ProposedAssociation)> = vec![
            (
                30,
                ProposedAssociation {
                    from_id: "a".into(),
                    to_id: "b".into(),
                    weight: 0.3,
                    reason: "low".into(),
                    proposer_name: "safety-net".into(),
                },
            ),
            (
                100,
                ProposedAssociation {
                    from_id: "a".into(),
                    to_id: "b".into(),
                    weight: 0.4,
                    reason: "high".into(),
                    proposer_name: "llm".into(),
                },
            ),
        ];
        sort_by_priority_descending(&mut tagged);

        let proposals: Vec<ProposedAssociation> =
            tagged.into_iter().map(|(_, proposal)| proposal).collect();
        let merged = merge_and_dedupe(proposals, &[], &known(&["a", "b"]));

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].proposer_name, "llm");
    }
}
