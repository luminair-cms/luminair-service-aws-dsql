//! Dynamic Schema Synchronization and JSON Schema Loader.
//!
//! Provides end-to-end functionality for Milestone 3B:
//! 1. Loading declarative JSON schemas from disk
//! 2. Constructing the domain `SchemaRegistry` and `SystemConfig`
//! 3. Introspecting live database schema from PostgreSQL / AWS Aurora DSQL
//! 4. Calculating schema drift and generating topological migration plans
//! 5. Executing dynamic DDL outside transaction blocks with `sea-query`

pub mod builder;
pub mod diff;
pub mod executor;
pub mod introspector;
pub mod loader;
pub mod model;
pub mod naming;
pub mod planner;

use domain::entities::system_config::SystemConfig;
use domain::services::schema_registry::SchemaRegistry;
use sqlx::PgPool;
use std::path::Path;
use thiserror::Error;

pub use builder::build_desired_schema;
pub use diff::{DiffError, MigrationStep, SafetyPolicy, compute_diff};
pub use executor::{ExecutionSummary, ExecutorError, execute_migration_plan, step_to_sql};
pub use introspector::{IntrospectorError, SYSTEM_TABLES, introspect_database_schema};
pub use loader::{
    SchemaLoaderError, load_document_type_from_str, load_relation_from_str, load_schema_registry,
    load_system_config_from_str,
};
pub use model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
pub use naming::{
    attribute_to_column_name, document_type_to_table_name, foreign_key_column_name, index_name,
    is_reserved_sql_keyword, junction_table_name, junction_target_index_name, kebab_to_snake,
    link_owner_fk_name, link_owner_unique_index_name, link_table_name, link_target_fk_name,
    link_target_index_name, published_fk_name, published_table_name,
};
pub use planner::{MigrationPlan, plan_migrations};

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
