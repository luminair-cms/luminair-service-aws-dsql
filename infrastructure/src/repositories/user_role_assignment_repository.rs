//! PostgreSQL / Aurora DSQL implementation of `UserRoleAssignmentRepository`.

use std::future::Future;

use chrono::{DateTime, Utc};
use domain::entities::auth::user_role_assignment::UserRoleAssignment;
use domain::errors::DomainError;
use domain::ports::UserRoleAssignmentRepository;
use domain::value_objects::{RoleId, UserId, UserRoleAssignmentId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Sqlx-backed repository for user role assignments.
#[derive(Debug, Clone)]
pub struct SqlxUserRoleAssignmentRepository {
    pool: PgPool,
}

impl SqlxUserRoleAssignmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRoleAssignmentRepository for SqlxUserRoleAssignmentRepository {
    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Vec<UserRoleAssignment>, DomainError>> + Send {
        let pool = self.pool.clone();
        let user_id_str = user_id.as_ref().to_string();
        async move {
            let rows = sqlx::query(
                r#"
                SELECT id, user_id, role_id, granted_at, granted_by
                FROM user_role_assignments
                WHERE user_id = $1
                ORDER BY granted_at ASC
                "#,
            )
            .bind(&user_id_str)
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let mut assignments = Vec::with_capacity(rows.len());
            for row in rows {
                let id: Uuid = row.get("id");
                let uid: String = row.get("user_id");
                let role_uuid: Uuid = row.get("role_id");
                let granted_at: DateTime<Utc> = row.get("granted_at");
                let granted_by: Option<String> = row.get("granted_by");

                assignments.push(UserRoleAssignment {
                    id: UserRoleAssignmentId::new(id),
                    user_id: UserId::try_new(uid)
                        .map_err(|e| DomainError::Storage(e.to_string()))?,
                    role_id: RoleId::new(role_uuid),
                    granted_at,
                    granted_by: granted_by
                        .map(|s| {
                            UserId::try_new(s).map_err(|e| DomainError::Storage(e.to_string()))
                        })
                        .transpose()?,
                });
            }

            Ok(assignments)
        }
    }

    fn exists_admin(
        &self,
        admin_role_id: RoleId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let role_uuid = *admin_role_id.as_ref();
            let row = sqlx::query(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM user_role_assignments
                    WHERE role_id = $1
                ) AS exists
                "#,
            )
            .bind(role_uuid)
            .fetch_one(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let exists: bool = row.get("exists");
            Ok(exists)
        }
    }

    fn save(
        &self,
        assignment: &UserRoleAssignment,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let assignment = assignment.clone();
        async move {
            let id = *assignment.id.as_ref();
            let user_id = assignment.user_id.as_ref();
            let role_id = *assignment.role_id.as_ref();
            let granted_by = assignment.granted_by.as_ref().map(|u| u.as_ref());

            sqlx::query(
                r#"
                INSERT INTO user_role_assignments (id, user_id, role_id, granted_at, granted_by)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (user_id, role_id) DO UPDATE SET
                    granted_at = EXCLUDED.granted_at,
                    granted_by = EXCLUDED.granted_by
                "#,
            )
            .bind(id)
            .bind(user_id)
            .bind(role_id)
            .bind(assignment.granted_at)
            .bind(granted_by)
            .execute(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            Ok(())
        }
    }

    fn delete(
        &self,
        id: UserRoleAssignmentId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let uuid = *id.as_ref();
            sqlx::query(
                r#"
                DELETE FROM user_role_assignments
                WHERE id = $1
                "#,
            )
            .bind(uuid)
            .execute(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            Ok(())
        }
    }
}
