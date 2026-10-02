//! Composition container wiring persistence adapters and application services.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use domain::system::SystemContext;
use sqlx::PgPool;
use thiserror::Error;

use crate::api::health::HealthChecker;
use crate::api::state::HttpState;
use crate::auth::{AuthAppState, TokenValidator};
use crate::persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};
use crate::persistence::PersistenceContainer;

/// Errors encountered when building an `AppContainer`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContainerBuildError {
    #[error("Missing authentication configuration: token validator or auth state must be provided")]
    MissingAuthConfiguration,
}

/// Central composition root container holding initialized services and adapters.
#[derive(Clone)]
pub struct AppContainer {
    pub context: &'static SystemContext,
    pub persistence: PersistenceContainer,
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
        context: &'static SystemContext,
    ) -> Self {
        Self::builder(pool, context).assemble_with_validator(validator)
    }

    /// Creates a builder for custom or step-by-step container assembly.
    pub fn builder(pool: PgPool, context: &'static SystemContext) -> AppContainerBuilder {
        AppContainerBuilder::new(pool, context)
    }

    /// Accessor for database connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.persistence.pool
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
}

/// Builder for assembling an `AppContainer` with custom components or overrides.
pub struct AppContainerBuilder {
    pool: PgPool,
    context: &'static SystemContext,
    validator: Option<Arc<dyn TokenValidator>>,
    auth: Option<AuthAppState>,
    persistence: Option<PersistenceContainer>,
    instance_repo: Option<SqlxDocumentInstanceRepository>,
    access_request_repo: Option<SqlxAccessRequestRepository>,
    assignment_repo: Option<SqlxUserRoleAssignmentRepository>,
    role_repo: Option<SqlxRoleRepository>,
    shadow_user_repo: Option<SqlxShadowUserRepository>,
}

impl AppContainerBuilder {
    /// Creates a new container builder with mandatory database pool and static schema context.
    pub fn new(pool: PgPool, context: &'static SystemContext) -> Self {
        Self {
            pool,
            context,
            validator: None,
            auth: None,
            persistence: None,
            instance_repo: None,
            access_request_repo: None,
            assignment_repo: None,
            role_repo: None,
            shadow_user_repo: None,
        }
    }

    /// Configures the token validator to use for authentication state construction.
    pub fn with_validator(mut self, validator: Arc<dyn TokenValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Configures an existing `AuthAppState`, reusing its repository instances.
    pub fn with_auth_state(mut self, auth: AuthAppState) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Overrides the entire persistence container.
    pub fn with_persistence(mut self, persistence: PersistenceContainer) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Overrides the document instance repository.
    pub fn with_instance_repo(mut self, repo: SqlxDocumentInstanceRepository) -> Self {
        self.instance_repo = Some(repo);
        self
    }

    /// Overrides the access request repository.
    pub fn with_access_request_repo(mut self, repo: SqlxAccessRequestRepository) -> Self {
        self.access_request_repo = Some(repo);
        self
    }

    /// Overrides the user role assignment repository.
    pub fn with_assignment_repo(mut self, repo: SqlxUserRoleAssignmentRepository) -> Self {
        self.assignment_repo = Some(repo);
        self
    }

    /// Overrides the role repository.
    pub fn with_role_repo(mut self, repo: SqlxRoleRepository) -> Self {
        self.role_repo = Some(repo);
        self
    }

    /// Overrides the shadow user repository.
    pub fn with_shadow_user_repo(mut self, repo: SqlxShadowUserRepository) -> Self {
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

        let mut persistence = self.persistence.unwrap_or_else(|| PersistenceContainer {
            pool: pool.clone(),
            instance_repo: SqlxDocumentInstanceRepository::new(pool.clone(), &context.schema),
            access_request_repo: auth.access_request_repo.clone(),
            assignment_repo: auth.assignment_repo.clone(),
            role_repo: auth.role_repo.clone(),
            shadow_user_repo: auth.shadow_user_repo.clone(),
        });

        if let Some(repo) = self.instance_repo {
            persistence.instance_repo = repo;
        }
        if let Some(repo) = self.access_request_repo {
            persistence.access_request_repo = repo;
        }
        if let Some(repo) = self.assignment_repo {
            persistence.assignment_repo = repo;
        }
        if let Some(repo) = self.role_repo {
            persistence.role_repo = repo;
        }
        if let Some(repo) = self.shadow_user_repo {
            persistence.shadow_user_repo = repo;
        }

        Self::finish_assemble(context, persistence, auth)
    }

    fn assemble_with_validator(self, validator: Arc<dyn TokenValidator>) -> AppContainer {
        let pool = self.pool;
        let context = self.context;

        let mut persistence = self
            .persistence
            .unwrap_or_else(|| PersistenceContainer::new(pool.clone(), &context.schema));

        if let Some(repo) = self.instance_repo {
            persistence.instance_repo = repo;
        }
        if let Some(repo) = self.access_request_repo {
            persistence.access_request_repo = repo;
        }
        if let Some(repo) = self.assignment_repo {
            persistence.assignment_repo = repo;
        }
        if let Some(repo) = self.role_repo {
            persistence.role_repo = repo;
        }
        if let Some(repo) = self.shadow_user_repo {
            persistence.shadow_user_repo = repo;
        }

        let auth = AuthAppState::from_parts(
            persistence.pool.clone(),
            validator,
            persistence.assignment_repo.clone(),
            persistence.role_repo.clone(),
            persistence.access_request_repo.clone(),
            persistence.shadow_user_repo.clone(),
        );

        Self::finish_assemble(context, persistence, auth)
    }

    fn finish_assemble(
        context: &'static SystemContext,
        persistence: PersistenceContainer,
        auth: AuthAppState,
    ) -> AppContainer {
        let documents_service = Arc::new(DocumentsServiceImpl::new(
            persistence.instance_repo.clone(),
            context,
        ));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            persistence.access_request_repo.clone(),
            persistence.assignment_repo.clone(),
            persistence.role_repo.clone(),
        ));

        let system_config_service = Arc::new(SystemConfigServiceImpl::new(context));
        let health_checker = Arc::new(HealthChecker::new(persistence.pool.clone()));

        AppContainer {
            context,
            persistence,
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
    use sqlx::postgres::PgPoolOptions;
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

        let custom_schema = Box::leak(Box::new(SchemaRegistry::default()));
        let custom_instance_repo = SqlxDocumentInstanceRepository::new(
            pool.clone(),
            custom_schema,
        );

        let container = AppContainer::builder(pool, context)
            .with_validator(validator)
            .with_instance_repo(custom_instance_repo)
            .build()
            .expect("build with override");

        assert!(std::ptr::eq(
            container.persistence.instance_repo.schema_registry(),
            custom_schema
        ));
    }

    #[tokio::test]
    async fn test_builder_with_persistence_override() {
        let pool = create_lazy_test_pool();
        let context = create_test_context();
        let validator = Arc::new(MockTokenValidator::new());

        let custom_persistence = PersistenceContainer::new(pool.clone(), &context.schema);

        let container = AppContainer::builder(pool, context)
            .with_validator(validator)
            .with_persistence(custom_persistence)
            .build()
            .expect("build with persistence override");

        assert_eq!(
            container.persistence.pool.size(),
            container.pool().size()
        );
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
