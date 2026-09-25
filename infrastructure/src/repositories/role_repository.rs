//! PostgreSQL / Aurora DSQL implementation of `RoleRepository`.

use std::collections::HashMap;
use std::future::Future;

use domain::entities::auth::role::{Permission, Role};
use domain::errors::DomainError;
use domain::ports::RoleRepository;
use domain::value_objects::{DocumentTypeId, RoleId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Sqlx-backed repository for roles and permissions.
#[derive(Debug, Clone)]
pub struct SqlxRoleRepository {
    pool: PgPool,
}

impl SqlxRoleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn permission_to_db(perm: &Permission) -> (&'static str, Option<String>) {
    match perm {
        Permission::ManageSchema => ("ManageSchema", None),
        Permission::ManageRoles => ("ManageRoles", None),
        Permission::ManageUsers => ("ManageUsers", None),
        Permission::CreateDocument(opt) => (
            "CreateDocument",
            opt.as_ref().map(|id| id.as_ref().to_string()),
        ),
        Permission::ReadDocument(opt) => (
            "ReadDocument",
            opt.as_ref().map(|id| id.as_ref().to_string()),
        ),
        Permission::UpdateDocument(opt) => (
            "UpdateDocument",
            opt.as_ref().map(|id| id.as_ref().to_string()),
        ),
        Permission::DeleteDocument(opt) => (
            "DeleteDocument",
            opt.as_ref().map(|id| id.as_ref().to_string()),
        ),
        Permission::PublishDocument(opt) => (
            "PublishDocument",
            opt.as_ref().map(|id| id.as_ref().to_string()),
        ),
    }
}

fn permission_from_db(action: &str, doc_type: Option<String>) -> Result<Permission, DomainError> {
    let doc_id = match doc_type {
        Some(s) => {
            Some(DocumentTypeId::try_new(s).map_err(|e| DomainError::Storage(e.to_string()))?)
        }
        None => None,
    };
    match action {
        "ManageSchema" => Ok(Permission::ManageSchema),
        "ManageRoles" => Ok(Permission::ManageRoles),
        "ManageUsers" => Ok(Permission::ManageUsers),
        "CreateDocument" => Ok(Permission::CreateDocument(doc_id)),
        "ReadDocument" => Ok(Permission::ReadDocument(doc_id)),
        "UpdateDocument" => Ok(Permission::UpdateDocument(doc_id)),
        "DeleteDocument" => Ok(Permission::DeleteDocument(doc_id)),
        "PublishDocument" => Ok(Permission::PublishDocument(doc_id)),
        other => Err(DomainError::Storage(format!(
            "unknown permission action: {other}"
        ))),
    }
}

impl RoleRepository for SqlxRoleRepository {
    fn find_by_id(
        &self,
        id: RoleId,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let role_uuid = *id.as_ref();
            let row_opt = sqlx::query(
                r#"
                SELECT id, name, description
                FROM roles
                WHERE id = $1
                "#,
            )
            .bind(role_uuid)
            .fetch_optional(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let row = match row_opt {
                Some(r) => r,
                None => return Ok(None),
            };

            let name: String = row.get("name");
            let description: Option<String> = row.get("description");

            let perm_rows = sqlx::query(
                r#"
                SELECT action, document_type_id
                FROM role_permissions
                WHERE role_id = $1
                ORDER BY created_at ASC
                "#,
            )
            .bind(role_uuid)
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let mut permissions = Vec::with_capacity(perm_rows.len());
            for p_row in perm_rows {
                let action: String = p_row.get("action");
                let doc_type: Option<String> = p_row.get("document_type_id");
                permissions.push(permission_from_db(&action, doc_type)?);
            }

            Ok(Some(Role {
                id,
                name,
                description,
                permissions,
            }))
        }
    }

    fn find_by_name(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send {
        let pool = self.pool.clone();
        let name_owned = name.to_string();
        async move {
            let row_opt = sqlx::query(
                r#"
                SELECT id, name, description
                FROM roles
                WHERE name = $1
                "#,
            )
            .bind(&name_owned)
            .fetch_optional(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let row = match row_opt {
                Some(r) => r,
                None => return Ok(None),
            };

            let role_uuid: Uuid = row.get("id");
            let role_id = RoleId::new(role_uuid);
            let description: Option<String> = row.get("description");

            let perm_rows = sqlx::query(
                r#"
                SELECT action, document_type_id
                FROM role_permissions
                WHERE role_id = $1
                ORDER BY created_at ASC
                "#,
            )
            .bind(role_uuid)
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let mut permissions = Vec::with_capacity(perm_rows.len());
            for p_row in perm_rows {
                let action: String = p_row.get("action");
                let doc_type: Option<String> = p_row.get("document_type_id");
                permissions.push(permission_from_db(&action, doc_type)?);
            }

            Ok(Some(Role {
                id: role_id,
                name: name_owned,
                description,
                permissions,
            }))
        }
    }

    fn find_all(&self) -> impl Future<Output = Result<Vec<Role>, DomainError>> + Send {
        let pool = self.pool.clone();
        async move {
            let role_rows = sqlx::query(
                r#"
                SELECT id, name, description
                FROM roles
                ORDER BY name ASC
                "#,
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            if role_rows.is_empty() {
                return Ok(Vec::new());
            }

            let perm_rows = sqlx::query(
                r#"
                SELECT role_id, action, document_type_id
                FROM role_permissions
                ORDER BY created_at ASC
                "#,
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            let mut perms_by_role: HashMap<Uuid, Vec<Permission>> = HashMap::new();
            for p_row in perm_rows {
                let role_uuid: Uuid = p_row.get("role_id");
                let action: String = p_row.get("action");
                let doc_type: Option<String> = p_row.get("document_type_id");
                let perm = permission_from_db(&action, doc_type)?;
                perms_by_role.entry(role_uuid).or_default().push(perm);
            }

            let mut roles = Vec::with_capacity(role_rows.len());
            for row in role_rows {
                let role_uuid: Uuid = row.get("id");
                let name: String = row.get("name");
                let description: Option<String> = row.get("description");
                let permissions = perms_by_role.remove(&role_uuid).unwrap_or_default();

                roles.push(Role {
                    id: RoleId::new(role_uuid),
                    name,
                    description,
                    permissions,
                });
            }

            Ok(roles)
        }
    }

    fn save(&self, role: &Role) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let role = role.clone();
        async move {
            let mut tx = pool
                .begin()
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            let role_uuid = *role.id.as_ref();

            sqlx::query(
                r#"
                INSERT INTO roles (id, name, description, updated_at)
                VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
                ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    description = EXCLUDED.description,
                    updated_at = CURRENT_TIMESTAMP
                "#,
            )
            .bind(role_uuid)
            .bind(&role.name)
            .bind(&role.description)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            // Replace permissions
            sqlx::query(
                r#"
                DELETE FROM role_permissions
                WHERE role_id = $1
                "#,
            )
            .bind(role_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

            for perm in &role.permissions {
                let (action, doc_type) = permission_to_db(perm);
                let perm_id = Uuid::now_v7();

                sqlx::query(
                    r#"
                    INSERT INTO role_permissions (id, role_id, action, document_type_id)
                    VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(perm_id)
                .bind(role_uuid)
                .bind(action)
                .bind(doc_type)
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            }

            tx.commit()
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            Ok(())
        }
    }
}
