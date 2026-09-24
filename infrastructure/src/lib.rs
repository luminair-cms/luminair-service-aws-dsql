//! Luminair Infrastructure crate.

pub mod migrations;

pub use migrations::{MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations};
