//! PostgreSQL / Aurora DSQL implementation of `AccessRequestRepository`.

use std::future::Future;

use chrono::{DateTime, Utc};
use domain::entities::auth::access_request::{AccessRequest, AccessRequestStatus};
use domain::errors::DomainError;
use domain::ports::AccessRequestRepository;
use domain::value_objects::{AccessRequestId, RoleId, UserId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Sqlx-backed repository for access requests.
#[derive(Debug, Clone)]
pub struct SqlxAccessRequestRepository {
    pool: PgPool,
}

impl SqlxAccessRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn status_to_db(status: &AccessRequestStatus) -> (&'static str, Option<&str>) {
    match status {
        AccessRequestStatus::Pending => ("pending", None),
        AccessRequestStatus::Approved => ("approved", None),
        AccessRequestStatus::Rejected { reason } => ("rejected", reason.as_deref()),
    }
}

fn status_from_db(
    status_str: &str,
    reason: Option<String>,
) -> Result<AccessRequestStatus, DomainError> {
    match status_str {
        "pending" => Ok(AccessRequestStatus::Pending),
        "approved" => Ok(AccessRequestStatus::Approved),
        "rejected" => Ok(AccessRequestStatus::Rejected { reason }),
        other => Err(DomainError::Storage(format!(
            "unknown access request status: {other}"
        ))),
    }
}

fn map_row_to_access_request(row: sqlx::postgres::PgRow) -> Result<AccessRequest, DomainError> {
    let id: Uuid = row.get("id");
    let user_id: String = row.get("user_id");
    let email: Option<String> = row.get("email");
    let name: Option<String> = row.get("name");
    let requested_at: DateTime<Utc> = row.get("requested_at");
    let status_str: String = row.get("status");
    let rejection_reason: Option<String> = row.get("rejection_reason");
    let reviewed_by: Option<String> = row.get("reviewed_by");
    let reviewed_at: Option<DateTime<Utc>> = row.get("reviewed_at");
    let assigned_roles_value: serde_json::Value = row.get("assigned_roles");

    let assigned_roles = serde_json::from_value::<Vec<RoleId>>(assigned_roles_value)
        .map_err(|e| DomainError::Storage(format!("failed to deserialize assigned_roles: {e}")))?;

    Ok(AccessRequest {
        id: AccessRequestId::new(id),
        user_id: UserId::try_new(user_id).map_err(|e| DomainError::Storage(e.to_string()))?,
        email,
        name,
        requested_at,
        status: status_from_db(&status_str, rejection_reason)?,
        reviewed_by: reviewed_by
            .map(|s| UserId::try_new(s).map_err(|e| DomainError::Storage(e.to_string())))
            .transpose()?,
        reviewed_at,
        assigned_roles,
    })
}

impl AccessRequestRepository for SqlxAccessRequestRepository {
    fn find_by_id(
        &self,
        id: AccessRequestId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let uuid = *id.as_ref();
            let row_opt = sqlx::query(
                r#"
                SELECT id, user_id, email, name, requested_at, status, rejection_reason,
                       reviewed_by, reviewed_at, assigned_roles
                FROM access_requests
                WHERE id = $1
                "#,
            )
            .bind(uuid)
            .fetch_optional(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            row_opt.map(map_row_to_access_request).transpose()
        }
    }

    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send {
        let pool = self.pool.clone();
        let user_id_str = user_id.as_ref().to_string();
        async move {
            let row_opt = sqlx::query(
                r#"
                SELECT id, user_id, email, name, requested_at, status, rejection_reason,
                       reviewed_by, reviewed_at, assigned_roles
                FROM access_requests
                WHERE user_id = $1
                ORDER BY requested_at DESC
                LIMIT 1
                "#,
            )
            .bind(&user_id_str)
            .fetch_optional(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            row_opt.map(map_row_to_access_request).transpose()
        }
    }

    fn find_pending(&self) -> impl Future<Output = Result<Vec<AccessRequest>, DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let rows = sqlx::query(
                r#"
                SELECT id, user_id, email, name, requested_at, status, rejection_reason,
                       reviewed_by, reviewed_at, assigned_roles
                FROM access_requests
                WHERE status = 'pending'
                ORDER BY requested_at ASC
                "#,
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            rows.into_iter().map(map_row_to_access_request).collect()
        }
    }

    fn save(
        &self,
        request: &AccessRequest,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let request = request.clone();
        async move {
            let id = *request.id.as_ref();
            let user_id = request.user_id.as_ref();
            let (status, reason) = status_to_db(&request.status);
            let reviewed_by = request.reviewed_by.as_ref().map(|u| u.as_ref());
            let assigned_roles_json =
                serde_json::to_value(&request.assigned_roles).map_err(|e| {
                    DomainError::Storage(format!("failed to serialize assigned_roles: {e}"))
                })?;

            sqlx::query(
                r#"
                INSERT INTO access_requests (
                    id, user_id, email, name, requested_at, status, rejection_reason,
                    reviewed_by, reviewed_at, assigned_roles
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (id) DO UPDATE SET
                    status = EXCLUDED.status,
                    rejection_reason = EXCLUDED.rejection_reason,
                    reviewed_by = EXCLUDED.reviewed_by,
                    reviewed_at = EXCLUDED.reviewed_at,
                    assigned_roles = EXCLUDED.assigned_roles
                "#,
            )
            .bind(id)
            .bind(user_id)
            .bind(&request.email)
            .bind(&request.name)
            .bind(request.requested_at)
            .bind(status)
            .bind(reason)
            .bind(reviewed_by)
            .bind(request.reviewed_at)
            .bind(assigned_roles_json)
            .execute(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            Ok(())
        }
    }
}
