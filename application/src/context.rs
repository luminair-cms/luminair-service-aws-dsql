//! Security and caller identity context.

use domain::entities::document_instance::DocumentInstance;
use domain::entities::role::{Permission, Role};
use domain::services::authorization::AuthorizationService;
use domain::value_objects::{RoleId, UserId};

use crate::errors::ApplicationError;

/// Authenticated caller context carrying the verified identity and roles of the actor.
#[derive(Debug, Clone)]
pub struct CallerContext {
    /// The unique identifier of the authenticated user (e.g. OIDC sub claim).
    pub user_id: UserId,
    /// Roles assigned to the user.
    pub roles: Vec<Role>,
}

impl CallerContext {
    /// Creates a new `CallerContext` for a verified user and their roles.
    pub fn new(user_id: UserId, roles: Vec<Role>) -> Self {
        Self { user_id, roles }
    }

    /// Creates an internal system caller context with administrative privileges.
    pub fn system() -> Self {
        let user_id = UserId::try_new("system-internal").expect("valid system user id");
        let admin_role = Role {
            id: RoleId::new(uuid::Uuid::nil()),
            name: "system_admin".to_string(),
            description: Some("System internal role with full privileges".to_string()),
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
        Self {
            user_id,
            roles: vec![admin_role],
        }
    }

    /// Evaluates whether the caller has permission to perform `action` on the optional `instance`.
    ///
    /// Precedence:
    /// 1. Owner rule (creators have full rights to their own instances).
    /// 2. RBAC (across all assigned roles).
    /// 3. Default DENY.
    pub fn can(&self, action: &Permission, instance: Option<&DocumentInstance>) -> bool {
        AuthorizationService::can(&self.user_id, action, instance, &self.roles)
    }

    /// Enforces that the caller has permission to perform `action` on the optional `instance`.
    ///
    /// Returns `Ok(())` if allowed, or `Err(ApplicationError::Unauthorized)` if denied.
    pub fn check_permission(
        &self,
        action: &Permission,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        if self.can(action, instance) {
            Ok(())
        } else {
            Err(ApplicationError::Unauthorized {
                user_id: self.user_id.clone(),
                action: action.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::value_objects::DocumentTypeId;
    use uuid::Uuid;

    fn make_test_instance(owner: &UserId) -> (DocumentTypeId, DocumentInstance) {
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let instance = DocumentInstance::new(type_id, Some(owner.clone()), Utc::now());
        (type_id, instance)
    }

    #[test]
    fn test_system_caller_has_all_permissions() {
        let ctx = CallerContext::system();
        let type_id = DocumentTypeId::new(Uuid::now_v7());

        assert!(ctx.can(&Permission::ManageSchema, None));
        assert!(ctx.can(&Permission::ManageRoles, None));
        assert!(ctx.can(&Permission::ManageUsers, None));
        assert!(ctx.can(&Permission::CreateDocument(Some(type_id)), None));
        assert!(ctx.can(&Permission::PublishDocument(Some(type_id)), None));

        assert!(ctx
            .check_permission(&Permission::ManageSchema, None)
            .is_ok());
    }

    #[test]
    fn test_check_permission_denied_returns_unauthorized() {
        let user_id = UserId::try_new("user-reader").expect("valid user");
        let ctx = CallerContext::new(user_id.clone(), Vec::new());
        let type_id = DocumentTypeId::new(Uuid::now_v7());

        let res = ctx.check_permission(&Permission::DeleteDocument(Some(type_id)), None);
        match res {
            Err(ApplicationError::Unauthorized {
                user_id: err_user,
                action,
            }) => {
                assert_eq!(err_user, user_id);
                assert_eq!(action, Permission::DeleteDocument(Some(type_id)));
            }
            _ => panic!("expected Unauthorized error"),
        }
    }

    #[test]
    fn test_owner_rule_allows_read_and_update_without_roles() {
        let owner = UserId::try_new("user-owner").expect("valid user");
        let ctx = CallerContext::new(owner.clone(), Vec::new());
        let (type_id, instance) = make_test_instance(&owner);

        // Owner may read and update without any role
        assert!(ctx.can(&Permission::UpdateDocument(Some(type_id)), Some(&instance)));
        assert!(ctx.can(&Permission::ReadDocument(Some(type_id)), Some(&instance)));
        assert!(ctx.check_permission(&Permission::UpdateDocument(Some(type_id)), Some(&instance)).is_ok());
        // Publish and Delete require explicit RBAC — owner rule does not apply
        assert!(!ctx.can(&Permission::PublishDocument(Some(type_id)), Some(&instance)));
        assert!(!ctx.can(&Permission::DeleteDocument(Some(type_id)), Some(&instance)));
    }

    #[test]
    fn test_rbac_allows_action_with_role() {
        let user = UserId::try_new("user-editor").expect("valid user");
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "editor".to_string(),
            description: None,
            permissions: vec![Permission::CreateDocument(Some(type_id))],
        };
        let ctx = CallerContext::new(user, vec![role]);

        assert!(ctx.can(&Permission::CreateDocument(Some(type_id)), None));
        assert!(!ctx.can(&Permission::DeleteDocument(Some(type_id)), None));
    }
}
