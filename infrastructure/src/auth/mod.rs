//! Authentication, Security & Bootstrap for Luminair (ADR-005).
//!
//! Provides:
//! - Token validation and Claims extraction (Cognito, Keycloak, OIDC).
//! - Shadow user tracking in PostgreSQL / AWS Aurora DSQL.
//! - Axum extractors (`AuthenticatedClaims`, `AuthUser`) enforcing RBAC and enrollment status.
//! - Idempotent startup bootstrap hook (`BOOTSTRAP_ADMIN_SUB`).

pub mod bootstrap;
pub mod claims;
pub mod config;
pub mod errors;
pub mod extractors;
pub mod shadow_users;
pub mod validator;

pub use bootstrap::run_bootstrap;
pub use claims::Claims;
pub use config::AuthConfig;
pub use errors::{AuthError, ForbiddenReason};
pub use extractors::{AuthAppState, AuthUser, AuthenticatedClaims};
pub use shadow_users::{ShadowUser, SqlxShadowUserRepository};
pub use validator::{JwksTokenValidator, MockTokenValidator, SecretTokenValidator, TokenValidator};
