use serde::{Deserialize, Serialize};

use crate::value_objects::{DocumentTypeId, RoleId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    ManageSchema,
    CreateDocument(Option<DocumentTypeId>),
    ReadDocument(Option<DocumentTypeId>),
    UpdateDocument(Option<DocumentTypeId>),
    DeleteDocument(Option<DocumentTypeId>),
    PublishDocument(Option<DocumentTypeId>),
    ManageRoles,
    ManageUsers,
}

impl Permission {
    /// Checks if this permission satisfies the requested action.
    /// `None` indicates a wildcard granting access to all document types.
    pub fn matches(&self, action: &Permission) -> bool {
        match (self, action) {
            (Permission::ManageSchema, Permission::ManageSchema) => true,
            (Permission::ManageRoles, Permission::ManageRoles) => true,
            (Permission::ManageUsers, Permission::ManageUsers) => true,
            (Permission::CreateDocument(None), Permission::CreateDocument(_)) => true,
            (Permission::CreateDocument(Some(a)), Permission::CreateDocument(Some(b))) => a == b,
            (Permission::ReadDocument(None), Permission::ReadDocument(_)) => true,
            (Permission::ReadDocument(Some(a)), Permission::ReadDocument(Some(b))) => a == b,
            (Permission::UpdateDocument(None), Permission::UpdateDocument(_)) => true,
            (Permission::UpdateDocument(Some(a)), Permission::UpdateDocument(Some(b))) => a == b,
            (Permission::DeleteDocument(None), Permission::DeleteDocument(_)) => true,
            (Permission::DeleteDocument(Some(a)), Permission::DeleteDocument(Some(b))) => a == b,
            (Permission::PublishDocument(None), Permission::PublishDocument(_)) => true,
            (Permission::PublishDocument(Some(a)), Permission::PublishDocument(Some(b))) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

impl Role {
    pub fn has_permission(&self, action: &Permission) -> bool {
        self.permissions.iter().any(|p| p.matches(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_matches_wildcard_and_exact() {
        let type_a = DocumentTypeId::try_new("article").unwrap();
        let type_b = DocumentTypeId::try_new("author").unwrap();

        let wildcard = Permission::ReadDocument(None);
        let exact_a = Permission::ReadDocument(Some(type_a));
        let exact_b = Permission::ReadDocument(Some(type_b));

        // Wildcard matches any type
        assert!(wildcard.matches(&exact_a));
        assert!(wildcard.matches(&exact_b));

        // Exact matches only same type
        assert!(exact_a.matches(&exact_a));
        assert!(!exact_a.matches(&exact_b));
        assert!(!exact_a.matches(&wildcard));

        // Different actions do not match
        assert!(!Permission::ManageSchema.matches(&exact_a));
        assert!(!Permission::CreateDocument(None).matches(&exact_a));
    }
}
