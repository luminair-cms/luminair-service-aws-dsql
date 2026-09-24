use std::future::Future;

use crate::entities::auth::access_request::AccessRequest;
use crate::entities::auth::role::Role;
use crate::entities::auth::user_role_assignment::UserRoleAssignment;
use crate::errors::DomainError;
use crate::value_objects::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};

pub trait RoleRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: RoleId,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send;

    fn find_by_name(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send;

    fn find_all(&self) -> impl Future<Output = Result<Vec<Role>, DomainError>> + Send;

    fn save(&self, role: &Role) -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait UserRoleAssignmentRepository: Send + Sync {
    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Vec<UserRoleAssignment>, DomainError>> + Send;

    fn exists_admin(
        &self,
        admin_role_id: RoleId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send;

    fn save(
        &self,
        assignment: &UserRoleAssignment,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn delete(
        &self,
        id: UserRoleAssignmentId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait AccessRequestRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: AccessRequestId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send;

    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send;

    fn find_pending(&self) -> impl Future<Output = Result<Vec<AccessRequest>, DomainError>> + Send;

    fn save(&self, request: &AccessRequest)
    -> impl Future<Output = Result<(), DomainError>> + Send;
}
