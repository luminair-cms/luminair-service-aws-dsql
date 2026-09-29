//! Health and readiness probe handlers and health check services.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sqlx::PgPool;

use super::state::HttpState;

/// Service checking infrastructure dependencies for health and readiness.
#[derive(Clone)]
pub struct HealthChecker {
    pool: PgPool,
}

impl HealthChecker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies live database connectivity.
    pub async fn check_readiness(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Liveness probe: returns 200 OK if service process is responding.
pub async fn liveness() -> Response {
    (StatusCode::OK, axum::Json(json!({ "status": "ok" }))).into_response()
}

/// Readiness probe: verifies live database connectivity through the HealthChecker service.
pub async fn readiness(State(state): State<HttpState>) -> Response {
    match state.health_checker.check_readiness().await {
        Ok(_) => (StatusCode::OK, axum::Json(json!({ "status": "ready" }))).into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "status": "unhealthy",
                "error": e,
            })),
        )
            .into_response(),
    }
}
