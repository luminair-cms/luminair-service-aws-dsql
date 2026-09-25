//! Integration and End-to-End Tests for Dynamic Schema Loader and DDL.
//!
//! Validates:
//! 1. Declarative schema loading from disk (`schema/document-types/`, `schema/relations/`, `schema/system-config.json`)
//! 2. File name validation invariant (file name == singularName + .json)
//! 3. Conversion to domain `SchemaRegistry` and target `DatabaseSchema` AST
//! 4. Topological migration planning and idempotency
//! 5. Database execution and drift detection (when PostgreSQL / DSQL instance is available)

use infrastructure::run_migrations;
use infrastructure::schema_loader::{
    DatabaseSchema, SafetyPolicy, SchemaLoaderError, build_desired_schema, compute_diff,
    load_schema_registry, plan_migrations, step_to_sql, sync_schemas,
};
use sqlx::PgPool;
use std::fs;
use std::path::Path;

fn setup_sample_schema_dir(temp_dir: &Path) {
    let doc_types_dir = temp_dir.join("document-types");
    let relations_dir = temp_dir.join("relations");
    fs::create_dir_all(&doc_types_dir).unwrap();
    fs::create_dir_all(&relations_dir).unwrap();

    // 1. article.json
    let article_json = r#"{
        "kind": "collection",
        "info": {
            "displayName": "Article",
            "singularName": "article",
            "pluralName": "articles",
            "description": "Blog articles"
        },
        "options": {
            "draftAndPublish": true
        },
        "attributes": {
            "title": {
                "type": "text",
                "required": true,
                "constraints": [
                    { "minLength": 5 },
                    { "maxLength": 150 }
                ]
            },
            "slug": {
                "type": "uid",
                "required": true,
                "unique": true,
                "constraints": [
                    { "pattern": "^[a-z0-9-]+$" }
                ]
            },
            "views": {
                "type": { "integer": "int64" },
                "required": false,
                "constraints": [
                    { "min": 0 }
                ]
            },
            "content": {
                "type": "localizedText",
                "required": false,
                "constraints": [
                    { "minLength": 10 }
                ]
            }
        }
    }"#;
    fs::write(doc_types_dir.join("article.json"), article_json).unwrap();

    // 2. author.json
    let author_json = r#"{
        "kind": "collection",
        "info": {
            "displayName": "Author",
            "singularName": "author",
            "pluralName": "authors"
        },
        "options": {
            "draftAndPublish": false
        },
        "attributes": {
            "name": {
                "type": "text",
                "required": true
            },
            "email": {
                "type": "email",
                "required": true,
                "unique": true
            }
        }
    }"#;
    fs::write(doc_types_dir.join("author.json"), author_json).unwrap();

    // 3. site-setting.json (SingleType)
    let site_setting_json = r#"{
        "kind": "singleType",
        "info": {
            "displayName": "Site Setting",
            "singularName": "site-setting",
            "pluralName": "site-settings"
        },
        "options": {
            "draftAndPublish": false
        },
        "attributes": {
            "site-name": {
                "type": "text",
                "required": true
            },
            "support-email": {
                "type": "email",
                "required": false
            }
        }
    }"#;
    fs::write(doc_types_dir.join("site-setting.json"), site_setting_json).unwrap();

    // 4. article-author.json relation (1:1 / N:1)
    let relation_json = r#"{
        "ownerType": "article",
        "ownerAttr": "author",
        "ownerKind": "hasOne",
        "targetType": "author",
        "inverse": {
            "inverseAttr": "articles"
        }
    }"#;
    fs::write(relations_dir.join("article-author.json"), relation_json).unwrap();

    // 5. system-config.json
    let config_json = r#"{
        "locales": ["en", "uk", "de"],
        "defaultLocale": "en"
    }"#;
    fs::write(temp_dir.join("system-config.json"), config_json).unwrap();
}

#[test]
fn test_load_schema_registry_from_directory() {
    let temp_dir = std::env::temp_dir().join(format!("luminair_test_{}", uuid::Uuid::now_v7()));
    setup_sample_schema_dir(&temp_dir);

    let (registry, config) = load_schema_registry(&temp_dir).unwrap();

    // Verify document types
    assert!(registry.find_type_by_name("articles").is_some());
    assert!(registry.find_type_by_name("authors").is_some());
    assert!(registry.find_type_by_name("site-settings").is_some());

    // Verify system config
    assert_eq!(config.default_locale.as_ref(), "en");
    assert_eq!(config.available_locales.len(), 3);

    // Verify relations
    let article_id = registry.find_type_by_name("articles").unwrap().id.clone();
    let rels = registry.find_relations_for(&article_id);
    assert_eq!(rels.len(), 1);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_file_name_must_equal_singular_name_or_fail() {
    let temp_dir =
        std::env::temp_dir().join(format!("luminair_test_mismatch_{}", uuid::Uuid::now_v7()));
    let doc_types_dir = temp_dir.join("document-types");
    fs::create_dir_all(&doc_types_dir).unwrap();

    // File name is "post.json" but singularName is "article"
    let mismatched = r#"{
        "kind": "collection",
        "info": {
            "displayName": "Article",
            "singularName": "article",
            "pluralName": "articles"
        },
        "attributes": {}
    }"#;
    fs::write(doc_types_dir.join("post.json"), mismatched).unwrap();

    let res = load_schema_registry(&temp_dir);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        matches!(err, SchemaLoaderError::FileNameMismatch { .. }),
        "expected FileNameMismatch but got: {:?}",
        err
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_build_desired_schema_ast_and_diff() {
    let temp_dir = std::env::temp_dir().join(format!("luminair_test_ast_{}", uuid::Uuid::now_v7()));
    setup_sample_schema_dir(&temp_dir);

    let (registry, _) = load_schema_registry(&temp_dir).unwrap();
    let desired = build_desired_schema(&registry);

    // Expected tables: articles, articles__published, authors, site_setting, articles__author_link, articles__author_link__published
    assert!(desired.find_table("articles").is_some());
    assert!(desired.find_table("articles__published").is_some());
    assert!(desired.find_table("authors").is_some());
    assert!(desired.find_table("site_setting").is_some());
    assert!(desired.find_table("articles__author_link").is_some());
    assert!(
        desired
            .find_table("articles__author_link__published")
            .is_some()
    );

    // SingleType and collection without draftAndPublish do not generate published tables
    assert!(desired.find_table("authors__published").is_none());
    assert!(desired.find_table("site_setting__published").is_none());

    let articles = desired.find_table("articles").unwrap();
    assert!(articles.find_column("id").is_some());
    assert!(articles.find_column("title").is_some());
    assert!(articles.find_column("slug").is_some());
    assert!(articles.find_column("views").is_some());
    assert!(articles.find_column("content").is_some());
    // In the new universal link table design, relation columns are NOT added to entity tables
    assert!(articles.find_column("author_id").is_none());

    // Published mirror table assertions
    let published = desired.find_table("articles__published").unwrap();
    assert!(published.find_column("id").is_some());
    assert!(published.find_column("published_version").is_some());
    assert!(published.find_column("title").is_some());
    assert!(published.find_column("slug").is_some());
    assert!(
        published
            .find_foreign_key("fk_articles__published_id")
            .is_some()
    );

    // Draft link table assertions
    let link = desired.find_table("articles__author_link").unwrap();
    assert!(link.is_junction());
    assert!(link.find_column("owner_id").is_some());
    assert!(link.find_column("target_id").is_some());
    assert!(
        link.find_foreign_key("fk_articles__author_link_owner")
            .is_some()
    );
    assert!(
        link.find_foreign_key("fk_articles__author_link_target")
            .is_some()
    );
    // HasOne has unique index on owner_id
    assert!(link.find_index("uq_articles__author_link_owner").is_some());
    assert!(
        link.find_index("uq_articles__author_link_owner")
            .unwrap()
            .unique
    );
    assert!(
        link.find_index("idx_articles__author_link_target")
            .is_some()
    );

    // Published link table assertions (Dual Link Tables with Variant 1 Public Filter Principle)
    let pub_link = desired
        .find_table("articles__author_link__published")
        .unwrap();
    assert!(pub_link.is_junction());
    assert!(pub_link.find_column("owner_id").is_some());
    assert!(pub_link.find_column("target_id").is_some());
    let pub_owner_fk = pub_link
        .find_foreign_key("fk_articles__author_link__published_owner")
        .unwrap();
    assert_eq!(pub_owner_fk.referenced_table, "articles__published");
    let pub_target_fk = pub_link
        .find_foreign_key("fk_articles__author_link__published_target")
        .unwrap();
    // Author has draftAndPublish: false -> points to base authors table
    assert_eq!(pub_target_fk.referenced_table, "authors");
    assert!(
        pub_link
            .find_index("uq_articles__author_link__published_owner")
            .is_some()
    );
    assert!(
        pub_link
            .find_index("uq_articles__author_link__published_owner")
            .unwrap()
            .unique
    );
    assert!(
        pub_link
            .find_index("idx_articles__author_link__published_target")
            .is_some()
    );

    // SingleType singleton invariant
    let site_setting = desired.find_table("site_setting").unwrap();
    assert!(site_setting.is_singleton);
    assert!(site_setting.find_column("_singleton").is_some());

    // Empty actual schema -> diff produces 6 CreateTable + indexes
    let actual = DatabaseSchema::new();
    let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
    let plan = plan_migrations(steps);

    assert_eq!(plan.steps.len(), 6);
    for step in &plan.steps {
        let sql = step_to_sql(step);
        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS"#));
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_sync_schemas_end_to_end_with_live_db_if_available() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_default();

    if database_url.is_empty() {
        eprintln!("Skipping live database test: TEST_DATABASE_URL not set");
        return;
    }

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping live DB test: failed to connect: {e}");
            return;
        }
    };

    // 1. Run static migrations
    run_migrations(&pool)
        .await
        .expect("static migrations failed");

    // 2. Setup schema directory
    let temp_dir =
        std::env::temp_dir().join(format!("luminair_test_live_{}", uuid::Uuid::now_v7()));
    setup_sample_schema_dir(&temp_dir);

    // 3. First sync: creates dynamic tables
    let result = sync_schemas(&pool, &temp_dir, SafetyPolicy::AdditiveOnly)
        .await
        .expect("sync_schemas failed");

    assert!(!result.executed_statements.is_empty());
    println!(
        "Executed {} DDL statements",
        result.executed_statements.len()
    );

    // 4. Second sync: must be a no-op (drift = 0, idempotent)
    let second_result = sync_schemas(&pool, &temp_dir, SafetyPolicy::AdditiveOnly)
        .await
        .expect("second sync_schemas failed");

    assert_eq!(
        second_result.executed_statements.len(),
        0,
        "expected 0 statements on second sync but got {:?}",
        second_result.executed_statements
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}
