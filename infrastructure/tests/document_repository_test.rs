//! Integration tests for `SqlxDocumentInstanceRepository`.
//!
//! Validates:
//! 1. CRUD operations for dynamic document instances
//! 2. Two-Table publication lifecycle (draft table & published mirror table)
//! 3. SingleType singleton row invariant
//! 4. Dual link tables & relation saving (draft and published link tables)
//! 5. Two-phase batch relation loading (`fetch_relations`) for owner and inverse sides
//! 6. Query filtering and pagination

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use domain::entities::document_instance::{
    AuditTrail, DocumentContent, DocumentInstance, PublicationState, ResolvedRelation,
};
use domain::ports::DocumentInstanceRepository;
use domain::ports::document_instance_repository::{FieldFilter, Pagination};
use domain::types::content_value::ContentValue;
use domain::types::domain_value::DomainValue;
use domain::types::primitive_value::PrimitiveValue;
use domain::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId, Email, UserId};
use infrastructure::migrations::run_migrations;
use infrastructure::repositories::SqlxDocumentInstanceRepository;
use infrastructure::schema_loader::{SafetyPolicy, load_schema_registry, sync_schemas};
use sqlx::PgPool;
use uuid::Uuid;

async fn get_test_pool() -> Option<PgPool> {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_default();

    if db_url.trim().is_empty() {
        return None;
    }

    let pool = PgPool::connect(&db_url).await.ok()?;
    run_migrations(&pool).await.ok()?;
    Some(pool)
}

fn setup_sample_schema_dir(temp_dir: &Path) {
    let doc_types_dir = temp_dir.join("document-types");
    let relations_dir = temp_dir.join("relations");
    fs::create_dir_all(&doc_types_dir).unwrap();
    fs::create_dir_all(&relations_dir).unwrap();

    // 1. article.json (Collection with Draft & Publish)
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
                "required": true
            },
            "slug": {
                "type": "uid",
                "required": true,
                "unique": true
            },
            "views": {
                "type": { "integer": "int64" },
                "required": false
            }
        }
    }"#;
    fs::write(doc_types_dir.join("article.json"), article_json).unwrap();

    // 2. author.json (Collection without Draft & Publish)
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
        "locales": ["en", "uk"],
        "defaultLocale": "en"
    }"#;
    fs::write(temp_dir.join("system-config.json"), config_json).unwrap();
}

#[tokio::test]
async fn test_document_instance_repository_lifecycle() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_document_instance_repository_lifecycle: DATABASE_URL not set");
            return;
        }
    };

    let temp_dir = std::env::temp_dir().join(format!("luminair_repo_test_{}", Uuid::now_v7()));
    setup_sample_schema_dir(&temp_dir);

    // Sync schema to DB
    let _sync_res = sync_schemas(&pool, &temp_dir, SafetyPolicy::AllowDestructive)
        .await
        .expect("sync schemas failed");

    let (registry, _config) = load_schema_registry(&temp_dir).expect("load registry failed");
    let repo = SqlxDocumentInstanceRepository::new(pool.clone(), Arc::new(registry));

    let author_type = DocumentTypeId::try_new("author").unwrap();
    let article_type = DocumentTypeId::try_new("article").unwrap();
    let setting_type = DocumentTypeId::try_new("site-setting").unwrap();

    // 1. Create and save an Author instance
    let author_id = DocumentInstanceId::new(Uuid::now_v7());
    let mut author_fields = HashMap::new();
    author_fields.insert(
        AttributeId::try_new("name").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Alice Writer".to_string(),
        ))),
    );
    author_fields.insert(
        AttributeId::try_new("email").unwrap(),
        ContentValue::Scalar(DomainValue::Email(
            Email::try_new("alice@example.com").unwrap(),
        )),
    );

    let author = DocumentInstance {
        id: author_id,
        db_row_id: Some(author_id),
        document_type_id: author_type.clone(),
        content: DocumentContent {
            fields: author_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        },
        relations: HashMap::new(),
        populated_relations: HashMap::new(),
        audit: AuditTrail {
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: None,
            updated_by: None,
            version: 1,
        },
    };

    repo.save(&author).await.expect("save author failed");

    // Verify Author saved
    let fetched_author = repo
        .find_by_id(author_type.clone(), author_id)
        .await
        .expect("fetch author query failed")
        .expect("author should exist");
    assert_eq!(fetched_author.id, author_id);
    assert_eq!(
        fetched_author
            .content
            .fields
            .get(&AttributeId::try_new("name").unwrap()),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Alice Writer".to_string())
        )))
    );

    // 2. Create and save an Article instance in Draft mode, with relation to Author
    let article_id = DocumentInstanceId::new(Uuid::now_v7());
    let mut article_fields = HashMap::new();
    article_fields.insert(
        AttributeId::try_new("title").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "First Post".to_string(),
        ))),
    );
    article_fields.insert(
        AttributeId::try_new("slug").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uid(
            "first-post".to_string(),
        ))),
    );
    article_fields.insert(
        AttributeId::try_new("views").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100))),
    );

    let mut article_relations = HashMap::new();
    let author_attr = AttributeId::try_new("author").unwrap();
    article_relations.insert(
        author_attr.clone(),
        vec![ResolvedRelation {
            attribute_id: author_attr.clone(),
            target_instance_id: author_id,
        }],
    );

    let article = DocumentInstance {
        id: article_id,
        db_row_id: Some(article_id),
        document_type_id: article_type.clone(),
        content: DocumentContent {
            fields: article_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        },
        relations: article_relations,
        populated_relations: HashMap::new(),
        audit: AuditTrail {
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: None,
            updated_by: None,
            version: 1,
        },
    };

    repo.save(&article).await.expect("save article failed");

    // 3. Verify Article find_by_id & relations
    let fetched_article = repo
        .find_by_id(article_type.clone(), article_id)
        .await
        .expect("fetch article query failed")
        .expect("article should exist");
    assert_eq!(fetched_article.id, article_id);
    assert_eq!(
        fetched_article.content.publication_state,
        PublicationState::Draft {
            last_published_revision: None
        }
    );
    assert_eq!(
        fetched_article.relations.get(&author_attr).unwrap().len(),
        1
    );
    assert_eq!(
        fetched_article.relations.get(&author_attr).unwrap()[0].target_instance_id,
        author_id
    );

    // 4. Test count & exists_for_type
    let count = repo
        .count(article_type.clone(), vec![])
        .await
        .expect("count failed");
    assert_eq!(count, 1);
    let exists = repo
        .exists_for_type(article_type.clone())
        .await
        .expect("exists failed");
    assert!(exists);

    // 5. Test find_by_type with FieldFilter and Pagination
    let slug_filter = FieldFilter {
        attribute_id: AttributeId::try_new("slug").unwrap(),
        value: DomainValue::Primitive(PrimitiveValue::Uid("first-post".to_string())),
    };
    let page = repo
        .find_by_type(
            article_type.clone(),
            Pagination::default(),
            vec![slug_filter],
        )
        .await
        .expect("find_by_type failed");
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, article_id);

    // 6. Test Two-Phase Batch Relation Loading (fetch_relations)
    // Owner side: article -> author
    let rel_map = repo
        .fetch_relations(
            article_type.clone(),
            std::slice::from_ref(&author_attr),
            &[article_id],
        )
        .await
        .expect("fetch_relations owner side failed");
    let authors = rel_map.get(&author_attr).unwrap().get(&article_id).unwrap();
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0].id, author_id);

    // Inverse side: author -> articles
    let articles_attr = AttributeId::try_new("articles").unwrap();
    let inv_rel_map = repo
        .fetch_relations(
            author_type.clone(),
            std::slice::from_ref(&articles_attr),
            &[author_id],
        )
        .await
        .expect("fetch_relations inverse side failed");
    let articles = inv_rel_map
        .get(&articles_attr)
        .unwrap()
        .get(&author_id)
        .unwrap();
    assert_eq!(articles.len(), 1);
    assert_eq!(articles[0].id, article_id);

    // 7. Test Publication: Save as Published
    let mut pub_article = article.clone();
    let published_by_user = UserId::try_new("user-admin-1").unwrap();
    pub_article.content.publication_state = PublicationState::Published {
        revision: 1,
        published_at: Utc::now(),
        published_by: Some(published_by_user.clone()),
    };
    repo.save(&pub_article)
        .await
        .expect("save published article failed");

    let fetched_pub = repo
        .find_by_id(article_type.clone(), article_id)
        .await
        .expect("fetch published article failed")
        .expect("article should exist");
    match fetched_pub.content.publication_state {
        PublicationState::Published {
            revision,
            published_by,
            ..
        } => {
            assert_eq!(revision, 1);
            assert_eq!(published_by, Some(published_by_user));
        }
        other => panic!("Expected PublicationState::Published, got: {other:?}"),
    }

    // 8. Test SingleType: site-setting singleton row invariant
    let setting_id = DocumentInstanceId::new(Uuid::now_v7());
    let mut setting_fields = HashMap::new();
    setting_fields.insert(
        AttributeId::try_new("site-name").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "My CMS Site".to_string(),
        ))),
    );
    let setting = DocumentInstance {
        id: setting_id,
        db_row_id: Some(setting_id),
        document_type_id: setting_type.clone(),
        content: DocumentContent {
            fields: setting_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        },
        relations: HashMap::new(),
        populated_relations: HashMap::new(),
        audit: AuditTrail {
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: None,
            updated_by: None,
            version: 1,
        },
    };
    repo.save(&setting).await.expect("save setting failed");

    let count_settings = repo
        .count(setting_type.clone(), vec![])
        .await
        .expect("count setting failed");
    assert_eq!(count_settings, 1);

    // Save second instance for SingleType with different ID -> must update the existing singleton row
    let setting_id_2 = DocumentInstanceId::new(Uuid::now_v7());
    let mut setting_fields_2 = HashMap::new();
    setting_fields_2.insert(
        AttributeId::try_new("site-name").unwrap(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Updated CMS Site".to_string(),
        ))),
    );
    let setting_2 = DocumentInstance {
        id: setting_id_2,
        db_row_id: Some(setting_id_2),
        document_type_id: setting_type.clone(),
        content: DocumentContent {
            fields: setting_fields_2,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        },
        relations: HashMap::new(),
        populated_relations: HashMap::new(),
        audit: AuditTrail {
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: None,
            updated_by: None,
            version: 1,
        },
    };
    repo.save(&setting_2).await.expect("update setting failed");

    let count_settings_after = repo
        .count(setting_type.clone(), vec![])
        .await
        .expect("count setting failed");
    assert_eq!(count_settings_after, 1);

    // 9. Test Delete
    repo.delete(article_type.clone(), article_id)
        .await
        .expect("delete article failed");
    let after_delete = repo
        .find_by_id(article_type.clone(), article_id)
        .await
        .expect("fetch after delete failed");
    assert!(after_delete.is_none());
    assert_eq!(repo.count(article_type.clone(), vec![]).await.unwrap(), 0);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}
