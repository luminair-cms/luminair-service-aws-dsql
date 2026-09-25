//! Authentication and bootstrap configuration.

/// Configuration for OIDC validation and administrative bootstrapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfig {
    /// OIDC Identity Provider Issuer URL (e.g. `https://cognito-idp.us-east-1.amazonaws.com/...` or Keycloak realm).
    pub issuer_url: Option<String>,
    /// Expected JWT audience (`aud` claim).
    pub audience: Option<String>,
    /// OIDC `sub` of the bootstrap administrator (read from `BOOTSTRAP_ADMIN_SUB`).
    pub bootstrap_admin_sub: Option<String>,
    /// Display auth provider identifier (read from `BOOTSTRAP_AUTH_TYPE`, default `"oidc"`).
    pub bootstrap_auth_type: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            issuer_url: None,
            audience: None,
            bootstrap_admin_sub: None,
            bootstrap_auth_type: "oidc".to_string(),
        }
    }
}

impl AuthConfig {
    /// Loads authentication configuration from environment variables:
    /// - `AUTH_ISSUER_URL`: Issuer URL
    /// - `AUTH_AUDIENCE`: Expected audience
    /// - `BOOTSTRAP_ADMIN_SUB`: Initial administrator OIDC `sub`
    /// - `BOOTSTRAP_AUTH_TYPE`: Optional identity provider descriptor (default `"oidc"`)
    pub fn from_env() -> Self {
        let issuer_url = std::env::var("AUTH_ISSUER_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let audience = std::env::var("AUTH_AUDIENCE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let bootstrap_admin_sub = std::env::var("BOOTSTRAP_ADMIN_SUB")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let bootstrap_auth_type = std::env::var("BOOTSTRAP_AUTH_TYPE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "oidc".to_string());

        Self {
            issuer_url,
            audience,
            bootstrap_admin_sub,
            bootstrap_auth_type,
        }
    }
}
