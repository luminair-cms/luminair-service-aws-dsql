//! Composition root container wiring persistence adapters and application services.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use domain::system::SystemContext;
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

/// Errors encountered when building an `AppContainer`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContainerBuildError {
    #[error("Missing authentication configuration: token validator or auth state must be provided")]
    MissingAuthConfiguration,
}

/// Central composition root container holding initialized services and adapters.
#[derive(Clone)]
pub struct AppContainer {
    pub pool: PgPool,
    pub context: &'static SystemContext,
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

        let system_context: &'static SystemContext = Box::leak(Box::new(SystemContext {
            schema: sync_result.registry,
            config: sync_result.system_config,
        }));

        let container = Self::new(pool.clone(), validator, system_context);

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
        context: &'static SystemContext,
    ) -> Self {
        Self::builder(pool, context).assemble_with_validator(validator)
    }

    /// Creates a builder for custom or step-by-step container assembly.
    pub fn builder(pool: PgPool, context: &'static SystemContext) -> AppContainerBuilder {
        AppContainerBuilder::new(pool, context)
    }

    /// Converts the container into Axum HTTP routing state for route handlers.
    pub fn to_http_state(&self) -> HttpState {
        HttpState {
            auth: self.auth.clone(),
            context: self.context,
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

/// Builder for assembling an `AppContainer` with custom components or overrides.
pub struct AppContainerBuilder {
    pool: PgPool,
    context: &'static SystemContext,
    validator: Option<Arc<dyn TokenValidator>>,
    auth: Option<AuthAppState>,
    instance_repo: Option<Arc<SqlxDocumentInstanceRepository>>,
    access_request_repo: Option<Arc<SqlxAccessRequestRepository>>,
    assignment_repo: Option<Arc<SqlxUserRoleAssignmentRepository>>,
    role_repo: Option<Arc<SqlxRoleRepository>>,
    shadow_user_repo: Option<Arc<SqlxShadowUserRepository>>,
}

impl AppContainerBuilder {
    /// Creates a new container builder with required foundational dependencies.
    pub fn new(pool: PgPool, context: &'static SystemContext) -> Self {
        Self {
            pool,
            context,
            validator: None,
            auth: None,
            instance_repo: None,
            access_request_repo: None,
            assignment_repo: None,
            role_repo: None,
            shadow_user_repo: None,
        }
    }

    /// Sets the token validator for authenticating requests.
    pub fn with_validator(mut self, validator: Arc<dyn TokenValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Sets a pre-configured `AuthAppState`, reusing its repository instances.
    pub fn with_auth_state(mut self, auth: AuthAppState) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Overrides the document instance repository.
    pub fn with_instance_repo(mut self, repo: Arc<SqlxDocumentInstanceRepository>) -> Self {
        self.instance_repo = Some(repo);
        self
    }

    /// Overrides the access request repository.
    pub fn with_access_request_repo(mut self, repo: Arc<SqlxAccessRequestRepository>) -> Self {
        self.access_request_repo = Some(repo);
        self
    }

    /// Overrides the user role assignment repository.
    pub fn with_assignment_repo(mut self, repo: Arc<SqlxUserRoleAssignmentRepository>) -> Self {
        self.assignment_repo = Some(repo);
        self
    }

    /// Overrides the role repository.
    pub fn with_role_repo(mut self, repo: Arc<SqlxRoleRepository>) -> Self {
        self.role_repo = Some(repo);
        self
    }

    /// Overrides the shadow user repository.
    pub fn with_shadow_user_repo(mut self, repo: Arc<SqlxShadowUserRepository>) -> Self {
        self.shadow_user_repo = Some(repo);
        self
    }

    /// Builds the `AppContainer`, returning an error if authentication is unconfigured.
    pub fn build(mut self) -> Result<AppContainer, ContainerBuildError> {
        if let Some(auth) = self.auth.take() {
            Ok(self.assemble_with_auth(auth))
        } else if let Some(validator) = self.validator.take() {
            Ok(self.assemble_with_validator(validator))
        } else {
            Err(ContainerBuildError::MissingAuthConfiguration)
        }
    }

    fn assemble_with_auth(self, auth: AuthAppState) -> AppContainer {
        let pool = self.pool;
        let context = self.context;

        let instance_repo = self.instance_repo.unwrap_or_else(|| {
            Arc::new(SqlxDocumentInstanceRepository::new(
                pool.clone(),
                &context.schema,
            ))
        });
        let access_request_repo = self
            .access_request_repo
            .unwrap_or_else(|| Arc::new(auth.access_request_repo.clone()));
        let assignment_repo = self
            .assignment_repo
            .unwrap_or_else(|| Arc::new(auth.assignment_repo.clone()));
        let role_repo = self
            .role_repo
            .unwrap_or_else(|| Arc::new(auth.role_repo.clone()));
        let shadow_user_repo = self
            .shadow_user_repo
            .unwrap_or_else(|| Arc::new(auth.shadow_user_repo.clone()));

        Self::finish_assemble(
            pool,
            context,
            instance_repo,
            access_request_repo,
            assignment_repo,
            role_repo,
            shadow_user_repo,
            auth,
        )
    }

    fn assemble_with_validator(self, validator: Arc<dyn TokenValidator>) -> AppContainer {
        let pool = self.pool;
        let context = self.context;

        let instance_repo = self.instance_repo.unwrap_or_else(|| {
            Arc::new(SqlxDocumentInstanceRepository::new(
                pool.clone(),
                &context.schema,
            ))
        });
        let access_request_repo = self
            .access_request_repo
            .unwrap_or_else(|| Arc::new(SqlxAccessRequestRepository::new(pool.clone())));
        let assignment_repo = self
            .assignment_repo
            .unwrap_or_else(|| Arc::new(SqlxUserRoleAssignmentRepository::new(pool.clone())));
        let role_repo = self
            .role_repo
            .unwrap_or_else(|| Arc::new(SqlxRoleRepository::new(pool.clone())));
        let shadow_user_repo = self
            .shadow_user_repo
            .unwrap_or_else(|| Arc::new(SqlxShadowUserRepository::new(pool.clone())));

        let auth = AuthAppState::from_parts(
            pool.clone(),
            validator,
            assignment_repo.clone(),
            role_repo.clone(),
            access_request_repo.clone(),
            shadow_user_repo.clone(),
        );

        Self::finish_assemble(
            pool,
            context,
            instance_repo,
            access_request_repo,
            assignment_repo,
            role_repo,
            shadow_user_repo,
            auth,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_assemble(
        pool: PgPool,
        context: &'static SystemContext,
        instance_repo: Arc<SqlxDocumentInstanceRepository>,
        access_request_repo: Arc<SqlxAccessRequestRepository>,
        assignment_repo: Arc<SqlxUserRoleAssignmentRepository>,
        role_repo: Arc<SqlxRoleRepository>,
        shadow_user_repo: Arc<SqlxShadowUserRepository>,
        auth: AuthAppState,
    ) -> AppContainer {
        let documents_service = Arc::new(DocumentsServiceImpl::new(instance_repo.clone(), context));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            access_request_repo.clone(),
            assignment_repo.clone(),
            role_repo.clone(),
        ));

        let system_config_service = Arc::new(SystemConfigServiceImpl::new(context));
        let health_checker = Arc::new(HealthChecker::new(pool.clone()));

        AppContainer {
            pool,
            context,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MockTokenValidator;
    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};
    use uuid::Uuid;

    fn create_test_context() -> &'static SystemContext {
        let en = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en)
            .expect("valid system config");
        let schema = SchemaRegistry::default();
        Box::leak(Box::new(SystemContext { schema, config }))
    }

    fn create_lazy_test_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost:5432/test")
            .expect("connect_lazy succeeds")
    }

    #[tokio::test]
    async fn test_builder_missing_auth_configuration() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();

        let result = AppContainer::builder(pool, context).build();
        assert_eq!(
            result.err(),
            Some(ContainerBuildError::MissingAuthConfiguration)
        );
    }

    #[tokio::test]
    async fn test_builder_with_validator_succeeds() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();
        let validator = Arc::new(MockTokenValidator::new());

        let container = AppContainer::builder(pool, context)
            .with_validator(validator)
            .build()
            .expect("build with validator");

        assert!(std::ptr::eq(container.context, context));
    }

    #[tokio::test]
    async fn test_builder_with_auth_state_reuses_repositories() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();
        let validator = Arc::new(MockTokenValidator::new());
        let auth = AuthAppState::new(pool.clone(), validator);

        let container = AppContainer::builder(pool, context)
            .with_auth_state(auth.clone())
            .build()
            .expect("build with auth state");

        assert!(Arc::ptr_eq(&container.auth.resolver, &auth.resolver));
    }

    #[tokio::test]
    async fn test_builder_with_repository_override() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();
        let validator = Arc::new(MockTokenValidator::new());

        let custom_instance_repo = Arc::new(SqlxDocumentInstanceRepository::new(
            pool.clone(),
            &context.schema,
        ));

        let container = AppContainer::builder(pool, context)
            .with_validator(validator)
            .with_instance_repo(custom_instance_repo.clone())
            .build()
            .expect("build with override");

        assert!(Arc::ptr_eq(&container.instance_repo, &custom_instance_repo));
    }

    #[tokio::test]
    async fn test_app_container_new_and_http_state_conversion() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();
        let validator = Arc::new(MockTokenValidator::new());

        let container = AppContainer::new(pool, validator, context);
        let http_state = container.to_http_state();

        assert!(std::ptr::eq(http_state.context, context));
    }
}
