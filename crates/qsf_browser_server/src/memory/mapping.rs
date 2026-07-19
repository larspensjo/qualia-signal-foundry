//! Pure conversions from persisted types into wire DTOs.

use std::collections::{HashMap, HashSet};

use qsf_memory::{Association, MemoryRecord, MemoryRecordKind, MemoryStoreContents};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::dto::{AssociationDisplay, AssociationDisplayEdge, MemoryDetail, MemoryListItem};

pub fn format_ts(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339).expect("RFC3339 always formats")
}

pub fn kind_str(kind: &MemoryRecordKind) -> &'static str {
    match kind {
        MemoryRecordKind::Concept => "concept",
        MemoryRecordKind::ArchitectureNote => "architecture_note",
        MemoryRecordKind::Experiment => "experiment",
        MemoryRecordKind::Decision => "decision",
        MemoryRecordKind::Question => "question",
        MemoryRecordKind::Observation => "observation",
    }
}

pub struct Index<'a> {
    pub by_id: HashMap<&'a str, &'a MemoryRecord>,
    pub outgoing: HashMap<&'a str, Vec<&'a Association>>,
    pub incoming: HashMap<&'a str, Vec<&'a Association>>,
}

pub fn build_index(store: &MemoryStoreContents) -> Index<'_> {
    let mut by_id = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&Association>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&Association>> = HashMap::new();
    for r in &store.records {
        by_id.insert(r.id.as_str(), r);
    }
    for a in &store.associations {
        outgoing
            .entry(a.from_memory_id.as_str())
            .or_default()
            .push(a);
        incoming.entry(a.to_memory_id.as_str()).or_default().push(a);
    }
    Index {
        by_id,
        outgoing,
        incoming,
    }
}

pub fn to_list_item(record: &MemoryRecord, index: &Index<'_>) -> MemoryListItem {
    let association_count = index.outgoing.get(record.id.as_str()).map_or(0, Vec::len)
        + index.incoming.get(record.id.as_str()).map_or(0, Vec::len);
    MemoryListItem {
        id: record.id.clone(),
        kind: kind_str(&record.kind).to_string(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        tags: record.tags.clone(),
        created_at: format_ts(record.created_at),
        last_reinforced_at: record.last_reinforced_at.map(format_ts),
        importance: record.importance,
        reinforcement_count: record.reinforcement_count,
        estimated_tokens: record.estimated_tokens,
        association_count,
    }
}

pub fn to_detail(record: &MemoryRecord, index: &Index<'_>) -> MemoryDetail {
    let outgoing_vec = index
        .outgoing
        .get(record.id.as_str())
        .cloned()
        .unwrap_or_default();
    let incoming_vec = index
        .incoming
        .get(record.id.as_str())
        .cloned()
        .unwrap_or_default();
    let outgoing = sort_and_map_assocs(record.id.as_str(), &outgoing_vec, true, index);
    let incoming = sort_and_map_assocs(record.id.as_str(), &incoming_vec, false, index);
    MemoryDetail {
        id: record.id.clone(),
        kind: kind_str(&record.kind).to_string(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        tags: record.tags.clone(),
        created_at: format_ts(record.created_at),
        last_reinforced_at: record.last_reinforced_at.map(format_ts),
        importance: record.importance,
        reinforcement_count: record.reinforcement_count,
        source_reference: record.source_reference.clone(),
        estimated_tokens: record.estimated_tokens,
        incoming_count: incoming.len(),
        outgoing_count: outgoing.len(),
        incoming,
        outgoing,
    }
}

fn sort_and_map_assocs(
    self_id: &str,
    assocs: &[&Association],
    outgoing: bool,
    index: &Index<'_>,
) -> Vec<AssociationDisplay> {
    let mut sorted = assocs.to_vec();
    sorted.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
        .into_iter()
        .map(|a| {
            let other_id = if outgoing {
                a.to_memory_id.as_str()
            } else {
                a.from_memory_id.as_str()
            };
            AssociationDisplay {
                other_id: other_id.to_string(),
                other_title: index.by_id.get(other_id).map(|r| r.title.clone()),
                weight: a.weight,
                last_reinforced_at: format_ts(a.last_reinforced_at),
                reason: a.reason.clone(),
            }
        })
        .filter(|d| d.other_id != self_id)
        .collect()
}

pub fn association_edge(a: &Association) -> AssociationDisplayEdge {
    AssociationDisplayEdge {
        from_id: a.from_memory_id.clone(),
        to_id: a.to_memory_id.clone(),
        weight: a.weight,
        last_reinforced_at: format_ts(a.last_reinforced_at),
        reason: a.reason.clone(),
    }
}

pub fn orphan_ids(store: &MemoryStoreContents) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for a in &store.associations {
        referenced.insert(a.from_memory_id.clone());
        referenced.insert(a.to_memory_id.clone());
    }
    store
        .records
        .iter()
        .map(|r| r.id.clone())
        .filter(|id| !referenced.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_memory::{ASSOCIATION_SCHEMA_VERSION, MEMORY_RECORD_SCHEMA_VERSION};
    use time::macros::datetime;

    fn fixture() -> MemoryStoreContents {
        let r = |id: &str, title: &str| MemoryRecord {
            schema_version: MEMORY_RECORD_SCHEMA_VERSION,
            id: id.to_string(),
            kind: MemoryRecordKind::Concept,
            title: title.to_string(),
            summary: "s".into(),
            tags: vec!["tag".into()],
            created_at: datetime!(2026-05-20 0:00 UTC),
            importance: 0.5,
            reinforcement_count: 0,
            last_reinforced_at: Some(datetime!(2026-05-20 0:00 UTC)),
            source_reference: "src".into(),
            estimated_tokens: 10,
            provenance: Default::default(),
            trust_tier: Default::default(),
            time_sensitive_decay_half_life_days: None,
            superseded_by: None,
        };
        let a = |from: &str, to: &str, weight: f64| Association {
            schema_version: ASSOCIATION_SCHEMA_VERSION,
            from_memory_id: from.into(),
            to_memory_id: to.into(),
            weight,
            reason: "r".into(),
            last_reinforced_at: datetime!(2026-05-20 0:00 UTC),
        };
        MemoryStoreContents {
            records: vec![r("a", "A"), r("b", "B")],
            associations: vec![a("a", "b", 0.9), a("a", "ghost", 0.5), a("b", "a", 0.3)],
            ..MemoryStoreContents::default()
        }
    }

    #[test]
    fn detail_lists_incoming_and_outgoing_sorted_by_weight_desc() {
        let store = fixture();
        let idx = build_index(&store);
        let detail = to_detail(&store.records[0], &idx);
        assert_eq!(detail.outgoing.len(), 2);
        assert!(detail.outgoing[0].weight >= detail.outgoing[1].weight);
        assert_eq!(detail.incoming.len(), 1);
    }

    #[test]
    fn broken_edge_other_title_is_null() {
        let store = fixture();
        let idx = build_index(&store);
        let detail = to_detail(&store.records[0], &idx);
        let ghost = detail
            .outgoing
            .iter()
            .find(|d| d.other_id == "ghost")
            .unwrap();
        assert!(ghost.other_title.is_none());
    }

    #[test]
    fn orphan_ids_excludes_associated_records() {
        let store = fixture();
        let orphans = orphan_ids(&store);
        assert!(!orphans.contains("a"));
        assert!(!orphans.contains("b"));
    }
}
