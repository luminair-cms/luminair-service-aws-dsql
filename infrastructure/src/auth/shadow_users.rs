//! Shadow user tracking in PostgreSQL / AWS Aurora DSQL (ADR-005).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::errors::AuthError;

/// Local cached record of a verified OIDC user identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowUser {
    pub user_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub auth_type: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// SQLx repository for shadow user tracking.
#[derive(Debug, Clone)]
pub struct SqlxShadowUserRepository {
    pool: PgPool,
}

impl SqlxShadowUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upserts a shadow user record on authenticated request:
    /// - If user does not exist, inserts with `first_seen = NOW()`, `last_seen = NOW()`.
    /// - If user already exists, updates `last_seen = NOW()`, and updates `email`/`name` if provided.
    pub async fn upsert(
        &self,
        user_id: &str,
        email: Option<&str>,
        name: Option<&str>,
        auth_type: Option<&str>,
    ) -> Result<ShadowUser, AuthError> {
        let row = sqlx::query(
            r#"
            INSERT INTO shadow_users (user_id, email, name, auth_type, first_seen, last_seen)
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id) DO UPDATE SET
                email = COALESCE($2, shadow_users.email),
                name = COALESCE($3, shadow_users.name),
                auth_type = COALESCE($4, shadow_users.auth_type),
                last_seen = CURRENT_TIMESTAMP
            RETURNING user_id, email, name, auth_type, first_seen, last_seen
            "#,
        )
        .bind(user_id)
        .bind(email)
        .bind(name)
        .bind(auth_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

        Ok(ShadowUser {
            user_id: row.get("user_id"),
            email: row.get("email"),
            name: row.get("name"),
            auth_type: row.get("auth_type"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
        })
    }

    /// Finds a shadow user by their OIDC `user_id` (`sub`).
    pub async fn find_by_id(&self, user_id: &str) -> Result<Option<ShadowUser>, AuthError> {
        let opt_row = sqlx::query(
            r#"
            SELECT user_id, email, name, auth_type, first_seen, last_seen
            FROM shadow_users
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

        Ok(opt_row.map(|row| ShadowUser {
            user_id: row.get("user_id"),
            email: row.get("email"),
            name: row.get("name"),
            auth_type: row.get("auth_type"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
        }))
    }

    /// Lists all shadow users ordered by `last_seen DESC`.
    pub async fn find_all(&self) -> Result<Vec<ShadowUser>, AuthError> {
        let rows = sqlx::query(
            r#"
            SELECT user_id, email, name, auth_type, first_seen, last_seen
            FROM shadow_users
            ORDER BY last_seen DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| ShadowUser {
                user_id: row.get("user_id"),
                email: row.get("email"),
                name: row.get("name"),
                auth_type: row.get("auth_type"),
                first_seen: row.get("first_seen"),
                last_seen: row.get("last_seen"),
            })
            .collect())
    }
}
