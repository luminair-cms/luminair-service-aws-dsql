//! Composition root container wiring persistence adapters and application services.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use domain::schema::SchemaRegistry;
use domain::system::SystemConfig;
use sqlx::PgPool;

use crate::api::health::HealthChecker;
use crate::api::state::HttpState;
use crate::auth::{AuthAppState, TokenValidator};
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};

/// Central composition root container holding initialized services and adapters.
#[derive(Clone)]
pub struct AppContainer {
    pub pool: PgPool,
    pub schema_registry: Arc<SchemaRegistry>,
    pub system_config: Arc<SystemConfig>,
    pub instance_repo: Arc<SqlxDocumentInstanceRepository>,
    pub access_request_repo: Arc<SqlxAccessRequestRepository>,
    pub assignment_repo: Arc<SqlxUserRoleAssignmentRepository>,
    pub role_repo: Arc<SqlxRoleRepository>,
    pub shadow_user_repo: Arc<SqlxShadowUserRepository>,
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
    pub auth: AuthAppState,
}

impl AppContainer {
    /// Creates and wires the complete composition container from base dependencies.
    pub fn new(
        pool: PgPool,
        validator: Arc<dyn TokenValidator>,
        schema_registry: Arc<SchemaRegistry>,
        system_config: Arc<SystemConfig>,
    ) -> Self {
        let auth = AuthAppState::new(pool.clone(), validator);
        Self::with_auth_state(pool, auth, schema_registry, system_config)
    }

    /// Creates and wires the composition container with an existing AuthAppState.
    pub fn with_auth_state(
        pool: PgPool,
        auth: AuthAppState,
        schema_registry: Arc<SchemaRegistry>,
        system_config: Arc<SystemConfig>,
    ) -> Self {
        let instance_repo = Arc::new(SqlxDocumentInstanceRepository::new(
            pool.clone(),
            schema_registry.clone(),
        ));

        let documents_service = Arc::new(DocumentsServiceImpl::new(
            instance_repo.clone(),
            schema_registry.clone(),
            system_config.clone(),
        ));

        let access_request_repo = Arc::new(SqlxAccessRequestRepository::new(pool.clone()));
        let assignment_repo = Arc::new(SqlxUserRoleAssignmentRepository::new(pool.clone()));
        let role_repo = Arc::new(SqlxRoleRepository::new(pool.clone()));
        let shadow_user_repo = Arc::new(SqlxShadowUserRepository::new(pool.clone()));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            access_request_repo.clone(),
            assignment_repo.clone(),
            role_repo.clone(),
        ));

        let system_config_service = Arc::new(SystemConfigServiceImpl::new(system_config.clone()));
        let health_checker = Arc::new(HealthChecker::new(pool.clone()));

        Self {
            pool,
            schema_registry,
            system_config,
            instance_repo,
            access_request_repo,
            assignment_repo,
            role_repo,
            shadow_user_repo,
            documents_service,
            access_requests_service,
            system_config_service,
            health_checker,
            auth,
        }
    }

    /// Converts the container into Axum HTTP routing state for route handlers.
    pub fn to_http_state(&self) -> HttpState {
        HttpState {
            auth: self.auth.clone(),
            schema_registry: self.schema_registry.clone(),
            system_config: self.system_config.clone(),
            documents_service: self.documents_service.clone(),
            access_requests_service: self.access_requests_service.clone(),
            system_config_service: self.system_config_service.clone(),
            health_checker: self.health_checker.clone(),
        }
    }

    /// Backwards-compatible alias for `to_http_state`.
    pub fn to_app_state(&self) -> HttpState {
        self.to_http_state()
    }
}
