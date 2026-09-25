//! Centralized naming derivations for database identifiers.
//!
//! Enforces ADR-008 and ADR-009 naming conventions:
//! - Collection table: plural name in snake_case (e.g., `articles`, `blog_posts`)
//! - SingleType table: singular name in snake_case (e.g., `site_setting`)
//! - Attribute columns: attribute ID in snake_case (e.g., `hero_image`)
//! - Foreign key columns: `{attribute}_id` in snake_case
//! - Junction tables: `{owner_table}__{owner_attr}` with double underscore `__`
//! - Indexes: `idx_{table}_{column}` and `idx_{junction_table}_target`

use domain::entities::document_type::{DocumentKind, DocumentType};
use domain::value_objects::AttributeId;

/// List of reserved SQL and PostgreSQL keywords that cannot be used as unquoted table or column identifiers.
const RESERVED_SQL_KEYWORDS: &[&str] = &[
    "all",
    "alter",
    "and",
    "as",
    "asc",
    "begin",
    "by",
    "case",
    "check",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "current_date",
    "current_time",
    "current_timestamp",
    "default",
    "delete",
    "desc",
    "distinct",
    "do",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "false",
    "fetch",
    "for",
    "foreign",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "left",
    "like",
    "limit",
    "natural",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "revoke",
    "right",
    "rollback",
    "select",
    "table",
    "then",
    "to",
    "true",
    "union",
    "unique",
    "update",
    "user",
    "values",
    "when",
    "where",
    "with",
];

/// Checks if an identifier is a reserved SQL/PostgreSQL keyword (case-insensitive).
pub fn is_reserved_sql_keyword(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    RESERVED_SQL_KEYWORDS.contains(&lower.as_str())
}

/// Converts a kebab-case identifier to snake_case.
/// E.g. `blog-post` -> `blog_post`, `hero-image-url` -> `hero_image_url`.
pub fn kebab_to_snake(s: &str) -> String {
    s.replace('-', "_")
}

/// Derives the physical database table name for a `DocumentType`.
///
/// - `DocumentKind::Collection` -> plural name in snake_case (e.g. `blog_articles`)
/// - `DocumentKind::SingleType` -> singular name in snake_case (e.g. `site_setting`)
pub fn document_type_to_table_name(doc_type: &DocumentType) -> String {
    match doc_type.kind {
        DocumentKind::Collection => kebab_to_snake(&doc_type.info.plural_name),
        DocumentKind::SingleType => kebab_to_snake(&doc_type.info.singular_name),
    }
}

/// Derives the database table name given kind and names.
pub fn type_names_to_table_name(
    kind: DocumentKind,
    singular_name: &str,
    plural_name: &str,
) -> String {
    match kind {
        DocumentKind::Collection => kebab_to_snake(plural_name),
        DocumentKind::SingleType => kebab_to_snake(singular_name),
    }
}

/// Derives the column name for an `AttributeId`.
pub fn attribute_to_column_name(attr: &AttributeId) -> String {
    kebab_to_snake(attr.as_ref())
}

/// Derives the foreign key column name for a 1:1 or N:1 relation attribute.
/// E.g. `author` -> `author_id`, `parent-category` -> `parent_category_id`.
pub fn foreign_key_column_name(attr: &AttributeId) -> String {
    format!("{}_id", kebab_to_snake(attr.as_ref()))
}

/// Derives the junction table name for an N:N relation using the double-underscore `__` separator.
/// E.g. `articles` and `tags` -> `articles__tags`.
pub fn junction_table_name(owner_table: &str, owner_attr: &AttributeId) -> String {
    format!("{}__{}", owner_table, kebab_to_snake(owner_attr.as_ref()))
}

/// Derives a standard B-tree index name for a table and column.
/// E.g. `idx_articles_slug`
pub fn index_name(table: &str, column: &str) -> String {
    format!("idx_{table}_{column}")
}

/// Derives the target column index name for a junction table.
/// E.g. `idx_articles__tags_target`
pub fn junction_target_index_name(junction_table: &str) -> String {
    format!("idx_{junction_table}_target")
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::document_type::{DocumentTypeInfo, DocumentTypeOptions};
    use domain::value_objects::DocumentTypeId;
    use indexmap::IndexMap;

    #[test]
    fn test_kebab_to_snake() {
        assert_eq!(kebab_to_snake("blog-post"), "blog_post");
        assert_eq!(kebab_to_snake("single"), "single");
        assert_eq!(kebab_to_snake("user-profile-avatar"), "user_profile_avatar");
    }

    #[test]
    fn test_document_type_to_table_name_collection() {
        let dt = DocumentType {
            id: DocumentTypeId::try_new("blog-post").unwrap(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Blog Post".into(),
                singular_name: "blog-post".into(),
                plural_name: "blog-posts".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: IndexMap::new(),
        };
        assert_eq!(document_type_to_table_name(&dt), "blog_posts");
    }

    #[test]
    fn test_document_type_to_table_name_singletype() {
        let dt = DocumentType {
            id: DocumentTypeId::try_new("site-setting").unwrap(),
            kind: DocumentKind::SingleType,
            info: DocumentTypeInfo {
                title: "Site Setting".into(),
                singular_name: "site-setting".into(),
                plural_name: "site-settings".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: IndexMap::new(),
        };
        assert_eq!(document_type_to_table_name(&dt), "site_setting");
    }

    #[test]
    fn test_attribute_to_column_name() {
        let attr = AttributeId::try_new("hero-image-url").unwrap();
        assert_eq!(attribute_to_column_name(&attr), "hero_image_url");
    }

    #[test]
    fn test_foreign_key_column_name() {
        let attr = AttributeId::try_new("author-profile").unwrap();
        assert_eq!(foreign_key_column_name(&attr), "author_profile_id");
    }

    #[test]
    fn test_junction_table_name() {
        let attr = AttributeId::try_new("tagged-categories").unwrap();
        assert_eq!(
            junction_table_name("articles", &attr),
            "articles__tagged_categories"
        );
    }

    #[test]
    fn test_index_names() {
        assert_eq!(index_name("articles", "slug"), "idx_articles_slug");
        assert_eq!(
            junction_target_index_name("articles__tags"),
            "idx_articles__tags_target"
        );
    }

    #[test]
    fn test_reserved_sql_keywords() {
        assert!(is_reserved_sql_keyword("user"));
        assert!(is_reserved_sql_keyword("USER"));
        assert!(is_reserved_sql_keyword("table"));
        assert!(is_reserved_sql_keyword("select"));
        assert!(is_reserved_sql_keyword("order"));
        assert!(!is_reserved_sql_keyword("articles"));
        assert!(!is_reserved_sql_keyword("author_name"));
    }
}
