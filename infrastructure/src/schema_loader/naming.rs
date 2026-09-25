//! Centralized naming derivations for database identifiers.
//!
//! Enforces ADR-008 and ADR-009 naming conventions:
//! - Collection table: plural name in snake_case (e.g., `articles`, `blog_posts`)
//! - SingleType table: singular name in snake_case (e.g., `site_setting`)
//! - Published mirror table: `{table}__published` (e.g., `articles__published`)
//! - Attribute columns: attribute ID in snake_case (e.g., `hero_image`)
//! - Universal link tables: `{owner_table}__{owner_attr}_link` (e.g., `articles__tags_link`)
//! - Indexes: `idx_{table}_{column}`, `uq_{link}_owner`, and `idx_{link}_target`
//! - Foreign keys: `fk_{published}_id`, `fk_{link}_owner`, `fk_{link}_target`

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

/// Derives the physical database table name for a published mirror table.
/// E.g. `articles` -> `articles__published`, `site_setting` -> `site_setting__published`.
pub fn published_table_name(table_name: &str) -> String {
    format!("{table_name}__published")
}

/// Derives the physical database table name for a published mirror link table.
/// E.g. `articles__tags_link` -> `articles__tags_link__published`.
pub fn published_link_table_name(link_table: &str) -> String {
    format!("{link_table}__published")
}

/// Derives the column name for an `AttributeId`.
pub fn attribute_to_column_name(attr: &AttributeId) -> String {
    kebab_to_snake(attr.as_ref())
}

/// Derives the foreign key column name for an attribute.
/// E.g. `author` -> `author_id`, `parent-category` -> `parent_category_id`.
pub fn foreign_key_column_name(attr: &AttributeId) -> String {
    format!("{}_id", kebab_to_snake(attr.as_ref()))
}

/// Derives the universal link table name for any relation attribute using the double-underscore `__` separator and `_link` suffix.
/// E.g. `articles` and `author` -> `articles__author_link`.
/// E.g. `articles` and `tags` -> `articles__tags_link`.
pub fn link_table_name(owner_table: &str, owner_attr: &AttributeId) -> String {
    format!(
        "{}__{}_link",
        owner_table,
        kebab_to_snake(owner_attr.as_ref())
    )
}

/// Alias for `link_table_name` for backward compatibility.
pub fn junction_table_name(owner_table: &str, owner_attr: &AttributeId) -> String {
    link_table_name(owner_table, owner_attr)
}

/// Derives a standard B-tree index name for a table and column.
/// E.g. `idx_articles_slug`
pub fn index_name(table: &str, column: &str) -> String {
    format!("idx_{table}_{column}")
}

/// Derives the owner unique index name for a HasOne link table.
/// E.g. `uq_articles__author_link_owner`
pub fn link_owner_unique_index_name(link_table: &str) -> String {
    format!("uq_{link_table}_owner")
}

/// Derives the target column index name for a link table.
/// E.g. `idx_articles__tags_link_target`
pub fn link_target_index_name(link_table: &str) -> String {
    format!("idx_{link_table}_target")
}

/// Alias for `link_target_index_name` for backward compatibility.
pub fn junction_target_index_name(link_table: &str) -> String {
    link_target_index_name(link_table)
}

/// Derives the foreign key constraint name for a published mirror table referencing the base table.
/// E.g. `fk_articles__published_id`
pub fn published_fk_name(published_table: &str) -> String {
    format!("fk_{published_table}_id")
}

/// Derives the owner foreign key constraint name for a link table referencing the owner table.
/// E.g. `fk_articles__tags_link_owner`
pub fn link_owner_fk_name(link_table: &str) -> String {
    format!("fk_{link_table}_owner")
}

/// Derives the target foreign key constraint name for a link table referencing the target table.
/// E.g. `fk_articles__tags_link_target`
pub fn link_target_fk_name(link_table: &str) -> String {
    format!("fk_{link_table}_target")
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
    fn test_published_table_name() {
        assert_eq!(published_table_name("articles"), "articles__published");
        assert_eq!(
            published_table_name("site_setting"),
            "site_setting__published"
        );
    }

    #[test]
    fn test_published_link_table_name() {
        assert_eq!(
            published_link_table_name("articles__tags_link"),
            "articles__tags_link__published"
        );
        assert_eq!(
            link_owner_fk_name("articles__tags_link__published"),
            "fk_articles__tags_link__published_owner"
        );
        assert_eq!(
            link_target_fk_name("articles__tags_link__published"),
            "fk_articles__tags_link__published_target"
        );
        assert_eq!(
            link_owner_unique_index_name("articles__tags_link__published"),
            "uq_articles__tags_link__published_owner"
        );
        assert_eq!(
            link_target_index_name("articles__tags_link__published"),
            "idx_articles__tags_link__published_target"
        );
    }

    #[test]
    fn test_link_table_name() {
        let attr = AttributeId::try_new("tagged-categories").unwrap();
        assert_eq!(
            link_table_name("articles", &attr),
            "articles__tagged_categories_link"
        );
        assert_eq!(
            junction_table_name("articles", &attr),
            "articles__tagged_categories_link"
        );
    }

    #[test]
    fn test_index_names() {
        assert_eq!(index_name("articles", "slug"), "idx_articles_slug");
        assert_eq!(
            link_owner_unique_index_name("articles__author_link"),
            "uq_articles__author_link_owner"
        );
        assert_eq!(
            link_target_index_name("articles__tags_link"),
            "idx_articles__tags_link_target"
        );
        assert_eq!(
            junction_target_index_name("articles__tags_link"),
            "idx_articles__tags_link_target"
        );
    }

    #[test]
    fn test_foreign_key_names() {
        assert_eq!(
            published_fk_name("articles__published"),
            "fk_articles__published_id"
        );
        assert_eq!(
            link_owner_fk_name("articles__tags_link"),
            "fk_articles__tags_link_owner"
        );
        assert_eq!(
            link_target_fk_name("articles__tags_link"),
            "fk_articles__tags_link_target"
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
