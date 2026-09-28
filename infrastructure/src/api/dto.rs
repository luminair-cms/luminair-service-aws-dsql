//! Data transfer objects, envelope serialization, and JSON schema conversions.

use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use application::commands::documents::RelationAction;
use domain::auth::{AccessRequest, AccessRequestStatus};
use domain::common::{Email, Url};
use domain::content::{
    ContentValue, DocumentInstance, DocumentInstanceId, DomainValue, PrimitiveValue,
    PublicationState,
};
use domain::schema::{AttributeId, DocumentType, FieldType, PrimitiveType, SchemaRegistry};
use domain::system::LocaleId;

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
            email: req.email.as_ref().map(|e| e.as_ref().to_string()),
            name: req.name.as_ref().map(|n| n.as_ref().to_string()),
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

/// Helper to extract a list of `DocumentInstanceId`s from a JSON value.
/// Supports:
/// - A single UUID string: `"0192..."`
/// - A single object with an "id" field: `{"id": "0192..."}`
/// - An array of UUID strings or objects with "id": `["0192...", {"id": "0192..."}]`
pub fn parse_target_ids(val: &serde_json::Value) -> Result<Vec<DocumentInstanceId>, ApiError> {
    match val {
        serde_json::Value::String(s) => {
            let u = Uuid::parse_str(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid uuid '{s}': {e}")))?;
            Ok(vec![DocumentInstanceId::new(u)])
        }
        serde_json::Value::Object(obj) => {
            let id_val = obj.get("id").ok_or_else(|| {
                ApiError::BadRequest("expected 'id' field in relation object".into())
            })?;
            let s = id_val
                .as_str()
                .ok_or_else(|| ApiError::BadRequest("expected string for relation 'id'".into()))?;
            let u = Uuid::parse_str(s)
                .map_err(|e| ApiError::BadRequest(format!("invalid uuid '{s}': {e}")))?;
            Ok(vec![DocumentInstanceId::new(u)])
        }
        serde_json::Value::Array(arr) => {
            let mut ids = Vec::with_capacity(arr.len());
            for item in arr {
                let u = match item {
                    serde_json::Value::String(s) => Uuid::parse_str(s)
                        .map_err(|e| ApiError::BadRequest(format!("invalid uuid '{s}': {e}")))?,
                    serde_json::Value::Object(obj) => {
                        let id_val = obj.get("id").ok_or_else(|| {
                            ApiError::BadRequest("expected 'id' field in relation object".into())
                        })?;
                        let s = id_val.as_str().ok_or_else(|| {
                            ApiError::BadRequest("expected string for relation 'id'".into())
                        })?;
                        Uuid::parse_str(s)
                            .map_err(|e| ApiError::BadRequest(format!("invalid uuid '{s}': {e}")))?
                    }
                    _ => {
                        return Err(ApiError::BadRequest(
                            "expected string or object with 'id' for relation item".into(),
                        ));
                    }
                };
                ids.push(DocumentInstanceId::new(u));
            }
            Ok(ids)
        }
        _ => Err(ApiError::BadRequest(
            "expected string, object with 'id', or array of IDs for relation".into(),
        )),
    }
}

/// Parses a relational mutation action from a JSON value.
/// Supports Strapi action objects (`connect`, `disconnect`, `set`, `unset`)
/// as well as shorthands (UUID string, array of UUIDs, or null).
pub fn parse_relation_action(val: &serde_json::Value) -> Result<RelationAction, ApiError> {
    if val.is_null() {
        return Ok(RelationAction::Unset);
    }

    match val {
        serde_json::Value::String(_) | serde_json::Value::Array(_) => {
            let ids = parse_target_ids(val)?;
            Ok(RelationAction::Set(ids))
        }
        serde_json::Value::Object(map) => {
            let known_keys: Vec<&str> = ["connect", "disconnect", "set", "unset"]
                .into_iter()
                .filter(|k| map.contains_key(*k))
                .collect();

            if known_keys.len() > 1 {
                return Err(ApiError::BadRequest(format!(
                    "multiple relation actions specified ({known_keys:?}); specify at most one action per relation attribute"
                )));
            }

            if let Some(action_name) = known_keys.first() {
                match *action_name {
                    "connect" => {
                        let ids = parse_target_ids(&map["connect"])?;
                        Ok(RelationAction::Connect(ids))
                    }
                    "disconnect" => {
                        let ids = parse_target_ids(&map["disconnect"])?;
                        Ok(RelationAction::Disconnect(ids))
                    }
                    "set" => {
                        let ids = parse_target_ids(&map["set"])?;
                        Ok(RelationAction::Set(ids))
                    }
                    "unset" => {
                        if map["unset"].as_bool() == Some(true) {
                            Ok(RelationAction::Unset)
                        } else {
                            Err(ApiError::BadRequest(
                                "'unset' must be set to true (e.g. {\"unset\": true})".into(),
                            ))
                        }
                    }
                    _ => unreachable!(),
                }
            } else if map.contains_key("id") {
                let ids = parse_target_ids(val)?;
                Ok(RelationAction::Set(ids))
            } else {
                Err(ApiError::BadRequest(
                    "invalid relation action: expected 'connect', 'disconnect', 'set', 'unset', or shorthand target ID".into(),
                ))
            }
        }
        _ => Err(ApiError::BadRequest(
            "invalid relation format: expected object, string UUID, array of UUIDs, or null".into(),
        )),
    }
}

/// Parsed representation of document write payloads containing scalar fields and relational actions.
pub type ParsedPayload = (
    HashMap<AttributeId, ContentValue>,
    HashMap<AttributeId, RelationAction>,
);

/// Parses request JSON object body into scalar fields and relational actions.
pub fn parse_payload_from_json(
    body: &serde_json::Value,
    doc_type: &DocumentType,
    schema_reg: &SchemaRegistry,
) -> Result<ParsedPayload, ApiError> {
    let obj = body
        .as_object()
        .ok_or_else(|| ApiError::BadRequest("Request body must be a JSON object".into()))?;

    let mut fields = HashMap::new();
    let mut relations = HashMap::new();

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
        } else if schema_reg
            .find_relation_for_attr(&doc_type.id, &attr_id)
            .is_some()
        {
            let action = parse_relation_action(v)?;
            relations.insert(attr_id, action);
        } else {
            // Include unrecognized attribute so SchemaRegistry::validate_content can reject with domain error
            fields.insert(attr_id, ContentValue::Null);
        }
    }

    Ok((fields, relations))
}

/// Parses request JSON object body into domain `HashMap<AttributeId, ContentValue>`.
pub fn parse_fields_from_json(
    body: &serde_json::Value,
    doc_type: &DocumentType,
) -> Result<HashMap<AttributeId, ContentValue>, ApiError> {
    let empty_reg = SchemaRegistry::default();
    let (fields, _) = parse_payload_from_json(body, doc_type, &empty_reg)?;
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
    for def in &doc_type.fields {
        let attr_id = &def.id;
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::{
        DocumentKind, DocumentTypeId, DocumentTypeInfo, DocumentTypeOptions, FieldDefinition,
        OwnerRelationKind, Relation,
    };
    use indexmap::IndexSet;

    #[test]
    fn test_parse_target_ids_formats() {
        let u1 = Uuid::now_v7();
        let u2 = Uuid::now_v7();

        // 1. Single string UUID
        let single_str = json!(u1.to_string());
        let res = parse_target_ids(&single_str).unwrap();
        assert_eq!(res, vec![DocumentInstanceId::new(u1)]);

        // 2. Single object with "id"
        let single_obj = json!({ "id": u1.to_string() });
        let res = parse_target_ids(&single_obj).unwrap();
        assert_eq!(res, vec![DocumentInstanceId::new(u1)]);

        // 3. Array of strings and objects
        let arr = json!([u1.to_string(), { "id": u2.to_string() }]);
        let res = parse_target_ids(&arr).unwrap();
        assert_eq!(
            res,
            vec![DocumentInstanceId::new(u1), DocumentInstanceId::new(u2)]
        );

        // 4. Invalid UUID
        let invalid = json!("not-a-uuid");
        assert!(parse_target_ids(&invalid).is_err());

        // 5. Object missing "id"
        let missing_id = json!({ "name": "something" });
        assert!(parse_target_ids(&missing_id).is_err());
    }

    #[test]
    fn test_parse_relation_actions() {
        let u1 = Uuid::now_v7();
        let u2 = Uuid::now_v7();

        // Connect
        let connect_val = json!({ "connect": [u1.to_string()] });
        assert_eq!(
            parse_relation_action(&connect_val).unwrap(),
            RelationAction::Connect(vec![DocumentInstanceId::new(u1)])
        );

        // Disconnect
        let disc_val = json!({ "disconnect": [u1.to_string()] });
        assert_eq!(
            parse_relation_action(&disc_val).unwrap(),
            RelationAction::Disconnect(vec![DocumentInstanceId::new(u1)])
        );

        // Set
        let set_val = json!({ "set": [u1.to_string(), u2.to_string()] });
        assert_eq!(
            parse_relation_action(&set_val).unwrap(),
            RelationAction::Set(vec![
                DocumentInstanceId::new(u1),
                DocumentInstanceId::new(u2)
            ])
        );

        // Unset
        let unset_val = json!({ "unset": true });
        assert_eq!(
            parse_relation_action(&unset_val).unwrap(),
            RelationAction::Unset
        );

        let invalid_unset = json!({ "unset": false });
        assert!(parse_relation_action(&invalid_unset).is_err());

        // Shorthands
        assert_eq!(
            parse_relation_action(&serde_json::Value::Null).unwrap(),
            RelationAction::Unset
        );

        let shorthand_single = json!(u1.to_string());
        assert_eq!(
            parse_relation_action(&shorthand_single).unwrap(),
            RelationAction::Set(vec![DocumentInstanceId::new(u1)])
        );

        let shorthand_arr = json!([u1.to_string()]);
        assert_eq!(
            parse_relation_action(&shorthand_arr).unwrap(),
            RelationAction::Set(vec![DocumentInstanceId::new(u1)])
        );

        let shorthand_obj = json!({ "id": u1.to_string() });
        assert_eq!(
            parse_relation_action(&shorthand_obj).unwrap(),
            RelationAction::Set(vec![DocumentInstanceId::new(u1)])
        );

        // Multiple actions rejected
        let conflict = json!({
            "connect": [u1.to_string()],
            "disconnect": [u2.to_string()]
        });
        assert!(parse_relation_action(&conflict).is_err());
    }

    #[test]
    fn test_parse_payload_separates_fields_and_relations() {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let target_type_id = DocumentTypeId::try_new("tag").unwrap();
        let title_attr = AttributeId::try_new("title").unwrap();
        let tags_attr = AttributeId::try_new("tags").unwrap();

        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: title_attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![],
        });

        let doc_type = DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Article".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
        };

        let relation = Relation {
            id: domain::schema::RelationId::derive(&type_id, &tags_attr),
            owner_type: type_id.clone(),
            owner_attr: tags_attr.clone(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type: target_type_id,
            inverse: None,
        };

        let schema_reg = SchemaRegistry::new(vec![doc_type.clone()], vec![relation]);

        let tag_id = Uuid::now_v7();
        let payload = json!({
            "id": "ignored-id",
            "publication-status": "draft",
            "title": "Rust in Action",
            "tags": { "connect": [tag_id.to_string()] },
            "unknown-field": "value"
        });

        let (parsed_fields, parsed_relations) =
            parse_payload_from_json(&payload, &doc_type, &schema_reg).unwrap();

        // 1. Scalar field 'title' correctly parsed
        assert!(parsed_fields.contains_key(&title_attr));
        match parsed_fields.get(&title_attr).unwrap() {
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(s))) => {
                assert_eq!(s, "Rust in Action");
            }
            other => panic!("unexpected value: {other:?}"),
        }

        // 2. Relation 'tags' correctly separated into parsed_relations
        assert!(!parsed_fields.contains_key(&tags_attr));
        assert!(parsed_relations.contains_key(&tags_attr));
        assert_eq!(
            parsed_relations.get(&tags_attr).unwrap(),
            &RelationAction::Connect(vec![DocumentInstanceId::new(tag_id)])
        );

        // 3. Unknown field placed into fields as Null so domain validator can reject it
        let unknown_attr = AttributeId::try_new("unknown-field").unwrap();
        assert_eq!(parsed_fields.get(&unknown_attr), Some(&ContentValue::Null));
    }
}
