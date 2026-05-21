use axum::Router;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

use qsf_browser_server::{cli::Args, health, memory::routes_stub, state::AppState};

fn app(args: Args) -> Router {
    let state = AppState::load(&args);
    Router::new()
        .merge(health::router())
        .merge(routes_stub::router())
        .with_state(state)
}

#[tokio::test]
async fn missing_store_path_yields_missing_file_on_health() {
    let args = Args {
        store: "/nonexistent/memory-store.json".into(),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let response = app(args)
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["load_error"]["kind"], "missing_file");
}

#[tokio::test]
async fn missing_store_path_yields_503_on_data_endpoints() {
    let args = Args {
        store: "/nonexistent/memory-store.json".into(),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let response = app(args)
        .oneshot(
            Request::builder()
                .uri("/api/memories")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["load_error"]["kind"], "missing_file");
}

#[tokio::test]
async fn missing_store_path_yields_503_on_id_data_endpoints() {
    let args = Args {
        store: std::env::temp_dir().join("qsf-definitely-missing-memory-store.json"),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };

    for uri in [
        "/api/memories/example-id",
        "/api/memories/example-id/neighborhood",
        "/api/memories/example-id/raw",
    ] {
        let response = app(args.clone())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 503, "unexpected status for {uri}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["load_error"]["kind"], "missing_file");
    }
}
