//! Type-Safe DDL Generation via `sea-query` and Database Execution.
//!
//! Converts `MigrationStep`s into idempotent PostgreSQL / Aurora DSQL DDL strings
//! and executes each statement independently outside transaction blocks.

use sea_query::{
    Alias, ColumnDef, Expr, ForeignKey, ForeignKeyAction, Index, PostgresQueryBuilder, Table,
};
use sqlx::PgPool;
use thiserror::Error;

use super::diff::MigrationStep;
use super::model::{
    ColumnDefinition, ForeignKeyAction as ModelFkAction, IndexDefinition, SqlColumnType,
    TableDefinition,
};
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

    if table.is_junction() {
        // Composite PK (owner_id, target_id)
        for col in &table.columns {
            let mut col_def = build_column_def_internal(col, false);
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

    // Append foreign key constraints
    for fk in &table.foreign_keys {
        let mut fk_stmt = ForeignKey::create();
        fk_stmt.name(&fk.name);
        for col in &fk.columns {
            fk_stmt.from(Alias::new(&table.name), Alias::new(col));
        }
        for ref_col in &fk.referenced_columns {
            fk_stmt.to(Alias::new(&fk.referenced_table), Alias::new(ref_col));
        }
        fk_stmt.on_delete(map_fk_action(fk.on_delete));
        fk_stmt.on_update(map_fk_action(fk.on_update));
        stmt.foreign_key(&mut fk_stmt);
    }

    stmt.to_string(PostgresQueryBuilder)
}

fn map_fk_action(action: ModelFkAction) -> ForeignKeyAction {
    match action {
        ModelFkAction::Cascade => ForeignKeyAction::Cascade,
        ModelFkAction::Restrict => ForeignKeyAction::Restrict,
        ModelFkAction::SetNull => ForeignKeyAction::SetNull,
        ModelFkAction::NoAction => ForeignKeyAction::NoAction,
    }
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

/// Internal helper to map a `ColumnDefinition` to a `sea_query::ColumnDef`, optionally including inline primary_key().
fn build_column_def_internal(col: &ColumnDefinition, include_pk: bool) -> ColumnDef {
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

    if include_pk && col.is_primary_key {
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

/// Maps a `ColumnDefinition` to a `sea_query::ColumnDef`.
pub fn build_column_def(col: &ColumnDefinition) -> ColumnDef {
    build_column_def_internal(col, col.is_primary_key)
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
        ColumnDefinition, ForeignKeyDefinition, IndexDefinition, SqlColumnType, TableDefinition,
        TableKind,
    };

    #[test]
    fn test_sea_query_foreign_key() {
        let mut stmt = Table::create();
        stmt.table(Alias::new("articles__published"))
            .if_not_exists()
            .col(
                ColumnDef::new(Alias::new("id"))
                    .uuid()
                    .not_null()
                    .primary_key(),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_articles__published_id")
                    .from(Alias::new("articles__published"), Alias::new("id"))
                    .to(Alias::new("articles"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Cascade),
            );
        let sql = stmt.to_string(PostgresQueryBuilder);
        println!("Generated SQL: {sql}");
    }

    #[test]
    fn test_step_to_sql_create_table() {
        let mut table = TableDefinition::new("articles", TableKind::Entity, false);
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
    fn test_step_to_sql_published_table_with_foreign_key() {
        let mut table = TableDefinition::new("articles__published", TableKind::Published, false);
        table.columns.insert(ColumnDefinition {
            name: "id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });
        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles__published_id",
            vec!["id".into()],
            "articles",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles__published""#));
        assert!(sql.contains(r#"CONSTRAINT "fk_articles__published_id" FOREIGN KEY ("id") REFERENCES "articles" ("id") ON DELETE CASCADE"#));
    }

    #[test]
    fn test_step_to_sql_link_table() {
        let mut table = TableDefinition::new("articles__tags_link", TableKind::Link, false);
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
        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles__tags_link_owner",
            vec!["owner_id".into()],
            "articles",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));
        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles__tags_link_target",
            vec!["target_id".into()],
            "tags",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles__tags_link""#));
        assert!(sql.contains(r#"PRIMARY KEY ("owner_id", "target_id")"#));
        // Verify columns do NOT contain redundant inline PRIMARY KEY
        assert!(!sql.contains(r#""owner_id" uuid NOT NULL PRIMARY KEY"#));
        assert!(!sql.contains(r#""target_id" uuid NOT NULL PRIMARY KEY"#));
        assert!(sql.contains(r#"CONSTRAINT "fk_articles__tags_link_owner" FOREIGN KEY ("owner_id") REFERENCES "articles" ("id") ON DELETE CASCADE"#));
        assert!(sql.contains(r#"CONSTRAINT "fk_articles__tags_link_target" FOREIGN KEY ("target_id") REFERENCES "tags" ("id") ON DELETE CASCADE"#));
    }

    #[test]
    fn test_step_to_sql_published_link_table() {
        let mut table =
            TableDefinition::new("articles__tags_link__published", TableKind::Link, false);
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
        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles__tags_link__published_owner",
            vec!["owner_id".into()],
            "articles__published",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));
        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles__tags_link__published_target",
            vec!["target_id".into()],
            "tags__published",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles__tags_link__published""#));
        assert!(sql.contains(r#"PRIMARY KEY ("owner_id", "target_id")"#));
        assert!(!sql.contains(r#""owner_id" uuid NOT NULL PRIMARY KEY"#));
        assert!(!sql.contains(r#""target_id" uuid NOT NULL PRIMARY KEY"#));
        assert!(sql.contains(r#"CONSTRAINT "fk_articles__tags_link__published_owner" FOREIGN KEY ("owner_id") REFERENCES "articles__published" ("id") ON DELETE CASCADE"#));
        assert!(sql.contains(r#"CONSTRAINT "fk_articles__tags_link__published_target" FOREIGN KEY ("target_id") REFERENCES "tags__published" ("id") ON DELETE CASCADE"#));
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
            kind: TableKind::Entity,
        };
        let sql = step_to_sql(&step);
        assert_eq!(sql, r#"DROP TABLE IF EXISTS "legacy""#);
    }
}
