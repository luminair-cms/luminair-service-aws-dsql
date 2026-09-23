use std::collections::HashMap;

use crate::entities::document_instance::DocumentContent;
use crate::entities::document_type::DocumentType;
use crate::entities::relation::{Relation, RelationView};
use crate::entities::system_config::SystemConfig;
use crate::errors::DomainError;
use crate::types::content_value::ContentValue;
use crate::types::field_type::FieldType;
use crate::value_objects::{DocumentTypeId, RelationId};

#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    types: HashMap<DocumentTypeId, DocumentType>,
    by_name: HashMap<String, DocumentTypeId>,
    relations: HashMap<RelationId, Relation>,
}

impl SchemaRegistry {
    pub fn new(types: Vec<DocumentType>, relations: Vec<Relation>) -> Self {
        let mut by_name = HashMap::new();
        let mut type_map = HashMap::new();
        for dt in types {
            by_name.insert(dt.info.plural_name.clone(), dt.id);
            type_map.insert(dt.id, dt);
        }
        let mut relation_map = HashMap::new();
        for r in relations {
            relation_map.insert(r.id, r);
        }
        Self {
            types: type_map,
            by_name,
            relations: relation_map,
        }
    }

    pub fn find_type(&self, id: DocumentTypeId) -> Option<&DocumentType> {
        self.types.get(&id)
    }

    pub fn find_type_by_name(&self, plural_name: &str) -> Option<&DocumentType> {
        self.by_name.get(plural_name).and_then(|id| self.types.get(id))
    }

    pub fn find_relations_for(&self, type_id: DocumentTypeId) -> Vec<RelationView> {
        self.relations
            .values()
            .filter_map(|r| r.view_for(type_id))
            .collect()
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(|k| k.as_str())
    }

    pub fn validate_content(
        &self,
        type_id: DocumentTypeId,
        content: &DocumentContent,
        system_config: &SystemConfig,
    ) -> Result<(), Vec<DomainError>> {
        let doc_type = match self.find_type(type_id) {
            Some(dt) => dt,
            None => return Err(vec![DomainError::DocumentTypeNotFound(type_id)]),
        };

        let mut errors = Vec::new();

        // 1. Validate declared fields
        for (attr_id, field_def) in &doc_type.fields {
            match content.fields.get(attr_id) {
                None | Some(ContentValue::Null) => {
                    if field_def.required {
                        errors.push(DomainError::InvalidFieldValue {
                            attribute_id: attr_id.clone(),
                            reason: "field is required but missing or null".to_string(),
                        });
                    }
                }
                Some(ContentValue::Scalar(val)) => {
                    if !val.matches_field_type(&field_def.field_type) {
                        errors.push(DomainError::InvalidFieldValue {
                            attribute_id: attr_id.clone(),
                            reason: format!(
                                "value does not match field type {:?}",
                                field_def.field_type
                            ),
                        });
                    }
                }
                Some(ContentValue::LocalizedText(map)) => {
                    if field_def.field_type != FieldType::LocalizedText {
                        errors.push(DomainError::InvalidFieldValue {
                            attribute_id: attr_id.clone(),
                            reason: "localized text value provided for non-localized field"
                                .to_string(),
                        });
                    } else {
                        for locale in map.keys() {
                            if !system_config.contains_locale(locale) {
                                errors.push(DomainError::UnknownLocale(locale.clone()));
                            }
                        }
                    }
                }
            }
        }

        // 2. Reject undeclared fields
        for attr_id in content.fields.keys() {
            if !doc_type.fields.contains_key(attr_id) {
                errors.push(DomainError::UnknownAttribute(attr_id.clone()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use uuid::Uuid;

    use crate::entities::document_instance::PublicationState;
    use crate::entities::document_type::{DocumentKind, DocumentTypeInfo, DocumentTypeOptions};
    use crate::entities::field_definition::FieldDefinition;
    use crate::entities::relation::{OwnerRelationKind, RelationInverse};
    use crate::types::domain_value::DomainValue;
    use crate::types::field_type::PrimitiveType;
    use crate::types::primitive_value::PrimitiveValue;
    use crate::value_objects::{AttributeId, LocaleId, SystemConfigId};

    fn make_test_setup() -> (DocumentType, SystemConfig) {
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let title_attr = AttributeId::try_new("title").unwrap();
        let body_attr = AttributeId::try_new("body").unwrap();

        let mut fields = IndexMap::new();
        fields.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr,
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![],
            },
        );
        fields.insert(
            body_attr.clone(),
            FieldDefinition {
                id: body_attr,
                field_type: FieldType::LocalizedText,
                required: false,
                unique: false,
                constraints: vec![],
            },
        );

        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Articles".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
        };

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone(), uk],
            en,
        )
        .unwrap();

        (doc_type, config)
    }

    #[test]
    fn test_find_type_by_id() {
        let (doc_type, _) = make_test_setup();
        let id = doc_type.id;
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);
        assert!(registry.find_type(id).is_some());
    }

    #[test]
    fn test_find_type_unknown_id() {
        let (doc_type, _) = make_test_setup();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);
        let unknown = DocumentTypeId::new(Uuid::now_v7());
        assert!(registry.find_type(unknown).is_none());
    }

    #[test]
    fn test_find_type_by_name() {
        let (doc_type, _) = make_test_setup();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);
        assert!(registry.find_type_by_name("articles").is_some());
        assert!(registry.find_type_by_name("authors").is_none());
    }

    #[test]
    fn test_find_relations_for_owner() {
        let owner_type = DocumentTypeId::new(Uuid::now_v7());
        let target_type = DocumentTypeId::new(Uuid::now_v7());
        let rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type,
            owner_attr: AttributeId::try_new("tags").unwrap(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: None,
        };

        let registry = SchemaRegistry::new(vec![], vec![rel]);
        let views = registry.find_relations_for(owner_type);
        assert_eq!(views.len(), 1);
        assert!(matches!(views[0], RelationView::Unidirectional { .. }));
    }

    #[test]
    fn test_find_relations_for_inverse() {
        let owner_type = DocumentTypeId::new(Uuid::now_v7());
        let target_type = DocumentTypeId::new(Uuid::now_v7());
        let rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type,
            owner_attr: AttributeId::try_new("tags").unwrap(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: Some(RelationInverse {
                inverse_attr: AttributeId::try_new("articles").unwrap(),
            }),
        };

        let registry = SchemaRegistry::new(vec![], vec![rel]);
        let views = registry.find_relations_for(target_type);
        assert_eq!(views.len(), 1);
        assert!(matches!(views[0], RelationView::InverseSide { .. }));
    }

    #[test]
    fn test_validate_content_correct() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id;
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
        );
        let mut loc_map = HashMap::new();
        loc_map.insert(LocaleId::try_new("en").unwrap(), "Content".to_string());
        fields.insert(
            AttributeId::try_new("body").unwrap(),
            ContentValue::LocalizedText(loc_map),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        assert!(registry.validate_content(type_id, &content, &config).is_ok());
    }

    #[test]
    fn test_validate_content_wrong_type() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id;
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut fields = HashMap::new();
        // Title expects Text, providing Integer
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(123))),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        let errs = registry.validate_content(type_id, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], DomainError::InvalidFieldValue { .. }));
    }

    #[test]
    fn test_validate_content_unknown_locale() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id;
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
        );
        let mut loc_map = HashMap::new();
        let fr = LocaleId::try_new("fr").unwrap(); // not in config available_locales
        loc_map.insert(fr.clone(), "Bonjour".to_string());
        fields.insert(
            AttributeId::try_new("body").unwrap(),
            ContentValue::LocalizedText(loc_map),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        let errs = registry.validate_content(type_id, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], DomainError::UnknownLocale(loc) if *loc == fr));
    }
}
