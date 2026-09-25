//! Global application state wiring for Axum handlers and extractors.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use axum::extract::FromRef;
use domain::entities::system_config::SystemConfig;
use domain::services::schema_registry::SchemaRegistry;
use sqlx::PgPool;

use crate::auth::AuthAppState;
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxSnapshotRepository, SqlxUserRoleAssignmentRepository,
};

/// Global application state shared across all HTTP routes.
#[derive(Clone)]
pub struct AppState {
    pub auth: AuthAppState,
    pub pool: PgPool,
    pub schema_registry: Arc<SchemaRegistry>,
    pub system_config: Arc<SystemConfig>,
    pub documents_service:
        Arc<DocumentsServiceImpl<SqlxDocumentInstanceRepository, SqlxSnapshotRepository>>,
    pub access_requests_service: Arc<
        AccessRequestsServiceImpl<
            SqlxAccessRequestRepository,
            SqlxUserRoleAssignmentRepository,
            SqlxRoleRepository,
        >,
    >,
    pub system_config_service: Arc<SystemConfigServiceImpl>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        auth: AuthAppState,
        schema_registry: Arc<SchemaRegistry>,
        system_config: Arc<SystemConfig>,
    ) -> Self {
        let instance_repo = Arc::new(SqlxDocumentInstanceRepository::new(
            pool.clone(),
            schema_registry.clone(),
        ));
        let snapshot_repo = Arc::new(SqlxSnapshotRepository::new(
            pool.clone(),
            schema_registry.clone(),
        ));

        let documents_service = Arc::new(DocumentsServiceImpl::new(
            instance_repo,
            snapshot_repo,
            schema_registry.clone(),
            system_config.clone(),
        ));

        let access_request_repo = Arc::new(SqlxAccessRequestRepository::new(pool.clone()));
        let assignment_repo = Arc::new(SqlxUserRoleAssignmentRepository::new(pool.clone()));
        let role_repo = Arc::new(SqlxRoleRepository::new(pool.clone()));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            access_request_repo,
            assignment_repo,
            role_repo,
        ));

        let system_config_service = Arc::new(SystemConfigServiceImpl::new(system_config.clone()));

        Self {
            auth,
            pool,
            schema_registry,
            system_config,
            documents_service,
            access_requests_service,
            system_config_service,
        }
    }
}

impl FromRef<AppState> for AuthAppState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}
