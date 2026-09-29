//! HTTP routing state for Axum handlers and extractors.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use axum::extract::FromRef;
use domain::schema::SchemaRegistry;
use domain::system::SystemConfig;
use sqlx::PgPool;

use super::health::HealthChecker;
use crate::auth::AuthAppState;
use crate::composition::AppContainer;
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxUserRoleAssignmentRepository,
};

/// HTTP routing state shared across all Axum routes.
///
/// Contains strictly application services and metadata required by HTTP handlers.
/// Database connection pools and raw repositories are isolated in `AppContainer`.
#[derive(Clone)]
pub struct HttpState {
    pub auth: AuthAppState,
    pub schema_registry: Arc<SchemaRegistry>,
    pub system_config: Arc<SystemConfig>,
    pub documents_service: Arc<DocumentsServiceImpl<SqlxDocumentInstanceRepository>>,
    pub access_requests_service: Arc<
        AccessRequestsServiceImpl<
            SqlxAccessRequestRepository,
            SqlxUserRoleAssignmentRepository,
            SqlxRoleRepository,
        >,
    >,
    pub system_config_service: Arc<SystemConfigServiceImpl>,
    pub health_checker: Arc<HealthChecker>,
}

/// Backwards-compatible alias for `HttpState`.
pub type AppState = HttpState;

impl HttpState {
    /// Creates a new HttpState using the composition root container.
    pub fn new(
        pool: PgPool,
        auth: AuthAppState,
        schema_registry: Arc<SchemaRegistry>,
        system_config: Arc<SystemConfig>,
    ) -> Self {
        let container = AppContainer::with_auth_state(
            pool,
            auth,
            schema_registry,
            system_config,
        );
        container.to_http_state()
    }
}

impl FromRef<HttpState> for AuthAppState {
    fn from_ref(state: &HttpState) -> Self {
        state.auth.clone()
    }
}
