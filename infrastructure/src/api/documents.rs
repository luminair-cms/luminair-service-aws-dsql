//! Dynamic collection and singleton document instance HTTP handlers.

use std::collections::HashMap;

use application::commands::documents::{
    CreateDocumentCommand, DeleteDocumentCommand, FindByIdCommand, FindDocumentsCommand,
    ListSnapshotsCommand, PublishDocumentCommand, UnpublishDocumentCommand, UpdateDocumentCommand,
};
use application::services::documents::DocumentsService;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::entities::document_type::{DocumentKind, DocumentType};
use domain::ports::document_instance_repository::{FieldFilter, Pagination};
use domain::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId};
use uuid::Uuid;

use super::dto::{
    CollectionResponse, SingleResponse, document_to_json, parse_fields_from_json, snapshot_to_json,
    string_to_domain_value,
};
use super::errors::ApiError;
use super::state::AppState;
use crate::auth::AuthUser;

fn parse_pagination(params: &HashMap<String, String>) -> Pagination {
    let page = params
        .get("page")
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    let page_size = params
        .get("page_size")
        .or_else(|| params.get("pageSize"))
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(25)
        .clamp(1, 100);
    Pagination { page, page_size }
}

fn parse_populate(params: &HashMap<String, String>) -> Option<Vec<AttributeId>> {
    params.get("populate").map(|s| {
        s.split(',')
            .filter_map(|part| AttributeId::try_new(part.trim()).ok())
            .collect()
    })
}

fn parse_filters(
    params: &HashMap<String, String>,
    doc_type: &DocumentType,
) -> Result<Vec<FieldFilter>, ApiError> {
    let mut filters = Vec::new();
    for (k, v) in params {
        if k == "page" || k == "page_size" || k == "pageSize" || k == "populate" {
            continue;
        }

        let attr_name = if let Some(stripped) = k.strip_prefix("filters[") {
            if let Some(end_idx) = stripped.find(']') {
                &stripped[..end_idx]
            } else {
                continue;
            }
        } else if doc_type.fields.keys().any(|a| a.as_ref() == k) {
            k.as_str()
        } else {
            continue;
        };

        if let Ok(attr_id) = AttributeId::try_new(attr_name)
            && let Some(field_def) = doc_type.fields.get(&attr_id)
        {
            let domain_val = string_to_domain_value(v, &field_def.field_type)?;
            filters.push(FieldFilter {
                attribute_id: attr_id,
                value: domain_val,
            });
        }
    }
    Ok(filters)
}

// ----------------------------------------------------------------------------
// Root Slug Handlers: /api/{slug}
// ----------------------------------------------------------------------------

/// Handles GET /api/{slug}:
/// - If `slug` matches a collection (pluralName), lists paginated documents.
/// - If `slug` matches a single-type (singularName), gets the unique singleton document.
pub async fn handle_root_get(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    // 1. Check if slug matches Collection pluralName
    if let Some(doc_type) = state.schema_registry.find_type_by_name(&slug)
        && doc_type.kind == DocumentKind::Collection
    {
        let pagination = parse_pagination(&params);
        let populate = parse_populate(&params);
        let filters = parse_filters(&params, doc_type)?;

        let mut cmd =
            FindDocumentsCommand::new(doc_type.id.clone(), pagination).with_filters(filters);
        if let Some(pop) = populate {
            cmd = cmd.with_populate(pop);
        }

        let (instances, total) = state.documents_service.find(&auth.caller, cmd).await?;

        let items: Vec<serde_json::Value> = instances
            .iter()
            .map(|inst| document_to_json(inst, doc_type))
            .collect();

        return Ok(axum::Json(CollectionResponse::new(
            items,
            pagination.page,
            pagination.page_size,
            total,
        ))
        .into_response());
    }

    // 2. Check if slug matches SingleType singularName
    if let Ok(type_id) = DocumentTypeId::try_new(&slug)
        && let Some(doc_type) = state.schema_registry.find_type(&type_id)
        && doc_type.kind == DocumentKind::SingleType
    {
        let populate = parse_populate(&params);
        let mut cmd = FindDocumentsCommand::new(
            type_id.clone(),
            Pagination {
                page: 1,
                page_size: 1,
            },
        );
        if let Some(pop) = populate {
            cmd = cmd.with_populate(pop);
        }

        let (instances, _) = state.documents_service.find(&auth.caller, cmd).await?;

        let inst = instances
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::NotFound(format!("singleton '{slug}' has no content yet")))?;

        return Ok(
            axum::Json(SingleResponse::new(document_to_json(&inst, doc_type))).into_response(),
        );
    }

    Err(ApiError::NotFound(format!(
        "document type '{slug}' was not found"
    )))
}

/// Handles POST /api/{slug}:
/// - Creates a new draft document instance in a collection.
pub async fn handle_root_post(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    if let Some(doc_type) = state.schema_registry.find_type_by_name(&slug)
        && doc_type.kind == DocumentKind::Collection
    {
        let fields = parse_fields_from_json(&body, doc_type)?;
        let cmd = CreateDocumentCommand::new(doc_type.id.clone(), fields);
        let instance = state.documents_service.create(&auth.caller, cmd).await?;

        return Ok((
            StatusCode::CREATED,
            axum::Json(SingleResponse::new(document_to_json(&instance, doc_type))),
        )
            .into_response());
    }

    if let Ok(type_id) = DocumentTypeId::try_new(&slug)
        && let Some(doc_type) = state.schema_registry.find_type(&type_id)
        && doc_type.kind == DocumentKind::SingleType
    {
        return Err(ApiError::BadRequest(format!(
            "document type '{slug}' is a single-type; use PUT /api/{slug} to create or update"
        )));
    }

    Err(ApiError::NotFound(format!(
        "document type '{slug}' was not found"
    )))
}

/// Handles PUT /api/{slug}:
/// - Upserts the singleton document instance for a single-type.
pub async fn handle_root_put(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    if let Ok(type_id) = DocumentTypeId::try_new(&slug)
        && let Some(doc_type) = state.schema_registry.find_type(&type_id)
        && doc_type.kind == DocumentKind::SingleType
    {
        let fields = parse_fields_from_json(&body, doc_type)?;

        // Check if singleton instance already exists
        let (existing, _) = state
            .documents_service
            .find(
                &auth.caller,
                FindDocumentsCommand::new(
                    type_id.clone(),
                    Pagination {
                        page: 1,
                        page_size: 1,
                    },
                ),
            )
            .await?;

        if let Some(inst) = existing.into_iter().next() {
            let update_cmd = UpdateDocumentCommand::new(inst.id, type_id, fields);
            let updated = state
                .documents_service
                .update(&auth.caller, update_cmd)
                .await?;
            return Ok(
                axum::Json(SingleResponse::new(document_to_json(&updated, doc_type)))
                    .into_response(),
            );
        } else {
            let create_cmd = CreateDocumentCommand::new(type_id, fields);
            let created = state
                .documents_service
                .create(&auth.caller, create_cmd)
                .await?;
            return Ok((
                StatusCode::CREATED,
                axum::Json(SingleResponse::new(document_to_json(&created, doc_type))),
            )
                .into_response());
        }
    }

    if let Some(_doc_type) = state.schema_registry.find_type_by_name(&slug) {
        return Err(ApiError::BadRequest(format!(
            "cannot PUT collection root '/api/{slug}'; specify entry id '/api/{slug}/{{id}}'"
        )));
    }

    Err(ApiError::NotFound(format!(
        "document type '{slug}' was not found"
    )))
}

/// Handles DELETE /api/{slug}:
/// - Deletes the singleton document instance for a single-type.
pub async fn handle_root_delete(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    if let Ok(type_id) = DocumentTypeId::try_new(&slug)
        && let Some(_doc_type) = state.schema_registry.find_type(&type_id)
        && _doc_type.kind == DocumentKind::SingleType
    {
        let (existing, _) = state
            .documents_service
            .find(
                &auth.caller,
                FindDocumentsCommand::new(
                    type_id.clone(),
                    Pagination {
                        page: 1,
                        page_size: 1,
                    },
                ),
            )
            .await?;

        if let Some(inst) = existing.into_iter().next() {
            state
                .documents_service
                .delete(&auth.caller, DeleteDocumentCommand::new(inst.id, type_id))
                .await?;
        }
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    if let Some(_doc_type) = state.schema_registry.find_type_by_name(&slug) {
        return Err(ApiError::BadRequest(format!(
            "cannot DELETE collection root '/api/{slug}'; specify entry id '/api/{slug}/{{id}}'"
        )));
    }

    Err(ApiError::NotFound(format!(
        "document type '{slug}' was not found"
    )))
}

// ----------------------------------------------------------------------------
// Singleton Workflow Handlers: /api/{slug}/publish, /unpublish, /snapshots
// ----------------------------------------------------------------------------

/// Publishes the singleton instance for a single-type.
pub async fn handle_singleton_publish(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let type_id = DocumentTypeId::try_new(&slug)
        .map_err(|_| ApiError::NotFound(format!("document type '{slug}' was not found")))?;
    let doc_type = state
        .schema_registry
        .find_type(&type_id)
        .ok_or_else(|| ApiError::NotFound(format!("document type '{slug}' was not found")))?;

    if doc_type.kind != DocumentKind::SingleType {
        return Err(ApiError::BadRequest(format!(
            "'{slug}' is a collection; use POST /api/{slug}/{{id}}/publish"
        )));
    }

    let (existing, _) = state
        .documents_service
        .find(
            &auth.caller,
            FindDocumentsCommand::new(
                type_id.clone(),
                Pagination {
                    page: 1,
                    page_size: 1,
                },
            ),
        )
        .await?;

    let inst = existing.into_iter().next().ok_or_else(|| {
        ApiError::NotFound(format!("singleton '{slug}' has no content to publish"))
    })?;

    let snapshot = state
        .documents_service
        .publish(&auth.caller, PublishDocumentCommand::new(inst.id, type_id))
        .await?;

    Ok(axum::Json(SingleResponse::new(snapshot_to_json(&snapshot, doc_type))).into_response())
}

/// Unpublishes the singleton instance for a single-type.
pub async fn handle_singleton_unpublish(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let type_id = DocumentTypeId::try_new(&slug)
        .map_err(|_| ApiError::NotFound(format!("document type '{slug}' was not found")))?;
    let doc_type = state
        .schema_registry
        .find_type(&type_id)
        .ok_or_else(|| ApiError::NotFound(format!("document type '{slug}' was not found")))?;

    if doc_type.kind != DocumentKind::SingleType {
        return Err(ApiError::BadRequest(format!(
            "'{slug}' is a collection; use POST /api/{slug}/{{id}}/unpublish"
        )));
    }

    let (existing, _) = state
        .documents_service
        .find(
            &auth.caller,
            FindDocumentsCommand::new(
                type_id.clone(),
                Pagination {
                    page: 1,
                    page_size: 1,
                },
            ),
        )
        .await?;

    let inst = existing.into_iter().next().ok_or_else(|| {
        ApiError::NotFound(format!("singleton '{slug}' has no content to unpublish"))
    })?;

    let updated = state
        .documents_service
        .unpublish(
            &auth.caller,
            UnpublishDocumentCommand::new(inst.id, type_id),
        )
        .await?;

    Ok(axum::Json(SingleResponse::new(document_to_json(&updated, doc_type))).into_response())
}

/// Lists published snapshots for a singleton instance.
pub async fn handle_singleton_snapshots(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let type_id = DocumentTypeId::try_new(&slug)
        .map_err(|_| ApiError::NotFound(format!("document type '{slug}' was not found")))?;
    let doc_type = state
        .schema_registry
        .find_type(&type_id)
        .ok_or_else(|| ApiError::NotFound(format!("document type '{slug}' was not found")))?;

    if doc_type.kind != DocumentKind::SingleType {
        return Err(ApiError::BadRequest(format!(
            "'{slug}' is a collection; use GET /api/{slug}/{{id}}/snapshots"
        )));
    }

    let (existing, _) = state
        .documents_service
        .find(
            &auth.caller,
            FindDocumentsCommand::new(
                type_id.clone(),
                Pagination {
                    page: 1,
                    page_size: 1,
                },
            ),
        )
        .await?;

    let inst = existing
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::NotFound(format!("singleton '{slug}' has no content yet")))?;

    let snapshots = state
        .documents_service
        .list_snapshots(&auth.caller, ListSnapshotsCommand::new(type_id, inst.id))
        .await?;

    let items: Vec<serde_json::Value> = snapshots
        .iter()
        .map(|s| snapshot_to_json(s, doc_type))
        .collect();

    Ok(axum::Json(SingleResponse::new(items)).into_response())
}

// ----------------------------------------------------------------------------
// Collection Item Handlers: /api/{slug}/{id}
// ----------------------------------------------------------------------------

/// Fetches a single collection document instance by UUID.
pub async fn handle_collection_get(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let populate = parse_populate(&params);
    let mut cmd = FindByIdCommand::new(doc_type.id.clone(), inst_id);
    if let Some(pop) = populate {
        cmd = cmd.with_populate(pop);
    }

    let instance = state
        .documents_service
        .find_by_id(&auth.caller, cmd)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("document instance with id '{id}' was not found"))
        })?;

    Ok(axum::Json(SingleResponse::new(document_to_json(&instance, doc_type))).into_response())
}

/// Updates an existing collection document instance by UUID.
pub async fn handle_collection_put(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let fields = parse_fields_from_json(&body, doc_type)?;
    let cmd = UpdateDocumentCommand::new(inst_id, doc_type.id.clone(), fields);

    let updated = state.documents_service.update(&auth.caller, cmd).await?;

    Ok(axum::Json(SingleResponse::new(document_to_json(&updated, doc_type))).into_response())
}

/// Deletes a collection document instance by UUID.
pub async fn handle_collection_delete(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let cmd = DeleteDocumentCommand::new(inst_id, doc_type.id.clone());
    state.documents_service.delete(&auth.caller, cmd).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Publishes a collection document instance by UUID.
pub async fn handle_collection_publish(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let cmd = PublishDocumentCommand::new(inst_id, doc_type.id.clone());
    let snapshot = state.documents_service.publish(&auth.caller, cmd).await?;

    Ok(axum::Json(SingleResponse::new(snapshot_to_json(&snapshot, doc_type))).into_response())
}

/// Unpublishes a collection document instance by UUID.
pub async fn handle_collection_unpublish(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let cmd = UnpublishDocumentCommand::new(inst_id, doc_type.id.clone());
    let updated = state.documents_service.unpublish(&auth.caller, cmd).await?;

    Ok(axum::Json(SingleResponse::new(document_to_json(&updated, doc_type))).into_response())
}

/// Lists published snapshots for a collection document instance.
pub async fn handle_collection_snapshots(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let doc_type = state
        .schema_registry
        .find_type_by_name(&slug)
        .ok_or_else(|| ApiError::NotFound(format!("collection '{slug}' was not found")))?;

    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid instance id: {e}")))?;
    let inst_id = DocumentInstanceId::new(uuid);

    let cmd = ListSnapshotsCommand::new(doc_type.id.clone(), inst_id);
    let snapshots = state
        .documents_service
        .list_snapshots(&auth.caller, cmd)
        .await?;

    let items: Vec<serde_json::Value> = snapshots
        .iter()
        .map(|s| snapshot_to_json(s, doc_type))
        .collect();

    Ok(axum::Json(SingleResponse::new(items)).into_response())
}
