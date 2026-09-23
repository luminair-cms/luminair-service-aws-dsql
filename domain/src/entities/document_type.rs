use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::value_objects::{AttributeId, DocumentTypeId};
use super::field_definition::FieldDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentKind {
    Collection,
    SingleType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTypeInfo {
    pub title: String,
    pub singular_name: String,
    pub plural_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTypeOptions {
    pub draft_and_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentType {
    pub id: DocumentTypeId,
    pub kind: DocumentKind,
    pub info: DocumentTypeInfo,
    pub options: DocumentTypeOptions,
    pub fields: IndexMap<AttributeId, FieldDefinition>,
}
