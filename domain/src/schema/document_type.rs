use std::borrow::Borrow;
use std::hash::{Hash, Hasher};

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use super::field_definition::FieldDefinition;
use super::ids::{AttributeId, DocumentTypeId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentType {
    pub id: DocumentTypeId,
    pub kind: DocumentKind,
    pub info: DocumentTypeInfo,
    pub options: DocumentTypeOptions,
    pub fields: IndexSet<FieldDefinition>,
}

impl DocumentType {
    /// Looks up a field definition by its string identifier in $O(1)$ time without allocating.
    pub fn find_field(&self, id: &str) -> Option<&FieldDefinition> {
        self.fields.get(id)
    }

    /// Looks up a field definition by its typed `AttributeId` in $O(1)$ time.
    pub fn get_field(&self, id: &AttributeId) -> Option<&FieldDefinition> {
        self.fields.get(id)
    }

    /// Checks if an attribute identifier is defined in this document type.
    pub fn has_field(&self, id: &str) -> bool {
        self.fields.contains(id)
    }
}

impl PartialEq for DocumentType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for DocumentType {}

impl Hash for DocumentType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.as_ref().hash(state);
    }
}

impl Borrow<DocumentTypeId> for DocumentType {
    fn borrow(&self) -> &DocumentTypeId {
        &self.id
    }
}

impl Borrow<str> for DocumentType {
    fn borrow(&self) -> &str {
        self.id.as_ref()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::types::{FieldType, PrimitiveType};

    #[test]
    fn test_document_type_field_lookups() {
        let title_attr = AttributeId::try_new("title").unwrap();
        let body_attr = AttributeId::try_new("body").unwrap();

        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: title_attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![],
        });

        let doc_type = DocumentType {
            id: DocumentTypeId::try_new("article").unwrap(),
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

        // Lookup by &str
        assert!(doc_type.find_field("title").is_some());
        assert!(doc_type.find_field("body").is_none());
        assert!(doc_type.has_field("title"));
        assert!(!doc_type.has_field("body"));

        // Lookup by &AttributeId
        assert!(doc_type.get_field(&title_attr).is_some());
        assert!(doc_type.get_field(&body_attr).is_none());
    }

    #[test]
    fn test_document_type_borrow_in_index_set() {
        let doc_type = DocumentType {
            id: DocumentTypeId::try_new("article").unwrap(),
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
            fields: IndexSet::new(),
        };

        let mut set = IndexSet::new();
        assert!(set.insert(doc_type));

        let type_id = DocumentTypeId::try_new("article").unwrap();
        assert!(set.get(&type_id).is_some());
        assert!(set.get("article").is_some());
        assert!(set.get("page").is_none());
    }
}
