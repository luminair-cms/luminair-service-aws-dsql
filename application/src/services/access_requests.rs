//! Access requests application service.

use std::future::Future;
use std::sync::Arc;

use chrono::Utc;
use domain::entities::access_request::AccessRequest;
use domain::entities::role::Permission;
use domain::entities::user_role_assignment::UserRoleAssignment;
use domain::errors::DomainError;
use domain::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::value_objects::AccessRequestId;

use crate::commands::access_requests::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

/// Port trait defining use cases for user access requests and enrollment.
pub trait AccessRequestsService: Send + Sync + 'static {
    /// Submits a new access request for an authenticated IdP identity.
    fn submit(
        &self,
        cmd: SubmitAccessRequestCommand,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    /// Approves a pending access request, assigning specified roles.
    fn approve(
        &self,
        caller: &CallerContext,
        cmd: ApproveAccessRequestCommand,
    ) -> impl Future<Output = Result<Vec<UserRoleAssignment>, ApplicationError>> + Send;

    /// Rejects a pending access request with an optional reason.
    fn reject(
        &self,
        caller: &CallerContext,
        cmd: RejectAccessRequestCommand,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    /// Fetches an access request by identifier (allowed for the requester or users with `ManageUsers`).
    fn get_by_id(
        &self,
        caller: &CallerContext,
        request_id: AccessRequestId,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    /// Lists all pending access requests awaiting administrator review.
    fn list_pending(
        &self,
        caller: &CallerContext,
    ) -> impl Future<Output = Result<Vec<AccessRequest>, ApplicationError>> + Send;
}

/// Generic implementation of `AccessRequestsService` monomorphized over repository adapters.
pub struct AccessRequestsServiceImpl<A, U, R> {
    pub access_request_repo: Arc<A>,
    pub assignment_repo: Arc<U>,
    pub role_repo: Arc<R>,
}

impl<A, U, R> AccessRequestsServiceImpl<A, U, R>
where
    A: AccessRequestRepository + 'static,
    U: UserRoleAssignmentRepository + 'static,
    R: RoleRepository + 'static,
{
    pub fn new(
        access_request_repo: Arc<A>,
        assignment_repo: Arc<U>,
        role_repo: Arc<R>,
    ) -> Self {
        Self {
            access_request_repo,
            assignment_repo,
            role_repo,
        }
    }
}

impl<A, U, R> AccessRequestsService for AccessRequestsServiceImpl<A, U, R>
where
    A: AccessRequestRepository + 'static,
    U: UserRoleAssignmentRepository + 'static,
    R: RoleRepository + 'static,
{
    async fn submit(
        &self,
        cmd: SubmitAccessRequestCommand,
    ) -> Result<AccessRequest, ApplicationError> {
        // Enforce invariant: at most one active (Pending or Approved) request per user
        if let Some(existing) = self
            .access_request_repo
            .find_by_user(&cmd.user_id)
            .await?
            && existing.is_active()
        {
            return Err(ApplicationError::Domain(
                DomainError::AccessRequestAlreadyActive(cmd.user_id),
            ));
        }

        let request = AccessRequest::new(cmd.user_id, cmd.email, cmd.name, Utc::now());
        self.access_request_repo.save(&request).await?;
        Ok(request)
    }

    async fn approve(
        &self,
        caller: &CallerContext,
        cmd: ApproveAccessRequestCommand,
    ) -> Result<Vec<UserRoleAssignment>, ApplicationError> {
        caller.check_permission(&Permission::ManageUsers, None)?;

        let mut request = self
            .access_request_repo
            .find_by_id(cmd.request_id)
            .await?
            .ok_or(ApplicationError::Domain(DomainError::AccessRequestNotFound(
                cmd.request_id,
            )))?;

        // Validate that all assigned roles exist
        for role_id in &cmd.role_ids {
            if self.role_repo.find_by_id(*role_id).await?.is_none() {
                return Err(ApplicationError::NotFound {
                    entity: "Role",
                    id: role_id.to_string(),
                });
            }
        }

        let assignments = request.approve(
            caller.user_id.clone(),
            cmd.role_ids,
            Utc::now(),
        )?;

        // Persist generated assignments
        for assignment in &assignments {
            self.assignment_repo.save(assignment).await?;
        }

        // Persist updated request status
        self.access_request_repo.save(&request).await?;

        Ok(assignments)
    }

    async fn reject(
        &self,
        caller: &CallerContext,
        cmd: RejectAccessRequestCommand,
    ) -> Result<AccessRequest, ApplicationError> {
        caller.check_permission(&Permission::ManageUsers, None)?;

        let mut request = self
            .access_request_repo
            .find_by_id(cmd.request_id)
            .await?
            .ok_or(ApplicationError::Domain(DomainError::AccessRequestNotFound(
                cmd.request_id,
            )))?;

        request.reject(caller.user_id.clone(), cmd.reason, Utc::now())?;
        self.access_request_repo.save(&request).await?;

        Ok(request)
    }

    async fn get_by_id(
        &self,
        caller: &CallerContext,
        request_id: AccessRequestId,
    ) -> Result<AccessRequest, ApplicationError> {
        let request = self
            .access_request_repo
            .find_by_id(request_id)
            .await?
            .ok_or(ApplicationError::Domain(DomainError::AccessRequestNotFound(
                request_id,
            )))?;

        // Requester can view their own request; other users require ManageUsers
        if request.user_id != caller.user_id {
            caller.check_permission(&Permission::ManageUsers, None)?;
        }

        Ok(request)
    }

    async fn list_pending(
        &self,
        caller: &CallerContext,
    ) -> Result<Vec<AccessRequest>, ApplicationError> {
        caller.check_permission(&Permission::ManageUsers, None)?;
        let pending = self.access_request_repo.find_pending().await?;
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use domain::entities::access_request::AccessRequestStatus;
    use domain::entities::role::Role;
    use domain::value_objects::{RoleId, UserId};
    use uuid::Uuid;

    use crate::test_support::{
        FakeAccessRequestRepository, FakeRoleRepository, FakeUserRoleAssignmentRepository,
    };

    fn make_test_fixture() -> (
        AccessRequestsServiceImpl<
            FakeAccessRequestRepository,
            FakeUserRoleAssignmentRepository,
            FakeRoleRepository,
        >,
        CallerContext,
        Role,
    ) {
        let access_repo = Arc::new(FakeAccessRequestRepository::new());
        let assignment_repo = Arc::new(FakeUserRoleAssignmentRepository::new());


        let editor_role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "editor".to_string(),
            description: None,
            permissions: vec![Permission::CreateDocument(None)],
        };
        let role_repo = Arc::new(FakeRoleRepository::new().with_role(editor_role.clone()));

        let service = AccessRequestsServiceImpl::new(
            access_repo,
            assignment_repo,
            role_repo,
        );

        let admin = CallerContext::system();
        (service, admin, editor_role)
    }

    #[tokio::test]
    async fn test_submit_creates_pending_request() {
        let (service, _, _) = make_test_fixture();
        let user = UserId::try_new("user_new").unwrap();

        let req = service
            .submit(SubmitAccessRequestCommand::new(
                user.clone(),
                Some("new@example.com".into()),
                Some("New User".into()),
            ))
            .await
            .expect("submit success");

        assert_eq!(req.user_id, user);
        assert!(matches!(req.status, AccessRequestStatus::Pending));

        let stored = service
            .access_request_repo
            .find_by_id(req.id)
            .await
            .unwrap()
            .expect("stored");
        assert_eq!(stored.id, req.id);
    }

    #[tokio::test]
    async fn test_submit_duplicate_active_fails() {
        let (service, _, _) = make_test_fixture();
        let user = UserId::try_new("user_dup").unwrap();

        service
            .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
            .await
            .expect("first submit ok");

        let second = service
            .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
            .await;

        assert!(matches!(
            second,
            Err(ApplicationError::Domain(DomainError::AccessRequestAlreadyActive(u))) if u == user
        ));
    }

    #[tokio::test]
    async fn test_approve_creates_role_assignments() {
        let (service, admin, role) = make_test_fixture();
        let user = UserId::try_new("user_to_approve").unwrap();

        let req = service
            .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
            .await
            .unwrap();

        let assignments = service
            .approve(
                &admin,
                ApproveAccessRequestCommand::new(req.id, vec![role.id]),
            )
            .await
            .expect("approve success");

        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].user_id, user);
        assert_eq!(assignments[0].role_id, role.id);

        let updated_req = service
            .access_request_repo
            .find_by_id(req.id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(updated_req.status, AccessRequestStatus::Approved));
        assert_eq!(updated_req.assigned_roles, vec![role.id]);

        let stored_assignments = service
            .assignment_repo
            .find_by_user(&user)
            .await
            .unwrap();
        assert_eq!(stored_assignments.len(), 1);
    }

    #[tokio::test]
    async fn test_approve_missing_role_fails() {
        let (service, admin, _) = make_test_fixture();
        let user = UserId::try_new("user_missing_role").unwrap();

        let req = service
            .submit(SubmitAccessRequestCommand::new(user, None, None))
            .await
            .unwrap();

        let fake_role_id = RoleId::new(Uuid::now_v7());
        let res = service
            .approve(
                &admin,
                ApproveAccessRequestCommand::new(req.id, vec![fake_role_id]),
            )
            .await;

        assert!(matches!(res, Err(ApplicationError::NotFound { entity: "Role", .. })));
    }

    #[tokio::test]
    async fn test_reject_records_reason() {
        let (service, admin, _) = make_test_fixture();
        let user = UserId::try_new("user_to_reject").unwrap();

        let req = service
            .submit(SubmitAccessRequestCommand::new(user, None, None))
            .await
            .unwrap();

        let rejected = service
            .reject(
                &admin,
                RejectAccessRequestCommand::new(req.id, Some("Not qualified".into())),
            )
            .await
            .expect("reject success");

        assert!(matches!(
            rejected.status,
            AccessRequestStatus::Rejected {
                reason: Some(ref r)
            } if r == "Not qualified"
        ));
    }

    #[tokio::test]
    async fn test_unauthorized_admin_operations_denied() {
        let (service, _, role) = make_test_fixture();
        let user = UserId::try_new("normal_user").unwrap();
        let normal_caller = CallerContext::new(user.clone(), vec![]);

        let req = service
            .submit(SubmitAccessRequestCommand::new(user, None, None))
            .await
            .unwrap();

        let approve_res = service
            .approve(
                &normal_caller,
                ApproveAccessRequestCommand::new(req.id, vec![role.id]),
            )
            .await;
        assert!(matches!(approve_res, Err(ApplicationError::Unauthorized { .. })));

        let reject_res = service
            .reject(
                &normal_caller,
                RejectAccessRequestCommand::new(req.id, None),
            )
            .await;
        assert!(matches!(reject_res, Err(ApplicationError::Unauthorized { .. })));

        let list_res = service.list_pending(&normal_caller).await;
        assert!(matches!(list_res, Err(ApplicationError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_get_by_id_owner_allowed() {
        let (service, _, _) = make_test_fixture();
        let user = UserId::try_new("owner_user").unwrap();
        let owner_caller = CallerContext::new(user.clone(), vec![]);

        let req = service
            .submit(SubmitAccessRequestCommand::new(user, None, None))
            .await
            .unwrap();

        let fetched = service
            .get_by_id(&owner_caller, req.id)
            .await
            .expect("owner can view own request");
        assert_eq!(fetched.id, req.id);
    }
}
