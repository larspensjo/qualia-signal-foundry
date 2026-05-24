//! Idempotency ledger helpers for cross-turn co-retrieval coverage.
//!
//! A `ProcessedRange` records that the cross-turn pass has been run with each
//! turn in `[first_turn_index, last_turn_index]` serving as a window anchor.
//! Overlap-target turns reached from that range are not marked processed by the
//! entry; they remain candidates for later ranges where they serve as anchors.
//! Durable association changes and the matching range are persisted together in
//! `MemoryStoreContents` so a rerun can skip already-covered anchors.

pub use qsf_memory::{ProcessedRange, ProcessedRangeKind};

pub fn covers(range: &ProcessedRange, session_id: &str, turn_index: usize) -> bool {
    range.session_id == session_id
        && range.first_turn_index <= turn_index
        && turn_index <= range.last_turn_index
}

pub fn uncovered_turn_indices(
    ranges: &[ProcessedRange],
    session_id: &str,
    start: usize,
    end_inclusive: usize,
) -> Vec<usize> {
    (start..=end_inclusive)
        .filter(|turn_index| {
            !ranges
                .iter()
                .any(|range| covers(range, session_id, *turn_index))
        })
        .collect()
}

pub fn contiguous_ranges(indices: &[usize]) -> Vec<(usize, usize)> {
    let Some(first) = indices.first().copied() else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut start = first;
    let mut previous = first;
    for index in indices.iter().copied().skip(1) {
        if index == previous + 1 {
            previous = index;
        } else {
            ranges.push((start, previous));
            start = index;
            previous = index;
        }
    }
    ranges.push((start, previous));
    ranges
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn covers_turn_inside_range() {
        let range = ProcessedRange {
            session_id: "s".into(),
            first_turn_index: 2,
            last_turn_index: 5,
            kind: ProcessedRangeKind::LiveBatch,
            at: ts(),
        };

        assert!(covers(&range, "s", 2));
        assert!(covers(&range, "s", 5));
        assert!(!covers(&range, "s", 1));
        assert!(!covers(&range, "s", 6));
        assert!(!covers(&range, "other", 3));
    }

    #[test]
    fn uncovered_indices_excludes_covered_turns() {
        let ranges = vec![
            ProcessedRange {
                session_id: "s".into(),
                first_turn_index: 0,
                last_turn_index: 2,
                kind: ProcessedRangeKind::LiveBatch,
                at: ts(),
            },
            ProcessedRange {
                session_id: "s".into(),
                first_turn_index: 5,
                last_turn_index: 5,
                kind: ProcessedRangeKind::SessionEnd,
                at: ts(),
            },
        ];

        assert_eq!(uncovered_turn_indices(&ranges, "s", 0, 6), vec![3, 4, 6]);
    }

    #[test]
    fn contiguous_ranges_groups_adjacent_indices() {
        assert_eq!(
            contiguous_ranges(&[0, 1, 3, 5, 6]),
            vec![(0, 1), (3, 3), (5, 6)]
        );
    }

    #[test]
    fn serde_roundtrip() {
        let range = ProcessedRange {
            session_id: "session.1".into(),
            first_turn_index: 0,
            last_turn_index: 9,
            kind: ProcessedRangeKind::SleepSafetyNet,
            at: ts(),
        };

        let json = serde_json::to_string(&range).unwrap();
        let parsed: ProcessedRange = serde_json::from_str(&json).unwrap();

        assert_eq!(range, parsed);
        assert!(json.contains("sleep_safety_net"));
    }
}
