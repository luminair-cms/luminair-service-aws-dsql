//! Shadow user tracking in PostgreSQL / AWS Aurora DSQL (ADR-005).
//!
//! Re-exported from `crate::persistence::repositories` for backward compatibility.

pub use crate::persistence::repositories::{ShadowUser, SqlxShadowUserRepository};
