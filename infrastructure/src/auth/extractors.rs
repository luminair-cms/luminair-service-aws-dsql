//! Axum extractors for authenticated claims and role-verified callers (ADR-005).

use std::sync::Arc;

use application::context::CallerContext;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use sqlx::PgPool;

use super::claims::Claims;
use super::errors::AuthError;
use super::resolver::AuthContextResolver;
use super::shadow_users::SqlxShadowUserRepository;
use super::validator::TokenValidator;
use crate::persistence::repositories::{
    SqlxAccessRequestRepository, SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

/// Shared application state containing authentication dependencies.
#[derive(Clone)]
pub struct AuthAppState {
    pub pool: PgPool,
    pub validator: Arc<dyn TokenValidator>,
    pub resolver: Arc<
        AuthContextResolver<
            SqlxUserRoleAssignmentRepository,
            SqlxRoleRepository,
            SqlxAccessRequestRepository,
        >,
    >,
    pub role_repo: SqlxRoleRepository,
    pub assignment_repo: SqlxUserRoleAssignmentRepository,
    pub access_request_repo: SqlxAccessRequestRepository,
    pub shadow_user_repo: SqlxShadowUserRepository,
}

impl AuthAppState {
    pub fn new(pool: PgPool, validator: Arc<dyn TokenValidator>) -> Self {
        let role_repo = Arc::new(SqlxRoleRepository::new(pool.clone()));
        let assignment_repo = Arc::new(SqlxUserRoleAssignmentRepository::new(pool.clone()));
        let access_request_repo = Arc::new(SqlxAccessRequestRepository::new(pool.clone()));
        let shadow_user_repo = Arc::new(SqlxShadowUserRepository::new(pool.clone()));

        Self::from_parts(
            pool,
            validator,
            assignment_repo,
            role_repo,
            access_request_repo,
            shadow_user_repo,
        )
    }

    /// Creates an `AuthAppState` sharing existing repository instances.
    pub fn from_parts(
        pool: PgPool,
        validator: Arc<dyn TokenValidator>,
        assignment_repo: Arc<SqlxUserRoleAssignmentRepository>,
        role_repo: Arc<SqlxRoleRepository>,
        access_request_repo: Arc<SqlxAccessRequestRepository>,
        shadow_user_repo: Arc<SqlxShadowUserRepository>,
    ) -> Self {
        let resolver = Arc::new(AuthContextResolver::new(
            assignment_repo.clone(),
            role_repo.clone(),
            access_request_repo.clone(),
            shadow_user_repo.clone(),
        ));

        Self {
            pool,
            validator,
            resolver,
            role_repo: (*role_repo).clone(),
            assignment_repo: (*assignment_repo).clone(),
            access_request_repo: (*access_request_repo).clone(),
            shadow_user_repo: (*shadow_user_repo).clone(),
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

        let AuthenticatedClaims(claims) =
            AuthenticatedClaims::from_request_parts(parts, state).await?;

        let caller = auth_state.resolver.resolve_context(&claims).await?;

        Ok(AuthUser { caller, claims })
    }
}
