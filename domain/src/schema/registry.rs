use indexmap::IndexSet;
use std::collections::HashMap;

use super::document_type::DocumentType;
use super::ids::{AttributeId, DocumentTypeId, RelationId};
use super::relation::{Relation, RelationView};
use crate::content::instance::DocumentContent;
use crate::errors::DomainError;
use crate::system::config::SystemConfig;

#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    types: IndexSet<DocumentType>,
    by_name: HashMap<String, DocumentTypeId>,
    relations: IndexSet<Relation>,
}

impl SchemaRegistry {
    pub fn new(types: Vec<DocumentType>, relations: Vec<Relation>) -> Self {
        let mut by_name = HashMap::new();
        let mut type_set = IndexSet::new();
        for dt in types {
            by_name.insert(dt.info.plural_name.clone(), dt.id.clone());
            type_set.insert(dt);
        }
        let mut relation_set = IndexSet::new();
        for r in relations {
            relation_set.insert(r);
        }
        Self {
            types: type_set,
            by_name,
            relations: relation_set,
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
            .iter()
            .filter_map(|r| r.view_for(type_id))
            .collect()
    }

    pub fn all_relations(&self) -> impl Iterator<Item = &Relation> {
        self.relations.iter()
    }

    pub fn find_relation(&self, id: &RelationId) -> Option<&Relation> {
        self.relations.get(id)
    }

    pub fn find_relation_by_name(&self, name: &str) -> Option<&Relation> {
        self.relations.get(name)
    }

    pub fn find_relation_for_attr(
        &self,
        type_id: &DocumentTypeId,
        attr: &AttributeId,
    ) -> Option<&Relation> {
        self.relations.iter().find(|r| {
            (&r.owner_type == type_id && &r.owner_attr == attr)
                || (&r.target_type == type_id
                    && r.inverse.as_ref().map(|i| &i.inverse_attr) == Some(attr))
        })
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(|k| k.as_str())
    }

    pub fn all_types(&self) -> impl Iterator<Item = &DocumentType> {
        self.types.iter()
    }

    pub fn validate_content(
        &self,
        type_id: &DocumentTypeId,
        content: &DocumentContent,
        system_config: &SystemConfig,
    ) -> Result<(), Vec<DomainError>> {
        let doc_type = self
            .find_type(type_id)
            .ok_or_else(|| vec![DomainError::DocumentTypeNotFound(type_id.clone())])?;

        super::validator::validate_content(doc_type, content, system_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexSet;
    use uuid::Uuid;

    use crate::content::instance::PublicationState;
    use crate::content::values::{ContentValue, DomainValue, PrimitiveValue};
    use crate::schema::document_type::{DocumentKind, DocumentTypeInfo, DocumentTypeOptions};
    use crate::schema::field_definition::FieldDefinition;
    use crate::schema::relation::{OwnerRelationKind, RelationInverse};
    use crate::schema::types::{FieldType, PrimitiveType};
    use crate::system::ids::{LocaleId, SystemConfigId};

    fn make_test_setup() -> (DocumentType, SystemConfig) {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let title_attr = AttributeId::try_new("title").unwrap();

        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: title_attr,
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![],
        });

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
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();

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
        let owner_attr = AttributeId::try_new("tags").unwrap();
        let id = RelationId::derive(&owner_type, &owner_attr);
        let rel = Relation {
            id,
            owner_type: owner_type.clone(),
            owner_attr,
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
        let owner_attr = AttributeId::try_new("tags").unwrap();
        let id = RelationId::derive(&owner_type, &owner_attr);
        let rel = Relation {
            id,
            owner_type,
            owner_attr,
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
    fn test_validate_content_delegates_to_validator() {
        let (doc_type, config) = make_test_setup();
        let type_id = doc_type.id.clone();
        let registry = SchemaRegistry::new(vec![doc_type], vec![]);

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
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

        let unknown_type = DocumentTypeId::try_new("unknown").unwrap();
        let err = registry
            .validate_content(&unknown_type, &content, &config)
            .unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(err[0], DomainError::DocumentTypeNotFound(_)));
    }
}
