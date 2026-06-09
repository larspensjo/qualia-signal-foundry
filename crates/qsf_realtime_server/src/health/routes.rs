use axum::{Json, Router, routing::get};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn health_route_returns_ok() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let state = crate::state::AppState::new(
            "test-api-key",
            "http://127.0.0.1:9999",
            tempdir.path().to_path_buf(),
        )
        .expect("state");
        let app = router().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
