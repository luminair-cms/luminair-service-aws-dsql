use crate::entities::auth::role::{Permission, Role};
use crate::entities::document_instance::DocumentInstance;
use crate::value_objects::UserId;

pub struct AuthorizationService;

impl AuthorizationService {
    /// Evaluates access authorization using the precedence rule:
    /// 1. Owner rule (creators can read, update, delete, publish their own instances).
    /// 2. RBAC (evaluation across all assigned roles).
    /// 3. Default DENY.
    pub fn can(
        user_id: &UserId,
        action: &Permission,
        instance: Option<&DocumentInstance>,
        roles: &[Role],
    ) -> bool {
        // 1. Special Owner Rule: Owner has read/update/delete/publish permission on their instance
        if let Some(inst) = instance
            && inst.is_owned_by(user_id)
        {
            match action {
                Permission::UpdateDocument(_)
                | Permission::DeleteDocument(_)
                | Permission::PublishDocument(_)
                | Permission::ReadDocument(_) => return true,
                _ => {}
            }
        }

        // 2. RBAC check
        roles.iter().any(|role| role.has_permission(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    use crate::value_objects::DocumentTypeId;

    fn make_test_fixture() -> (UserId, UserId, DocumentTypeId, DocumentInstance) {
        let owner = UserId::try_new("owner_user").unwrap();
        let other = UserId::try_new("other_user").unwrap();
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let instance = DocumentInstance::new(type_id, Some(owner.clone()), Utc::now());
        (owner, other, type_id, instance)
    }

    #[test]
    fn test_owner_always_allowed() {
        let (owner, _, type_id, instance) = make_test_fixture();
        let action = Permission::UpdateDocument(Some(type_id));
        // Owner has no roles assigned, but owner rule permits
        assert!(AuthorizationService::can(&owner, &action, Some(&instance), &[]));
    }

    #[test]
    fn test_rbac_explicit_permission_granted() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id));
        let role = Role {
            id: crate::value_objects::RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(type_id))],
        };

        assert!(AuthorizationService::can(&other, &action, Some(&instance), &[role]));
    }

    #[test]
    fn test_rbac_wildcard_permission() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id));
        let role = Role {
            id: crate::value_objects::RoleId::new(Uuid::now_v7()),
            name: "global_reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(None)], // wildcard
        };

        assert!(AuthorizationService::can(&other, &action, Some(&instance), &[role]));
    }

    #[test]
    fn test_rbac_wrong_permission_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::DeleteDocument(Some(type_id));
        let role = Role {
            id: crate::value_objects::RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            permissions: vec![Permission::ReadDocument(Some(type_id))],
            description: None,
        };

        assert!(!AuthorizationService::can(&other, &action, Some(&instance), &[role]));
    }

    #[test]
    fn test_no_roles_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id));
        assert!(!AuthorizationService::can(&other, &action, Some(&instance), &[]));
    }

    #[test]
    fn test_admin_all_permissions() {
        let (_, other, type_id, instance) = make_test_fixture();
        let admin_role = Role {
            id: crate::value_objects::RoleId::new(Uuid::now_v7()),
            name: "admin".into(),
            description: None,
            permissions: vec![
                Permission::ManageSchema,
                Permission::ManageRoles,
                Permission::ManageUsers,
                Permission::CreateDocument(None),
                Permission::ReadDocument(None),
                Permission::UpdateDocument(None),
                Permission::DeleteDocument(None),
                Permission::PublishDocument(None),
            ],
        };

        let action = Permission::DeleteDocument(Some(type_id));
        assert!(AuthorizationService::can(&other, &action, Some(&instance), &[admin_role]));
    }
}
