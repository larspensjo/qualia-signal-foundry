use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

use qsf_browser_server::{cli::Args, health, memory::routes, session_context, state::AppState};

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/small-store.json")
}

fn app() -> axum::Router {
    app_for_store(fixture_path())
}

fn app_for_store(store: std::path::PathBuf) -> axum::Router {
    let args = Args {
        store,
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let state = AppState::load(&args);
    axum::Router::new()
        .merge(health::router())
        .merge(routes::router())
        .merge(session_context::router())
        .with_state(state)
}

async fn get_json(uri: &str) -> (axum::http::StatusCode, serde_json::Value) {
    get_json_from(app(), uri).await
}

async fn get_json_from(
    app: axum::Router,
    uri: &str,
) -> (axum::http::StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn summary_reports_broken_associations() {
    let (status, json) = get_json("/api/store/summary").await;
    assert_eq!(status, 200);
    assert_eq!(json["record_count"], 2);
    assert_eq!(json["association_count"], 2);
    assert_eq!(json["broken_associations_count"], 1);
}

#[tokio::test]
async fn list_memories_sorts_and_filters() {
    let (status, json) = get_json("/api/memories?tag=y&sort=oldest&limit=1").await;
    assert_eq!(status, 200);
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["id"], "b");
}

#[tokio::test]
async fn list_memories_can_sort_by_recent_activity() {
    let (status, json) = get_json("/api/memories?sort=recent_activity").await;
    assert_eq!(status, 200);
    assert_eq!(json["items"][0]["id"], "a");
    assert_eq!(
        json["items"][0]["last_reinforced_at"],
        "2026-05-20T00:00:00Z"
    );
}

#[tokio::test]
async fn detail_surfaces_broken_edge_as_null_other_title() {
    let (status, json) = get_json("/api/memories/a").await;
    assert_eq!(status, 200);
    let outgoing = json["outgoing"].as_array().unwrap();
    let ghost = outgoing.iter().find(|e| e["other_id"] == "ghost").unwrap();
    assert!(ghost["other_title"].is_null());
}

#[tokio::test]
async fn raw_endpoint_preserves_extra_fields() {
    let (status, json) = get_json("/api/memories/a/raw").await;
    assert_eq!(status, 200);
    assert_eq!(json["future_field"], "kept");
}

#[tokio::test]
async fn neighborhood_includes_broken_edge_member_missing() {
    let (status, json) = get_json("/api/memories/a/neighborhood").await;
    assert_eq!(status, 200);
    let edges = json["edges"].as_array().unwrap();
    let to_ids: Vec<String> = edges
        .iter()
        .map(|e| e["to_id"].as_str().unwrap().to_string())
        .collect();
    assert!(to_ids.contains(&"ghost".to_string()));

    let members = json["members"].as_array().unwrap();
    let member_ids: Vec<String> = members
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert!(member_ids.contains(&"b".to_string()));
    assert!(!member_ids.contains(&"ghost".to_string()));
}

#[tokio::test]
async fn neighborhood_limit_keeps_strongest_edges_first() {
    let (status, json) = get_json("/api/memories/a/neighborhood?limit=1").await;
    assert_eq!(status, 200);
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["to_id"], "b");
    assert_eq!(edges[0]["weight"], 0.9);
}

#[tokio::test]
async fn missing_memory_returns_404() {
    let (status, json) = get_json("/api/memories/missing").await;
    assert_eq!(status, 404);
    assert_eq!(json["id"], "missing");
}

#[tokio::test]
async fn session_search_finds_adjacent_session_state() {
    let dir = tempfile::TempDir::new().unwrap();
    let store_path = dir.path().join("memory-store.json");
    std::fs::write(
        &store_path,
        r#"{
            "records": [],
            "associations": []
        }"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session-state.json"),
        r#"{
            "session_id": "s-test",
            "turns": [],
            "summarized_turns": [
                {
                    "turn_index": 0,
                    "summary": "The user asked the assistant to remember that its name is Ari."
                }
            ]
        }"#,
    )
    .unwrap();

    let (status, json) =
        get_json_from(app_for_store(store_path), "/api/session/search?q=Ari").await;

    assert_eq!(status, 200);
    assert_eq!(json["available"], true);
    assert_eq!(json["session_id"], "s-test");
    assert_eq!(json["total"], 1);
    assert_eq!(json["items"][0]["kind"], "turn_summary");
    assert_eq!(json["items"][0]["turn_index"], 0);
    assert!(
        json["items"][0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Ari")
    );
}
