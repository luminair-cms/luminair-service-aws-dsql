//! Luminair Infrastructure crate.

pub mod api;
pub mod auth;
pub mod migrations;
pub mod repositories;
pub mod schema_loader;

pub use api::{
    AccessRequestDto, ApiError, AppState, CollectionMeta, CollectionResponse, PaginationMeta,
    ProblemDetails, SingleResponse, create_router,
};

pub use auth::{
    AuthConfig, AuthError, AuthUser, AuthenticatedClaims, Claims, JwksTokenValidator,
    MockTokenValidator, SecretTokenValidator, ShadowUser, SqlxShadowUserRepository, TokenValidator,
    run_bootstrap,
};
pub use migrations::{MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations};
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxSnapshotRepository, SqlxUserRoleAssignmentRepository,
};
pub use schema_loader::{
    SchemaSyncError, SchemaSyncResult, build_desired_schema, execute_migration_plan,
    introspect_database_schema, load_schema_registry, sync_schemas,
};
