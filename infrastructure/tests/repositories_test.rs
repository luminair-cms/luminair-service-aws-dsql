//! Integration tests for static system repositories:
//! - `SqlxRoleRepository`
//! - `SqlxUserRoleAssignmentRepository`
//! - `SqlxAccessRequestRepository`

use chrono::Utc;
use domain::entities::auth::access_request::AccessRequest;
use domain::entities::auth::role::{Permission, Role};
use domain::entities::auth::user_role_assignment::UserRoleAssignment;
use domain::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::value_objects::{DocumentTypeId, RoleId, UserId, UserRoleAssignmentId};
use infrastructure::migrations::{ROLE_ADMIN_ID, ROLE_EDITOR_ID, run_migrations};
use infrastructure::repositories::{
    SqlxAccessRequestRepository, SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};
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

#[tokio::test]
async fn test_role_repository_crud() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_role_repository_crud: DATABASE_URL not set");
            return;
        }
    };

    let repo = SqlxRoleRepository::new(pool);

    // 1. Verify seeded admin role can be fetched by ID and Name
    let admin_id = RoleId::new(ROLE_ADMIN_ID);
    let admin_by_id = repo
        .find_by_id(admin_id)
        .await
        .unwrap()
        .expect("admin role exists");
    assert_eq!(admin_by_id.name, "admin");
    assert!(admin_by_id.permissions.contains(&Permission::ManageSchema));
    assert!(admin_by_id.permissions.contains(&Permission::ManageRoles));
    assert!(admin_by_id.permissions.contains(&Permission::ManageUsers));

    let admin_by_name = repo
        .find_by_name("admin")
        .await
        .unwrap()
        .expect("admin role exists");
    assert_eq!(admin_by_name.id, admin_id);

    // 2. Verify find_all contains at least the 3 seeded roles
    let all = repo.find_all().await.unwrap();
    assert!(all.len() >= 3);
    assert!(all.iter().any(|r| r.name == "admin"));
    assert!(all.iter().any(|r| r.name == "editor"));
    assert!(all.iter().any(|r| r.name == "viewer"));

    // 3. Create a custom role with document-specific permissions
    let custom_id = RoleId::new(Uuid::now_v7());
    let article_type = DocumentTypeId::try_new("article").unwrap();
    let custom_role = Role {
        id: custom_id,
        name: format!("article_author_{}", Uuid::now_v7()),
        description: Some("Can write articles".into()),
        permissions: vec![
            Permission::CreateDocument(Some(article_type.clone())),
            Permission::ReadDocument(Some(article_type.clone())),
            Permission::UpdateDocument(Some(article_type.clone())),
        ],
    };

    repo.save(&custom_role).await.unwrap();

    let fetched = repo
        .find_by_id(custom_id)
        .await
        .unwrap()
        .expect("custom role saved");
    assert_eq!(fetched.name, custom_role.name);
    assert_eq!(fetched.description, custom_role.description);
    assert_eq!(fetched.permissions.len(), 3);
    assert!(
        fetched
            .permissions
            .contains(&Permission::CreateDocument(Some(article_type)))
    );

    // 4. Update role
    let mut updated = fetched;
    updated.description = Some("Updated description".into());
    updated.permissions.push(Permission::PublishDocument(None));
    repo.save(&updated).await.unwrap();

    let fetched_updated = repo.find_by_id(custom_id).await.unwrap().unwrap();
    assert_eq!(
        fetched_updated.description,
        Some("Updated description".into())
    );
    assert_eq!(fetched_updated.permissions.len(), 4);
    assert!(
        fetched_updated
            .permissions
            .contains(&Permission::PublishDocument(None))
    );
}

#[tokio::test]
async fn test_user_role_assignment_repository_crud() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_user_role_assignment_repository_crud: DATABASE_URL not set");
            return;
        }
    };

    let repo = SqlxUserRoleAssignmentRepository::new(pool);
    let admin_role_id = RoleId::new(ROLE_ADMIN_ID);
    let editor_role_id = RoleId::new(ROLE_EDITOR_ID);

    let user_id = UserId::try_new(format!("oidc-user-{}", Uuid::now_v7())).unwrap();

    // Initial check
    let initial = repo.find_by_user(&user_id).await.unwrap();
    assert!(initial.is_empty());

    // 1. Assign admin role
    let assignment1 = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: user_id.clone(),
        role_id: admin_role_id,
        granted_at: Utc::now(),
        granted_by: None,
    };
    repo.save(&assignment1).await.unwrap();

    // 2. Assign editor role
    let granter = UserId::try_new("admin-user-001").unwrap();
    let assignment2 = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: user_id.clone(),
        role_id: editor_role_id,
        granted_at: Utc::now(),
        granted_by: Some(granter.clone()),
    };
    repo.save(&assignment2).await.unwrap();

    let assignments = repo.find_by_user(&user_id).await.unwrap();
    assert_eq!(assignments.len(), 2);
    assert!(
        assignments
            .iter()
            .any(|a| a.role_id == admin_role_id && a.granted_by.is_none())
    );
    assert!(
        assignments
            .iter()
            .any(|a| a.role_id == editor_role_id && a.granted_by == Some(granter.clone()))
    );

    // Check exists_admin
    assert!(repo.exists_admin(admin_role_id).await.unwrap());

    // 3. Delete assignment
    repo.delete(assignment1.id).await.unwrap();
    let remaining = repo.find_by_user(&user_id).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].role_id, editor_role_id);
}

#[tokio::test]
async fn test_access_request_repository_crud() {
    let pool = match get_test_pool().await {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_access_request_repository_crud: DATABASE_URL not set");
            return;
        }
    };

    let repo = SqlxAccessRequestRepository::new(pool);
    let user_id = UserId::try_new(format!("new-user-{}", Uuid::now_v7())).unwrap();

    // 1. Create a pending request
    let mut request = AccessRequest::new(
        user_id.clone(),
        Some("test@example.com".into()),
        Some("Test User".into()),
        Utc::now(),
    );
    repo.save(&request).await.unwrap();

    // 2. Find by id and find by user
    let by_id = repo
        .find_by_id(request.id)
        .await
        .unwrap()
        .expect("request exists");
    assert_eq!(by_id.user_id, user_id);
    assert_eq!(by_id.email, Some("test@example.com".into()));
    assert!(matches!(
        by_id.status,
        domain::entities::auth::access_request::AccessRequestStatus::Pending
    ));

    let by_user = repo
        .find_by_user(&user_id)
        .await
        .unwrap()
        .expect("request by user exists");
    assert_eq!(by_user.id, request.id);

    // 3. Find pending
    let pending = repo.find_pending().await.unwrap();
    assert!(pending.iter().any(|r| r.id == request.id));

    // 4. Approve request
    let admin_user = UserId::try_new("super-admin").unwrap();
    let roles = vec![RoleId::new(ROLE_EDITOR_ID)];
    request
        .approve(admin_user.clone(), roles.clone(), Utc::now())
        .unwrap();
    repo.save(&request).await.unwrap();

    let approved = repo.find_by_id(request.id).await.unwrap().unwrap();
    assert!(matches!(
        approved.status,
        domain::entities::auth::access_request::AccessRequestStatus::Approved
    ));
    assert_eq!(approved.reviewed_by, Some(admin_user));
    assert_eq!(approved.assigned_roles, roles);
}
