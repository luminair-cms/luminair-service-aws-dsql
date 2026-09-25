//! Type-Safe DDL Generation via `sea-query` and Database Execution.
//!
//! Converts `MigrationStep`s into idempotent PostgreSQL / Aurora DSQL DDL strings
//! and executes each statement independently outside transaction blocks.

use sea_query::{Alias, ColumnDef, Expr, Index, PostgresQueryBuilder, Table};
use sqlx::PgPool;
use thiserror::Error;

use super::diff::MigrationStep;
use super::model::{ColumnDefinition, IndexDefinition, SqlColumnType, TableDefinition};
use super::planner::MigrationPlan;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("SQL execution error during DDL migration: {source}\nFailed statement: {sql}")]
    Database {
        sql: String,
        #[source]
        source: sqlx::Error,
    },
}

/// Result summary of executing a migration plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionSummary {
    pub executed_statements: Vec<String>,
}

/// Translates a single `MigrationStep` into a PostgreSQL DDL string.
pub fn step_to_sql(step: &MigrationStep) -> String {
    match step {
        MigrationStep::CreateTable(table) => build_create_table_sql(table),
        MigrationStep::DropTable { name, .. } => Table::drop()
            .table(Alias::new(name))
            .if_exists()
            .to_string(PostgresQueryBuilder),
        MigrationStep::AddColumn { table, column } => {
            let col_def = build_column_def(column);
            Table::alter()
                .table(Alias::new(table))
                .add_column_if_not_exists(col_def)
                .to_string(PostgresQueryBuilder)
        }
        MigrationStep::DropColumn { table, column } => Table::alter()
            .table(Alias::new(table))
            .drop_column_if_exists(Alias::new(column))
            .to_string(PostgresQueryBuilder),
        MigrationStep::CreateIndex(index) => build_create_index_sql(index),
        MigrationStep::DropIndex { name, .. } => Index::drop()
            .name(name)
            .if_exists()
            .to_string(PostgresQueryBuilder),
    }
}

/// Builds SQL for `CREATE TABLE IF NOT EXISTS`.
fn build_create_table_sql(table: &TableDefinition) -> String {
    let mut stmt = Table::create();
    stmt.table(Alias::new(&table.name)).if_not_exists();

    if table.is_junction {
        // Composite PK (owner_id, target_id)
        for col in &table.columns {
            let mut col_def = build_column_def(col);
            stmt.col(&mut col_def);
        }
        stmt.primary_key(
            Index::create()
                .col(Alias::new("owner_id"))
                .col(Alias::new("target_id")),
        );
    } else {
        for col in &table.columns {
            let mut col_def = build_column_def(col);
            stmt.col(&mut col_def);
        }
    }

    stmt.to_string(PostgresQueryBuilder)
}

/// Builds SQL for `CREATE [UNIQUE] INDEX IF NOT EXISTS`.
fn build_create_index_sql(index: &IndexDefinition) -> String {
    let mut stmt = Index::create();
    stmt.if_not_exists()
        .name(&index.name)
        .table(Alias::new(&index.table_name));

    if index.unique {
        stmt.unique();
    }

    for col in &index.columns {
        stmt.col(Alias::new(col));
    }

    stmt.to_string(PostgresQueryBuilder)
}

/// Maps a `ColumnDefinition` to a `sea_query::ColumnDef`.
pub fn build_column_def(col: &ColumnDefinition) -> ColumnDef {
    let mut def = ColumnDef::new(Alias::new(&col.name));

    match col.data_type {
        SqlColumnType::Uuid => {
            def.uuid();
        }
        SqlColumnType::Varchar(Some(len)) => {
            def.string_len(len as u32);
        }
        SqlColumnType::Varchar(None) => {
            def.string();
        }
        SqlColumnType::Text => {
            def.text();
        }
        SqlColumnType::SmallInt => {
            def.small_integer();
        }
        SqlColumnType::Integer => {
            def.integer();
        }
        SqlColumnType::BigInt => {
            def.big_integer();
        }
        SqlColumnType::Decimal(p, s) => {
            def.decimal_len(p as u32, s as u32);
        }
        SqlColumnType::Boolean => {
            def.boolean();
        }
        SqlColumnType::Date => {
            def.date();
        }
        SqlColumnType::Timestamptz => {
            def.timestamp_with_time_zone();
        }
        SqlColumnType::Jsonb => {
            def.json_binary();
        }
    }

    if !col.nullable {
        def.not_null();
    }

    if col.is_primary_key {
        def.primary_key();
    }

    if let Some(ref val) = col.default_value {
        if val.eq_ignore_ascii_case("CURRENT_TIMESTAMP") {
            def.default(Expr::current_timestamp());
        } else if val.eq_ignore_ascii_case("TRUE") {
            def.default(true);
        } else if val.eq_ignore_ascii_case("FALSE") {
            def.default(false);
        } else if let Ok(n) = val.parse::<i64>() {
            def.default(n);
        }
    }

    def
}

/// Executes a `MigrationPlan` statement-by-statement outside transaction blocks.
pub async fn execute_migration_plan(
    pool: &PgPool,
    plan: &MigrationPlan,
) -> Result<ExecutionSummary, ExecutorError> {
    let mut executed = Vec::new();

    for step in &plan.steps {
        let sql = step_to_sql(step);
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(pool)
            .await
            .map_err(|e| ExecutorError::Database {
                sql: sql.clone(),
                source: e,
            })?;
        executed.push(sql);
    }

    Ok(ExecutionSummary {
        executed_statements: executed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_loader::model::{
        ColumnDefinition, IndexDefinition, SqlColumnType, TableDefinition,
    };

    #[test]
    fn test_step_to_sql_create_table() {
        let mut table = TableDefinition::new("articles", false, false);
        table.columns.insert(ColumnDefinition {
            name: "id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });
        table.columns.insert(ColumnDefinition {
            name: "title".into(),
            data_type: SqlColumnType::Text,
            nullable: false,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles""#));
        assert!(sql.contains(r#""id" uuid NOT NULL PRIMARY KEY"#));
        assert!(sql.contains(r#""title" text NOT NULL"#));
    }

    #[test]
    fn test_step_to_sql_junction_table() {
        let mut table = TableDefinition::new("articles__tags", true, false);
        table.columns.insert(ColumnDefinition {
            name: "owner_id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });
        table.columns.insert(ColumnDefinition {
            name: "target_id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles__tags""#));
        assert!(sql.contains(r#"PRIMARY KEY ("owner_id", "target_id")"#));
    }

    #[test]
    fn test_step_to_sql_add_column() {
        let step = MigrationStep::AddColumn {
            table: "articles".into(),
            column: ColumnDefinition {
                name: "views".into(),
                data_type: SqlColumnType::BigInt,
                nullable: false,
                is_primary_key: false,
                default_value: Some("0".into()),
                unique: false,
            },
        };
        let sql = step_to_sql(&step);
        assert!(sql.contains(
            r#"ALTER TABLE "articles" ADD COLUMN IF NOT EXISTS "views" bigint NOT NULL DEFAULT 0"#
        ));
    }

    #[test]
    fn test_step_to_sql_create_index() {
        let idx = IndexDefinition::new("idx_articles_slug", "articles", vec!["slug".into()], true);
        let step = MigrationStep::CreateIndex(idx);
        let sql = step_to_sql(&step);
        assert!(sql.contains(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_articles_slug" ON "articles" ("slug")"#
        ));
    }

    #[test]
    fn test_step_to_sql_drop_table() {
        let step = MigrationStep::DropTable {
            name: "legacy".into(),
            is_junction: false,
        };
        let sql = step_to_sql(&step);
        assert_eq!(sql, r#"DROP TABLE IF EXISTS "legacy""#);
    }
}
