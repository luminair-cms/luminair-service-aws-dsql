//! Persistence container managing database connection resources and repository adapters.

use domain::schema::SchemaRegistry;
use sqlx::PgPool;

use super::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};

/// Container holding initialized database connection pool and concrete repositories.
#[derive(Debug, Clone)]
pub struct PersistenceContainer {
    pub pool: PgPool,
    pub instance_repo: SqlxDocumentInstanceRepository,
    pub access_request_repo: SqlxAccessRequestRepository,
    pub assignment_repo: SqlxUserRoleAssignmentRepository,
    pub role_repo: SqlxRoleRepository,
    pub shadow_user_repo: SqlxShadowUserRepository,
}

impl PersistenceContainer {
    /// Constructs a persistence container with all repository adapters initialized.
    pub fn new(pool: PgPool, schema_registry: &'static SchemaRegistry) -> Self {
        Self {
            instance_repo: SqlxDocumentInstanceRepository::new(pool.clone(), schema_registry),
            access_request_repo: SqlxAccessRequestRepository::new(pool.clone()),
            assignment_repo: SqlxUserRoleAssignmentRepository::new(pool.clone()),
            role_repo: SqlxRoleRepository::new(pool.clone()),
            shadow_user_repo: SqlxShadowUserRepository::new(pool.clone()),
            pool,
        }
    }
}
