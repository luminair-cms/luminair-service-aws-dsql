use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{RoleId, UserId, UserRoleAssignmentId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRoleAssignment {
    pub id: UserRoleAssignmentId,
    pub user_id: UserId,
    pub role_id: RoleId,
    pub granted_at: DateTime<Utc>,
    pub granted_by: Option<UserId>, // None = system / bootstrap grant
}
