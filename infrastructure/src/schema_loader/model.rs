//! Database Schema AST modeled with `IndexSet<T>` and `Borrow<str>`.
//!
//! Preserves deterministic table, column, and index order without duplicating
//! identifier strings for map keys. Enables $O(1)$ lookups via `.get("name")`.

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::hash::{Hash, Hasher};

/// High-level representation of a relational database schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub tables: IndexSet<TableDefinition>,
}

impl DatabaseSchema {
    pub fn new() -> Self {
        Self {
            tables: IndexSet::new(),
        }
    }

    pub fn find_table(&self, name: &str) -> Option<&TableDefinition> {
        self.tables.get(name)
    }

    pub fn insert_table(&mut self, table: TableDefinition) -> bool {
        self.tables.insert(table)
    }
}

/// The architectural classification of a database table in Luminair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TableKind {
    /// A main document instance working draft table (e.g., `articles`, `site_setting`).
    #[default]
    Entity,
    /// A published mirror table with at most one row per instance (e.g., `articles__published`).
    Published,
    /// A universal relation link table (e.g., `articles__tags_link`, `articles__author_link`).
    Link,
}

/// Referential action triggered on update or delete of a foreign key constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForeignKeyAction {
    #[default]
    Cascade,
    Restrict,
    SetNull,
    NoAction,
}

/// Definition of a database foreign key constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyDefinition {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_delete: ForeignKeyAction,
    pub on_update: ForeignKeyAction,
}

impl ForeignKeyDefinition {
    pub fn new(
        name: impl Into<String>,
        columns: Vec<String>,
        referenced_table: impl Into<String>,
        referenced_columns: Vec<String>,
        on_delete: ForeignKeyAction,
        on_update: ForeignKeyAction,
    ) -> Self {
        Self {
            name: name.into(),
            columns,
            referenced_table: referenced_table.into(),
            referenced_columns,
            on_delete,
            on_update,
        }
    }
}

impl PartialEq for ForeignKeyDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ForeignKeyDefinition {}

impl Hash for ForeignKeyDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for ForeignKeyDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

/// Definition of a database table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    pub name: String,
    pub columns: IndexSet<ColumnDefinition>,
    pub indexes: IndexSet<IndexDefinition>,
    pub foreign_keys: IndexSet<ForeignKeyDefinition>,
    pub kind: TableKind,
    pub is_singleton: bool,
}

impl TableDefinition {
    pub fn new(name: impl Into<String>, kind: TableKind, is_singleton: bool) -> Self {
        Self {
            name: name.into(),
            columns: IndexSet::new(),
            indexes: IndexSet::new(),
            foreign_keys: IndexSet::new(),
            kind,
            is_singleton,
        }
    }

    pub fn is_junction(&self) -> bool {
        self.kind == TableKind::Link
    }

    pub fn is_published(&self) -> bool {
        self.kind == TableKind::Published
    }

    pub fn find_column(&self, name: &str) -> Option<&ColumnDefinition> {
        self.columns.get(name)
    }

    pub fn find_index(&self, name: &str) -> Option<&IndexDefinition> {
        self.indexes.get(name)
    }

    pub fn find_foreign_key(&self, name: &str) -> Option<&ForeignKeyDefinition> {
        self.foreign_keys.get(name)
    }
}

impl PartialEq for TableDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for TableDefinition {}

impl Hash for TableDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for TableDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

/// Definition of a database column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: SqlColumnType,
    pub nullable: bool,
    pub is_primary_key: bool,
    pub default_value: Option<String>,
    pub unique: bool,
}

impl ColumnDefinition {
    pub fn new(
        name: impl Into<String>,
        data_type: SqlColumnType,
        nullable: bool,
        is_primary_key: bool,
    ) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
            is_primary_key,
            default_value: None,
            unique: false,
        }
    }
}

impl PartialEq for ColumnDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ColumnDefinition {}

impl Hash for ColumnDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for ColumnDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

/// Definition of a database index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDefinition {
    pub name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

impl IndexDefinition {
    pub fn new(
        name: impl Into<String>,
        table_name: impl Into<String>,
        columns: Vec<String>,
        unique: bool,
    ) -> Self {
        Self {
            name: name.into(),
            table_name: table_name.into(),
            columns,
            unique,
        }
    }
}

impl PartialEq for IndexDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for IndexDefinition {}

impl Hash for IndexDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for IndexDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

/// SQL column data types supported by Luminair and AWS Aurora DSQL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlColumnType {
    Uuid,
    Varchar(Option<usize>),
    Text,
    SmallInt,
    Integer,
    BigInt,
    Decimal(u8, u8),
    Boolean,
    Date,
    Timestamptz,
    Jsonb,
}

impl SqlColumnType {
    /// Formats the column type into standard PostgreSQL DDL syntax.
    pub fn to_sql_string(&self) -> String {
        match self {
            SqlColumnType::Uuid => "UUID".to_string(),
            SqlColumnType::Varchar(Some(len)) => format!("VARCHAR({len})"),
            SqlColumnType::Varchar(None) => "VARCHAR".to_string(),
            SqlColumnType::Text => "TEXT".to_string(),
            SqlColumnType::SmallInt => "SMALLINT".to_string(),
            SqlColumnType::Integer => "INTEGER".to_string(),
            SqlColumnType::BigInt => "BIGINT".to_string(),
            SqlColumnType::Decimal(p, s) => format!("NUMERIC({p}, {s})"),
            SqlColumnType::Boolean => "BOOLEAN".to_string(),
            SqlColumnType::Date => "DATE".to_string(),
            SqlColumnType::Timestamptz => "TIMESTAMPTZ".to_string(),
            SqlColumnType::Jsonb => "JSONB".to_string(),
        }
    }

    /// Parses a data type name from PostgreSQL `information_schema.columns`.
    pub fn from_information_schema(
        data_type: &str,
        char_len: Option<i32>,
        numeric_precision: Option<i32>,
        numeric_scale: Option<i32>,
    ) -> Option<Self> {
        match data_type.to_ascii_lowercase().as_str() {
            "uuid" => Some(SqlColumnType::Uuid),
            "character varying" | "varchar" => {
                let len = char_len.and_then(|l| if l > 0 { Some(l as usize) } else { None });
                Some(SqlColumnType::Varchar(len))
            }
            "text" => Some(SqlColumnType::Text),
            "smallint" => Some(SqlColumnType::SmallInt),
            "integer" => Some(SqlColumnType::Integer),
            "bigint" => Some(SqlColumnType::BigInt),
            "numeric" | "decimal" => {
                let p = numeric_precision.unwrap_or(10) as u8;
                let s = numeric_scale.unwrap_or(2) as u8;
                Some(SqlColumnType::Decimal(p, s))
            }
            "boolean" => Some(SqlColumnType::Boolean),
            "date" => Some(SqlColumnType::Date),
            "timestamp with time zone" | "timestamptz" => Some(SqlColumnType::Timestamptz),
            "jsonb" => Some(SqlColumnType::Jsonb),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_definition_borrow_lookup() {
        let mut schema = DatabaseSchema::new();
        let mut table = TableDefinition::new("articles", TableKind::Entity, false);
        let col = ColumnDefinition::new("title", SqlColumnType::Text, false, false);
        table.columns.insert(col);

        schema.insert_table(table);

        // Lookup with &str without allocating String
        let found = schema.find_table("articles");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "articles");
        assert_eq!(found.unwrap().kind, TableKind::Entity);

        let col_found = found.unwrap().find_column("title");
        assert!(col_found.is_some());
        assert_eq!(col_found.unwrap().name, "title");
        assert_eq!(col_found.unwrap().data_type, SqlColumnType::Text);

        assert!(schema.find_table("non_existent").is_none());
    }

    #[test]
    fn test_index_definition_borrow_lookup() {
        let mut table = TableDefinition::new("articles", TableKind::Entity, false);
        let idx = IndexDefinition::new(
            "idx_articles_title",
            "articles",
            vec!["title".into()],
            false,
        );
        table.indexes.insert(idx);

        assert!(table.find_index("idx_articles_title").is_some());
        assert!(table.find_index("unknown").is_none());
    }

    #[test]
    fn test_foreign_key_definition_borrow_lookup() {
        let mut table = TableDefinition::new("articles__published", TableKind::Published, false);
        let fk = ForeignKeyDefinition::new(
            "fk_articles__published_id",
            vec!["id".into()],
            "articles",
            vec!["id".into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        );
        table.foreign_keys.insert(fk);

        assert!(
            table
                .find_foreign_key("fk_articles__published_id")
                .is_some()
        );
        assert!(table.find_foreign_key("unknown").is_none());
    }

    #[test]
    fn test_sql_type_formatting() {
        assert_eq!(SqlColumnType::Uuid.to_sql_string(), "UUID");
        assert_eq!(
            SqlColumnType::Varchar(Some(255)).to_sql_string(),
            "VARCHAR(255)"
        );
        assert_eq!(SqlColumnType::Text.to_sql_string(), "TEXT");
        assert_eq!(SqlColumnType::SmallInt.to_sql_string(), "SMALLINT");
        assert_eq!(SqlColumnType::Integer.to_sql_string(), "INTEGER");
        assert_eq!(SqlColumnType::BigInt.to_sql_string(), "BIGINT");
        assert_eq!(
            SqlColumnType::Decimal(10, 2).to_sql_string(),
            "NUMERIC(10, 2)"
        );
        assert_eq!(SqlColumnType::Boolean.to_sql_string(), "BOOLEAN");
        assert_eq!(SqlColumnType::Date.to_sql_string(), "DATE");
        assert_eq!(SqlColumnType::Timestamptz.to_sql_string(), "TIMESTAMPTZ");
        assert_eq!(SqlColumnType::Jsonb.to_sql_string(), "JSONB");
    }

    #[test]
    fn test_from_information_schema() {
        assert_eq!(
            SqlColumnType::from_information_schema("uuid", None, None, None),
            Some(SqlColumnType::Uuid)
        );
        assert_eq!(
            SqlColumnType::from_information_schema("character varying", Some(255), None, None),
            Some(SqlColumnType::Varchar(Some(255)))
        );
        assert_eq!(
            SqlColumnType::from_information_schema("numeric", None, Some(10), Some(2)),
            Some(SqlColumnType::Decimal(10, 2))
        );
        assert_eq!(
            SqlColumnType::from_information_schema("timestamp with time zone", None, None, None),
            Some(SqlColumnType::Timestamptz)
        );
    }
}
