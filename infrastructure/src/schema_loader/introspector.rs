//! Live Database Schema Introspector.
//!
//! Introspects live PostgreSQL / AWS Aurora DSQL tables, columns, constraints, and indexes
//! via `information_schema` and `pg_catalog` to construct an `ActualSchema` AST.

use indexmap::IndexSet;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use super::model::{
    ColumnDefinition, DatabaseSchema, IndexDefinition, SqlColumnType, TableDefinition,
};

/// List of internal system and migration tables that are excluded from dynamic schema management.
pub const SYSTEM_TABLES: &[&str] = &[
    "_sqlx_migrations",
    "roles",
    "permissions",
    "role_permissions",
    "user_roles",
    "access_requests",
    "shadow_users",
    "document_revision_snapshots",
];

#[derive(Debug, Error)]
pub enum IntrospectorError {
    #[error("Database query error during schema introspection: {0}")]
    Database(#[from] sqlx::Error),
}

type ColumnRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
);

/// Introspects the live database and returns the current `DatabaseSchema`.
pub async fn introspect_database_schema(
    pool: &PgPool,
) -> Result<DatabaseSchema, IntrospectorError> {
    let mut schema = DatabaseSchema::new();

    // 1. Fetch all base user tables in the 'public' schema
    let table_rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        ORDER BY table_name;
        "#,
    )
    .fetch_all(pool)
    .await?;

    let candidate_tables: Vec<String> = table_rows
        .into_iter()
        .map(|(t,)| t)
        .filter(|t| !SYSTEM_TABLES.contains(&t.as_str()))
        .collect();

    if candidate_tables.is_empty() {
        return Ok(schema);
    }

    // 2. Fetch primary key column mappings
    let pk_rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT
            tc.table_name,
            kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
          AND tc.table_schema = 'public';
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut pk_map: HashMap<String, HashSet<String>> = HashMap::new();
    for (tbl, col) in pk_rows {
        pk_map.entry(tbl).or_default().insert(col);
    }

    // 3. Fetch all columns for candidate tables
    let column_rows: Vec<ColumnRow> = sqlx::query_as(
        r#"
        SELECT
            table_name,
            column_name,
            data_type,
            is_nullable,
            column_default,
            character_maximum_length,
            numeric_precision,
            numeric_scale
        FROM information_schema.columns
        WHERE table_schema = 'public'
        ORDER BY table_name, ordinal_position;
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut columns_by_table: HashMap<String, IndexSet<ColumnDefinition>> = HashMap::new();
    for (tbl, col_name, data_type, is_nullable, col_def, char_len, num_prec, num_scale) in
        column_rows
    {
        if !candidate_tables.contains(&tbl) {
            continue;
        }

        let sql_type =
            SqlColumnType::from_information_schema(&data_type, char_len, num_prec, num_scale)
                .unwrap_or(SqlColumnType::Text);

        let nullable = is_nullable.eq_ignore_ascii_case("YES");
        let is_pk = pk_map
            .get(&tbl)
            .map(|set| set.contains(&col_name))
            .unwrap_or(false);

        let col = ColumnDefinition {
            name: col_name,
            data_type: sql_type,
            nullable,
            is_primary_key: is_pk,
            default_value: col_def,
            unique: false,
        };

        columns_by_table.entry(tbl).or_default().insert(col);
    }

    // 4. Fetch indexes from pg_catalog.pg_indexes
    let index_rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT
            tablename,
            indexname,
            indexdef
        FROM pg_catalog.pg_indexes
        WHERE schemaname = 'public';
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut indexes_by_table: HashMap<String, IndexSet<IndexDefinition>> = HashMap::new();
    for (tbl, idx_name, idx_def) in index_rows {
        if !candidate_tables.contains(&tbl) {
            continue;
        }
        // Exclude primary key indexes (which end in `_pkey`)
        if idx_name.ends_with("_pkey") {
            continue;
        }

        let is_unique = idx_def.to_uppercase().starts_with("CREATE UNIQUE INDEX");
        let cols = parse_index_columns(&idx_def);

        let idx = IndexDefinition {
            name: idx_name,
            table_name: tbl.clone(),
            columns: cols,
            unique: is_unique,
        };

        indexes_by_table.entry(tbl).or_default().insert(idx);
    }

    // 5. Assemble TableDefinitions
    for tbl_name in candidate_tables {
        let cols = columns_by_table.remove(&tbl_name).unwrap_or_default();
        let idxs = indexes_by_table.remove(&tbl_name).unwrap_or_default();
        let is_junction = tbl_name.contains("__");
        let is_singleton = cols.get("_singleton").is_some();

        let table = TableDefinition {
            name: tbl_name,
            columns: cols,
            indexes: idxs,
            is_junction,
            is_singleton,
        };

        schema.insert_table(table);
    }

    Ok(schema)
}

/// Parses the column names out of a PostgreSQL `CREATE [UNIQUE] INDEX ... ON table (col1, col2)` definition.
pub fn parse_index_columns(indexdef: &str) -> Vec<String> {
    if let (Some(start_paren), Some(end_paren)) = (indexdef.rfind('('), indexdef.rfind(')'))
        && start_paren < end_paren
    {
        let inner = &indexdef[start_paren + 1..end_paren];
        return inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index_columns() {
        let def1 = "CREATE UNIQUE INDEX idx_articles_slug ON public.articles USING btree (slug)";
        assert_eq!(parse_index_columns(def1), vec!["slug"]);

        let def2 = "CREATE INDEX idx_multi ON public.test USING btree (col_a, col_b)";
        assert_eq!(parse_index_columns(def2), vec!["col_a", "col_b"]);

        let def3 = r#"CREATE INDEX idx_quoted ON public.test USING btree ("col_a", "col_b")"#;
        assert_eq!(parse_index_columns(def3), vec!["col_a", "col_b"]);
    }

    #[test]
    fn test_system_tables_excluded() {
        assert!(SYSTEM_TABLES.contains(&"_sqlx_migrations"));
        assert!(SYSTEM_TABLES.contains(&"roles"));
        assert!(SYSTEM_TABLES.contains(&"document_revision_snapshots"));
    }
}
