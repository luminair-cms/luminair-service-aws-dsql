//! Access request and user onboarding endpoints.

use application::commands::access_requests::{
    ApproveAccessRequestCommand, RejectAccessRequestCommand, SubmitAccessRequestCommand,
};
use application::services::access_requests::AccessRequestsService;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::value_objects::{AccessRequestId, Email, RoleId};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::dto::{AccessRequestDto, SingleResponse};
use super::errors::ApiError;
use super::state::AppState;
use crate::auth::{AuthUser, AuthenticatedClaims};

#[derive(Debug, Deserialize)]
pub struct SubmitRequestBody {
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApproveRequestBody {
    #[serde(alias = "roleIds")]
    pub role_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectRequestBody {
    pub reason: Option<String>,
}

/// Submits an access request for the authenticated OIDC user.
pub async fn submit(
    AuthenticatedClaims(claims): AuthenticatedClaims,
    State(state): State<AppState>,
    body: Option<axum::Json<SubmitRequestBody>>,
) -> Result<Response, ApiError> {
    let user_id = claims.user_id()?;

    let (email, name) = match body {
        Some(axum::Json(b)) => (
            b.email.or(claims.email.clone()),
            b.name.or(claims.name.clone()),
        ),
        None => (claims.email.clone(), claims.name.clone()),
    };

    if let Some(ref e) = email {
        Email::try_new(e).map_err(|err| ApiError::BadRequest(format!("invalid email: {err}")))?;
    }

    let cmd = SubmitAccessRequestCommand {
        user_id,
        email,
        name,
    };

    let request = state.access_requests_service.submit(cmd).await?;

    Ok((
        StatusCode::CREATED,
        axum::Json(SingleResponse::new(AccessRequestDto::from(&request))),
    )
        .into_response())
}

/// Lists all pending access requests awaiting administrator review.
pub async fn list_pending(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let requests = state
        .access_requests_service
        .list_pending(&auth.caller)
        .await?;

    let dtos: Vec<AccessRequestDto> = requests.iter().map(AccessRequestDto::from).collect();

    Ok(axum::Json(SingleResponse::new(dtos)).into_response())
}

/// Approves a pending access request and assigns specified roles.
pub async fn approve(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ApproveRequestBody>,
) -> Result<Response, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid access request id: {e}")))?;
    let access_request_id = AccessRequestId::new(uuid);

    let mut role_ids = Vec::with_capacity(body.role_ids.len());
    for r_str in body.role_ids {
        let r_uuid = Uuid::parse_str(&r_str)
            .map_err(|e| ApiError::BadRequest(format!("invalid role id '{r_str}': {e}")))?;
        role_ids.push(RoleId::new(r_uuid));
    }

    let cmd = ApproveAccessRequestCommand {
        request_id: access_request_id,
        role_ids,
    };

    let assignments = state
        .access_requests_service
        .approve(&auth.caller, cmd)
        .await?;

    let assigned_dtos: Vec<serde_json::Value> = assignments
        .iter()
        .map(|a| {
            json!({
                "id": a.id.as_ref().to_string(),
                "userId": a.user_id.as_ref().to_string(),
                "roleId": a.role_id.as_ref().to_string(),
                "grantedAt": a.granted_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(axum::Json(SingleResponse::new(json!({
        "assignedRoles": assigned_dtos
    })))
    .into_response())
}

/// Rejects a pending access request with an optional note.
pub async fn reject(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<axum::Json<RejectRequestBody>>,
) -> Result<Response, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid access request id: {e}")))?;
    let access_request_id = AccessRequestId::new(uuid);

    let reason = body.and_then(|axum::Json(b)| b.reason);

    let cmd = RejectAccessRequestCommand {
        request_id: access_request_id,
        reason,
    };

    let request = state
        .access_requests_service
        .reject(&auth.caller, cmd)
        .await?;

    Ok(axum::Json(SingleResponse::new(AccessRequestDto::from(&request))).into_response())
}
