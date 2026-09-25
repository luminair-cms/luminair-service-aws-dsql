//! Axum router configuration binding all endpoints to application state.

use axum::Router;
use axum::routing::{get, post};

use super::access_requests;
use super::documents;
use super::health;
use super::schema;
use super::state::AppState;

/// Constructs the complete Axum HTTP router with all API routes.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health probes
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness))
        // Schema introspection
        .route("/api/schema/document-types", get(schema::list_types))
        .route("/api/schema/document-types/{id}", get(schema::get_type))
        // Access requests & onboarding
        .route("/api/access-requests", post(access_requests::submit))
        .route(
            "/api/admin/access-requests",
            get(access_requests::list_pending),
        )
        .route(
            "/api/admin/access-requests/{id}/approve",
            post(access_requests::approve),
        )
        .route(
            "/api/admin/access-requests/{id}/reject",
            post(access_requests::reject),
        )
        // Root slug handlers (collection list/create or singleton get/upsert/delete)
        .route(
            "/api/{slug}",
            get(documents::handle_root_get)
                .post(documents::handle_root_post)
                .put(documents::handle_root_put)
                .delete(documents::handle_root_delete),
        )
        // Singleton workflow endpoints
        .route(
            "/api/{slug}/publish",
            post(documents::handle_singleton_publish),
        )
        .route(
            "/api/{slug}/unpublish",
            post(documents::handle_singleton_unpublish),
        )
        .route(
            "/api/{slug}/snapshots",
            get(documents::handle_singleton_snapshots),
        )
        // Collection item endpoints
        .route(
            "/api/{slug}/{id}",
            get(documents::handle_collection_get)
                .put(documents::handle_collection_put)
                .delete(documents::handle_collection_delete),
        )
        .route(
            "/api/{slug}/{id}/publish",
            post(documents::handle_collection_publish),
        )
        .route(
            "/api/{slug}/{id}/unpublish",
            post(documents::handle_collection_unpublish),
        )
        .route(
            "/api/{slug}/{id}/snapshots",
            get(documents::handle_collection_snapshots),
        )
        .with_state(state)
}
