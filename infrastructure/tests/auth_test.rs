//! Integration and unit tests for Authentication, Security & Bootstrap (ADR-005, Phase 5).

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_test::TestServer;
use chrono::Utc;
use domain::entities::auth::access_request::AccessRequest;
use domain::entities::auth::user_role_assignment::UserRoleAssignment;
use domain::ports::{AccessRequestRepository, UserRoleAssignmentRepository};
use domain::value_objects::{RoleId, UserId, UserRoleAssignmentId};
use infrastructure::auth::{
    AuthAppState, AuthConfig, AuthUser, AuthenticatedClaims, Claims, MockTokenValidator,
    SecretTokenValidator, SqlxShadowUserRepository, TokenValidator, run_bootstrap,
};
use infrastructure::migrations::{ROLE_ADMIN_ID, ROLE_EDITOR_ID, run_migrations};
use infrastructure::repositories::{SqlxAccessRequestRepository, SqlxUserRoleAssignmentRepository};
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

#[test]
fn test_claims_and_config() {
    let claims = Claims {
        sub: "user-12345".to_string(),
        email: Some("user@example.com".to_string()),
        name: Some("User Name".to_string()),
        iss: Some("https://auth.example.com".to_string()),
        aud: Some(json!("luminair-api")),
        exp: Some(1900000000),
        iat: Some(1700000000),
    };

    assert_eq!(claims.user_id().unwrap().as_ref(), "user-12345");
    assert!(claims.matches_audience("luminair-api"));
    assert!(!claims.matches_audience("wrong-aud"));
    assert!(claims.matches_issuer("https://auth.example.com"));
    assert!(!claims.matches_issuer("https://other.com"));

    let config = AuthConfig {
        issuer_url: Some("https://auth.example.com".to_string()),
        audience: Some("luminair-api".to_string()),
        bootstrap_admin_sub: Some("admin-sub".to_string()),
        bootstrap_auth_type: "cognito".to_string(),
    };
    assert_eq!(config.bootstrap_auth_type, "cognito");
}

#[test]
fn test_secret_token_validator() {
    let validator = SecretTokenValidator::new("super-secret-key-1234567890123456")
        .with_audience("luminair-api")
        .with_issuer("https://auth.example.com");

    let claims = Claims {
        sub: "user-test".to_string(),
        email: Some("test@example.com".to_string()),
        name: Some("Test User".to_string()),
        iss: Some("https://auth.example.com".to_string()),
        aud: Some(json!("luminair-api")),
        exp: Some(2500000000),
        iat: Some(1700000000),
    };

    let token = validator.generate_token(&claims).unwrap();
    let validated = validator.validate(&token).unwrap();
    assert_eq!(validated.sub, "user-test");
    assert_eq!(validated.email, Some("test@example.com".to_string()));

    // Invalid secret validation fails
    let wrong_validator = SecretTokenValidator::new("wrong-secret-key-000000000000000");
    assert!(wrong_validator.validate(&token).is_err());
}

#[test]
fn test_mock_token_validator() {
    let mock = MockTokenValidator::new();
    let claims = Claims {
        sub: "mock-user".to_string(),
        email: Some("mock@example.com".to_string()),
        name: None,
        iss: None,
        aud: None,
        exp: None,
        iat: None,
    };

    // Generate unverified token via secret validator and decode with mock
    let sec = SecretTokenValidator::new("dummy");
    let token = sec.generate_token(&claims).unwrap();
    let decoded = mock.validate(&token).unwrap();
    assert_eq!(decoded.sub, "mock-user");

    // Override mode
    let override_mock = MockTokenValidator::with_claims(Claims {
        sub: "overridden-user".to_string(),
        email: None,
        name: None,
        iss: None,
        aud: None,
        exp: None,
        iat: None,
    });
    assert_eq!(
        override_mock.validate("any.dummy.token").unwrap().sub,
        "overridden-user"
    );
}

#[tokio::test]
async fn test_shadow_user_repository_crud() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_shadow_user_repository_crud: DATABASE_URL not set");
            return;
        }
    };

    let repo = SqlxShadowUserRepository::new(pool);
    let user_id = format!("test-shadow-{}", Uuid::now_v7());

    // 1. Initial upsert
    let shadow = repo
        .upsert(
            &user_id,
            Some("shadow@example.com"),
            Some("Shadow User"),
            Some("keycloak"),
        )
        .await
        .unwrap();
    assert_eq!(shadow.user_id, user_id);
    assert_eq!(shadow.email, Some("shadow@example.com".into()));
    assert_eq!(shadow.name, Some("Shadow User".into()));
    assert_eq!(shadow.auth_type, Some("keycloak".into()));

    // 2. Find by id
    let fetched = repo.find_by_id(&user_id).await.unwrap().expect("found");
    assert_eq!(fetched.user_id, user_id);

    // 3. Update existing user
    let updated = repo
        .upsert(
            &user_id,
            Some("shadow-new@example.com"),
            Some("Updated Name"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(updated.email, Some("shadow-new@example.com".into()));
    assert_eq!(updated.name, Some("Updated Name".into()));
    assert_eq!(updated.auth_type, Some("keycloak".into())); // preserved from previous
}

#[tokio::test]
async fn test_bootstrap_admin_hook_idempotent() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_bootstrap_admin_hook_idempotent: DATABASE_URL not set");
            return;
        }
    };

    let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
    let access_request_repo = SqlxAccessRequestRepository::new(pool.clone());
    let admin_sub = format!("boot-admin-{}", Uuid::now_v7());

    let config = AuthConfig {
        issuer_url: None,
        audience: None,
        bootstrap_admin_sub: Some(admin_sub.clone()),
        bootstrap_auth_type: "cognito".to_string(),
    };

    // 1. First run: seeds admin
    let seeded = run_bootstrap(&pool, &config, &assignment_repo, &access_request_repo)
        .await
        .unwrap();
    assert!(seeded, "Initial bootstrap should seed admin");

    let admin_user = UserId::try_new(&admin_sub).unwrap();
    let assignments = assignment_repo.find_by_user(&admin_user).await.unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].role_id, RoleId::new(ROLE_ADMIN_ID));
    assert!(assignments[0].granted_by.is_none());

    // Verify shadow user created
    let shadow_repo = SqlxShadowUserRepository::new(pool.clone());
    let shadow = shadow_repo.find_by_id(&admin_sub).await.unwrap().unwrap();
    assert_eq!(shadow.auth_type, Some("cognito".into()));

    // 2. Second run: idempotent, returns false
    let second_run = run_bootstrap(&pool, &config, &assignment_repo, &access_request_repo)
        .await
        .unwrap();
    assert!(!second_run, "Second bootstrap run should be skipped");

    // 3. Run with empty sub: skipped
    let empty_config = AuthConfig {
        issuer_url: None,
        audience: None,
        bootstrap_admin_sub: None,
        bootstrap_auth_type: "oidc".to_string(),
    };
    let skipped = run_bootstrap(&pool, &empty_config, &assignment_repo, &access_request_repo)
        .await
        .unwrap();
    assert!(!skipped);
}

// Axum handler for open onboarding endpoint (uses AuthenticatedClaims)
async fn request_access_handler(
    AuthenticatedClaims(claims): AuthenticatedClaims,
) -> impl IntoResponse {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "pending",
            "user_id": claims.sub,
            "email": claims.email
        })),
    )
}

// Axum handler for protected document endpoint (uses AuthUser)
async fn get_documents_handler(auth: AuthUser) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "user_id": auth.caller.user_id.as_ref(),
            "roles_count": auth.caller.roles.len(),
            "is_admin": auth.caller.roles.iter().any(|r| r.name == "admin")
        })),
    )
}

#[tokio::test]
async fn test_axum_auth_extractors_end_to_end() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_axum_auth_extractors_end_to_end: DATABASE_URL not set");
            return;
        }
    };

    let secret = "test-jwt-secret-key-1234567890123";
    let validator = Arc::new(SecretTokenValidator::new(secret));

    let auth_state = AuthAppState::new(pool.clone(), validator.clone());

    let app = Router::new()
        .route("/api/access-requests", post(request_access_handler))
        .route("/api/documents", get(get_documents_handler))
        .with_state(auth_state);

    let server = TestServer::new(app);

    // 1. Missing Authorization header -> 401 Unauthorized
    let resp = server.get("/api/documents").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(resp.json::<serde_json::Value>()["code"], "MISSING_TOKEN");

    // 2. Malformed Authorization header -> 401 Unauthorized
    let resp = server
        .get("/api/documents")
        .add_header("Authorization", "Basic 12345")
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.json::<serde_json::Value>()["code"],
        "INVALID_AUTH_HEADER"
    );

    // 3. Invalid JWT signature -> 401 Unauthorized
    let resp = server
        .get("/api/documents")
        .add_header("Authorization", "Bearer invalid.jwt.token")
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(resp.json::<serde_json::Value>()["code"], "INVALID_TOKEN");

    // 4. Valid JWT for user who has not requested access -> 403 ACCESS_NOT_REQUESTED
    let new_user_sub = format!("new-user-{}", Uuid::now_v7());
    let claims = Claims {
        sub: new_user_sub.clone(),
        email: Some("newuser@example.com".into()),
        name: Some("New User".into()),
        iss: None,
        aud: None,
        exp: Some(2500000000),
        iat: Some(1700000000),
    };
    let token = validator.generate_token(&claims).unwrap();

    let resp = server
        .get("/api/documents")
        .add_header("Authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(
        resp.json::<serde_json::Value>()["code"],
        "ACCESS_NOT_REQUESTED"
    );

    // 5. User calls POST /api/access-requests (open onboarding with AuthenticatedClaims) -> 202 Accepted
    let resp = server
        .post("/api/access-requests")
        .add_header("Authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::ACCEPTED);
    assert_eq!(resp.json::<serde_json::Value>()["status"], "pending");

    // Verify shadow user was automatically recorded
    let shadow_repo = SqlxShadowUserRepository::new(pool.clone());
    let shadow = shadow_repo
        .find_by_id(&new_user_sub)
        .await
        .unwrap()
        .expect("shadow user exists");
    assert_eq!(shadow.email, Some("newuser@example.com".into()));

    // 6. User submits pending request, queries /api/documents -> 403 ACCESS_PENDING
    let access_repo = SqlxAccessRequestRepository::new(pool.clone());
    let user_id = UserId::try_new(&new_user_sub).unwrap();
    let req = AccessRequest::new(
        user_id.clone(),
        Some("newuser@example.com".into()),
        Some("New User".into()),
        Utc::now(),
    );
    access_repo.save(&req).await.unwrap();

    let resp = server
        .get("/api/documents")
        .add_header("Authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(resp.json::<serde_json::Value>()["code"], "ACCESS_PENDING");

    // 7. Request rejected -> 403 ACCESS_REJECTED
    let mut rejected_req = req;
    let admin_user = UserId::try_new("super-admin").unwrap();
    rejected_req
        .reject(
            admin_user.clone(),
            Some("Not an employee".into()),
            Utc::now(),
        )
        .unwrap();
    access_repo.save(&rejected_req).await.unwrap();

    let resp = server
        .get("/api/documents")
        .add_header("Authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(resp.json::<serde_json::Value>()["code"], "ACCESS_REJECTED");

    // 8. User is assigned a role (e.g. editor) -> 200 OK with AuthUser carrying CallerContext!
    let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
    let assignment = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: user_id.clone(),
        role_id: RoleId::new(ROLE_EDITOR_ID),
        granted_at: Utc::now(),
        granted_by: Some(admin_user),
    };
    assignment_repo.save(&assignment).await.unwrap();

    let resp = server
        .get("/api/documents")
        .add_header("Authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status(StatusCode::OK);
    let body = resp.json::<serde_json::Value>();
    assert_eq!(body["user_id"], new_user_sub);
    assert_eq!(body["roles_count"], 1);
    assert_eq!(body["is_admin"], false);
}
