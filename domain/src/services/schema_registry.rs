use std::collections::HashMap;

use crate::entities::document_instance::DocumentContent;
use crate::entities::document_type::DocumentType;
use crate::entities::field_definition::FieldConstraint;
use crate::entities::relation::{Relation, RelationView};
use crate::entities::system_config::SystemConfig;
use crate::errors::DomainError;
use crate::types::content_value::ContentValue;
use crate::types::domain_value::DomainValue;
use crate::types::field_type::FieldType;
use crate::types::primitive_value::PrimitiveValue;
use crate::value_objects::{AttributeId, DocumentTypeId, RelationId};

/// Evaluates a single `FieldConstraint` against a scalar `DomainValue`.
/// Returns `Some(DomainError)` if the constraint is violated, `None` if satisfied.
fn evaluate_constraint(
    attr_id: &AttributeId,
    constraint: &FieldConstraint,
    value: &DomainValue,
) -> Option<DomainError> {
    let violated = |reason: String| -> Option<DomainError> {
        Some(DomainError::InvalidFieldValue {
            attribute_id: attr_id.clone(),
            reason,
        })
    };

    match (constraint, value) {
        (FieldConstraint::MinLength(min), DomainValue::Primitive(PrimitiveValue::Text(s)))
        | (FieldConstraint::MinLength(min), DomainValue::Primitive(PrimitiveValue::Uid(s))) => {
            if s.len() < *min {
                return violated(format!("length {} is below minimum {}", s.len(), min));
            }
        }
        (FieldConstraint::MaxLength(max), DomainValue::Primitive(PrimitiveValue::Text(s)))
        | (FieldConstraint::MaxLength(max), DomainValue::Primitive(PrimitiveValue::Uid(s))) => {
            if s.len() > *max {
                return violated(format!("length {} exceeds maximum {}", s.len(), max));
            }
        }
        (FieldConstraint::Pattern(pat), DomainValue::Primitive(PrimitiveValue::Text(s)))
        | (FieldConstraint::Pattern(pat), DomainValue::Primitive(PrimitiveValue::Uid(s))) => {
            // Compile regex; treat compilation failure as a constraint violation to surface config errors
            match regex::Regex::new(pat) {
                Ok(re) if !re.is_match(s) => {
                    return violated(format!("value '{}' does not match pattern '{}'", s, pat));
                }
                Err(e) => {
                    return violated(format!("invalid regex pattern '{}': {}", pat, e));
                }
                _ => {}
            }
        }
        (FieldConstraint::MinInteger(min), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n < *min {
                return violated(format!("value {} is below minimum {}", n, min));
            }
        }
        (FieldConstraint::MaxInteger(max), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n > *max {
                return violated(format!("value {} exceeds maximum {}", n, max));
            }
        }
        (FieldConstraint::MinDecimal(min), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d < min {
                return violated(format!("value {} is below minimum {}", d, min));
            }
        }
        (FieldConstraint::MaxDecimal(max), DomainValue::Primitive(PrimitiveValue::Decimal(d)))
            if d > max =>
        {
            return violated(format!("value {} exceeds maximum {}", d, max));
        }
        // Constraint not applicable to this value type — silently skip (type validation catches mismatches)
        _ => {}
    }
    None
}

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
            by_name.insert(dt.info.plural_name.clone(), dt.id.clone());
            type_map.insert(dt.id.clone(), dt);
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

    pub fn find_type(&self, id: &DocumentTypeId) -> Option<&DocumentType> {
        self.types.get(id)
    }

    pub fn find_type_by_name(&self, plural_name: &str) -> Option<&DocumentType> {
        self.by_name
            .get(plural_name)
            .and_then(|id| self.types.get(id))
    }

    pub fn find_relations_for(&self, type_id: &DocumentTypeId) -> Vec<RelationView> {
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
        type_id: &DocumentTypeId,
        content: &DocumentContent,
        system_config: &SystemConfig,
    ) -> Result<(), Vec<DomainError>> {
        let doc_type = match self.find_type(type_id) {
            Some(dt) => dt,
            None => return Err(vec![DomainError::DocumentTypeNotFound(type_id.clone())]),
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
                    } else {
                        // Evaluate FieldConstraints against the scalar value
                        for constraint in &field_def.constraints {
                            if let Some(err) = evaluate_constraint(attr_id, constraint, val) {
                                errors.push(err);
                            }
                        }
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
        let type_id = DocumentTypeId::try_new("article").unwrap();
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
        let id = doc_type.id.clone();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);
        assert!(registry.find_type(&id).is_some());
    }

    #[test]
    fn test_find_type_unknown_id() {
        let (doc_type, _) = make_test_setup();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);
        let unknown = DocumentTypeId::try_new("unknown-type").unwrap();
        assert!(registry.find_type(&unknown).is_none());
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
        let owner_type = DocumentTypeId::try_new("owner-type").unwrap();
        let target_type = DocumentTypeId::try_new("target-type").unwrap();
        let rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type: owner_type.clone(),
            owner_attr: AttributeId::try_new("tags").unwrap(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: None,
        };

        let registry = SchemaRegistry::new(vec![], vec![rel]);
        let views = registry.find_relations_for(&owner_type);
        assert_eq!(views.len(), 1);
        assert!(matches!(views[0], RelationView::Unidirectional { .. }));
    }

    #[test]
    fn test_find_relations_for_inverse() {
        let owner_type = DocumentTypeId::try_new("owner-type").unwrap();
        let target_type = DocumentTypeId::try_new("target-type").unwrap();
        let rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type,
            owner_attr: AttributeId::try_new("tags").unwrap(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type: target_type.clone(),
            inverse: Some(RelationInverse {
                inverse_attr: AttributeId::try_new("articles").unwrap(),
            }),
        };

        let registry = SchemaRegistry::new(vec![], vec![rel]);
        let views = registry.find_relations_for(&target_type);
        assert_eq!(views.len(), 1);
        assert!(matches!(views[0], RelationView::InverseSide { .. }));
    }

    #[test]
    fn test_validate_content_correct() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id.clone();
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

        assert!(
            registry
                .validate_content(&type_id, &content, &config)
                .is_ok()
        );
    }

    #[test]
    fn test_validate_content_wrong_type() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id.clone();
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

        let errs = registry
            .validate_content(&type_id, &content, &config)
            .unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], DomainError::InvalidFieldValue { .. }));
    }

    #[test]
    fn test_validate_content_unknown_locale() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id.clone();
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

        let errs = registry
            .validate_content(&type_id, &content, &config)
            .unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], DomainError::UnknownLocale(loc) if *loc == fr));
    }

    #[test]
    fn test_validate_content_field_constraint_min_length_violated() {
        use crate::entities::field_definition::FieldConstraint;
        use crate::types::field_type::PrimitiveType;

        let type_id = DocumentTypeId::try_new("slugged").unwrap();
        let slug_attr = AttributeId::try_new("slug").unwrap();
        let mut fields_def = IndexMap::new();
        fields_def.insert(
            slug_attr.clone(),
            FieldDefinition {
                id: slug_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![FieldConstraint::MinLength(5)],
            },
        );
        let doc_type = DocumentType {
            id: type_id.clone(),
            kind: crate::entities::document_type::DocumentKind::Collection,
            info: crate::entities::document_type::DocumentTypeInfo {
                title: "Slugged".into(),
                singular_name: "slugged".into(),
                plural_name: "sluggeds".into(),
                description: None,
            },
            options: crate::entities::document_type::DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut content_fields = HashMap::new();
        content_fields.insert(
            slug_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("hi".into()))),
        );
        let content = DocumentContent {
            fields: content_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = registry
            .validate_content(&type_id, &content, &config)
            .unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], DomainError::InvalidFieldValue { .. }));
        assert!(errs[0].to_string().contains("below minimum"));
    }

    #[test]
    fn test_validate_content_field_constraint_pattern_violated() {
        use crate::entities::field_definition::FieldConstraint;
        use crate::types::field_type::PrimitiveType;

        let type_id = DocumentTypeId::try_new("slugged").unwrap();
        let slug_attr = AttributeId::try_new("slug").unwrap();
        let mut fields_def = IndexMap::new();
        fields_def.insert(
            slug_attr.clone(),
            FieldDefinition {
                id: slug_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![FieldConstraint::Pattern("^[a-z0-9-]+$".into())],
            },
        );
        let doc_type = DocumentType {
            id: type_id.clone(),
            kind: crate::entities::document_type::DocumentKind::Collection,
            info: crate::entities::document_type::DocumentTypeInfo {
                title: "Slugged".into(),
                singular_name: "slugged".into(),
                plural_name: "sluggeds".into(),
                description: None,
            },
            options: crate::entities::document_type::DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut content_fields = HashMap::new();
        content_fields.insert(
            slug_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "INVALID SLUG!".into(),
            ))),
        );
        let content = DocumentContent {
            fields: content_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = registry
            .validate_content(&type_id, &content, &config)
            .unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].to_string().contains("does not match pattern"));
    }

    #[test]
    fn test_validate_content_multiple_errors_returned() {
        // Both title (wrong type) and body (unknown locale) fail → both errors should be returned
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id.clone();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut fields = HashMap::new();
        // Title: wrong type (Integer instead of Text)
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(999))),
        );
        // Body: unknown locale
        let mut loc_map = HashMap::new();
        loc_map.insert(LocaleId::try_new("fr").unwrap(), "Bonjour".to_string());
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

        let errs = registry
            .validate_content(&type_id, &content, &config)
            .unwrap_err();
        // Must return ALL errors, not just the first one
        assert_eq!(errs.len(), 2, "expected 2 errors but got {:?}", errs);
    }
}
