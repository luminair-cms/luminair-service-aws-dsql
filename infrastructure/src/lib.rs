//! Luminair Infrastructure crate.

pub mod api;
pub mod auth;
pub mod cli;
pub mod container;
pub mod migrations;
pub mod repositories;
pub mod schema_loader;

pub use api::{
    AccessRequestDto, ApiError, AppState, CollectionMeta, CollectionResponse, HealthChecker,
    HttpState, PaginationMeta, ProblemDetails, SingleResponse, create_router,
};
pub use auth::{
    AuthConfig, AuthContextResolver, AuthError, AuthUser, AuthenticatedClaims, Claims,
    JwksTokenValidator, MockTokenValidator, SecretTokenValidator, TokenValidator, run_bootstrap,
};
pub use cli::{
    BootstrapError, BootstrapMode, BootstrapOutcome, CliError, CliOutcome, ConfigError,
    DryRunSummary, MigrationSummary, RunMode, ServerConfig, dry_run, mask_database_url, migrate,
    print_help, run,
};
pub use container::{AppContainer, AppContainerBuilder, ContainerBuildError};
pub use migrations::{MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations};
pub use repositories::{
    ShadowUser, SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};
pub use schema_loader::{
    SchemaSyncError, SchemaSyncResult, build_desired_schema, execute_migration_plan,
    introspect_database_schema, load_schema_registry, sync_schemas,
};
