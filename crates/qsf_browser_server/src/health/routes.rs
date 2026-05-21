use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::memory::dto::LoadError;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum HealthResponse {
    Ok,
    Error { load_error: LoadError },
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    match state.loaded() {
        Ok(_) => Json(HealthResponse::Ok),
        Err(err) => Json(HealthResponse::Error {
            load_error: LoadError::from(err),
        }),
    }
}
