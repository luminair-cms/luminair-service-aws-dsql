use serde::{Deserialize, Serialize};

use crate::value_objects::{AttributeId, DocumentTypeId, RelationId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OwnerRelationKind {
    HasOne,
    HasMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InverseRelationKind {
    BelongsToOne,
    BelongsToMany,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationInverse {
    pub inverse_attr: AttributeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub id: RelationId,
    pub owner_type: DocumentTypeId,
    pub owner_attr: AttributeId,
    pub owner_kind: OwnerRelationKind,
    pub target_type: DocumentTypeId,
    pub inverse: Option<RelationInverse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationView {
    OwnerSide {
        attr: AttributeId,
        kind: OwnerRelationKind,
        other_type: DocumentTypeId,
    },
    InverseSide {
        attr: AttributeId,
        kind: InverseRelationKind,
        other_type: DocumentTypeId,
    },
    Unidirectional {
        attr: AttributeId,
        kind: OwnerRelationKind,
        target_type: DocumentTypeId,
    },
}

impl Relation {
    pub fn inverse_kind(&self) -> Option<InverseRelationKind> {
        if self.inverse.is_some() {
            match self.owner_kind {
                OwnerRelationKind::HasOne => Some(InverseRelationKind::BelongsToOne),
                OwnerRelationKind::HasMany => Some(InverseRelationKind::BelongsToMany),
            }
        } else {
            None
        }
    }

    pub fn view_for(&self, type_id: &DocumentTypeId) -> Option<RelationView> {
        if type_id == &self.owner_type {
            if self.inverse.is_some() {
                Some(RelationView::OwnerSide {
                    attr: self.owner_attr.clone(),
                    kind: self.owner_kind,
                    other_type: self.target_type.clone(),
                })
            } else {
                Some(RelationView::Unidirectional {
                    attr: self.owner_attr.clone(),
                    kind: self.owner_kind,
                    target_type: self.target_type.clone(),
                })
            }
        } else if type_id == &self.target_type {
            self.inverse.as_ref().map(|inv| RelationView::InverseSide {
                attr: inv.inverse_attr.clone(),
                kind: self.inverse_kind().expect("derived inverse kind"),
                other_type: self.owner_type.clone(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_test_ids() -> (
        DocumentTypeId,
        DocumentTypeId,
        RelationId,
        AttributeId,
        AttributeId,
    ) {
        (
            DocumentTypeId::try_new("article").unwrap(),
            DocumentTypeId::try_new("tag").unwrap(),
            RelationId::new(Uuid::now_v7()),
            AttributeId::try_new("tags").unwrap(),
            AttributeId::try_new("articles").unwrap(),
        )
    }

    #[test]
    fn test_view_for_bidirectional_owner_side() {
        let (owner_type, target_type, id, owner_attr, inverse_attr) = make_test_ids();
        let relation = Relation {
            id,
            owner_type: owner_type.clone(),
            owner_attr: owner_attr.clone(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type: target_type.clone(),
            inverse: Some(RelationInverse { inverse_attr }),
        };

        let view = relation.view_for(&owner_type);
        assert_eq!(
            view,
            Some(RelationView::OwnerSide {
                attr: owner_attr,
                kind: OwnerRelationKind::HasMany,
                other_type: target_type,
            })
        );
    }

    #[test]
    fn test_view_for_bidirectional_inverse_side() {
        let (owner_type, target_type, id, owner_attr, inverse_attr) = make_test_ids();
        let relation = Relation {
            id,
            owner_type: owner_type.clone(),
            owner_attr,
            owner_kind: OwnerRelationKind::HasMany,
            target_type: target_type.clone(),
            inverse: Some(RelationInverse {
                inverse_attr: inverse_attr.clone(),
            }),
        };

        let view = relation.view_for(&target_type);
        assert_eq!(
            view,
            Some(RelationView::InverseSide {
                attr: inverse_attr,
                kind: InverseRelationKind::BelongsToMany,
                other_type: owner_type,
            })
        );
    }

    #[test]
    fn test_view_for_unidirectional() {
        let (owner_type, target_type, id, owner_attr, _) = make_test_ids();
        let relation = Relation {
            id,
            owner_type: owner_type.clone(),
            owner_attr: owner_attr.clone(),
            owner_kind: OwnerRelationKind::HasOne,
            target_type: target_type.clone(),
            inverse: None,
        };

        let view = relation.view_for(&owner_type);
        assert_eq!(
            view,
            Some(RelationView::Unidirectional {
                attr: owner_attr,
                kind: OwnerRelationKind::HasOne,
                target_type: target_type.clone(),
            })
        );

        // Target type in unidirectional has no relation view
        assert_eq!(relation.view_for(&target_type), None);
    }

    #[test]
    fn test_view_for_unrelated_type() {
        let (owner_type, target_type, id, owner_attr, _) = make_test_ids();
        let unrelated = DocumentTypeId::try_new("author").unwrap();
        let relation = Relation {
            id,
            owner_type,
            owner_attr,
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: None,
        };

        assert_eq!(relation.view_for(&unrelated), None);
    }

    #[test]
    fn test_inverse_kind_has_many() {
        let (owner_type, target_type, id, owner_attr, inverse_attr) = make_test_ids();
        let relation = Relation {
            id,
            owner_type,
            owner_attr,
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: Some(RelationInverse { inverse_attr }),
        };

        assert_eq!(
            relation.inverse_kind(),
            Some(InverseRelationKind::BelongsToMany)
        );
    }

    #[test]
    fn test_inverse_kind_has_one() {
        let (owner_type, target_type, id, owner_attr, inverse_attr) = make_test_ids();
        let relation = Relation {
            id,
            owner_type,
            owner_attr,
            owner_kind: OwnerRelationKind::HasOne,
            target_type,
            inverse: Some(RelationInverse { inverse_attr }),
        };

        assert_eq!(
            relation.inverse_kind(),
            Some(InverseRelationKind::BelongsToOne)
        );
    }

    #[test]
    fn test_inverse_kind_none_when_no_inverse() {
        let (owner_type, target_type, id, owner_attr, _) = make_test_ids();
        let relation = Relation {
            id,
            owner_type,
            owner_attr,
            owner_kind: OwnerRelationKind::HasMany,
            target_type,
            inverse: None,
        };

        assert_eq!(relation.inverse_kind(), None);
    }
}
