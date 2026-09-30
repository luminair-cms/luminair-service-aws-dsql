//! HTTP routing state for Axum handlers and extractors.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use axum::extract::FromRef;
use domain::system::SystemContext;
use sqlx::PgPool;

use super::health::HealthChecker;
use crate::auth::AuthAppState;
use crate::composition::{AppContainer, ContainerBuildError};
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
    pub context: &'static SystemContext,
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
    /// Creates a new HttpState with an existing AuthAppState using the composition container builder.
    pub fn new(
        pool: PgPool,
        auth: AuthAppState,
        context: &'static SystemContext,
    ) -> Result<Self, ContainerBuildError> {
        let container = AppContainer::builder(pool, context)
            .with_auth_state(auth)
            .build()?;
        Ok(container.to_http_state())
    }
}

impl From<&AppContainer> for HttpState {
    fn from(container: &AppContainer) -> Self {
        container.to_http_state()
    }
}

impl From<AppContainer> for HttpState {
    fn from(container: AppContainer) -> Self {
        container.to_http_state()
    }
}

impl FromRef<HttpState> for AuthAppState {
    fn from_ref(state: &HttpState) -> Self {
        state.auth.clone()
    }
}
