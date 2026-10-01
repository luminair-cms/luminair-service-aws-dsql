//! Dynamic schema migration pipeline: AST modeling, introspection, diffing, planning, and DDL execution.

pub mod builder;
pub mod diff;
pub mod executor;
pub mod introspector;
pub mod model;
pub mod planner;
pub mod sync;

pub use builder::build_desired_schema;
pub use diff::{DiffError, MigrationStep, SafetyPolicy, compute_diff};
pub use executor::{ExecutionSummary, ExecutorError, execute_migration_plan, step_to_sql};
pub use introspector::{IntrospectorError, SYSTEM_TABLES, introspect_database_schema};
pub use model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
pub use planner::{MigrationPlan, plan_migrations};
pub use sync::{SchemaSyncError, SchemaSyncResult, sync_dynamic_schemas};
