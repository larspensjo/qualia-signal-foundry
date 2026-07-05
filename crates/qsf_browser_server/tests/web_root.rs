use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;

use qsf_browser_server::{cli::Args, health, memory::routes, state::AppState, web};

fn app() -> axum::Router {
    let args = Args {
        store: "tests/fixtures/small-store.json".into(),
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
    };
    let state = AppState::load(&args);
    axum::Router::new()
        .merge(web::router())
        .merge(health::router())
        .merge(routes::router())
        .with_state(state)
}

#[tokio::test]
async fn root_points_to_vite_ui_and_health_endpoint() {
    let response = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains(r#".\scripts\qsf.ps1 browser"#));
    assert!(html.contains("/api/health"));
}
