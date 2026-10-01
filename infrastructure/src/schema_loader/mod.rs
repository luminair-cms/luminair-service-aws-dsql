//! Declarative JSON Schema Loader and Migration Re-exports.
//!
//! Provides:
//! 1. Loading declarative JSON schemas from disk into `SchemaRegistry` and `SystemConfig`
//! 2. Backward-compatible re-exports from `crate::persistence::migration`

pub mod loader;
pub mod naming;

use std::path::Path;
use domain::schema::SchemaRegistry;
use domain::system::SystemConfig;
use sqlx::PgPool;
use thiserror::Error;

pub use loader::{
    SchemaLoaderError, load_document_type_from_str, load_relation_from_str, load_schema_registry,
    load_system_config_from_str,
};

pub use crate::persistence::migration::{
    ColumnDefinition, DatabaseSchema, DiffError, ExecutionSummary, ExecutorError,
    ForeignKeyAction, ForeignKeyDefinition, IndexDefinition, IntrospectorError, MigrationPlan,
    MigrationStep, SYSTEM_TABLES, SafetyPolicy, SqlColumnType, TableDefinition, TableKind,
    build_desired_schema, compute_diff, execute_migration_plan, introspect_database_schema,
    plan_migrations, step_to_sql, sync_dynamic_schemas,
};

pub use naming::*;

/// Errors encountered during schema synchronization.
#[derive(Debug, Error)]
pub enum SchemaSyncError {
    #[error("Schema loader error: {0}")]
    Loader(#[from] SchemaLoaderError),

    #[error("Database introspection error: {0}")]
    Introspector(#[from] IntrospectorError),

    #[error("Schema drift error: {0}")]
    Diff(#[from] DiffError),

    #[error("DDL execution error: {0}")]
    Executor(#[from] ExecutorError),
}

/// The result of running schema synchronization against a live database.
#[derive(Debug, Clone)]
pub struct SchemaSyncResult {
    pub registry: SchemaRegistry,
    pub system_config: SystemConfig,
    pub desired_schema: DatabaseSchema,
    pub plan: MigrationPlan,
    pub executed_statements: Vec<String>,
}

/// Orchestrates complete schema synchronization from JSON files to live database DDL.
pub async fn sync_schemas(
    pool: &PgPool,
    schema_dir: &Path,
    safety_policy: SafetyPolicy,
) -> Result<SchemaSyncResult, SchemaSyncError> {
    // 1. Load declarative schemas from directory
    let (registry, system_config) = load_schema_registry(schema_dir)?;

    // 2. Build target DesiredSchema AST
    let desired_schema = build_desired_schema(&registry);

    // 3. Introspect live database schema
    let actual_schema = introspect_database_schema(pool).await?;

    // 4. Compute schema drift with safety policy
    let steps = compute_diff(&actual_schema, &desired_schema, safety_policy)?;

    // 5. Order migration steps topologically
    let plan = plan_migrations(steps);

    // 6. Execute DDL migration plan
    let summary = execute_migration_plan(pool, &plan).await?;

    Ok(SchemaSyncResult {
        registry,
        system_config,
        desired_schema,
        plan,
        executed_statements: summary.executed_statements,
    })
}
