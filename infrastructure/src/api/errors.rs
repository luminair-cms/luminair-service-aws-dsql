//! RFC 9457 Problem Details error envelope and HTTP response mapping.

use application::errors::ApplicationError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::AuthError;

/// Standard RFC 9457 Problem Details payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
}

/// Unified API error enum covering application, auth, and HTTP presentation failures.
#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Application(#[from] ApplicationError),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        ApiError::Application(ApplicationError::Domain(err))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Auth(auth_err) => auth_err.into_response(),
            ApiError::Application(app_err) => map_application_error(app_err),
            ApiError::BadRequest(msg) => problem_response(
                StatusCode::BAD_REQUEST,
                "https://luminair.io/errors/bad-request",
                "Bad Request",
                msg,
                None,
            ),
            ApiError::NotFound(msg) => problem_response(
                StatusCode::NOT_FOUND,
                "https://luminair.io/errors/not-found",
                "Resource Not Found",
                msg,
                None,
            ),
            ApiError::Conflict(msg) => problem_response(
                StatusCode::CONFLICT,
                "https://luminair.io/errors/conflict",
                "Conflict",
                msg,
                None,
            ),
            ApiError::Forbidden(msg) => problem_response(
                StatusCode::FORBIDDEN,
                "https://luminair.io/errors/forbidden",
                "Forbidden",
                msg,
                None,
            ),
            ApiError::Internal(msg) => problem_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "https://luminair.io/errors/internal-error",
                "Internal Server Error",
                msg,
                None,
            ),
        }
    }
}

fn map_application_error(err: ApplicationError) -> Response {
    match err {
        ApplicationError::Unauthorized { user_id, action } => problem_response(
            StatusCode::FORBIDDEN,
            "https://luminair.io/errors/forbidden",
            "Forbidden",
            format!("user '{user_id}' lacks permission for action '{action:?}'"),
            None,
        ),
        ApplicationError::NotFound { entity, id } => problem_response(
            StatusCode::NOT_FOUND,
            "https://luminair.io/errors/not-found",
            "Resource Not Found",
            format!("{entity} with identifier '{id}' was not found"),
            None,
        ),
        ApplicationError::Validation(errors) => problem_response(
            StatusCode::BAD_REQUEST,
            "https://luminair.io/errors/validation-error",
            "Validation Failed",
            errors.join("; "),
            Some(errors),
        ),
        ApplicationError::Conflict(msg) => problem_response(
            StatusCode::CONFLICT,
            "https://luminair.io/errors/conflict",
            "Conflict",
            msg,
            None,
        ),
        ApplicationError::Internal(msg) => problem_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "https://luminair.io/errors/internal-error",
            "Internal Server Error",
            msg,
            None,
        ),
        ApplicationError::Domain(domain_err) => map_domain_error(domain_err),
    }
}

fn map_domain_error(err: DomainError) -> Response {
    match err {
        DomainError::DocumentTypeNotFound(type_id) => problem_response(
            StatusCode::NOT_FOUND,
            "https://luminair.io/errors/not-found",
            "Resource Not Found",
            format!("document type '{type_id}' was not found"),
            None,
        ),
        DomainError::DocumentInstanceNotFound(id) => problem_response(
            StatusCode::NOT_FOUND,
            "https://luminair.io/errors/not-found",
            "Resource Not Found",
            format!("document instance with id '{id}' was not found"),
            None,
        ),
        DomainError::SingleTypeAlreadyExists(type_id) => problem_response(
            StatusCode::CONFLICT,
            "https://luminair.io/errors/conflict",
            "Conflict",
            format!("single type '{type_id}' already has an instance"),
            None,
        ),
        DomainError::AccessRequestNotFound(id) => problem_response(
            StatusCode::NOT_FOUND,
            "https://luminair.io/errors/not-found",
            "Resource Not Found",
            format!("access request with id '{id}' was not found"),
            None,
        ),
        DomainError::AccessRequestAlreadyActive(user_id) => problem_response(
            StatusCode::CONFLICT,
            "https://luminair.io/errors/conflict",
            "Conflict",
            format!("active access request already exists for user '{user_id}'"),
            None,
        ),
        DomainError::Unauthorized(msg) => problem_response(
            StatusCode::FORBIDDEN,
            "https://luminair.io/errors/forbidden",
            "Forbidden",
            msg,
            None,
        ),
        DomainError::InvalidFieldValue {
            attribute_id,
            reason,
        } => problem_response(
            StatusCode::BAD_REQUEST,
            "https://luminair.io/errors/invalid-field",
            "Invalid Field Value",
            format!("invalid field value for attribute '{attribute_id}': {reason}"),
            None,
        ),
        DomainError::UnknownLocale(loc) => problem_response(
            StatusCode::BAD_REQUEST,
            "https://luminair.io/errors/unknown-locale",
            "Unknown Locale",
            format!("unsupported locale: {loc}"),
            None,
        ),
        DomainError::UnknownAttribute(attr) => problem_response(
            StatusCode::BAD_REQUEST,
            "https://luminair.io/errors/unknown-attribute",
            "Unknown Attribute",
            format!("undeclared attribute: {attr}"),
            None,
        ),
        DomainError::InvalidStateTransition { reason } => problem_response(
            StatusCode::CONFLICT,
            "https://luminair.io/errors/conflict",
            "Invalid State Transition",
            reason,
            None,
        ),
        DomainError::Storage(msg) => problem_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "https://luminair.io/errors/internal-error",
            "Internal Server Error",
            msg,
            None,
        ),
    }
}

fn problem_response(
    status: StatusCode,
    problem_type: &str,
    title: &str,
    detail: String,
    errors: Option<Vec<String>>,
) -> Response {
    let body = ProblemDetails {
        problem_type: problem_type.to_string(),
        title: title.to_string(),
        status: status.as_u16(),
        detail,
        instance: None,
        errors,
    };

    (
        status,
        [("content-type", "application/problem+json")],
        axum::Json(body),
    )
        .into_response()
}
