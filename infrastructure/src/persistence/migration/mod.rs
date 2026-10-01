//! Static and dynamic database migrations.

pub mod dynamic;
pub mod embedded;

pub use dynamic::*;
pub use embedded::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations as run_static_migrations,
};

// Re-export run_migrations as convenience alias
pub use embedded::run_migrations;
