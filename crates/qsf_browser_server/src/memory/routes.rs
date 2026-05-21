use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use std::cmp::Reverse;
// `axum_extra::extract::Query` supports repeated query params like
// `?tag=x&tag=y`, which is the browser API contract for tag filters.
use axum_extra::extract::Query;
use qsf_memory::{LoadedStore, dangling_association_ids};

use super::dto::{LoadError, MemoryDetail, MemoryListItem, MemoryPage, Neighborhood, StoreSummary};
use super::filters::{ListQuery, filter_records, paginate, sort_records};
use super::mapping::{
    association_edge, build_index, kind_str, orphan_ids, to_detail, to_list_item,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/store/summary", get(store_summary))
        .route("/api/memories", get(list_memories))
        .route("/api/memories/{id}", get(get_memory))
        .route("/api/memories/{id}/raw", get(get_memory_raw))
        .route(
            "/api/memories/{id}/neighborhood",
            get(get_memory_neighborhood),
        )
}

fn loaded_or_503(state: &AppState) -> Result<&LoadedStore, (StatusCode, Json<serde_json::Value>)> {
    state.loaded().map_err(|err| {
        let body = serde_json::json!({
            "message": "store failed to load",
            "load_error": LoadError::from(err),
        });
        (StatusCode::SERVICE_UNAVAILABLE, Json(body))
    })
}

async fn store_summary(
    State(state): State<AppState>,
) -> Result<Json<StoreSummary>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    // TODO: If list/summary requests become hot, cache an owned index in AppState
    // beside the immutable LoadedStore instead of rebuilding these maps per request.
    let index = build_index(store);
    let dangling = dangling_association_ids(store);
    let orphans = orphan_ids(store);
    let missing_lr = store
        .records
        .iter()
        .filter(|r| r.last_reinforced_at.is_none())
        .count();

    let mut records_by_kind: std::collections::BTreeMap<String, usize> = Default::default();
    for r in &store.records {
        *records_by_kind
            .entry(kind_str(&r.kind).to_string())
            .or_insert(0) += 1;
    }

    let mut tag_counts: std::collections::HashMap<String, usize> = Default::default();
    for r in &store.records {
        for t in &r.tags {
            *tag_counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut records_by_tag: Vec<(String, usize)> = tag_counts.into_iter().collect();
    records_by_tag.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    records_by_tag.truncate(20);

    let mut newest: Vec<_> = store.records.iter().collect();
    newest.sort_by_key(|b| Reverse(b.created_at));
    newest.truncate(5);

    let mut most_reinforced: Vec<_> = store.records.iter().collect();
    most_reinforced.sort_by_key(|b| Reverse(b.reinforcement_count));
    most_reinforced.truncate(5);

    let mut highest_importance: Vec<_> = store.records.iter().collect();
    highest_importance.sort_by(|a, b| {
        b.importance
            .partial_cmp(&a.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    highest_importance.truncate(5);

    let mut strongest: Vec<_> = store.associations.iter().collect();
    strongest.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    strongest.truncate(5);

    Ok(Json(StoreSummary {
        record_count: store.records.len(),
        association_count: store.associations.len(),
        broken_associations_count: dangling.len(),
        total_estimated_tokens: store.records.iter().map(|r| r.estimated_tokens).sum(),
        records_by_kind,
        records_by_tag,
        newest: newest.iter().map(|r| to_list_item(r, &index)).collect(),
        most_reinforced: most_reinforced
            .iter()
            .map(|r| to_list_item(r, &index))
            .collect(),
        highest_importance: highest_importance
            .iter()
            .map(|r| to_list_item(r, &index))
            .collect(),
        strongest_associations: strongest.iter().map(|a| association_edge(a)).collect(),
        orphaned_count: orphans.len(),
        missing_last_reinforced_count: missing_lr,
    }))
}

async fn list_memories(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<MemoryPage>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);

    let mut filtered = filter_records(store, &index, &query);
    sort_records(&mut filtered, query.sort.as_deref(), &index);
    let total = filtered.len();
    let (offset, limit, page) = paginate(&filtered, &query);
    Ok(Json(MemoryPage {
        total,
        offset,
        limit,
        items: page.iter().map(|r| to_list_item(r, &index)).collect(),
    }))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MemoryDetail>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);
    match index.by_id.get(id.as_str()).copied() {
        Some(r) => Ok(Json(to_detail(r, &index))),
        None => Err(not_found(id)),
    }
}

async fn get_memory_raw(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    match loaded.raw_records.get(&id) {
        Some(value) => Ok(Json(value.clone())),
        None => Err(not_found(id)),
    }
}

async fn get_memory_neighborhood(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<NeighborhoodQuery>,
) -> Result<Json<Neighborhood>, (StatusCode, Json<serde_json::Value>)> {
    let loaded = loaded_or_503(&state)?;
    let store = &loaded.contents;
    let index = build_index(store);
    let limit = query.limit.unwrap_or(8).clamp(1, 64);

    let center_record = match index.by_id.get(id.as_str()).copied() {
        Some(r) => r,
        None => return Err(not_found(id)),
    };
    let center = to_list_item(center_record, &index);

    let mut edges: Vec<_> = store
        .associations
        .iter()
        .filter(|a| a.from_memory_id == id || a.to_memory_id == id)
        .map(association_edge)
        .collect();
    edges.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    edges.truncate(limit);

    let mut member_ids: std::collections::HashSet<String> = Default::default();
    for e in &edges {
        if e.from_id != id {
            member_ids.insert(e.from_id.clone());
        }
        if e.to_id != id {
            member_ids.insert(e.to_id.clone());
        }
    }
    let mut members: Vec<MemoryListItem> = member_ids
        .iter()
        .filter_map(|m| index.by_id.get(m.as_str()).map(|r| to_list_item(r, &index)))
        .collect();
    // Stable order keeps JSON output deterministic across requests.
    members.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(Neighborhood {
        center,
        edges,
        members,
    }))
}

fn not_found(id: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "message": "memory not found", "id": id })),
    )
}

#[derive(serde::Deserialize)]
struct NeighborhoodQuery {
    limit: Option<usize>,
}
