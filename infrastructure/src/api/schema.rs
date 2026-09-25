//! Schema introspection HTTP endpoints.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use domain::value_objects::DocumentTypeId;

use super::dto::{SingleResponse, document_type_to_json};
use super::errors::ApiError;
use super::state::AppState;
use crate::auth::AuthUser;

/// Lists all registered document types.
pub async fn list_types(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let types: Vec<serde_json::Value> = state
        .schema_registry
        .all_types()
        .map(document_type_to_json)
        .collect();

    Ok(axum::Json(SingleResponse::new(types)).into_response())
}

/// Returns the schema definition for a specific document type by its singular name.
pub async fn get_type(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let type_id = DocumentTypeId::try_new(&id)
        .map_err(|_| ApiError::NotFound(format!("document type '{id}' was not found")))?;

    let doc_type = state
        .schema_registry
        .find_type(&type_id)
        .ok_or_else(|| ApiError::NotFound(format!("document type '{id}' was not found")))?;

    Ok(axum::Json(SingleResponse::new(document_type_to_json(doc_type))).into_response())
}
