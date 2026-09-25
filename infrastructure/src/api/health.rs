//! Health and readiness probe handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::state::AppState;

/// Liveness probe: returns 200 OK if service process is responding.
pub async fn liveness() -> Response {
    (StatusCode::OK, axum::Json(json!({ "status": "ok" }))).into_response()
}

/// Readiness probe: verifies live database connectivity.
pub async fn readiness(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, axum::Json(json!({ "status": "ready" }))).into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "status": "unhealthy",
                "error": e.to_string(),
            })),
        )
            .into_response(),
    }
}
