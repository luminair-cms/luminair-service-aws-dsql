//! Startup bootstrap hook for seeding the initial administrator (ADR-005).

use chrono::Utc;
use domain::entities::auth::access_request::{AccessRequest, AccessRequestStatus};
use domain::entities::auth::user_role_assignment::UserRoleAssignment;
use domain::ports::{AccessRequestRepository, UserRoleAssignmentRepository};
use domain::value_objects::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};
use sqlx::PgPool;
use uuid::Uuid;

use super::config::AuthConfig;
use super::errors::AuthError;
use super::shadow_users::SqlxShadowUserRepository;
use crate::migrations::ROLE_ADMIN_ID;

/// Seeds the initial administrator user idempotently on application startup.
///
/// Workflow (ADR-005):
/// 1. Reads `BOOTSTRAP_ADMIN_SUB` from configuration. If not set or empty, skips.
/// 2. Checks if an active administrator role assignment already exists for the user.
///    If yes, logs and exits (no-op).
/// 3. Upserts a `shadow_users` record for the admin identity.
/// 4. Creates an approved `AccessRequest` record for audit trail.
/// 5. Creates a `UserRoleAssignment` with `role_id: ROLE_ADMIN_ID` and `granted_by: None`.
///
/// Returns `Ok(true)` if an admin was newly seeded, or `Ok(false)` if skipped.
pub async fn run_bootstrap<A, AR>(
    pool: &PgPool,
    config: &AuthConfig,
    assignment_repo: &A,
    access_request_repo: &AR,
) -> Result<bool, AuthError>
where
    A: UserRoleAssignmentRepository,
    AR: AccessRequestRepository,
{
    let admin_sub = match &config.bootstrap_admin_sub {
        Some(sub) if !sub.trim().is_empty() => sub.trim(),
        _ => return Ok(false),
    };

    let admin_user_id = UserId::try_new(admin_sub).map_err(|e| {
        AuthError::Bootstrap(format!("Invalid BOOTSTRAP_ADMIN_SUB '{admin_sub}': {e}"))
    })?;

    let admin_role_id = RoleId::new(ROLE_ADMIN_ID);

    // 1. Check if user is already assigned the admin role
    let existing_assignments = assignment_repo
        .find_by_user(&admin_user_id)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

    if existing_assignments
        .iter()
        .any(|a| a.role_id == admin_role_id)
    {
        tracing::info!(
            user_id = %admin_user_id,
            "Bootstrap: admin role assignment already exists, skipping"
        );
        return Ok(false);
    }

    // 2. Upsert shadow user record
    let shadow_repo = SqlxShadowUserRepository::new(pool.clone());
    shadow_repo
        .upsert(
            admin_sub,
            None,
            Some("Bootstrap Admin"),
            Some(&config.bootstrap_auth_type),
        )
        .await?;

    // 3. Create approved AccessRequest for audit trail if no active request exists
    let existing_request = access_request_repo
        .find_by_user(&admin_user_id)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

    if existing_request.is_none() {
        let request = AccessRequest {
            id: AccessRequestId::new(Uuid::now_v7()),
            user_id: admin_user_id.clone(),
            email: None,
            name: Some("Bootstrap Admin".to_string()),
            requested_at: Utc::now(),
            status: AccessRequestStatus::Approved,
            reviewed_by: Some(admin_user_id.clone()),
            reviewed_at: Some(Utc::now()),
            assigned_roles: vec![admin_role_id],
        };

        access_request_repo
            .save(&request)
            .await
            .map_err(|e| AuthError::Storage(e.to_string()))?;
    }

    // 4. Create UserRoleAssignment for admin role (granted_by: None indicates system/bootstrap grant)
    let assignment = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: admin_user_id.clone(),
        role_id: admin_role_id,
        granted_at: Utc::now(),
        granted_by: None,
    };

    assignment_repo
        .save(&assignment)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

    tracing::info!(
        user_id = %admin_user_id,
        "Bootstrap: initial admin role seeded successfully"
    );

    Ok(true)
}
