//! End-to-end integration tests for REST API (Phase 6, Milestone 6).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::TestServer;
use infrastructure::api::create_router;
use infrastructure::api::dto::SingleResponse;
use infrastructure::api::state::AppState;
use infrastructure::auth::{AuthAppState, AuthConfig, Claims, SecretTokenValidator, run_bootstrap};
use infrastructure::migrations::{ROLE_EDITOR_ID, run_migrations};
use infrastructure::repositories::{SqlxAccessRequestRepository, SqlxUserRoleAssignmentRepository};
use infrastructure::schema_loader::{SafetyPolicy, load_schema_registry, sync_schemas};
use serde_json::json;
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

    // 3. homepage.json (SingleType with Draft & Publish)
    let homepage_json = r#"{
        "kind": "singleType",
        "info": {
            "displayName": "Homepage",
            "singularName": "homepage",
            "pluralName": "homepages"
        },
        "options": {
            "draftAndPublish": true
        },
        "attributes": {
            "hero-title": {
                "type": "text",
                "required": true
            }
        }
    }"#;
    fs::write(doc_types_dir.join("homepage.json"), homepage_json).unwrap();

    // 4. tag.json (Collection without Draft & Publish)
    let tag_json = r#"{
        "kind": "collection",
        "info": {
            "displayName": "Tag",
            "singularName": "tag",
            "pluralName": "tags"
        },
        "options": {
            "draftAndPublish": false
        },
        "attributes": {
            "name": {
                "type": "text",
                "required": true
            }
        }
    }"#;
    fs::write(doc_types_dir.join("tag.json"), tag_json).unwrap();

    // 5. article-author.json relation (1:1 / N:1)
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

    // 6. article-tag.json relation (N:M / HasMany)
    let tag_rel_json = r#"{
        "ownerType": "article",
        "ownerAttr": "tags",
        "ownerKind": "hasMany",
        "targetType": "tag",
        "inverse": {
            "inverseAttr": "articles"
        }
    }"#;
    fs::write(relations_dir.join("article-tag.json"), tag_rel_json).unwrap();

    // 7. system-config.json
    let config_json = r#"{
        "locales": ["en", "uk"],
        "defaultLocale": "en"
    }"#;
    fs::write(temp_dir.join("system-config.json"), config_json).unwrap();
}

struct TestContext {
    server: TestServer,
    admin_token: String,
    validator: Arc<SecretTokenValidator>,
}

async fn setup_test_server(pool: &PgPool) -> TestContext {
    let temp_dir = std::env::temp_dir().join(format!("luminair_api_test_{}", Uuid::now_v7()));
    setup_sample_schema_dir(&temp_dir);

    // Sync schema to DB
    let (schema_registry, system_config) = load_schema_registry(&temp_dir).unwrap();
    let schema_registry = Arc::new(schema_registry);
    let system_config = Arc::new(system_config);
    sync_schemas(pool, &temp_dir, SafetyPolicy::AllowDestructive)
        .await
        .unwrap();

    let secret = "test-jwt-secret-key-1234567890123";
    let validator = Arc::new(SecretTokenValidator::new(secret));

    // Bootstrap initial admin
    let admin_sub = format!("admin-{}", Uuid::now_v7());
    let auth_config = AuthConfig {
        issuer_url: None,
        audience: None,
        bootstrap_admin_sub: Some(admin_sub.clone()),
        bootstrap_auth_type: "cognito".to_string(),
    };
    let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
    let access_request_repo = SqlxAccessRequestRepository::new(pool.clone());
    run_bootstrap(pool, &auth_config, &assignment_repo, &access_request_repo)
        .await
        .unwrap();

    let admin_claims = Claims {
        sub: admin_sub,
        email: Some("admin@example.com".into()),
        name: Some("System Admin".into()),
        iss: None,
        aud: None,
        exp: Some(2500000000),
        iat: Some(1700000000),
    };
    let admin_token = validator.generate_token(&admin_claims).unwrap();

    let auth_state = AuthAppState::new(pool.clone(), validator.clone());
    let app_state = AppState::new(
        pool.clone(),
        auth_state,
        schema_registry.clone(),
        system_config.clone(),
    );

    let router = create_router(app_state);
    let server = TestServer::new(router);

    TestContext {
        server,
        admin_token,
        validator,
    }
}

#[tokio::test]
async fn test_health_probes() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_health_probes: DATABASE_URL not set");
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // GET /health
    let resp = ctx.server.get("/health").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["status"], "ok");

    // GET /ready
    let resp = ctx.server.get("/ready").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["status"], "ready");
}

#[tokio::test]
async fn test_unauthenticated_requests_rejected() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_unauthenticated_requests_rejected: DATABASE_URL not set");
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // Request without Authorization header
    let resp = ctx.server.get("/api/articles").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    let problem: serde_json::Value = resp.json();
    assert_eq!(problem["code"], "MISSING_TOKEN");
}

#[tokio::test]
async fn test_schema_introspection_endpoints() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_schema_introspection_endpoints: DATABASE_URL not set");
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // 1. List all document types
    let resp = ctx
        .server
        .get("/api/schema/document-types")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: SingleResponse<Vec<serde_json::Value>> = resp.json();
    assert!(body.data.len() >= 3);
    assert!(body.data.iter().any(|dt| dt["id"] == "article"));
    assert!(body.data.iter().any(|dt| dt["id"] == "homepage"));

    // 2. Get specific schema by singularName
    let resp = ctx
        .server
        .get("/api/schema/document-types/article")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: SingleResponse<serde_json::Value> = resp.json();
    assert_eq!(body.data["id"], "article");
    assert_eq!(body.data["info"]["displayName"], "Article");
    assert_eq!(body.data["options"]["draftAndPublish"], true);

    // 3. Unknown document type -> 404
    let resp = ctx
        .server
        .get("/api/schema/document-types/nonexistent")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_access_request_onboarding_lifecycle() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_access_request_onboarding_lifecycle: DATABASE_URL not set");
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // 1. New user token
    let new_user_sub = format!("user-{}", Uuid::now_v7());
    let user_claims = Claims {
        sub: new_user_sub.clone(),
        email: Some("new-user@example.com".into()),
        name: Some("New User".into()),
        iss: None,
        aud: None,
        exp: Some(2500000000),
        iat: Some(1700000000),
    };
    let user_token = ctx.validator.generate_token(&user_claims).unwrap();

    // User tries to read articles before requesting access -> 403 ACCESS_NOT_REQUESTED
    let resp = ctx
        .server
        .get("/api/articles")
        .add_header("Authorization", format!("Bearer {user_token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(
        resp.json::<serde_json::Value>()["code"],
        "ACCESS_NOT_REQUESTED"
    );

    // 2. User submits access request
    let resp = ctx
        .server
        .post("/api/access-requests")
        .add_header("Authorization", format!("Bearer {user_token}"))
        .json(&json!({
            "email": "new-user@example.com",
            "name": "New User"
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    let request_id = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["status"], "pending");

    // Duplicate request -> 409 Conflict
    let resp = ctx
        .server
        .post("/api/access-requests")
        .add_header("Authorization", format!("Bearer {user_token}"))
        .json(&json!({
            "email": "new-user@example.com",
            "name": "New User"
        }))
        .await;
    resp.assert_status(StatusCode::CONFLICT);

    // User calls articles now -> 403 ACCESS_PENDING
    let resp = ctx
        .server
        .get("/api/articles")
        .add_header("Authorization", format!("Bearer {user_token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(resp.json::<serde_json::Value>()["code"], "ACCESS_PENDING");

    // 3. Admin lists pending requests
    let resp = ctx
        .server
        .get("/api/admin/access-requests")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let pending_list: serde_json::Value = resp.json();
    let items = pending_list["data"].as_array().unwrap();
    assert!(items.iter().any(|r| r["id"] == request_id));

    // 4. Admin approves request with ROLE_EDITOR_ID
    let resp = ctx
        .server
        .post(&format!("/api/admin/access-requests/{request_id}/approve"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "roleIds": [ROLE_EDITOR_ID.to_string()]
        }))
        .await;
    resp.assert_status(StatusCode::OK);

    // 5. User now has editor permissions and can access articles!
    let resp = ctx
        .server
        .get("/api/articles")
        .add_header("Authorization", format!("Bearer {user_token}"))
        .await;
    resp.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_collection_crud_and_publishing_workflows() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!(
                "Skipping test_collection_crud_and_publishing_workflows: DATABASE_URL not set"
            );
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // 1. Create a draft article (POST /api/articles)
    let resp = ctx
        .server
        .post("/api/articles")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "title": "Getting Started with Luminair",
            "slug": "getting-started",
            "views": 42
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = resp.json();
    let article_id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["title"], "Getting Started with Luminair");
    assert_eq!(created["data"]["slug"], "getting-started");
    assert_eq!(created["data"]["views"], 42);
    assert_eq!(created["data"]["publication-status"], "draft");

    // 2. Get article by ID (GET /api/articles/{id})
    let resp = ctx
        .server
        .get(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let fetched: serde_json::Value = resp.json();
    assert_eq!(fetched["data"]["id"], article_id);
    assert_eq!(fetched["data"]["title"], "Getting Started with Luminair");

    // 3. List articles with pagination (GET /api/articles)
    let resp = ctx
        .server
        .get("/api/articles?page=1&page_size=10")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let list: serde_json::Value = resp.json();
    assert_eq!(list["meta"]["pagination"]["total"], 1);
    assert_eq!(list["data"].as_array().unwrap().len(), 1);

    // 4. Filter articles by slug (GET /api/articles?filters[slug][$eq]=getting-started)
    let resp = ctx
        .server
        .get("/api/articles?filters[slug][$eq]=getting-started")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let filtered: serde_json::Value = resp.json();
    assert_eq!(filtered["data"].as_array().unwrap().len(), 1);

    // Filter with non-matching slug
    let resp = ctx
        .server
        .get("/api/articles?filters[slug][$eq]=non-existent")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let filtered_empty: serde_json::Value = resp.json();
    assert_eq!(filtered_empty["data"].as_array().unwrap().len(), 0);

    // 5. Update article (PUT /api/articles/{id})
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "title": "Advanced Luminair",
            "slug": "getting-started",
            "views": 100
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    assert_eq!(updated["data"]["title"], "Advanced Luminair");
    assert_eq!(updated["data"]["views"], 100);

    // 6. Publish article (POST /api/articles/{id}/publish)
    let resp = ctx
        .server
        .post(&format!("/api/articles/{article_id}/publish"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let published_doc: serde_json::Value = resp.json();
    assert_eq!(published_doc["data"]["publication-status"], "published");
    assert_eq!(published_doc["data"]["id"], article_id);

    // Verify instance now shows publication-status: published
    let resp = ctx
        .server
        .get(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    let article_now: serde_json::Value = resp.json();
    assert_eq!(article_now["data"]["publication-status"], "published");

    // 7. Unpublish article (POST /api/articles/{id}/unpublish)
    let resp = ctx
        .server
        .post(&format!("/api/articles/{article_id}/unpublish"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let unpublished: serde_json::Value = resp.json();
    assert_eq!(unpublished["data"]["publication-status"], "draft");

    // 9. Delete article (DELETE /api/articles/{id})
    let resp = ctx
        .server
        .delete(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify gone (GET /api/articles/{id} -> 404)
    let resp = ctx
        .server
        .get(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_singleton_crud_and_publishing_workflows() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!(
                "Skipping test_singleton_crud_and_publishing_workflows: DATABASE_URL not set"
            );
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // 1. Initial GET on empty singleton -> 404
    let resp = ctx
        .server
        .get("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);

    // 2. Upsert singleton (PUT /api/homepage)
    let resp = ctx
        .server
        .put("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "hero-title": "Welcome to Luminair CMS"
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = resp.json();
    assert_eq!(created["data"]["hero-title"], "Welcome to Luminair CMS");
    assert_eq!(created["data"]["publication-status"], "draft");

    // 3. GET singleton (GET /api/homepage)
    let resp = ctx
        .server
        .get("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let fetched: serde_json::Value = resp.json();
    assert_eq!(fetched["data"]["hero-title"], "Welcome to Luminair CMS");

    // 4. Update singleton (PUT /api/homepage)
    let resp = ctx
        .server
        .put("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "hero-title": "Welcome to Luminair Cloud"
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    assert_eq!(updated["data"]["hero-title"], "Welcome to Luminair Cloud");

    // 5. Publish singleton (POST /api/homepage/publish)
    let resp = ctx
        .server
        .post("/api/homepage/publish")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let published: serde_json::Value = resp.json();
    assert_eq!(published["data"]["publication-status"], "published");

    // 6. Unpublish singleton (POST /api/homepage/unpublish)
    let resp = ctx
        .server
        .post("/api/homepage/unpublish")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let unpublished: serde_json::Value = resp.json();
    assert_eq!(unpublished["data"]["publication-status"], "draft");

    // 8. Delete singleton (DELETE /api/homepage)
    let resp = ctx
        .server
        .delete("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify cleared (GET /api/homepage -> 404)
    let resp = ctx
        .server
        .get("/api/homepage")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_validation_and_error_handling() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_validation_and_error_handling: DATABASE_URL not set");
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // Missing required field 'slug' on article -> 400 Bad Request
    let resp = ctx
        .server
        .post("/api/articles")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "title": "Article without slug"
        }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    let problem: serde_json::Value = resp.json();
    assert_eq!(problem["title"], "Validation Failed");

    // Non-existent route /api/unknown-collection -> 404
    let resp = ctx
        .server
        .get("/api/unknown-collection")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_relational_mutations_and_read_after_write() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!(
                "Skipping test_relational_mutations_and_read_after_write: DATABASE_URL not set"
            );
            return;
        }
    };
    let ctx = setup_test_server(&pool).await;

    // 1. Create authors
    let resp = ctx
        .server
        .post("/api/authors")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "name": "Ferris",
            "email": "ferris@rust-lang.org"
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let author1_id = resp.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = ctx
        .server
        .post("/api/authors")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "name": "Corro",
            "email": "corro@rust-lang.org"
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let author2_id = resp.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Create tags
    let resp = ctx
        .server
        .post("/api/tags")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({ "name": "Rust" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let tag1_id = resp.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = ctx
        .server
        .post("/api/tags")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({ "name": "Cloud" }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let tag2_id = resp.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 3. Create article with connect author and read-after-write populate (POST /api/articles?populate=author)
    let resp = ctx
        .server
        .post("/api/articles?populate=author")
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "title": "Architecting on Aurora DSQL",
            "slug": "architecting-aurora-dsql",
            "views": 10,
            "author": { "connect": [&author1_id] }
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = resp.json();
    let article_id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["title"], "Architecting on Aurora DSQL");
    assert_eq!(created["data"]["publication-status"], "draft");

    // Read-after-write verification: author populated in single roundtrip!
    let populated_authors = created["data"]["author"].as_array().expect("author array");
    assert_eq!(populated_authors.len(), 1);
    assert_eq!(populated_authors[0]["id"], author1_id);
    assert_eq!(populated_authors[0]["name"], "Ferris");

    // 4. Update article with set tags and populate (PUT /api/articles/{id}?populate=tags)
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}?populate=tags"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "tags": { "set": [&tag1_id, &tag2_id] }
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    let populated_tags = updated["data"]["tags"].as_array().expect("tags array");
    assert_eq!(populated_tags.len(), 2);
    let tag_ids: Vec<&str> = populated_tags
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(tag_ids.contains(&tag1_id.as_str()));
    assert!(tag_ids.contains(&tag2_id.as_str()));

    // 5. Disconnect tag1 (PUT /api/articles/{id}?populate=tags)
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}?populate=tags"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "tags": { "disconnect": [&tag1_id] }
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    let populated_tags = updated["data"]["tags"].as_array().expect("tags array");
    assert_eq!(populated_tags.len(), 1);
    assert_eq!(populated_tags[0]["id"], tag2_id);

    // 6. Unset author via shorthand null (PUT /api/articles/{id}?populate=author)
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}?populate=author"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "author": null
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    let populated_authors = updated["data"]["author"].as_array().expect("author array");
    assert_eq!(populated_authors.len(), 0);

    // 7. Set author via shorthand UUID string (PUT /api/articles/{id}?populate=author)
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}?populate=author"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "author": &author1_id
        }))
        .await;
    resp.assert_status(StatusCode::OK);
    let updated: serde_json::Value = resp.json();
    let populated_authors = updated["data"]["author"].as_array().expect("author array");
    assert_eq!(populated_authors.len(), 1);
    assert_eq!(populated_authors[0]["id"], author1_id);

    // 8. Cardinality violation: attempt to connect 2 targets to HasOne relation
    let resp = ctx
        .server
        .put(&format!("/api/articles/{article_id}"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .json(&json!({
            "author": { "set": [&author1_id, &author2_id] }
        }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let problem: serde_json::Value = resp.json();
    assert_eq!(problem["title"], "Validation Failed");

    // 9. Publish article and verify relations persist
    let resp = ctx
        .server
        .post(&format!("/api/articles/{article_id}/publish"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);

    // Query published instance with populate
    let resp = ctx
        .server
        .get(&format!("/api/articles/{article_id}?populate=author,tags"))
        .add_header("Authorization", format!("Bearer {}", ctx.admin_token))
        .await;
    resp.assert_status(StatusCode::OK);
    let published: serde_json::Value = resp.json();
    assert_eq!(published["data"]["publication-status"], "published");
    let authors = published["data"]["author"]
        .as_array()
        .expect("author array");
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0]["id"], author1_id);
    let tags = published["data"]["tags"].as_array().expect("tags array");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0]["id"], tag2_id);
}
