//! Schema synchronization pipeline: Registry -> Desired AST -> Introspect -> Diff -> Plan -> DDL Execution.

use domain::schema::SchemaRegistry;
use sqlx::PgPool;
use thiserror::Error;

use super::builder::build_desired_schema;
use super::diff::{DiffError, SafetyPolicy, compute_diff};
use super::executor::{ExecutorError, execute_migration_plan};
use super::introspector::{IntrospectorError, introspect_database_schema};
use super::planner::plan_migrations;

/// Errors encountered during dynamic database schema synchronization.
#[derive(Debug, Error)]
pub enum SchemaSyncError {
    #[error("Database introspection error: {0}")]
    Introspector(#[from] IntrospectorError),

    #[error("Schema drift error: {0}")]
    Diff(#[from] DiffError),

    #[error("DDL execution error: {0}")]
    Executor(#[from] ExecutorError),
}

/// The result of running schema synchronization against a live database.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchemaSyncResult {
    pub executed_statements: Vec<String>,
}

/// Synchronizes a live database schema to match an in-memory `SchemaRegistry`.
///
/// 1. Builds desired database schema AST from the `SchemaRegistry`
/// 2. Introspects current live database schema from `information_schema`
/// 3. Computes schema diff adhering to `SafetyPolicy`
/// 4. Plans migration steps topologically (parent tables before child link tables)
/// 5. Executes generated DDL via SeaQuery
pub async fn sync_dynamic_schemas(
    pool: &PgPool,
    registry: &SchemaRegistry,
    policy: SafetyPolicy,
) -> Result<SchemaSyncResult, SchemaSyncError> {
    let desired = build_desired_schema(registry);
    let current = introspect_database_schema(pool).await?;
    let steps = compute_diff(&current, &desired, policy)?;

    if steps.is_empty() {
        return Ok(SchemaSyncResult {
            executed_statements: Vec::new(),
        });
    }

    let plan = plan_migrations(steps);
    let summary = execute_migration_plan(pool, &plan).await?;

    Ok(SchemaSyncResult {
        executed_statements: summary.executed_statements,
    })
}
