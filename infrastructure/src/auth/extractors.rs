//! Axum extractors for authenticated claims and role-verified callers (ADR-005).

use std::sync::Arc;

use application::context::CallerContext;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use domain::entities::auth::access_request::AccessRequestStatus;
use domain::entities::auth::role::Role;
use domain::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use sqlx::PgPool;

use super::claims::Claims;
use super::errors::AuthError;
use super::shadow_users::SqlxShadowUserRepository;
use super::validator::TokenValidator;
use crate::repositories::{
    SqlxAccessRequestRepository, SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

/// Shared application state containing authentication dependencies.
#[derive(Clone)]
pub struct AuthAppState {
    pub pool: PgPool,
    pub validator: Arc<dyn TokenValidator>,
    pub role_repo: SqlxRoleRepository,
    pub assignment_repo: SqlxUserRoleAssignmentRepository,
    pub access_request_repo: SqlxAccessRequestRepository,
    pub shadow_user_repo: SqlxShadowUserRepository,
}

impl AuthAppState {
    pub fn new(pool: PgPool, validator: Arc<dyn TokenValidator>) -> Self {
        let role_repo = SqlxRoleRepository::new(pool.clone());
        let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
        let access_request_repo = SqlxAccessRequestRepository::new(pool.clone());
        let shadow_user_repo = SqlxShadowUserRepository::new(pool.clone());
        Self {
            pool,
            validator,
            role_repo,
            assignment_repo,
            access_request_repo,
            shadow_user_repo,
        }
    }
}

/// Extractor for verified OIDC JWT claims without requiring approved roles.
///
/// Used for endpoints open to any authenticated IdP user (such as `POST /api/access-requests`).
/// Automatically upserts the caller's identity into the `shadow_users` table.
#[derive(Debug, Clone)]
pub struct AuthenticatedClaims(pub Claims);

impl<S> FromRequestParts<S> for AuthenticatedClaims
where
    AuthAppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_state = AuthAppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorizationHeader)?
            .to_str()
            .map_err(|_| AuthError::InvalidAuthorizationHeader)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .ok_or(AuthError::InvalidAuthorizationHeader)?
            .trim();

        if token.is_empty() {
            return Err(AuthError::InvalidAuthorizationHeader);
        }

        // 1. Validate JWT signature and claims
        let claims = auth_state.validator.validate(token)?;

        // 2. Upsert shadow user record
        let _ = auth_state
            .shadow_user_repo
            .upsert(
                &claims.sub,
                claims.email.as_deref(),
                claims.name.as_deref(),
                None,
            )
            .await;

        Ok(AuthenticatedClaims(claims))
    }
}

/// Extractor for fully authenticated and authorized callers with active roles.
///
/// Enforces enrollment lifecycle (ADR-005):
/// - If user has approved roles -> returns `AuthUser` carrying `CallerContext`.
/// - If user has a pending access request -> returns 403 `ACCESS_PENDING`.
/// - If user was rejected -> returns 403 `ACCESS_REJECTED`.
/// - If user never requested access -> returns 403 `ACCESS_NOT_REQUESTED`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub caller: CallerContext,
    pub claims: Claims,
}

impl<S> FromRequestParts<S> for AuthUser
where
    AuthAppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_state = AuthAppState::from_ref(state);

        // 1. Extract verified claims and upsert shadow user
        let AuthenticatedClaims(claims) =
            AuthenticatedClaims::from_request_parts(parts, state).await?;
        let user_id = claims.user_id()?;

        // 2. Fetch assigned roles
        let assignments = auth_state
            .assignment_repo
            .find_by_user(&user_id)
            .await
            .map_err(|e| AuthError::Storage(e.to_string()))?;

        if !assignments.is_empty() {
            let mut roles: Vec<Role> = Vec::with_capacity(assignments.len());
            for assignment in assignments {
                if let Some(role) = auth_state
                    .role_repo
                    .find_by_id(assignment.role_id)
                    .await
                    .map_err(|e| AuthError::Storage(e.to_string()))?
                {
                    roles.push(role);
                }
            }

            if !roles.is_empty() {
                let caller = CallerContext::new(user_id, roles);
                return Ok(AuthUser { caller, claims });
            }
        }

        // 3. User has no active role assignments: check AccessRequest status
        let access_request = auth_state
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
