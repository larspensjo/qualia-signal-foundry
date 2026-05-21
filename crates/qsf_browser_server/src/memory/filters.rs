use std::cmp::Reverse;
use std::collections::HashSet;

use qsf_memory::{MemoryRecord, MemoryStoreContents};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::mapping::{Index, orphan_ids};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    #[serde(default, rename = "tag")]
    pub tags: Vec<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
    pub last_reinforced_from: Option<String>,
    pub last_reinforced_to: Option<String>,
    pub delta_since: Option<String>,
    pub min_importance: Option<f64>,
    pub min_reinforcement_count: Option<u32>,
    pub has_associations: Option<bool>,
    pub orphaned: Option<bool>,
    pub missing_last_reinforced: Option<bool>,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 500;

pub fn filter_records<'a>(
    store: &'a MemoryStoreContents,
    index: &Index<'a>,
    query: &ListQuery,
) -> Vec<&'a MemoryRecord> {
    let q = query.q.as_deref().map(str::to_lowercase);
    let kind = query.kind.as_deref();
    let tags: HashSet<&str> = query.tags.iter().map(String::as_str).collect();
    let created_from = parse_ts(query.created_from.as_deref());
    let created_to = parse_ts(query.created_to.as_deref());
    let last_from = parse_ts(query.last_reinforced_from.as_deref());
    let last_to = parse_ts(query.last_reinforced_to.as_deref());
    let delta_since = parse_ts(query.delta_since.as_deref());
    let orphans = if query.orphaned.is_some() {
        Some(orphan_ids(store))
    } else {
        None
    };

    store
        .records
        .iter()
        .filter(|r| match &q {
            Some(needle) => keyword_hit(r, needle),
            None => true,
        })
        .filter(|r| match kind {
            Some(k) => super::mapping::kind_str(&r.kind) == k,
            None => true,
        })
        .filter(|r| tags.is_empty() || r.tags.iter().any(|t| tags.contains(t.as_str())))
        .filter(|r| created_from.is_none_or(|t| r.created_at >= t))
        .filter(|r| created_to.is_none_or(|t| r.created_at <= t))
        .filter(|r| match last_from {
            Some(t) => r.last_reinforced_at.is_some_and(|lr| lr >= t),
            None => true,
        })
        .filter(|r| match last_to {
            Some(t) => r.last_reinforced_at.is_some_and(|lr| lr <= t),
            None => true,
        })
        .filter(|r| match delta_since {
            Some(t) => r.created_at >= t || r.last_reinforced_at.is_some_and(|lr| lr >= t),
            None => true,
        })
        .filter(|r| query.min_importance.is_none_or(|m| r.importance >= m))
        .filter(|r| {
            query
                .min_reinforcement_count
                .is_none_or(|m| r.reinforcement_count >= m)
        })
        .filter(|r| match query.has_associations {
            Some(true) => {
                index.outgoing.contains_key(r.id.as_str())
                    || index.incoming.contains_key(r.id.as_str())
            }
            Some(false) => {
                !(index.outgoing.contains_key(r.id.as_str())
                    || index.incoming.contains_key(r.id.as_str()))
            }
            None => true,
        })
        .filter(|r| match (&orphans, query.orphaned) {
            (Some(set), Some(true)) => set.contains(&r.id),
            (Some(set), Some(false)) => !set.contains(&r.id),
            _ => true,
        })
        .filter(|r| match query.missing_last_reinforced {
            Some(true) => r.last_reinforced_at.is_none(),
            Some(false) => r.last_reinforced_at.is_some(),
            None => true,
        })
        .collect()
}

fn keyword_hit(record: &MemoryRecord, needle: &str) -> bool {
    let haystacks = [
        record.title.to_lowercase(),
        record.summary.to_lowercase(),
        record.source_reference.to_lowercase(),
    ];
    if haystacks.iter().any(|h| h.contains(needle)) {
        return true;
    }
    record
        .tags
        .iter()
        .any(|t| t.to_lowercase().contains(needle))
}

fn parse_ts(s: Option<&str>) -> Option<OffsetDateTime> {
    s.and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
}

pub fn sort_records<'a>(
    records: &mut Vec<&'a MemoryRecord>,
    sort: Option<&str>,
    index: &Index<'a>,
) {
    match sort.unwrap_or("newest") {
        "oldest" => records.sort_by_key(|a| a.created_at),
        "most_reinforced" => {
            records.sort_by_key(|b| Reverse(b.reinforcement_count));
        }
        "highest_importance" => {
            records.sort_by(|a, b| {
                b.importance
                    .partial_cmp(&a.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "strongest_connected" => {
            let strength = |id: &str| -> f64 {
                let out = index.outgoing.get(id).into_iter().flatten();
                let inb = index.incoming.get(id).into_iter().flatten();
                out.chain(inb).map(|a| a.weight).sum()
            };
            records.sort_by(|a, b| {
                strength(b.id.as_str())
                    .partial_cmp(&strength(a.id.as_str()))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "largest_tokens" => records.sort_by_key(|b| Reverse(b.estimated_tokens)),
        _ => records.sort_by_key(|b| Reverse(b.created_at)),
    }
}

pub fn paginate<'a>(
    records: &[&'a MemoryRecord],
    query: &ListQuery,
) -> (usize, usize, Vec<&'a MemoryRecord>) {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let slice = records.iter().skip(offset).take(limit).copied().collect();
    (offset, limit, slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_memory::{
        ASSOCIATION_SCHEMA_VERSION, Association, MEMORY_RECORD_SCHEMA_VERSION, MemoryRecordKind,
    };
    use time::macros::datetime;

    fn fixture() -> MemoryStoreContents {
        let r = |id: &str,
                 title: &str,
                 kind: MemoryRecordKind,
                 tags: Vec<&str>,
                 created_at: OffsetDateTime,
                 imp: f64,
                 reinf: u32,
                 last_reinforced_at: Option<OffsetDateTime>,
                 tokens: usize| {
            MemoryRecord {
                schema_version: MEMORY_RECORD_SCHEMA_VERSION,
                id: id.into(),
                kind,
                title: title.into(),
                summary: format!("{title} summary"),
                tags: tags.into_iter().map(str::to_string).collect(),
                created_at,
                importance: imp,
                reinforcement_count: reinf,
                last_reinforced_at,
                source_reference: format!("src-{id}"),
                estimated_tokens: tokens,
            }
        };
        MemoryStoreContents {
            records: vec![
                r(
                    "a",
                    "Alpha",
                    MemoryRecordKind::Concept,
                    vec!["x"],
                    datetime!(2026-05-19 0:00 UTC),
                    0.9,
                    3,
                    Some(datetime!(2026-05-20 0:00 UTC)),
                    30,
                ),
                r(
                    "b",
                    "Beta",
                    MemoryRecordKind::Decision,
                    vec!["y"],
                    datetime!(2026-05-20 0:00 UTC),
                    0.1,
                    0,
                    None,
                    5,
                ),
                r(
                    "c",
                    "Gamma",
                    MemoryRecordKind::Observation,
                    vec!["z"],
                    datetime!(2026-05-18 0:00 UTC),
                    0.6,
                    1,
                    Some(datetime!(2026-05-18 12:00 UTC)),
                    15,
                ),
            ],
            associations: vec![
                Association {
                    schema_version: ASSOCIATION_SCHEMA_VERSION,
                    from_memory_id: "a".into(),
                    to_memory_id: "b".into(),
                    weight: 0.5,
                    reason: "r".into(),
                    last_reinforced_at: datetime!(2026-05-20 0:00 UTC),
                },
                Association {
                    schema_version: ASSOCIATION_SCHEMA_VERSION,
                    from_memory_id: "b".into(),
                    to_memory_id: "a".into(),
                    weight: 0.2,
                    reason: "r".into(),
                    last_reinforced_at: datetime!(2026-05-20 0:00 UTC),
                },
            ],
        }
    }

    fn ids(records: &[&MemoryRecord]) -> Vec<String> {
        records.iter().map(|r| r.id.clone()).collect()
    }

    fn filter_ids(q: ListQuery) -> Vec<String> {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        ids(&filter_records(&store, &idx, &q))
    }

    #[test]
    fn keyword_search_matches_title_case_insensitive() {
        assert_eq!(
            filter_ids(ListQuery {
                q: Some("alpha".into()),
                ..Default::default()
            }),
            vec!["a"]
        );
    }

    #[test]
    fn keyword_search_matches_summary_source_reference_and_tags() {
        assert_eq!(
            filter_ids(ListQuery {
                q: Some("gamma summary".into()),
                ..Default::default()
            }),
            vec!["c"]
        );
        assert_eq!(
            filter_ids(ListQuery {
                q: Some("src-b".into()),
                ..Default::default()
            }),
            vec!["b"]
        );
        assert_eq!(
            filter_ids(ListQuery {
                q: Some("z".into()),
                ..Default::default()
            }),
            vec!["c"]
        );
    }

    #[test]
    fn kind_filter_keeps_matching_kind() {
        assert_eq!(
            filter_ids(ListQuery {
                kind: Some("decision".into()),
                ..Default::default()
            }),
            vec!["b"]
        );
    }

    #[test]
    fn tag_filter_keeps_only_tagged() {
        assert_eq!(
            filter_ids(ListQuery {
                tags: vec!["z".into()],
                ..Default::default()
            }),
            vec!["c"]
        );
    }

    #[test]
    fn multiple_tag_filters_match_any_requested_tag() {
        assert_eq!(
            filter_ids(ListQuery {
                tags: vec!["x".into(), "y".into()],
                ..Default::default()
            }),
            vec!["a", "b"]
        );
    }

    #[test]
    fn created_range_filters_inclusively() {
        assert_eq!(
            filter_ids(ListQuery {
                created_from: Some("2026-05-19T12:00:00Z".into()),
                created_to: Some("2026-05-20T00:00:00Z".into()),
                ..Default::default()
            }),
            vec!["b"]
        );
    }

    #[test]
    fn last_reinforced_range_requires_present_timestamp() {
        assert_eq!(
            filter_ids(ListQuery {
                last_reinforced_from: Some("2026-05-19T00:00:00Z".into()),
                ..Default::default()
            }),
            vec!["a"]
        );
    }

    #[test]
    fn delta_since_matches_created_or_last_reinforced() {
        assert_eq!(
            filter_ids(ListQuery {
                delta_since: Some("2026-05-19T12:00:00Z".into()),
                ..Default::default()
            }),
            vec!["a", "b"]
        );
    }

    #[test]
    fn min_importance_threshold() {
        assert_eq!(
            filter_ids(ListQuery {
                min_importance: Some(0.5),
                ..Default::default()
            }),
            vec!["a", "c"]
        );
    }

    #[test]
    fn min_reinforcement_count_threshold() {
        assert_eq!(
            filter_ids(ListQuery {
                min_reinforcement_count: Some(1),
                ..Default::default()
            }),
            vec!["a", "c"]
        );
    }

    #[test]
    fn has_associations_true_keeps_connected() {
        assert_eq!(
            filter_ids(ListQuery {
                has_associations: Some(true),
                ..Default::default()
            }),
            vec!["a", "b"]
        );
    }

    #[test]
    fn has_associations_false_keeps_unconnected() {
        assert_eq!(
            filter_ids(ListQuery {
                has_associations: Some(false),
                ..Default::default()
            }),
            vec!["c"]
        );
    }

    #[test]
    fn orphaned_true_returns_only_unreferenced() {
        assert_eq!(
            filter_ids(ListQuery {
                orphaned: Some(true),
                ..Default::default()
            }),
            vec!["c"]
        );
    }

    #[test]
    fn missing_last_reinforced_filter() {
        assert_eq!(
            filter_ids(ListQuery {
                missing_last_reinforced: Some(true),
                ..Default::default()
            }),
            vec!["b"]
        );
    }

    #[test]
    fn sort_keys_order_expected_records() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let query = ListQuery::default();

        let mut all = filter_records(&store, &idx, &query);
        sort_records(&mut all, Some("newest"), &idx);
        assert_eq!(ids(&all), vec!["b", "a", "c"]);

        sort_records(&mut all, Some("oldest"), &idx);
        assert_eq!(ids(&all), vec!["c", "a", "b"]);

        sort_records(&mut all, Some("highest_importance"), &idx);
        assert_eq!(ids(&all), vec!["a", "c", "b"]);

        sort_records(&mut all, Some("most_reinforced"), &idx);
        assert_eq!(ids(&all), vec!["a", "c", "b"]);

        sort_records(&mut all, Some("largest_tokens"), &idx);
        assert_eq!(ids(&all), vec!["a", "c", "b"]);

        sort_records(&mut all, Some("strongest_connected"), &idx);
        assert_eq!(ids(&all)[..2], ["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn paginate_clamps_limit() {
        let store = fixture();
        let idx = super::super::mapping::build_index(&store);
        let all = filter_records(&store, &idx, &ListQuery::default());
        let (offset, limit, page) = paginate(
            &all,
            &ListQuery {
                limit: Some(0),
                offset: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(offset, 1);
        assert_eq!(limit, 1);
        assert_eq!(ids(&page), vec!["b"]);

        let (offset, limit, page) = paginate(
            &all,
            &ListQuery {
                limit: Some(10_000),
                ..Default::default()
            },
        );
        assert_eq!(offset, 0);
        assert_eq!(limit, MAX_LIMIT);
        assert_eq!(page.len(), 3);
    }
}
