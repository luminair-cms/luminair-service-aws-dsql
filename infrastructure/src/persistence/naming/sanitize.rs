//! Sanitization and validation utilities for database identifiers.

/// List of reserved SQL and PostgreSQL keywords that cannot be used as unquoted table or column identifiers.
pub const RESERVED_SQL_KEYWORDS: &[&str] = &[
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kebab_to_snake() {
        assert_eq!(kebab_to_snake("blog-post"), "blog_post");
        assert_eq!(kebab_to_snake("single"), "single");
        assert_eq!(kebab_to_snake("user-profile-avatar"), "user_profile_avatar");
    }

    #[test]
    fn test_reserved_keywords() {
        assert!(is_reserved_sql_keyword("user"));
        assert!(is_reserved_sql_keyword("USER"));
        assert!(is_reserved_sql_keyword("select"));
        assert!(!is_reserved_sql_keyword("article"));
        assert!(!is_reserved_sql_keyword("title"));
    }
}
