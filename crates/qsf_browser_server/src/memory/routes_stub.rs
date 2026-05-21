use axum::{Json, Router, extract::State, http::StatusCode, routing::get};

use crate::memory::dto::LoadError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/store/summary", get(unavailable))
        .route("/api/memories", get(unavailable))
        .route("/api/memories/{id}", get(unavailable))
        .route("/api/memories/{id}/neighborhood", get(unavailable))
        .route("/api/memories/{id}/raw", get(unavailable))
}

async fn unavailable(State(state): State<AppState>) -> (StatusCode, Json<UnavailableBody>) {
    let body = match state.loaded() {
        Ok(_) => UnavailableBody {
            message: "endpoint not yet implemented".to_string(),
            load_error: None,
        },
        Err(err) => UnavailableBody {
            message: "store failed to load".to_string(),
            load_error: Some(LoadError::from(err)),
        },
    };
    (StatusCode::SERVICE_UNAVAILABLE, Json(body))
}

#[derive(serde::Serialize)]
struct UnavailableBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_error: Option<LoadError>,
}
