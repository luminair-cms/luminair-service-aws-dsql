//! Shadow user tracking in PostgreSQL / AWS Aurora DSQL (ADR-005).
//!
//! Re-exported from `crate::repositories::shadow_user_repository` for backward compatibility.

pub use crate::repositories::shadow_user_repository::{ShadowUser, SqlxShadowUserRepository};
