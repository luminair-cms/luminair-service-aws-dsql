//! Centralized database identifier naming conventions and providers.

pub mod columns;
pub mod provider;
pub mod sanitize;

pub use columns::{BaseSystemColumn, LinkColumn, PublishedSystemColumn};
pub use provider::{DocumentTableNaming, LinkTableNaming};
pub use sanitize::{RESERVED_SQL_KEYWORDS, is_reserved_sql_keyword, kebab_to_snake};

use domain::schema::{AttributeId, DocumentKind, DocumentType};

/// Derives the physical database table name for a `DocumentType`.
pub fn document_type_to_table_name(doc_type: &DocumentType) -> String {
    DocumentTableNaming::new(doc_type).table_name().to_string()
}

/// Derives the database table name given kind and names.
pub fn type_names_to_table_name(
    kind: DocumentKind,
    singular_name: &str,
    plural_name: &str,
) -> String {
    DocumentTableNaming::from_parts(kind, singular_name, plural_name, false)
        .table_name()
        .to_string()
}

/// Derives the physical database table name for a published mirror table.
pub fn published_table_name(table_name: &str) -> String {
    format!("{table_name}__published")
}

/// Derives the physical database table name for a published mirror link table.
pub fn published_link_table_name(link_table: &str) -> String {
    format!("{link_table}__published")
}

/// Derives the column name for an `AttributeId`.
pub fn attribute_to_column_name(attr: &AttributeId) -> String {
    kebab_to_snake(attr.as_ref())
}

/// Derives the foreign key column name for an attribute.
pub fn foreign_key_column_name(attr: &AttributeId) -> String {
    format!("{}_id", kebab_to_snake(attr.as_ref()))
}

/// Derives the universal link table name for any relation attribute.
pub fn link_table_name(owner_table: &str, owner_attr: &AttributeId) -> String {
    LinkTableNaming::new(owner_table, owner_attr)
        .table_name()
        .to_string()
}

/// Alias for `link_table_name` for backward compatibility.
pub fn junction_table_name(owner_table: &str, owner_attr: &AttributeId) -> String {
    link_table_name(owner_table, owner_attr)
}

/// Derives a standard B-tree index name for a table and column.
pub fn index_name(table: &str, column: &str) -> String {
    format!("idx_{table}_{column}")
}

/// Derives the owner unique index name for a HasOne link table.
pub fn link_owner_unique_index_name(link_table: &str) -> String {
    LinkTableNaming::from_table_name(link_table).owner_unique_index_name()
}

/// Derives the target column index name for a link table.
pub fn link_target_index_name(link_table: &str) -> String {
    LinkTableNaming::from_table_name(link_table).target_index_name()
}

/// Alias for `link_target_index_name` for backward compatibility.
pub fn junction_target_index_name(link_table: &str) -> String {
    link_target_index_name(link_table)
}

/// Derives the foreign key constraint name for a published mirror table referencing the base table.
pub fn published_fk_name(published_table: &str) -> String {
    format!("fk_{published_table}_id")
}

/// Derives the owner foreign key constraint name for a link table referencing the owner table.
pub fn link_owner_fk_name(link_table: &str) -> String {
    LinkTableNaming::from_table_name(link_table).link_owner_fk_name()
}

/// Derives the target foreign key constraint name for a link table referencing the target table.
pub fn link_target_fk_name(link_table: &str) -> String {
    LinkTableNaming::from_table_name(link_table).link_target_fk_name()
}
