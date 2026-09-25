//! JSON Schema File Loader and Invariant Validator.
//!
//! Loads runtime-immutable schema files:
//! - `schema/document-types/{singularName}.json`
//! - `schema/relations/*.json`
//! - `schema/system-config.json`
//!
//! Enforces:
//! 1. File name strictly equals `singularName.json`
//! 2. Valid kebab-case identifiers for types and attributes
//! 3. No SQL reserved keywords for table or column names
//! 4. Constraint applicability per field type
//! 5. Valid relation endpoints (owner and target types must exist)
//! 6. Preserves declared attribute order via `IndexMap`

use domain::entities::document_type::{
    DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions,
};
use domain::entities::field_definition::{FieldConstraint, FieldDefinition};
use domain::entities::relation::{OwnerRelationKind, Relation, RelationInverse};
use domain::entities::system_config::SystemConfig;
use domain::services::schema_registry::SchemaRegistry;
use domain::types::field_type::{FieldType, IntegerSize, PrimitiveType};
use domain::value_objects::{AttributeId, DocumentTypeId, LocaleId, RelationId, SystemConfigId};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

use super::naming::is_reserved_sql_keyword;

#[derive(Debug, Error)]
pub enum SchemaLoaderError {
    #[error("I/O error at '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON deserialization error in '{path}': {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "File name mismatch: file '{path}' must be named '{expected}.json' matching singularName"
    )]
    FileNameMismatch { path: String, expected: String },

    #[error("Invalid document type identifier '{0}': must be valid kebab-case (2-64 chars)")]
    InvalidDocumentTypeId(String),

    #[error("Invalid attribute identifier '{0}': must be valid kebab-case")]
    InvalidAttributeId(String),

    #[error(
        "Identifier '{identifier}' in '{context}' is a reserved SQL keyword and cannot be used"
    )]
    ReservedKeyword { identifier: String, context: String },

    #[error(
        "Constraint '{constraint}' is not applicable to field '{attribute}' of type '{field_type}'"
    )]
    InapplicableConstraint {
        attribute: String,
        constraint: String,
        field_type: String,
    },

    #[error("Relation references unknown owner type '{0}'")]
    UnknownRelationOwner(String),

    #[error("Relation references unknown target type '{0}'")]
    UnknownRelationTarget(String),

    #[error("Duplicate document type '{0}'")]
    DuplicateDocumentType(String),

    #[error("Duplicate relation attribute '{attr}' on type '{type_id}'")]
    DuplicateRelationAttribute { type_id: String, attr: String },

    #[error("System config error: {0}")]
    SystemConfig(String),
}

// ----------------------------------------------------------------------------
// DTOs for JSON Deserialization
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDocumentType {
    pub kind: RawDocumentKind,
    pub info: RawDocumentTypeInfo,
    #[serde(default)]
    pub options: RawDocumentTypeOptions,
    #[serde(alias = "fields")]
    pub attributes: IndexMap<String, RawFieldDefinition>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RawDocumentKind {
    Collection,
    #[serde(alias = "singleType", alias = "single-type")]
    SingleType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDocumentTypeInfo {
    #[serde(alias = "title")]
    pub display_name: String,
    pub singular_name: String,
    pub plural_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDocumentTypeOptions {
    #[serde(default = "default_true")]
    pub draft_and_publish: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFieldDefinition {
    #[serde(rename = "type")]
    pub field_type: RawFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub constraints: Vec<RawConstraint>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RawFieldType {
    Simple(String),
    Structured(RawStructuredType),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RawStructuredType {
    Integer(String),
    Decimal {
        precision: Option<u8>,
        scale: Option<u8>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawConstraint {
    #[serde(alias = "minimalLength")]
    pub min_length: Option<usize>,
    #[serde(alias = "maximalLength")]
    pub max_length: Option<usize>,
    #[serde(alias = "regex")]
    pub pattern: Option<String>,
    #[serde(alias = "minimalInteger", alias = "minimalDecimal", alias = "minimum")]
    pub min: Option<serde_json::Value>,
    #[serde(alias = "maximalInteger", alias = "maximalDecimal", alias = "maximum")]
    pub max: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRelation {
    pub id: Option<Uuid>,
    pub owner_type: String,
    pub owner_attr: String,
    pub owner_kind: RawOwnerRelationKind,
    pub target_type: String,
    pub inverse: Option<RawRelationInverse>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RawOwnerRelationKind {
    HasOne,
    HasMany,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRelationInverse {
    pub inverse_attr: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSystemConfig {
    pub id: Option<Uuid>,
    pub locales: Vec<String>,
    pub default_locale: String,
}

// ----------------------------------------------------------------------------
// Conversion & Validation Logic
// ----------------------------------------------------------------------------

pub fn parse_raw_field_type(raw: &RawFieldType) -> Result<FieldType, String> {
    match raw {
        RawFieldType::Simple(s) => match s.to_ascii_lowercase().as_str() {
            "text" | "string" => Ok(FieldType::Primitive(PrimitiveType::Text)),
            "uid" => Ok(FieldType::Primitive(PrimitiveType::Uid)),
            "uuid" => Ok(FieldType::Primitive(PrimitiveType::Uuid)),
            "boolean" | "bool" => Ok(FieldType::Primitive(PrimitiveType::Boolean)),
            "date" => Ok(FieldType::Primitive(PrimitiveType::Date)),
            "datetime" | "timestamp" => Ok(FieldType::Primitive(PrimitiveType::DateTime)),
            "email" => Ok(FieldType::Email),
            "url" => Ok(FieldType::Url),
            "localizedtext" | "localized_text" => Ok(FieldType::LocalizedText),
            "json" => Ok(FieldType::Json),
            "integer" | "int" | "int32" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32,
            ))),
            "int16" | "smallint" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16,
            ))),
            "int64" | "bigint" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64,
            ))),
            "decimal" | "numeric" => Ok(FieldType::Primitive(PrimitiveType::Decimal {
                precision: 10,
                scale: 2,
            })),
            other => Err(format!("unknown field type '{other}'")),
        },
        RawFieldType::Structured(st) => match st {
            RawStructuredType::Integer(size_str) => match size_str.to_ascii_lowercase().as_str() {
                "int16" | "i16" | "smallint" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                    IntegerSize::I16,
                ))),
                "int32" | "i32" | "integer" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                    IntegerSize::I32,
                ))),
                "int64" | "i64" | "bigint" => Ok(FieldType::Primitive(PrimitiveType::Integer(
                    IntegerSize::I64,
                ))),
                other => Err(format!("unknown integer size '{other}'")),
            },
            RawStructuredType::Decimal { precision, scale } => {
                let p = precision.unwrap_or(10);
                let s = scale.unwrap_or(2);
                Ok(FieldType::Primitive(PrimitiveType::Decimal {
                    precision: p,
                    scale: s,
                }))
            }
        },
    }
}

pub fn convert_raw_constraints(
    raw_constraints: &[RawConstraint],
    field_type: &FieldType,
    attr_name: &str,
) -> Result<Vec<FieldConstraint>, SchemaLoaderError> {
    let mut result = Vec::new();

    for rc in raw_constraints {
        if let Some(min_len) = rc.min_length {
            let c = FieldConstraint::MinLength(min_len);
            if !c.is_applicable_for(field_type) {
                return Err(SchemaLoaderError::InapplicableConstraint {
                    attribute: attr_name.to_string(),
                    constraint: format!("minLength: {min_len}"),
                    field_type: format!("{field_type:?}"),
                });
            }
            if !result.contains(&c) {
                result.push(c);
            }
        }

        if let Some(max_len) = rc.max_length {
            let c = FieldConstraint::MaxLength(max_len);
            if !c.is_applicable_for(field_type) {
                return Err(SchemaLoaderError::InapplicableConstraint {
                    attribute: attr_name.to_string(),
                    constraint: format!("maxLength: {max_len}"),
                    field_type: format!("{field_type:?}"),
                });
            }
            if !result.contains(&c) {
                result.push(c);
            }
        }

        if let Some(ref pat) = rc.pattern {
            let c = FieldConstraint::Pattern(pat.clone());
            if !c.is_applicable_for(field_type) {
                return Err(SchemaLoaderError::InapplicableConstraint {
                    attribute: attr_name.to_string(),
                    constraint: format!("pattern: {pat}"),
                    field_type: format!("{field_type:?}"),
                });
            }
            if !result.contains(&c) {
                result.push(c);
            }
        }

        if let Some(ref min_val) = rc.min {
            let c = match field_type {
                FieldType::Primitive(PrimitiveType::Integer(_)) => {
                    let n = parse_i64_from_value(min_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!("min: {min_val} (expected integer)"),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MinInteger(n)
                }
                FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
                    let d = parse_decimal_from_value(min_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!("min: {min_val} (expected decimal)"),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MinDecimal(d)
                }
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                | FieldType::LocalizedText => {
                    let n = parse_usize_from_value(min_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!(
                                "min: {min_val} (expected non-negative integer for string length)"
                            ),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MinLength(n)
                }
                _ => {
                    return Err(SchemaLoaderError::InapplicableConstraint {
                        attribute: attr_name.to_string(),
                        constraint: format!("min: {min_val}"),
                        field_type: format!("{field_type:?}"),
                    });
                }
            };
            if !result.contains(&c) {
                result.push(c);
            }
        }

        if let Some(ref max_val) = rc.max {
            let c = match field_type {
                FieldType::Primitive(PrimitiveType::Integer(_)) => {
                    let n = parse_i64_from_value(max_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!("max: {max_val} (expected integer)"),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MaxInteger(n)
                }
                FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
                    let d = parse_decimal_from_value(max_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!("max: {max_val} (expected decimal)"),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MaxDecimal(d)
                }
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                | FieldType::LocalizedText => {
                    let n = parse_usize_from_value(max_val).ok_or_else(|| {
                        SchemaLoaderError::InapplicableConstraint {
                            attribute: attr_name.to_string(),
                            constraint: format!(
                                "max: {max_val} (expected non-negative integer for string length)"
                            ),
                            field_type: format!("{field_type:?}"),
                        }
                    })?;
                    FieldConstraint::MaxLength(n)
                }
                _ => {
                    return Err(SchemaLoaderError::InapplicableConstraint {
                        attribute: attr_name.to_string(),
                        constraint: format!("max: {max_val}"),
                        field_type: format!("{field_type:?}"),
                    });
                }
            };
            if !result.contains(&c) {
                result.push(c);
            }
        }
    }

    Ok(result)
}

fn parse_usize_from_value(val: &serde_json::Value) -> Option<usize> {
    match val {
        serde_json::Value::Number(n) => n.as_u64().and_then(|u| usize::try_from(u).ok()),
        serde_json::Value::String(s) => s.parse::<usize>().ok(),
        _ => None,
    }
}

fn parse_i64_from_value(val: &serde_json::Value) -> Option<i64> {
    match val {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn parse_decimal_from_value(val: &serde_json::Value) -> Option<Decimal> {
    match val {
        serde_json::Value::Number(n) => n.to_string().parse::<Decimal>().ok(),
        serde_json::Value::String(s) => s.parse::<Decimal>().ok(),
        _ => None,
    }
}

/// Parses and validates a single `DocumentType` JSON file.
pub fn load_document_type_from_str(
    content: &str,
    file_stem: Option<&str>,
) -> Result<DocumentType, SchemaLoaderError> {
    let raw: RawDocumentType =
        serde_json::from_str(content).map_err(|e| SchemaLoaderError::Json {
            path: file_stem.unwrap_or("<in-memory>").to_string(),
            source: e,
        })?;

    // Invariant: file name must match singularName
    if let Some(stem) = file_stem
        && stem != raw.info.singular_name
    {
        return Err(SchemaLoaderError::FileNameMismatch {
            path: format!("{stem}.json"),
            expected: raw.info.singular_name.clone(),
        });
    }

    // Validate singularName as DocumentTypeId
    let type_id = DocumentTypeId::try_new(&raw.info.singular_name)
        .map_err(|_| SchemaLoaderError::InvalidDocumentTypeId(raw.info.singular_name.clone()))?;

    // Check reserved keywords
    if is_reserved_sql_keyword(&raw.info.singular_name) {
        return Err(SchemaLoaderError::ReservedKeyword {
            identifier: raw.info.singular_name.clone(),
            context: "document type singularName".into(),
        });
    }
    if is_reserved_sql_keyword(&raw.info.plural_name) {
        return Err(SchemaLoaderError::ReservedKeyword {
            identifier: raw.info.plural_name.clone(),
            context: "document type pluralName".into(),
        });
    }

    let kind = match raw.kind {
        RawDocumentKind::Collection => DocumentKind::Collection,
        RawDocumentKind::SingleType => DocumentKind::SingleType,
    };

    let mut fields = IndexMap::new();
    for (attr_name, raw_field) in raw.attributes {
        let attr_id = AttributeId::try_new(&attr_name)
            .map_err(|_| SchemaLoaderError::InvalidAttributeId(attr_name.clone()))?;

        if is_reserved_sql_keyword(&attr_name) {
            return Err(SchemaLoaderError::ReservedKeyword {
                identifier: attr_name.clone(),
                context: format!("attribute on type '{}'", raw.info.singular_name),
            });
        }

        let field_type = parse_raw_field_type(&raw_field.field_type).map_err(|msg| {
            SchemaLoaderError::InapplicableConstraint {
                attribute: attr_name.clone(),
                constraint: "type".into(),
                field_type: msg,
            }
        })?;

        let constraints = convert_raw_constraints(&raw_field.constraints, &field_type, &attr_name)?;

        let def = FieldDefinition {
            id: attr_id.clone(),
            field_type,
            required: raw_field.required,
            unique: raw_field.unique,
            constraints,
        };

        fields.insert(attr_id, def);
    }

    Ok(DocumentType {
        id: type_id,
        kind,
        info: DocumentTypeInfo {
            title: raw.info.display_name,
            singular_name: raw.info.singular_name,
            plural_name: raw.info.plural_name,
            description: raw.info.description,
        },
        options: DocumentTypeOptions {
            draft_and_publish: raw.options.draft_and_publish,
        },
        fields,
    })
}

/// Parses and validates a single `Relation` JSON file.
pub fn load_relation_from_str(
    content: &str,
    file_name: &str,
) -> Result<Relation, SchemaLoaderError> {
    let raw: RawRelation = serde_json::from_str(content).map_err(|e| SchemaLoaderError::Json {
        path: file_name.to_string(),
        source: e,
    })?;

    let owner_type = DocumentTypeId::try_new(&raw.owner_type)
        .map_err(|_| SchemaLoaderError::InvalidDocumentTypeId(raw.owner_type.clone()))?;
    let target_type = DocumentTypeId::try_new(&raw.target_type)
        .map_err(|_| SchemaLoaderError::InvalidDocumentTypeId(raw.target_type.clone()))?;
    let owner_attr = AttributeId::try_new(&raw.owner_attr)
        .map_err(|_| SchemaLoaderError::InvalidAttributeId(raw.owner_attr.clone()))?;

    let owner_kind = match raw.owner_kind {
        RawOwnerRelationKind::HasOne => OwnerRelationKind::HasOne,
        RawOwnerRelationKind::HasMany => OwnerRelationKind::HasMany,
    };

    let inverse = match raw.inverse {
        Some(inv) => {
            let inv_attr = AttributeId::try_new(&inv.inverse_attr)
                .map_err(|_| SchemaLoaderError::InvalidAttributeId(inv.inverse_attr.clone()))?;
            Some(RelationInverse {
                inverse_attr: inv_attr,
            })
        }
        None => None,
    };

    let id_uuid = raw.id.unwrap_or_else(Uuid::now_v7);

    Ok(Relation {
        id: RelationId::new(id_uuid),
        owner_type,
        owner_attr,
        owner_kind,
        target_type,
        inverse,
    })
}

/// Parses a `SystemConfig` JSON file.
pub fn load_system_config_from_str(
    content: &str,
    file_name: &str,
) -> Result<SystemConfig, SchemaLoaderError> {
    let raw: RawSystemConfig =
        serde_json::from_str(content).map_err(|e| SchemaLoaderError::Json {
            path: file_name.to_string(),
            source: e,
        })?;

    let config_id = SystemConfigId::new(raw.id.unwrap_or_else(Uuid::now_v7));
    let mut locales = Vec::new();
    for loc_str in raw.locales {
        let loc = LocaleId::try_new(&loc_str)
            .map_err(|_| SchemaLoaderError::SystemConfig(format!("invalid locale '{loc_str}'")))?;
        locales.push(loc);
    }

    let default_locale = LocaleId::try_new(&raw.default_locale).map_err(|_| {
        SchemaLoaderError::SystemConfig(format!("invalid default locale '{}'", raw.default_locale))
    })?;

    SystemConfig::new(config_id, locales, default_locale)
        .map_err(|msg| SchemaLoaderError::SystemConfig(format!("{msg:?}")))
}

/// Loads the entire `SchemaRegistry` and `SystemConfig` from a schema directory.
///
/// Directory structure:
/// ```text
/// dir/
/// ├── document-types/ (or directly inside dir/)
/// │   ├── article.json
/// │   └── category.json
/// ├── relations/
/// │   └── article-category.json
/// └── system-config.json
/// ```
pub fn load_schema_registry(
    base_dir: &Path,
) -> Result<(SchemaRegistry, SystemConfig), SchemaLoaderError> {
    let mut doc_types = Vec::new();
    let mut relations = Vec::new();

    // 1. Load document types
    let doc_types_dir = base_dir.join("document-types");
    let search_dir = if doc_types_dir.is_dir() {
        doc_types_dir
    } else {
        base_dir.to_path_buf()
    };

    if search_dir.is_dir() {
        let entries = fs::read_dir(&search_dir).map_err(|e| SchemaLoaderError::Io {
            path: search_dir.display().to_string(),
            source: e,
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| SchemaLoaderError::Io {
                path: search_dir.display().to_string(),
                source: e,
            })?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let file_name = path.file_name().unwrap().to_string_lossy();
                if file_name == "system-config.json" {
                    continue;
                }
                let stem = path.file_stem().unwrap().to_string_lossy();
                let content = fs::read_to_string(&path).map_err(|e| SchemaLoaderError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;

                let dt = load_document_type_from_str(&content, Some(&stem))?;
                if doc_types
                    .iter()
                    .any(|existing: &DocumentType| existing.id == dt.id)
                {
                    return Err(SchemaLoaderError::DuplicateDocumentType(dt.id.to_string()));
                }
                doc_types.push(dt);
            }
        }
    }

    // 2. Load relations
    let relations_dir = base_dir.join("relations");
    if relations_dir.is_dir() {
        let entries = fs::read_dir(&relations_dir).map_err(|e| SchemaLoaderError::Io {
            path: relations_dir.display().to_string(),
            source: e,
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| SchemaLoaderError::Io {
                path: relations_dir.display().to_string(),
                source: e,
            })?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let file_name = path.file_name().unwrap().to_string_lossy();
                let content = fs::read_to_string(&path).map_err(|e| SchemaLoaderError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
                let rel = load_relation_from_str(&content, &file_name)?;
                relations.push(rel);
            }
        }
    }

    // 3. Validate relation endpoints exist in doc_types
    for rel in &relations {
        if !doc_types.iter().any(|dt| dt.id == rel.owner_type) {
            return Err(SchemaLoaderError::UnknownRelationOwner(
                rel.owner_type.to_string(),
            ));
        }
        if !doc_types.iter().any(|dt| dt.id == rel.target_type) {
            return Err(SchemaLoaderError::UnknownRelationTarget(
                rel.target_type.to_string(),
            ));
        }
    }

    // 4. Load system config
    let config_path = base_dir.join("system-config.json");
    let system_config = if config_path.is_file() {
        let content = fs::read_to_string(&config_path).map_err(|e| SchemaLoaderError::Io {
            path: config_path.display().to_string(),
            source: e,
        })?;
        load_system_config_from_str(&content, "system-config.json")?
    } else {
        // Default system config with "en"
        let en = LocaleId::try_new("en").unwrap();
        SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap()
    };

    let registry = SchemaRegistry::new(doc_types, relations);
    Ok((registry, system_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_document_type_success() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article",
                "description": "Company articles"
            },
            "options": {
                "draftAndPublish": true
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "required": true,
                    "constraints": [
                        { "minLength": 5 },
                        { "maxLength": 100 }
                    ]
                },
                "priority": {
                    "type": { "integer": "int32" },
                    "required": true,
                    "unique": true,
                    "constraints": [
                        { "min": 1 },
                        { "max": 10 }
                    ]
                },
                "content": {
                    "type": "localizedText",
                    "required": false,
                    "constraints": [
                        { "minimalLength": 10 }
                    ]
                }
            }
        }"#;

        let dt = load_document_type_from_str(json, Some("article")).unwrap();
        assert_eq!(dt.id.as_ref(), "article");
        assert_eq!(dt.kind, DocumentKind::Collection);
        assert_eq!(dt.fields.len(), 3);

        let title_attr = AttributeId::try_new("title").unwrap();
        let title_field = dt.fields.get(&title_attr).unwrap();
        assert!(title_field.required);
        assert_eq!(title_field.constraints.len(), 2);

        let priority_attr = AttributeId::try_new("priority").unwrap();
        let priority_field = dt.fields.get(&priority_attr).unwrap();
        assert!(priority_field.unique);
        assert_eq!(
            priority_field.field_type,
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32))
        );

        let content_attr = AttributeId::try_new("content").unwrap();
        let content_field = dt.fields.get(&content_attr).unwrap();
        assert_eq!(content_field.field_type, FieldType::LocalizedText);
        assert_eq!(content_field.constraints.len(), 1);
    }

    #[test]
    fn test_file_name_mismatch_rejected() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article"
            },
            "attributes": {}
        }"#;

        let err = load_document_type_from_str(json, Some("wrong-name")).unwrap_err();
        assert!(matches!(err, SchemaLoaderError::FileNameMismatch { .. }));
    }

    #[test]
    fn test_reserved_sql_keyword_rejected() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article"
            },
            "attributes": {
                "select": {
                    "type": "text"
                }
            }
        }"#;

        let err = load_document_type_from_str(json, Some("article")).unwrap_err();
        assert!(matches!(err, SchemaLoaderError::ReservedKeyword { .. }));
    }

    #[test]
    fn test_inapplicable_constraint_rejected() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article"
            },
            "attributes": {
                "is-published": {
                    "type": "boolean",
                    "constraints": [
                        { "minLength": 5 }
                    ]
                }
            }
        }"#;

        let err = load_document_type_from_str(json, Some("article")).unwrap_err();
        assert!(matches!(
            err,
            SchemaLoaderError::InapplicableConstraint { .. }
        ));
    }

    #[test]
    fn test_load_relation_success() {
        let json = r#"{
            "ownerType": "article",
            "ownerAttr": "author",
            "ownerKind": "hasOne",
            "targetType": "author",
            "inverse": {
                "inverseAttr": "articles"
            }
        }"#;

        let rel = load_relation_from_str(json, "article-author.json").unwrap();
        assert_eq!(rel.owner_type.as_ref(), "article");
        assert_eq!(rel.owner_attr.as_ref(), "author");
        assert_eq!(rel.target_type.as_ref(), "author");
        assert_eq!(rel.owner_kind, OwnerRelationKind::HasOne);
        assert!(rel.inverse.is_some());
        assert_eq!(rel.inverse.unwrap().inverse_attr.as_ref(), "articles");
    }

    #[test]
    fn test_string_min_max_shortcuts() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "post",
                "pluralName": "posts",
                "displayName": "Post"
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "constraints": [
                        { "min": 5, "max": 100 }
                    ]
                },
                "slug": {
                    "type": "uid",
                    "constraints": [
                        { "minimalLength": 4 },
                        { "maximalLength": 10 }
                    ]
                },
                "summary": {
                    "type": "localizedText",
                    "constraints": [
                        { "min": 10, "max": 500 }
                    ]
                }
            }
        }"#;

        let dt = load_document_type_from_str(json, Some("post")).unwrap();

        let title_field = dt
            .fields
            .get(&AttributeId::try_new("title").unwrap())
            .unwrap();
        assert_eq!(
            title_field.constraints,
            vec![
                FieldConstraint::MinLength(5),
                FieldConstraint::MaxLength(100),
            ]
        );

        let slug_field = dt
            .fields
            .get(&AttributeId::try_new("slug").unwrap())
            .unwrap();
        assert_eq!(
            slug_field.constraints,
            vec![
                FieldConstraint::MinLength(4),
                FieldConstraint::MaxLength(10),
            ]
        );

        let summary_field = dt
            .fields
            .get(&AttributeId::try_new("summary").unwrap())
            .unwrap();
        assert_eq!(
            summary_field.constraints,
            vec![
                FieldConstraint::MinLength(10),
                FieldConstraint::MaxLength(500),
            ]
        );
    }

    #[test]
    fn test_email_and_url_cannot_have_length_or_min_max_constraints() {
        let json_email = r#"{
            "kind": "collection",
            "info": {
                "singularName": "member",
                "pluralName": "members",
                "displayName": "Member"
            },
            "attributes": {
                "contact": {
                    "type": "email",
                    "constraints": [
                        { "min": 5 }
                    ]
                }
            }
        }"#;
        let err = load_document_type_from_str(json_email, Some("member")).unwrap_err();
        assert!(matches!(
            err,
            SchemaLoaderError::InapplicableConstraint { .. }
        ));

        let json_url = r#"{
            "kind": "collection",
            "info": {
                "singularName": "member",
                "pluralName": "members",
                "displayName": "Member"
            },
            "attributes": {
                "website": {
                    "type": "url",
                    "constraints": [
                        { "maxLength": 100 }
                    ]
                }
            }
        }"#;
        let err = load_document_type_from_str(json_url, Some("member")).unwrap_err();
        assert!(matches!(
            err,
            SchemaLoaderError::InapplicableConstraint { .. }
        ));
    }
}
