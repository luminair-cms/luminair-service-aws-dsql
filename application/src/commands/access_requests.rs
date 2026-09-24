//! Commands for user access request operations.

use domain::value_objects::{AccessRequestId, RoleId, UserId};

/// Command to submit a new access request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitAccessRequestCommand {
    /// Identifier of the user submitting the request (OIDC sub).
    pub user_id: UserId,
    /// Email extracted from JWT claims (informational for admin review).
    pub email: Option<String>,
    /// Name extracted from JWT claims (informational for admin review).
    pub name: Option<String>,
}

impl SubmitAccessRequestCommand {
    pub fn new(user_id: UserId, email: Option<String>, name: Option<String>) -> Self {
        Self {
            user_id,
            email,
            name,
        }
    }
}

/// Command to approve a pending access request and assign roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveAccessRequestCommand {
    /// Identifier of the access request to approve.
    pub request_id: AccessRequestId,
    /// Roles to assign to the user upon approval.
    pub role_ids: Vec<RoleId>,
}

impl ApproveAccessRequestCommand {
    pub fn new(request_id: AccessRequestId, role_ids: Vec<RoleId>) -> Self {
        Self {
            request_id,
            role_ids,
        }
    }
}

/// Command to reject a pending access request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectAccessRequestCommand {
    /// Identifier of the access request to reject.
    pub request_id: AccessRequestId,
    /// Optional rejection reason for audit and feedback.
    pub reason: Option<String>,
}

impl RejectAccessRequestCommand {
    pub fn new(request_id: AccessRequestId, reason: Option<String>) -> Self {
        Self { request_id, reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_submit_access_request_command() {
        let user = UserId::try_new("sub_123").unwrap();
        let cmd = SubmitAccessRequestCommand::new(
            user.clone(),
            Some("user@example.com".into()),
            Some("John Doe".into()),
        );

        assert_eq!(cmd.user_id, user);
        assert_eq!(cmd.email.as_deref(), Some("user@example.com"));
        assert_eq!(cmd.name.as_deref(), Some("John Doe"));
    }

    #[test]
    fn test_approve_and_reject_commands() {
        let req_id = AccessRequestId::new(Uuid::now_v7());
        let role_id = RoleId::new(Uuid::now_v7());

        let approve = ApproveAccessRequestCommand::new(req_id, vec![role_id]);
        assert_eq!(approve.request_id, req_id);
        assert_eq!(approve.role_ids, vec![role_id]);

        let reject = RejectAccessRequestCommand::new(req_id, Some("Ineligible".into()));
        assert_eq!(reject.request_id, req_id);
        assert_eq!(reject.reason.as_deref(), Some("Ineligible"));
    }
}
