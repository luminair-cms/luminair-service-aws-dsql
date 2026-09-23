use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use uuid::Uuid;

use crate::entities::document_instance::DocumentInstance;
use crate::entities::document_type::{
    DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions,
};
use crate::entities::field_definition::FieldDefinition;
use crate::entities::relation::Relation;
use crate::services::schema_registry::SchemaRegistry;
use crate::types::field_type::{FieldType, PrimitiveType};
use crate::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId, LocaleId, UserId};

pub fn document_type_id() -> DocumentTypeId {
    DocumentTypeId::new(Uuid::from_u128(0x01932c4a_0000_7000_8000_000000000001))
}

pub fn instance_id() -> DocumentInstanceId {
    DocumentInstanceId::new(Uuid::from_u128(0x01932c4a_0000_7000_8000_000000000002))
}

pub fn user_id() -> UserId {
    UserId::try_new("test_user_sub").expect("valid test user id")
}

pub fn locale_en() -> LocaleId {
    LocaleId::try_new("en").expect("valid test locale")
}

pub fn locale_uk() -> LocaleId {
    LocaleId::try_new("uk").expect("valid test locale")
}

pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp")
}

pub fn make_text_field(id: &str) -> FieldDefinition {
    let attr = AttributeId::try_new(id).expect("valid attribute id");
    FieldDefinition {
        id: attr,
        field_type: FieldType::Primitive(PrimitiveType::Text),
        required: true,
        unique: false,
        constraints: Vec::new(),
    }
}

pub fn make_document_type(kind: DocumentKind) -> DocumentType {
    let mut fields = IndexMap::new();
    let field = make_text_field("title");
    fields.insert(field.id.clone(), field);

    DocumentType {
        id: document_type_id(),
        kind,
        info: DocumentTypeInfo {
            title: "Test Type".into(),
            singular_name: "test_type".into(),
            plural_name: "test_types".into(),
            description: None,
        },
        options: DocumentTypeOptions {
            draft_and_publish: true,
        },
        fields,
    }
}

pub fn make_instance(type_id: DocumentTypeId) -> DocumentInstance {
    DocumentInstance::new(type_id, Some(user_id()), now())
}

pub fn make_schema_registry(
    types: Vec<DocumentType>,
    relations: Vec<Relation>,
) -> SchemaRegistry {
    SchemaRegistry::new(types, relations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factories_create_valid_domain_objects() {
        let dt = make_document_type(DocumentKind::Collection);
        assert_eq!(dt.id, document_type_id());
        assert_eq!(dt.info.plural_name, "test_types");

        let inst = make_instance(dt.id);
        assert_eq!(inst.document_type_id, dt.id);
        assert!(inst.is_owned_by(&user_id()));

        let reg = make_schema_registry(vec![dt], vec![]);
        assert!(reg.find_type_by_name("test_types").is_some());
    }
}
