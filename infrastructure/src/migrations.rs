//! Embedded database migrations and schema provisioning.

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use uuid::{Uuid, uuid};

/// Compile-time embedded migrator targeting the `migrations` directory.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Deterministic role ID for the built-in `admin` role.
pub const ROLE_ADMIN_ID: Uuid = uuid!("01920000-0000-7000-8000-000000000001");

/// Deterministic role ID for the built-in `editor` role.
pub const ROLE_EDITOR_ID: Uuid = uuid!("01920000-0000-7000-8000-000000000002");

/// Deterministic role ID for the built-in `viewer` role.
pub const ROLE_VIEWER_ID: Uuid = uuid!("01920000-0000-7000-8000-000000000003");

/// Runs all pending static SQL migrations against the target database pool.
pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}
