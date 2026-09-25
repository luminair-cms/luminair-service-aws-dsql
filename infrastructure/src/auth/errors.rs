//! Authentication and authorization error types and HTTP response mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Reasons why an authenticated user is forbidden from accessing the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForbiddenReason {
    AccessPending,
    AccessRejected,
    AccessNotRequested,
}

/// Authentication and enrollment errors.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing Authorization header")]
    MissingAuthorizationHeader,

    #[error("Invalid Authorization header format; expected 'Bearer <token>'")]
    InvalidAuthorizationHeader,

    #[error("Invalid JWT token: {0}")]
    InvalidToken(String),

    #[error("JWT token has expired")]
    ExpiredToken,

    #[error("Invalid token claim '{claim}': {reason}")]
    InvalidClaim { claim: String, reason: String },

    #[error("Access request is pending administrative approval")]
    AccessPending,

    #[error("Access request was rejected")]
    AccessRejected { reason: Option<String> },

    #[error("Access has not been requested; call POST /api/access-requests to request access")]
    AccessNotRequested,

    #[error("Database storage error: {0}")]
    Storage(String),

    #[error("Bootstrap error: {0}")]
    Bootstrap(String),
}

#[derive(Serialize)]
struct ProblemDetails {
    status: u16,
    title: &'static str,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, title, code, hint, reason) = match &self {
            AuthError::MissingAuthorizationHeader => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                Some("MISSING_TOKEN"),
                Some("Include 'Authorization: Bearer <token>' in request headers"),
                None,
            ),
            AuthError::InvalidAuthorizationHeader => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                Some("INVALID_AUTH_HEADER"),
                Some("Authorization header must follow 'Bearer <token>' scheme"),
                None,
            ),
            AuthError::InvalidToken(_) => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                Some("INVALID_TOKEN"),
                None,
                None,
            ),
            AuthError::ExpiredToken => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                Some("TOKEN_EXPIRED"),
                None,
                None,
            ),
            AuthError::InvalidClaim { .. } => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                Some("INVALID_CLAIMS"),
                None,
                None,
            ),
            AuthError::AccessPending => (
                StatusCode::FORBIDDEN,
                "Forbidden",
                Some("ACCESS_PENDING"),
                Some("Awaiting administrator approval"),
                None,
            ),
            AuthError::AccessRejected { reason: r } => (
                StatusCode::FORBIDDEN,
                "Forbidden",
                Some("ACCESS_REJECTED"),
                Some("Your access request was rejected; you may submit a new request"),
                r.clone(),
            ),
            AuthError::AccessNotRequested => (
                StatusCode::FORBIDDEN,
                "Forbidden",
                Some("ACCESS_NOT_REQUESTED"),
                Some("POST /api/access-requests to request access"),
                None,
            ),
            AuthError::Storage(s) | AuthError::Bootstrap(s) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                Some("INTERNAL_ERROR"),
                None,
                Some(s.clone()),
            ),
        };

        let body = ProblemDetails {
            status: status.as_u16(),
            title,
            detail: self.to_string(),
            code,
            hint,
            reason,
        };

        (
            status,
            [("content-type", "application/problem+json")],
            axum::Json(body),
        )
            .into_response()
    }
}
