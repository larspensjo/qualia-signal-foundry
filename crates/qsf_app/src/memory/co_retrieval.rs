use std::collections::BTreeSet;

use time::OffsetDateTime;

use crate::memory::association::Association;

pub const CO_RETRIEVAL_INITIAL_WEIGHT: f64 = 0.3;
pub const CO_RETRIEVAL_STRENGTHEN_DELTA: f64 = 0.05;
pub const MAX_NEW_ASSOCIATIONS_PER_TURN: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub enum CoRetrievalDelta {
    Create {
        from: String,
        to: String,
        weight: f64,
        reason: String,
        at: OffsetDateTime,
    },
    Strengthen {
        from: String,
        to: String,
        new_weight: f64,
        at: OffsetDateTime,
    },
}

/// `retrieved` is a slice of (memory_id, retrieval_score) tuples.
/// `existing` is the current set of associations in the store.
/// Returns deterministically ordered deltas. At most MAX_NEW_ASSOCIATIONS_PER_TURN
/// Create deltas; excess pairs that have no existing association are dropped.
pub fn generate_deltas(
    retrieved: &[(String, f64)],
    existing: &[Association],
    turn_index: usize,
    session_id: &str,
    now: OffsetDateTime,
) -> Vec<CoRetrievalDelta> {
    if retrieved.len() < 2 {
        return vec![];
    }

    let mut seen_pairs = BTreeSet::new();
    let mut pairs: Vec<((String, String), f64)> = Vec::new();
    for first_index in 0..retrieved.len() {
        for second_index in (first_index + 1)..retrieved.len() {
            let (first_id, first_score) = &retrieved[first_index];
            let (second_id, second_score) = &retrieved[second_index];
            if first_id == second_id {
                continue;
            }

            let (from, to) = ordered_pair(first_id, second_id);
            if seen_pairs.insert((from.clone(), to.clone())) {
                pairs.push(((from, to), first_score + second_score));
            }
        }
    }

    pairs.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut deltas = Vec::new();
    let mut creates_used = 0;
    for ((from, to), _score) in pairs {
        if let Some(existing_association) = existing.iter().find(|association| {
            is_same_unordered_pair(
                &association.from_memory_id,
                &association.to_memory_id,
                &from,
                &to,
            )
        }) {
            deltas.push(CoRetrievalDelta::Strengthen {
                from,
                to,
                new_weight: (existing_association.weight + CO_RETRIEVAL_STRENGTHEN_DELTA).min(1.0),
                at: now,
            });
        } else if creates_used < MAX_NEW_ASSOCIATIONS_PER_TURN {
            deltas.push(CoRetrievalDelta::Create {
                from,
                to,
                weight: CO_RETRIEVAL_INITIAL_WEIGHT,
                reason: format!("co-retrieved in turn {turn_index} of session {session_id}"),
                at: now,
            });
            creates_used += 1;
        }
    }

    deltas
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn is_same_unordered_pair(
    left_from: &str,
    left_to: &str,
    right_from: &str,
    right_to: &str,
) -> bool {
    (left_from == right_from && left_to == right_to)
        || (left_from == right_to && left_to == right_from)
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-19T00:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn no_deltas_when_fewer_than_two_memories() {
        let deltas = generate_deltas(&[("a".to_string(), 1.0)], &[], 0, "s", now());

        assert!(deltas.is_empty());
    }

    #[test]
    fn creates_one_delta_per_pair_within_cap() {
        let retrieved = vec![
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.8),
            ("c".to_string(), 0.7),
        ];

        let deltas = generate_deltas(&retrieved, &[], 0, "s", now());

        assert_eq!(deltas.len(), 3);
        assert!(
            deltas
                .iter()
                .all(|delta| matches!(delta, CoRetrievalDelta::Create { .. }))
        );
    }

    #[test]
    fn strengthens_existing_associations_instead_of_creating() {
        let retrieved = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let existing = vec![Association::new("a", "b", 0.5, "earlier", now())];

        let deltas = generate_deltas(&retrieved, &existing, 0, "s", now());

        assert_eq!(deltas.len(), 1);
        assert!(matches!(
            deltas[0],
            CoRetrievalDelta::Strengthen { ref from, ref to, new_weight, .. }
                if from == "a" && to == "b" && (new_weight - 0.55).abs() < 1e-9
        ));
    }

    #[test]
    fn caps_creates_at_five_per_turn() {
        let retrieved: Vec<(String, f64)> = (0..6)
            .map(|index| (format!("memory.{index:02}"), 1.0 - 0.01 * index as f64))
            .collect();

        let deltas = generate_deltas(&retrieved, &[], 0, "s", now());
        let creates = deltas
            .iter()
            .filter(|delta| matches!(delta, CoRetrievalDelta::Create { .. }))
            .count();

        assert_eq!(creates, MAX_NEW_ASSOCIATIONS_PER_TURN);
    }

    #[test]
    fn deterministic_ordering_under_tied_scores() {
        let retrieved = vec![
            ("z".to_string(), 1.0),
            ("a".to_string(), 1.0),
            ("m".to_string(), 1.0),
        ];

        let deltas_first = generate_deltas(&retrieved, &[], 0, "s", now());
        let deltas_second = generate_deltas(&retrieved, &[], 0, "s", now());

        assert_eq!(deltas_first, deltas_second);
        match &deltas_first[0] {
            CoRetrievalDelta::Create { from, to, .. } => {
                assert_eq!(from.as_str(), "a");
                assert_eq!(to.as_str(), "m");
            }
            CoRetrievalDelta::Strengthen { .. } => panic!("expected Create"),
        }
    }

    #[test]
    fn existing_association_lookup_is_direction_independent() {
        let retrieved = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let existing = vec![Association::new("b", "a", 0.5, "earlier", now())];

        let deltas = generate_deltas(&retrieved, &existing, 0, "s", now());

        assert!(matches!(deltas[0], CoRetrievalDelta::Strengthen { .. }));
    }
}
