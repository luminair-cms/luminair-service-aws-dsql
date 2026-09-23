use async_trait::async_trait;

use crate::entities::auth::access_request::AccessRequest;
use crate::entities::auth::role::Role;
use crate::entities::auth::user_role_assignment::UserRoleAssignment;
use crate::errors::DomainError;
use crate::value_objects::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};

#[async_trait]
pub trait RoleRepository: Send + Sync {
    async fn find_by_id(&self, id: RoleId) -> Result<Option<Role>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Role>, DomainError>;
    async fn save(&self, role: &Role) -> Result<(), DomainError>;
}

#[async_trait]
pub trait UserRoleAssignmentRepository: Send + Sync {
    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<UserRoleAssignment>, DomainError>;
    async fn exists_admin(&self, admin_role_id: RoleId) -> Result<bool, DomainError>;
    async fn save(&self, assignment: &UserRoleAssignment) -> Result<(), DomainError>;
    async fn delete(&self, id: UserRoleAssignmentId) -> Result<(), DomainError>;
}

#[async_trait]
pub trait AccessRequestRepository: Send + Sync {
    async fn find_by_id(
        &self,
        id: AccessRequestId,
    ) -> Result<Option<AccessRequest>, DomainError>;
    async fn find_by_user(&self, user_id: &UserId) -> Result<Option<AccessRequest>, DomainError>;
    async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError>;
    async fn save(&self, request: &AccessRequest) -> Result<(), DomainError>;
}
