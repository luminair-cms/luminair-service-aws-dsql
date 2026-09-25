//! Data transfer objects, envelope serialization, and JSON schema conversions.

use std::collections::HashMap;

use chrono::Utc;
use domain::entities::auth::access_request::{AccessRequest, AccessRequestStatus};
use domain::entities::document_instance::{DocumentInstance, PublicationState};
use domain::entities::document_type::DocumentType;
use domain::entities::published_snapshot::PublishedSnapshot;
use domain::types::content_value::ContentValue;
use domain::types::domain_value::DomainValue;
use domain::types::field_type::{FieldType, PrimitiveType};
use domain::types::primitive_value::PrimitiveValue;
use domain::value_objects::{AttributeId, Email, LocaleId, Url};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::errors::ApiError;

/// Envelope for single resource endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleResponse<T> {
    pub data: T,
}

impl<T> SingleResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

/// Envelope for collection list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionResponse<T> {
    pub data: Vec<T>,
    pub meta: CollectionMeta,
}

impl<T> CollectionResponse<T> {
    pub fn new(data: Vec<T>, page: u32, page_size: u32, total: u64) -> Self {
        Self {
            data,
            meta: CollectionMeta {
                pagination: PaginationMeta {
                    page,
                    page_size,
                    total,
                },
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMeta {
    pub pagination: PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

/// DTO for access request representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessRequestDto {
    pub id: String,
    pub user_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub status: String,
    pub requested_at: String,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub assigned_roles: Vec<String>,
}

impl From<&AccessRequest> for AccessRequestDto {
    fn from(req: &AccessRequest) -> Self {
        let (status, rejection_reason) = match &req.status {
            AccessRequestStatus::Pending => ("pending", None),
            AccessRequestStatus::Approved => ("approved", None),
            AccessRequestStatus::Rejected { reason } => ("rejected", reason.clone()),
        };
        Self {
            id: req.id.as_ref().to_string(),
            user_id: req.user_id.as_ref().to_string(),
            email: req.email.clone(),
            name: req.name.clone(),
            status: status.to_string(),
            requested_at: req.requested_at.to_rfc3339(),
            reviewed_at: req.reviewed_at.map(|t| t.to_rfc3339()),
            reviewed_by: req.reviewed_by.as_ref().map(|u| u.as_ref().to_string()),
            rejection_reason,
            assigned_roles: req
                .assigned_roles
                .iter()
                .map(|r| r.as_ref().to_string())
                .collect(),
        }
    }
}

/// Converts a `DocumentInstance` to its API JSON representation.
pub fn document_to_json(
    instance: &DocumentInstance,
    _doc_type: &DocumentType,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), json!(instance.id.as_ref().to_string()));

    let status_str = match &instance.content.publication_state {
        PublicationState::Draft { .. } => "draft",
        PublicationState::Published { .. } => "published",
    };
    map.insert("publication-status".to_string(), json!(status_str));
    map.insert(
        "created-at".to_string(),
        json!(instance.audit.created_at.to_rfc3339()),
    );
    map.insert(
        "updated-at".to_string(),
        json!(instance.audit.updated_at.to_rfc3339()),
    );
    map.insert("version".to_string(), json!(instance.audit.version));

    for (attr_id, val) in &instance.content.fields {
        map.insert(attr_id.as_ref().to_string(), content_value_to_json(val));
    }

    for (attr_id, related_list) in &instance.populated_relations {
        let related_json: Vec<serde_json::Value> = related_list
            .iter()
            .map(|rel| {
                let mut rel_map = serde_json::Map::new();
                rel_map.insert("id".to_string(), json!(rel.id.as_ref().to_string()));
                for (r_attr, r_val) in &rel.content.fields {
                    rel_map.insert(r_attr.as_ref().to_string(), content_value_to_json(r_val));
                }
                serde_json::Value::Object(rel_map)
            })
            .collect();
        map.insert(
            attr_id.as_ref().to_string(),
            serde_json::Value::Array(related_json),
        );
    }

    serde_json::Value::Object(map)
}

/// Converts a `PublishedSnapshot` to its API JSON representation.
pub fn snapshot_to_json(
    snapshot: &PublishedSnapshot,
    _doc_type: &DocumentType,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), json!(snapshot.id.as_ref().to_string()));
    map.insert(
        "document-id".to_string(),
        json!(snapshot.instance_id.as_ref().to_string()),
    );
    map.insert("type-name".to_string(), json!(snapshot.type_name));
    map.insert("revision".to_string(), json!(snapshot.revision));
    map.insert(
        "published-at".to_string(),
        json!(snapshot.published_at.to_rfc3339()),
    );
    map.insert(
        "published-by".to_string(),
        json!(
            snapshot
                .published_by
                .as_ref()
                .map(|u| u.as_ref().to_string())
        ),
    );

    for (attr_id, val) in &snapshot.fields {
        map.insert(attr_id.as_ref().to_string(), content_value_to_json(val));
    }

    serde_json::Value::Object(map)
}

/// Serializes a domain `ContentValue` to `serde_json::Value`.
pub fn content_value_to_json(val: &ContentValue) -> serde_json::Value {
    match val {
        ContentValue::Null => serde_json::Value::Null,
        ContentValue::LocalizedText(map) => {
            let mut json_map = serde_json::Map::new();
            for (loc, text) in map {
                json_map.insert(
                    loc.as_ref().to_string(),
                    serde_json::Value::String(text.clone()),
                );
            }
            serde_json::Value::Object(json_map)
        }
        ContentValue::Scalar(domain_val) => match domain_val {
            DomainValue::Primitive(p) => match p {
                PrimitiveValue::Text(s) => serde_json::Value::String(s.clone()),
                PrimitiveValue::Uid(s) => serde_json::Value::String(s.clone()),
                PrimitiveValue::Uuid(u) => serde_json::Value::String(u.to_string()),
                PrimitiveValue::Integer(i) => json!(i),
                PrimitiveValue::Decimal(d) => json!(d.to_string()),
                PrimitiveValue::Boolean(b) => serde_json::Value::Bool(*b),
                PrimitiveValue::Date(d) => serde_json::Value::String(d.to_string()),
                PrimitiveValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
            },
            DomainValue::Email(e) => serde_json::Value::String(e.as_ref().to_string()),
            DomainValue::Url(u) => serde_json::Value::String(u.as_ref().to_string()),
            DomainValue::Json(map) => {
                let mut json_map = serde_json::Map::new();
                for (k, v) in map {
                    let primitive_val = match v {
                        PrimitiveValue::Text(s) => serde_json::Value::String(s.clone()),
                        PrimitiveValue::Uid(s) => serde_json::Value::String(s.clone()),
                        PrimitiveValue::Uuid(u) => serde_json::Value::String(u.to_string()),
                        PrimitiveValue::Integer(i) => json!(i),
                        PrimitiveValue::Decimal(d) => json!(d.to_string()),
                        PrimitiveValue::Boolean(b) => serde_json::Value::Bool(*b),
                        PrimitiveValue::Date(d) => serde_json::Value::String(d.to_string()),
                        PrimitiveValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
                    };
                    json_map.insert(k.clone(), primitive_val);
                }
                serde_json::Value::Object(json_map)
            }
        },
    }
}

/// Parses request JSON object body into domain `HashMap<AttributeId, ContentValue>`.
pub fn parse_fields_from_json(
    body: &serde_json::Value,
    doc_type: &DocumentType,
) -> Result<HashMap<AttributeId, ContentValue>, ApiError> {
    let obj = body
        .as_object()
        .ok_or_else(|| ApiError::BadRequest("Request body must be a JSON object".into()))?;

    let mut fields = HashMap::new();

    for (k, v) in obj {
        // Skip metadata fields that might be passed in payloads
        if k == "id"
            || k == "publication-status"
            || k == "created-at"
            || k == "updated-at"
            || k == "version"
        {
            continue;
        }

        let attr_id = AttributeId::try_new(k).map_err(|e| ApiError::BadRequest(e.to_string()))?;

        if let Some(field_def) = doc_type.fields.get(&attr_id) {
            let content_val = json_to_content_value(v, &field_def.field_type)?;
            fields.insert(attr_id, content_val);
        } else {
            // Include unrecognized attribute so SchemaRegistry::validate_content can reject with domain error
            fields.insert(attr_id, ContentValue::Null);
        }
    }

    Ok(fields)
}

/// Converts a single `serde_json::Value` to domain `ContentValue` based on expected `FieldType`.
pub fn json_to_content_value(
    val: &serde_json::Value,
    field_type: &FieldType,
) -> Result<ContentValue, ApiError> {
    if val.is_null() {
        return Ok(ContentValue::Null);
    }

    match field_type {
        FieldType::LocalizedText => {
            let obj = val.as_object().ok_or_else(|| {
                ApiError::BadRequest(
                    "expected JSON object with locale keys for localized text".into(),
                )
            })?;
            let mut map = HashMap::new();
            for (k, v) in obj {
                let loc = LocaleId::try_new(k).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                let s = v.as_str().ok_or_else(|| {
                    ApiError::BadRequest(format!("expected string for locale '{k}'"))
                })?;
                map.insert(loc, s.to_string());
            }
            Ok(ContentValue::LocalizedText(map))
        }
        FieldType::Primitive(PrimitiveType::Text) => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected string for text field".into()))?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Text(s.to_string()),
            )))
        }
        FieldType::Primitive(PrimitiveType::Uid) => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected string for uid field".into()))?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Uid(s.to_string()),
            )))
        }
        FieldType::Primitive(PrimitiveType::Uuid) => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected string for uuid field".into()))?;
            let u = Uuid::parse_str(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid uuid: {e}")))?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Uuid(u),
            )))
        }
        FieldType::Primitive(PrimitiveType::Integer(size)) => {
            let i = val.as_i64().ok_or_else(|| {
                ApiError::BadRequest(format!("expected integer for {size:?} field"))
            })?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Integer(i),
            )))
        }
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
            let d_str = if let Some(s) = val.as_str() {
                s.to_string()
            } else if let Some(n) = val.as_f64() {
                n.to_string()
            } else if let Some(i) = val.as_i64() {
                i.to_string()
            } else {
                return Err(ApiError::BadRequest(
                    "expected decimal number or string".into(),
                ));
            };
            let d = d_str.parse::<Decimal>().map_err(|e| {
                ApiError::BadRequest(format!("invalid decimal value '{d_str}': {e}"))
            })?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Decimal(d),
            )))
        }
        FieldType::Primitive(PrimitiveType::Boolean) => {
            let b = val
                .as_bool()
                .ok_or_else(|| ApiError::BadRequest("expected boolean value".into()))?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Boolean(b),
            )))
        }
        FieldType::Primitive(PrimitiveType::Date) => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected date string YYYY-MM-DD".into()))?;
            let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
                ApiError::BadRequest(format!("invalid date '{s}' (expected YYYY-MM-DD): {e}"))
            })?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Date(d),
            )))
        }
        FieldType::Primitive(PrimitiveType::DateTime) => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected RFC 3339 datetime string".into()))?;
            let dt = chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid datetime '{s}': {e}")))?
                .with_timezone(&Utc);
            Ok(ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::DateTime(dt),
            )))
        }
        FieldType::Email => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected email string".into()))?;
            let email = Email::try_new(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid email address: {e}")))?;
            Ok(ContentValue::Scalar(DomainValue::Email(email)))
        }
        FieldType::Url => {
            let s = val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected url string".into()))?;
            let url =
                Url::try_new(s).map_err(|e| ApiError::BadRequest(format!("invalid url: {e}")))?;
            Ok(ContentValue::Scalar(DomainValue::Url(url)))
        }
        FieldType::Json => {
            let obj = val.as_object().ok_or_else(|| {
                ApiError::BadRequest("expected JSON object for json field".into())
            })?;
            let mut map = HashMap::new();
            for (k, v) in obj {
                let p = match v {
                    serde_json::Value::String(s) => PrimitiveValue::Text(s.clone()),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            PrimitiveValue::Integer(i)
                        } else if let Some(f) = n.as_f64() {
                            PrimitiveValue::Decimal(
                                f.to_string().parse::<Decimal>().unwrap_or(Decimal::ZERO),
                            )
                        } else {
                            PrimitiveValue::Text(n.to_string())
                        }
                    }
                    serde_json::Value::Bool(b) => PrimitiveValue::Boolean(*b),
                    _ => PrimitiveValue::Text(v.to_string()),
                };
                map.insert(k.clone(), p);
            }
            Ok(ContentValue::Scalar(DomainValue::Json(map)))
        }
    }
}

/// Converts a URL query string value to a `DomainValue` based on `FieldType`.
pub fn string_to_domain_value(s: &str, field_type: &FieldType) -> Result<DomainValue, ApiError> {
    match field_type {
        FieldType::Primitive(PrimitiveType::Integer(_)) => {
            let i = s.parse::<i64>().map_err(|e| {
                ApiError::BadRequest(format!("invalid integer query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Primitive(PrimitiveValue::Integer(i)))
        }
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
            let d = s.parse::<Decimal>().map_err(|e| {
                ApiError::BadRequest(format!("invalid decimal query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Primitive(PrimitiveValue::Decimal(d)))
        }
        FieldType::Primitive(PrimitiveType::Boolean) => {
            let b = s.parse::<bool>().map_err(|e| {
                ApiError::BadRequest(format!("invalid boolean query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Primitive(PrimitiveValue::Boolean(b)))
        }
        FieldType::Primitive(PrimitiveType::Uuid) => {
            let u = Uuid::parse_str(s).map_err(|e| {
                ApiError::BadRequest(format!("invalid uuid query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Primitive(PrimitiveValue::Uuid(u)))
        }
        FieldType::Primitive(PrimitiveType::Date) => {
            let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
                ApiError::BadRequest(format!("invalid date query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Primitive(PrimitiveValue::Date(d)))
        }
        FieldType::Primitive(PrimitiveType::DateTime) => {
            let dt = chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| {
                    ApiError::BadRequest(format!("invalid datetime query param '{s}': {e}"))
                })?
                .with_timezone(&Utc);
            Ok(DomainValue::Primitive(PrimitiveValue::DateTime(dt)))
        }
        FieldType::Primitive(PrimitiveType::Uid) => {
            Ok(DomainValue::Primitive(PrimitiveValue::Uid(s.to_string())))
        }
        FieldType::Email => {
            let email = Email::try_new(s).map_err(|e| {
                ApiError::BadRequest(format!("invalid email query param '{s}': {e}"))
            })?;
            Ok(DomainValue::Email(email))
        }
        FieldType::Url => {
            let url = Url::try_new(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid url query param '{s}': {e}")))?;
            Ok(DomainValue::Url(url))
        }
        _ => Ok(DomainValue::Primitive(PrimitiveValue::Text(s.to_string()))),
    }
}

/// Converts a `DocumentType` to its introspection API JSON representation.
pub fn document_type_to_json(doc_type: &DocumentType) -> serde_json::Value {
    let mut attr_map = serde_json::Map::new();
    for (attr_id, def) in &doc_type.fields {
        let mut field_info = serde_json::Map::new();
        field_info.insert("type".to_string(), json!(format!("{:?}", def.field_type)));
        field_info.insert("required".to_string(), json!(def.required));
        field_info.insert("unique".to_string(), json!(def.unique));
        attr_map.insert(
            attr_id.as_ref().to_string(),
            serde_json::Value::Object(field_info),
        );
    }

    json!({
        "id": doc_type.id.as_ref(),
        "kind": format!("{:?}", doc_type.kind).to_lowercase(),
        "info": {
            "displayName": doc_type.info.title,
            "singularName": doc_type.info.singular_name,
            "pluralName": doc_type.info.plural_name,
            "description": doc_type.info.description,
        },
        "options": {
            "draftAndPublish": doc_type.options.draft_and_publish,
        },
        "attributes": attr_map
    })
}
