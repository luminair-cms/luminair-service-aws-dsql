//! Composition root container wiring persistence adapters and application services.

use std::sync::Arc;

use application::services::access_requests::AccessRequestsServiceImpl;
use application::services::documents::DocumentsServiceImpl;
use application::services::system_config::SystemConfigServiceImpl;
use domain::system::SystemContext;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

use super::config::{BootstrapMode, ConfigError, ServerConfig};
use crate::api::health::HealthChecker;
use crate::api::state::HttpState;
use crate::auth::{AuthAppState, AuthError, TokenValidator, run_bootstrap};
use crate::migrations::run_migrations;
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};
use crate::schema_loader::{
    SafetyPolicy, SchemaLoaderError, SchemaSyncError, build_desired_schema, load_schema_registry,
    sync_schemas,
};

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

    #[error("Schema loader error: {0}")]
    SchemaLoader(#[from] SchemaLoaderError),

    #[error("Administrator bootstrap error: {0}")]
    AuthBootstrap(#[from] AuthError),
}

/// Errors encountered when building an `AppContainer`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContainerBuildError {
    #[error("Missing authentication configuration: token validator or auth state must be provided")]
    MissingAuthConfiguration,
}

/// Helper to mask sensitive password credentials in database connection URLs.
pub fn mask_database_url(url: &str) -> String {
    if let Some(scheme_idx) = url.find("://") {
        let after_scheme = &url[scheme_idx + 3..];
        if let Some(at_idx) = after_scheme.find('@') {
            let user_info = &after_scheme[..at_idx];
            let rest = &after_scheme[at_idx..];
            let masked_user_info = if let Some(colon_idx) = user_info.find(':') {
                format!("{}:****", &user_info[..colon_idx])
            } else {
                "****".to_string()
            };
            return format!("{}{}{}", &url[..scheme_idx + 3], masked_user_info, rest);
        }
    }
    url.to_string()
}

/// Summary of operations executed during migration CLI mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSummary {
    /// True if static system migrations were executed successfully.
    pub static_migrations_applied: bool,
    /// DDL statements executed during dynamic document schema synchronization.
    pub executed_statements: Vec<String>,
}

/// Summary of configuration and schema validation executed during dry-run mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunSummary {
    /// Configured host:port server bind address.
    pub server_addr: String,
    /// Masked database connection URL (e.g. postgres://user:****@host:5432/db).
    pub database_url_masked: String,
    /// Maximum database connection pool size.
    pub max_db_connections: u32,
    /// Schema directory path.
    pub schema_dir: std::path::PathBuf,
    /// Number of loaded document types.
    pub document_types_count: usize,
    /// Names of loaded document types.
    pub document_type_names: Vec<String>,
    /// Number of loaded relations.
    pub relations_count: usize,
    /// Default locale identifier.
    pub default_locale: String,
    /// Available locale identifiers.
    pub available_locales: Vec<String>,
    /// Number of target database tables in the desired schema.
    pub target_tables_count: usize,
    /// Human-readable token validation mode (e.g. "HMAC Secret" or "OIDC JWKS").
    pub auth_mode: String,
}

/// The result outcome of executing application infrastructure bootstrapping.
pub enum BootstrapOutcome {
    /// Full service mode: application container initialized and ready to serve HTTP requests.
    Service(AppContainer),
    /// Migration-only CLI mode: database migrations and schema sync finished successfully.
    Migrated(MigrationSummary),
    /// Dry-run CLI mode: configuration and schemas validated successfully.
    DryRunValidated(DryRunSummary),
}

impl std::fmt::Debug for BootstrapOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service(_) => f.debug_tuple("Service").finish(),
            Self::Migrated(m) => f.debug_tuple("Migrated").field(m).finish(),
            Self::DryRunValidated(d) => f.debug_tuple("DryRunValidated").field(d).finish(),
        }
    }
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
    /// Bootstraps the application according to the specified mode (`Service`, `Migrate`, or `DryRun`).
    pub async fn bootstrap_with_mode(
        config: &ServerConfig,
        mode: BootstrapMode,
    ) -> Result<BootstrapOutcome, BootstrapError> {
        match mode {
            BootstrapMode::Service => {
                let container = Self::bootstrap(config).await?;
                Ok(BootstrapOutcome::Service(container))
            }
            BootstrapMode::Migrate => {
                let summary = Self::run_migrations_only(config).await?;
                Ok(BootstrapOutcome::Migrated(summary))
            }
            BootstrapMode::DryRun => {
                let summary = Self::dry_run(config)?;
                Ok(BootstrapOutcome::DryRunValidated(summary))
            }
        }
    }

    /// Runs only static and dynamic database schema migrations, then returns summary.
    pub async fn run_migrations_only(
        config: &ServerConfig,
    ) -> Result<MigrationSummary, BootstrapError> {
        tracing::info!("Connecting to database pool for migrations...");
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

        tracing::info!(
            "Migrations completed: {} dynamic DDL statements executed.",
            sync_result.executed_statements.len()
        );

        Ok(MigrationSummary {
            static_migrations_applied: true,
            executed_statements: sync_result.executed_statements,
        })
    }

    /// Validates environment configuration, authentication, and schemas without database connection.
    pub fn dry_run(config: &ServerConfig) -> Result<DryRunSummary, BootstrapError> {
        tracing::info!("Validating server configuration and schemas (dry-run)...");

        // 1. Validate database URL format
        if !config.database_url.starts_with("postgres://")
            && !config.database_url.starts_with("postgresql://")
        {
            return Err(ConfigError::InvalidEnvVar(
                "DATABASE_URL",
                "database URL must start with 'postgres://' or 'postgresql://'".into(),
            )
            .into());
        }

        // 2. Validate token validator initialization
        let _validator = config.init_token_validator()?;
        let auth_mode = if config.auth_secret.is_some() {
            "Secret HMAC token validator (symmetric)".to_string()
        } else if let Some(issuer) = &config.auth.issuer_url {
            format!("OIDC JWKS token validator (issuer: {issuer})")
        } else {
            "Unknown token validator".to_string()
        };

        // 3. Load and validate declarative schemas from disk
        tracing::info!(
            "Loading and testing schema definitions from '{}'...",
            config.schema_dir.display()
        );
        let (registry, system_config) = load_schema_registry(&config.schema_dir)?;

        // 4. Validate desired database schema AST construction
        let desired_schema = build_desired_schema(&registry);

        let document_type_names = registry
            .all_types()
            .map(|t| t.id.as_str().to_string())
            .collect::<Vec<_>>();

        let available_locales = system_config
            .available_locales
            .iter()
            .map(|l| l.as_str().to_string())
            .collect::<Vec<_>>();

        let summary = DryRunSummary {
            server_addr: config.server_addr(),
            database_url_masked: mask_database_url(&config.database_url),
            max_db_connections: config.max_db_connections,
            schema_dir: config.schema_dir.clone(),
            document_types_count: document_type_names.len(),
            document_type_names,
            relations_count: registry.all_relations().count(),
            default_locale: system_config.default_locale.as_str().to_string(),
            available_locales,
            target_tables_count: desired_schema.tables.len(),
            auth_mode,
        };

        Ok(summary)
    }

    /// Bootstraps the complete application infrastructure in Service mode:
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

    #[test]
    fn test_mask_database_url() {
        assert_eq!(
            mask_database_url("postgres://postgres:secret123@localhost:5432/luminair"),
            "postgres://postgres:****@localhost:5432/luminair"
        );
        assert_eq!(
            mask_database_url(
                "postgresql://app_user:pass@dsql.us-east-1.aws:5432/db?sslmode=require"
            ),
            "postgresql://app_user:****@dsql.us-east-1.aws:5432/db?sslmode=require"
        );
        assert_eq!(
            mask_database_url("postgres://localhost:5432/luminair"),
            "postgres://localhost:5432/luminair"
        );
    }

    #[test]
    fn test_dry_run_success() {
        let temp_dir =
            std::env::temp_dir().join(format!("luminair_dry_run_test_{}", Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        let relations_dir = temp_dir.join("relations");
        std::fs::create_dir_all(&doc_types_dir).unwrap();
        std::fs::create_dir_all(&relations_dir).unwrap();

        let article_json = r#"{
            "kind": "collection",
            "info": {
                "displayName": "Article",
                "singularName": "article",
                "pluralName": "articles"
            },
            "options": { "draftAndPublish": true },
            "attributes": {
                "title": { "type": "text", "required": true }
            }
        }"#;
        std::fs::write(doc_types_dir.join("article.json"), article_json).unwrap();

        let config = ServerConfig {
            mode: BootstrapMode::DryRun,
            database_url: "postgres://postgres:secret@localhost:5432/test".into(),
            max_db_connections: 10,
            schema_dir: temp_dir.clone(),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("test-jwt-secret-min-32-chars-long".into()),
        };

        let summary = AppContainer::dry_run(&config).expect("dry run should succeed");
        assert_eq!(summary.server_addr, "127.0.0.1:3000");
        assert_eq!(
            summary.database_url_masked,
            "postgres://postgres:****@localhost:5432/test"
        );
        assert_eq!(summary.document_types_count, 1);
        assert_eq!(summary.document_type_names, vec!["article"]);
        assert!(summary.target_tables_count >= 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dry_run_invalid_database_url() {
        let config = ServerConfig {
            mode: BootstrapMode::DryRun,
            database_url: "mysql://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: std::path::PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let err = AppContainer::dry_run(&config).unwrap_err();
        match err {
            BootstrapError::Config(ConfigError::InvalidEnvVar("DATABASE_URL", _)) => {}
            other => panic!("expected InvalidEnvVar for DATABASE_URL, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bootstrap_with_mode_dry_run() {
        let temp_dir =
            std::env::temp_dir().join(format!("luminair_bootstrap_mode_test_{}", Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        let relations_dir = temp_dir.join("relations");
        std::fs::create_dir_all(&doc_types_dir).unwrap();
        std::fs::create_dir_all(&relations_dir).unwrap();

        let config = ServerConfig {
            mode: BootstrapMode::DryRun,
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: temp_dir.clone(),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let outcome = AppContainer::bootstrap_with_mode(&config, BootstrapMode::DryRun)
            .await
            .expect("dry run outcome");

        match outcome {
            BootstrapOutcome::DryRunValidated(summary) => {
                assert_eq!(summary.document_types_count, 0);
            }
            _ => panic!("expected DryRunValidated"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
