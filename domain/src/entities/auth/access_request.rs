use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;
use crate::value_objects::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};
use super::user_role_assignment::UserRoleAssignment;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessRequestStatus {
    Pending,
    Approved,
    Rejected { reason: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRequest {
    pub id: AccessRequestId,
    pub user_id: UserId,
    pub email: Option<String>,
    pub name: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub status: AccessRequestStatus,
    pub reviewed_by: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub assigned_roles: Vec<RoleId>,
}

impl AccessRequest {
    pub fn new(
        user_id: UserId,
        email: Option<String>,
        name: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: AccessRequestId::new(Uuid::now_v7()),
            user_id,
            email,
            name,
            requested_at: now,
            status: AccessRequestStatus::Pending,
            reviewed_by: None,
            reviewed_at: None,
            assigned_roles: Vec::new(),
        }
    }

    pub fn approve(
        &mut self,
        by: UserId,
        roles: Vec<RoleId>,
        now: DateTime<Utc>,
    ) -> Result<Vec<UserRoleAssignment>, DomainError> {
        if self.status != AccessRequestStatus::Pending {
            return Err(DomainError::InvalidStateTransition {
                reason: format!("cannot approve access request in {:?} state", self.status),
            });
        }

        self.status = AccessRequestStatus::Approved;
        self.reviewed_by = Some(by.clone());
        self.reviewed_at = Some(now);
        self.assigned_roles = roles.clone();

        let assignments = roles
            .into_iter()
            .map(|role_id| UserRoleAssignment {
                id: UserRoleAssignmentId::new(Uuid::now_v7()),
                user_id: self.user_id.clone(),
                role_id,
                granted_at: now,
                granted_by: Some(by.clone()),
            })
            .collect();

        Ok(assignments)
    }

    pub fn reject(
        &mut self,
        by: UserId,
        reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if self.status != AccessRequestStatus::Pending {
            return Err(DomainError::InvalidStateTransition {
                reason: format!("cannot reject access request in {:?} state", self.status),
            });
        }

        self.status = AccessRequestStatus::Rejected { reason };
        self.reviewed_by = Some(by);
        self.reviewed_at = Some(now);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            AccessRequestStatus::Pending | AccessRequestStatus::Approved
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_request() -> (AccessRequest, UserId, DateTime<Utc>) {
        let user = UserId::try_new("user_req").unwrap();
        let now = Utc::now();
        let req = AccessRequest::new(user.clone(), Some("u@example.com".into()), Some("User".into()), now);
        (req, user, now)
    }

    #[test]
    fn test_new_request_is_pending() {
        let (req, user, now) = make_test_request();
        assert_eq!(req.status, AccessRequestStatus::Pending);
        assert_eq!(req.user_id, user);
        assert_eq!(req.requested_at, now);
        assert!(req.reviewed_by.is_none());
        assert!(req.reviewed_at.is_none());
        assert!(req.assigned_roles.is_empty());
    }

    #[test]
    fn test_approve_sets_status() {
        let (mut req, _, now) = make_test_request();
        let admin = UserId::try_new("admin").unwrap();
        let role = RoleId::new(Uuid::now_v7());

        req.approve(admin.clone(), vec![role], now).unwrap();
        assert_eq!(req.status, AccessRequestStatus::Approved);
        assert_eq!(req.reviewed_by, Some(admin));
        assert_eq!(req.reviewed_at, Some(now));
        assert_eq!(req.assigned_roles, vec![role]);
    }

    #[test]
    fn test_approve_returns_assignments() {
        let (mut req, user, now) = make_test_request();
        let admin = UserId::try_new("admin").unwrap();
        let role1 = RoleId::new(Uuid::now_v7());
        let role2 = RoleId::new(Uuid::now_v7());

        let assignments = req.approve(admin.clone(), vec![role1, role2], now).unwrap();
        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].user_id, user);
        assert_eq!(assignments[0].role_id, role1);
        assert_eq!(assignments[0].granted_by, Some(admin.clone()));
        assert_eq!(assignments[1].role_id, role2);
    }

    #[test]
    fn test_approve_already_approved_fails() {
        let (mut req, _, now) = make_test_request();
        let admin = UserId::try_new("admin").unwrap();
        let role = RoleId::new(Uuid::now_v7());

        req.approve(admin.clone(), vec![role], now).unwrap();
        let second = req.approve(admin, vec![role], now);
        assert!(matches!(second, Err(DomainError::InvalidStateTransition { .. })));
    }

    #[test]
    fn test_reject_sets_status() {
        let (mut req, _, now) = make_test_request();
        let admin = UserId::try_new("admin").unwrap();

        req.reject(admin.clone(), Some("Not authorized".into()), now).unwrap();
        assert_eq!(
            req.status,
            AccessRequestStatus::Rejected {
                reason: Some("Not authorized".into())
            }
        );
        assert_eq!(req.reviewed_by, Some(admin));
        assert_eq!(req.reviewed_at, Some(now));
    }

    #[test]
    fn test_reject_approved_request_fails() {
        let (mut req, _, now) = make_test_request();
        let admin = UserId::try_new("admin").unwrap();
        let role = RoleId::new(Uuid::now_v7());

        req.approve(admin.clone(), vec![role], now).unwrap();
        let res = req.reject(admin, None, now);
        assert!(matches!(res, Err(DomainError::InvalidStateTransition { .. })));
    }

    #[test]
    fn test_is_active_pending_and_approved() {
        let (mut req, _, now) = make_test_request();
        assert!(req.is_active()); // Pending

        let admin = UserId::try_new("admin").unwrap();
        let role = RoleId::new(Uuid::now_v7());
        req.approve(admin.clone(), vec![role], now).unwrap();
        assert!(req.is_active()); // Approved

        let (mut req2, _, now2) = make_test_request();
        req2.reject(admin, None, now2).unwrap();
        assert!(!req2.is_active()); // Rejected
    }
}
