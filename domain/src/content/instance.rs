use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ids::DocumentInstanceId;
use super::values::ContentValue;
use crate::auth::ids::UserId;
use crate::errors::DomainError;
use crate::schema::ids::{AttributeId, DocumentTypeId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentInstance {
    pub id: DocumentInstanceId,
    pub db_row_id: Option<DocumentInstanceId>,
    pub document_type_id: DocumentTypeId,
    pub content: DocumentContent,
    pub relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
    pub populated_relations: HashMap<AttributeId, Vec<DocumentInstance>>,
    pub audit: AuditTrail,
}

impl DocumentInstance {
    pub fn new(type_id: DocumentTypeId, by: Option<UserId>, now: DateTime<Utc>) -> Self {
        Self {
            id: DocumentInstanceId::new(Uuid::now_v7()),
            db_row_id: None,
            document_type_id: type_id,
            content: DocumentContent {
                fields: HashMap::new(),
                publication_state: PublicationState::Draft {
                    last_published_revision: None,
                },
            },
            relations: HashMap::new(),
            populated_relations: HashMap::new(),
            audit: AuditTrail {
                created_at: now,
                created_by: by.clone(),
                updated_at: now,
                updated_by: by,
                version: 1,
            },
        }
    }

    /// Returns a new instance with populated related document instances attached.
    pub fn with_populated_relations(
        mut self,
        populated: HashMap<AttributeId, Vec<DocumentInstance>>,
    ) -> Self {
        self.populated_relations = populated;
        self
    }

    /// Replaces all relations for the given attribute with the specified target IDs (deduplicated).
    pub fn set_relations(&mut self, attr: AttributeId, target_ids: Vec<DocumentInstanceId>) {
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for target_id in target_ids {
            if seen.insert(target_id) {
                list.push(ResolvedRelation {
                    attribute_id: attr.clone(),
                    target_instance_id: target_id,
                });
            }
        }
        self.relations.insert(attr, list);
    }

    /// Appends the specified target IDs to the existing relations for the given attribute, ignoring duplicates.
    pub fn connect_relations(&mut self, attr: AttributeId, target_ids: Vec<DocumentInstanceId>) {
        let list = self.relations.entry(attr.clone()).or_default();
        let mut seen: HashSet<_> = list.iter().map(|r| r.target_instance_id).collect();
        for target_id in target_ids {
            if seen.insert(target_id) {
                list.push(ResolvedRelation {
                    attribute_id: attr.clone(),
                    target_instance_id: target_id,
                });
            }
        }
    }

    /// Removes the specified target IDs from the relations of the given attribute.
    pub fn disconnect_relations(&mut self, attr: &AttributeId, target_ids: &[DocumentInstanceId]) {
        if let Some(list) = self.relations.get_mut(attr) {
            let targets_to_remove: HashSet<_> = target_ids.iter().copied().collect();
            list.retain(|r| !targets_to_remove.contains(&r.target_instance_id));
        }
    }

    /// Clears all relations for the given attribute.
    pub fn unset_relations(&mut self, attr: &AttributeId) {
        self.relations.remove(attr);
    }

    /// Publishes the document instance, advancing its revision and returning the new revision number.
    pub fn publish(&mut self, by: Option<UserId>, now: DateTime<Utc>) -> Result<u32, DomainError> {
        let new_revision = match self.content.publication_state {
            PublicationState::Draft {
                last_published_revision,
            } => last_published_revision.unwrap_or(0) + 1,
            PublicationState::Published { revision, .. } => revision + 1,
        };

        self.content.publication_state = PublicationState::Published {
            revision: new_revision,
            published_at: now,
            published_by: by.clone(),
        };

        self.touch(by, now);

        Ok(new_revision)
    }

    /// Transitions a published document back into a draft, recording the last published revision.
    pub fn unpublish(&mut self, now: DateTime<Utc>) -> Result<(), DomainError> {
        match self.content.publication_state {
            PublicationState::Published { revision, .. } => {
                self.content.publication_state = PublicationState::Draft {
                    last_published_revision: Some(revision),
                };
                self.audit.updated_at = now;
                self.audit.version += 1;
                Ok(())
            }
            PublicationState::Draft { .. } => Err(DomainError::InvalidStateTransition {
                reason: "cannot unpublish a document that is already a draft".to_string(),
            }),
        }
    }

    /// Records an edit/touch on the instance, updating updated_at and incrementing version.
    pub fn touch(&mut self, by: Option<UserId>, now: DateTime<Utc>) {
        self.audit.updated_at = now;
        self.audit.updated_by = by;
        self.audit.version += 1;
    }

    pub fn is_owned_by(&self, user_id: &UserId) -> bool {
        self.audit.created_by.as_ref() == Some(user_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContent {
    pub fields: HashMap<AttributeId, ContentValue>,
    pub publication_state: PublicationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicationState {
    Draft {
        last_published_revision: Option<u32>,
    },
    Published {
        revision: u32,
        published_at: DateTime<Utc>,
        published_by: Option<UserId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditTrail {
    pub created_at: DateTime<Utc>,
    pub created_by: Option<UserId>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRelation {
    pub attribute_id: AttributeId,
    pub target_instance_id: DocumentInstanceId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::values::{DomainValue, PrimitiveValue};
    use chrono::Duration;

    fn make_test_instance() -> (DocumentInstance, UserId, DateTime<Utc>) {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let user = UserId::try_new("user_123").unwrap();
        let now = Utc::now();
        let instance = DocumentInstance::new(type_id, Some(user.clone()), now);
        (instance, user, now)
    }

    #[test]
    fn test_new_instance_starts_as_draft() {
        let (instance, user, now) = make_test_instance();
        assert_eq!(
            instance.content.publication_state,
            PublicationState::Draft {
                last_published_revision: None
            }
        );
        assert_eq!(instance.audit.version, 1);
        assert_eq!(instance.audit.created_at, now);
        assert_eq!(instance.audit.created_by, Some(user));
        assert!(instance.db_row_id.is_none());
    }

    #[test]
    fn test_publish_first_time() {
        let (mut instance, user, now) = make_test_instance();
        let publish_time = now + Duration::seconds(10);
        let revision = instance
            .publish(Some(user.clone()), publish_time)
            .expect("publish success");

        assert_eq!(
            instance.content.publication_state,
            PublicationState::Published {
                revision: 1,
                published_at: publish_time,
                published_by: Some(user)
            }
        );
        assert_eq!(revision, 1);
        assert_eq!(instance.audit.version, 2);
    }

    #[test]
    fn test_publish_increments_revision() {
        let (mut instance, user, now) = make_test_instance();
        instance.publish(Some(user.clone()), now).unwrap();

        let unpublish_time = now + Duration::seconds(5);
        instance.unpublish(unpublish_time).unwrap();

        let republish_time = now + Duration::seconds(10);
        let revision = instance
            .publish(Some(user.clone()), republish_time)
            .unwrap();

        assert_eq!(revision, 2);
        assert_eq!(
            instance.content.publication_state,
            PublicationState::Published {
                revision: 2,
                published_at: republish_time,
                published_by: Some(user)
            }
        );
    }

    #[test]
    fn test_publish_preserves_content() {
        let (mut instance, user, now) = make_test_instance();
        let attr = AttributeId::try_new("title").unwrap();
        instance.content.fields.insert(
            attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "My Post".into(),
            ))),
        );

        let rev = instance.publish(Some(user), now).expect("publish success");

        assert_eq!(rev, 1);
        assert_eq!(instance.content.fields.len(), 1);
        assert!(instance.content.fields.contains_key(&attr));
    }

    #[test]
    fn test_unpublish_records_last_revision() {
        let (mut instance, user, now) = make_test_instance();
        instance.publish(Some(user), now).unwrap();

        let unpub_time = now + Duration::seconds(10);
        instance.unpublish(unpub_time).unwrap();

        assert_eq!(
            instance.content.publication_state,
            PublicationState::Draft {
                last_published_revision: Some(1)
            }
        );
        assert_eq!(instance.audit.updated_at, unpub_time);
        assert_eq!(instance.audit.version, 3); // initial (1) + publish (2) + unpublish (3)
    }

    #[test]
    fn test_unpublish_on_draft_fails() {
        let (mut instance, _, now) = make_test_instance();
        let res = instance.unpublish(now);
        assert!(matches!(
            res,
            Err(DomainError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_touch_bumps_version() {
        let (mut instance, user, now) = make_test_instance();
        assert_eq!(instance.audit.version, 1);

        let later = now + Duration::seconds(5);
        instance.touch(Some(user), later);
        assert_eq!(instance.audit.version, 2);
        assert_eq!(instance.audit.updated_at, later);
    }

    #[test]
    fn test_touch_updates_updated_at() {
        let (mut instance, user, now) = make_test_instance();
        let later = now + Duration::seconds(60);
        instance.touch(Some(user), later);

        assert!(instance.audit.updated_at > instance.audit.created_at);
        assert_eq!(instance.audit.updated_at, later);
    }

    #[test]
    fn test_is_owned_by_creator() {
        let (instance, user, _) = make_test_instance();
        assert!(instance.is_owned_by(&user));
    }

    #[test]
    fn test_is_owned_by_other_user() {
        let (instance, _, _) = make_test_instance();
        let other = UserId::try_new("other_user").unwrap();
        assert!(!instance.is_owned_by(&other));
    }

    #[test]
    fn test_set_relations_deduplicates_and_replaces() {
        let (mut instance, _, _) = make_test_instance();
        let attr = AttributeId::try_new("tags").unwrap();
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());

        // First set
        instance.set_relations(attr.clone(), vec![id1, id2, id1]);
        assert_eq!(instance.relations.get(&attr).unwrap().len(), 2);
        assert_eq!(
            instance.relations.get(&attr).unwrap()[0].target_instance_id,
            id1
        );
        assert_eq!(
            instance.relations.get(&attr).unwrap()[1].target_instance_id,
            id2
        );

        // Replace with new set
        let id3 = DocumentInstanceId::new(Uuid::now_v7());
        instance.set_relations(attr.clone(), vec![id3]);
        assert_eq!(instance.relations.get(&attr).unwrap().len(), 1);
        assert_eq!(
            instance.relations.get(&attr).unwrap()[0].target_instance_id,
            id3
        );
    }

    #[test]
    fn test_connect_relations_appends_and_skips_duplicates() {
        let (mut instance, _, _) = make_test_instance();
        let attr = AttributeId::try_new("tags").unwrap();
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1]);
        assert_eq!(instance.relations.get(&attr).unwrap().len(), 1);

        // Connect id2 and duplicate id1
        instance.connect_relations(attr.clone(), vec![id1, id2]);
        assert_eq!(instance.relations.get(&attr).unwrap().len(), 2);
        assert_eq!(
            instance.relations.get(&attr).unwrap()[0].target_instance_id,
            id1
        );
        assert_eq!(
            instance.relations.get(&attr).unwrap()[1].target_instance_id,
            id2
        );
    }

    #[test]
    fn test_disconnect_relations_removes_matching_targets() {
        let (mut instance, _, _) = make_test_instance();
        let attr = AttributeId::try_new("tags").unwrap();
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());
        let id3 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1, id2, id3]);
        assert_eq!(instance.relations.get(&attr).unwrap().len(), 3);

        instance.disconnect_relations(&attr, &[id2]);
        let remaining: Vec<_> = instance
            .relations
            .get(&attr)
            .unwrap()
            .iter()
            .map(|r| r.target_instance_id)
            .collect();
        assert_eq!(remaining, vec![id1, id3]);
    }

    #[test]
    fn test_unset_relations_clears_attribute() {
        let (mut instance, _, _) = make_test_instance();
        let attr = AttributeId::try_new("tags").unwrap();
        let id1 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1]);
        assert!(instance.relations.contains_key(&attr));

        instance.unset_relations(&attr);
        assert!(!instance.relations.contains_key(&attr));
    }
}
