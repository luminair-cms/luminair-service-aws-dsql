//! Luminair Infrastructure crate.

pub mod migrations;
pub mod repositories;
pub mod schema_loader;

pub use migrations::{MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations};
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxUserRoleAssignmentRepository,
};
pub use schema_loader::{
    SchemaSyncError, SchemaSyncResult, build_desired_schema, execute_migration_plan,
    introspect_database_schema, load_schema_registry, sync_schemas,
};
