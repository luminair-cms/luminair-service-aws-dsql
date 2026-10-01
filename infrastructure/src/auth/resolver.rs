//! Enrollment lifecycle and caller context resolution for authenticated identities (ADR-005).

use std::sync::Arc;

use application::context::CallerContext;
use domain::auth::{
    AccessRequestRepository, AccessRequestStatus, Role, RoleRepository,
    UserRoleAssignmentRepository,
};

use super::claims::Claims;
use super::errors::AuthError;
use crate::persistence::repositories::SqlxShadowUserRepository;

/// Service resolving validated JWT claims into authorized `CallerContext` or enrollment error.
#[derive(Clone)]
pub struct AuthContextResolver<U, R, A> {
    pub assignment_repo: Arc<U>,
    pub role_repo: Arc<R>,
    pub access_request_repo: Arc<A>,
    pub shadow_user_repo: Arc<SqlxShadowUserRepository>,
}

impl<U, R, A> AuthContextResolver<U, R, A>
where
    U: UserRoleAssignmentRepository,
    R: RoleRepository,
    A: AccessRequestRepository,
{
    pub fn new(
        assignment_repo: Arc<U>,
        role_repo: Arc<R>,
        access_request_repo: Arc<A>,
        shadow_user_repo: Arc<SqlxShadowUserRepository>,
    ) -> Self {
        Self {
            assignment_repo,
            role_repo,
            access_request_repo,
            shadow_user_repo,
        }
    }

    /// Resolves caller context following the ADR-005 enrollment lifecycle:
    /// 1. Upserts shadow user profile
    /// 2. Resolves assigned roles
    /// 3. If no roles, evaluates pending/rejected/unrequested access requests
    pub async fn resolve_context(&self, claims: &Claims) -> Result<CallerContext, AuthError> {
        let user_id = claims.user_id()?;

        // 1. Upsert shadow user record
        let _ = self
            .shadow_user_repo
            .upsert(
                &claims.sub,
                claims.email.as_deref(),
                claims.name.as_deref(),
                None,
            )
            .await;

        // 2. Fetch assigned roles
        let assignments = self
            .assignment_repo
            .find_by_user(&user_id)
            .await
            .map_err(|e| AuthError::Storage(e.to_string()))?;

        if !assignments.is_empty() {
            let mut roles: Vec<Role> = Vec::with_capacity(assignments.len());
            for assignment in assignments {
                if let Some(role) = self
                    .role_repo
                    .find_by_id(assignment.role_id)
                    .await
                    .map_err(|e| AuthError::Storage(e.to_string()))?
                {
                    roles.push(role);
                }
            }

            if !roles.is_empty() {
                return Ok(CallerContext::new(user_id, roles));
            }
        }

        // 3. User has no active role assignments: check AccessRequest status
        let access_request = self
            .access_request_repo
            .find_by_user(&user_id)
            .await
            .map_err(|e| AuthError::Storage(e.to_string()))?;

        match access_request {
            Some(req) => match req.status {
                AccessRequestStatus::Pending => Err(AuthError::AccessPending),
                AccessRequestStatus::Rejected { reason } => {
                    Err(AuthError::AccessRejected { reason })
                }
                AccessRequestStatus::Approved => {
                    // Approved but no assignments found yet
                    Err(AuthError::AccessPending)
                }
            },
            None => Err(AuthError::AccessNotRequested),
        }
    }
}
