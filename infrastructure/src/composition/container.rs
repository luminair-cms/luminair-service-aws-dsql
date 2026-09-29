//! Composition root container wiring persistence adapters and application services.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use domain::schema::SchemaRegistry;
use domain::system::SystemConfig;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

use super::config::{ConfigError, ServerConfig};
use crate::api::health::HealthChecker;
use crate::api::state::HttpState;
use crate::auth::{AuthAppState, AuthError, TokenValidator, run_bootstrap};
use crate::migrations::run_migrations;
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};
use crate::schema_loader::{SafetyPolicy, SchemaSyncError, sync_schemas};

/// Errors encountered during application infrastructure bootstrapping.
#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("Database connection error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Dynamic schema sync error: {0}")]
    SchemaSync(#[from] SchemaSyncError),

    #[error("Administrator bootstrap error: {0}")]
    AuthBootstrap(#[from] AuthError),
}

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
    /// Bootstraps the complete application infrastructure from configuration:
    /// 1. Connects to the database pool
    /// 2. Runs static database migrations (MIGRATOR)
    /// 3. Synchronizes declarative JSON document schemas (sync_schemas)
    /// 4. Initializes the token validator (JWKS or secret)
    /// 5. Assembles AppContainer
    /// 6. Executes the administrator bootstrap hook (run_bootstrap)
    pub async fn bootstrap(config: &ServerConfig) -> Result<Self, BootstrapError> {
        tracing::info!("Connecting to database pool...");
        let pool = PgPoolOptions::new()
            .max_connections(config.max_db_connections)
            .connect(&config.database_url)
            .await?;

        tracing::info!("Running static system migrations...");
        run_migrations(&pool).await?;

        tracing::info!(
            "Synchronizing document schemas from '{}'...",
            config.schema_dir.display()
        );
        let sync_result =
            sync_schemas(&pool, &config.schema_dir, SafetyPolicy::AdditiveOnly).await?;

        let validator = config.init_token_validator()?;

        let container = Self::new(
            pool.clone(),
            validator,
            Arc::new(sync_result.registry),
            Arc::new(sync_result.system_config),
        );

        tracing::info!("Checking administrator bootstrap hook...");
        run_bootstrap(
            &pool,
            &config.auth,
            container.assignment_repo.as_ref(),
            container.access_request_repo.as_ref(),
        )
        .await?;

        Ok(container)
    }

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
