//! Token claims extracted from validated JWTs.

use domain::value_objects::UserId;
use serde::{Deserialize, Serialize};

use super::errors::AuthError;

/// Standard OIDC JWT claims used by Luminair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Subject identifier (`sub`), which maps 1:1 to Luminair `UserId`.
    pub sub: String,
    /// User email address (if provided in token scopes).
    #[serde(default)]
    pub email: Option<String>,
    /// User full name / display name (if provided in token scopes).
    #[serde(default)]
    pub name: Option<String>,
    /// Issuer identifier (`iss`).
    #[serde(default)]
    pub iss: Option<String>,
    /// Audience (`aud`), which can be a single string or an array of strings.
    #[serde(default)]
    pub aud: Option<serde_json::Value>,
    /// Expiration time (UNIX epoch seconds).
    #[serde(default)]
    pub exp: Option<usize>,
    /// Issued-at time (UNIX epoch seconds).
    #[serde(default)]
    pub iat: Option<usize>,
}

impl Claims {
    /// Constructs a `UserId` value object from the `sub` claim.
    pub fn user_id(&self) -> Result<UserId, AuthError> {
        UserId::try_new(self.sub.clone()).map_err(|e| AuthError::InvalidClaim {
            claim: "sub".to_string(),
            reason: e.to_string(),
        })
    }

    /// Verifies that the claims match the expected audience, if configured.
    pub fn matches_audience(&self, expected_aud: &str) -> bool {
        match &self.aud {
            None => false,
            Some(serde_json::Value::String(s)) => s == expected_aud,
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .any(|item| item.as_str().map(|s| s == expected_aud).unwrap_or(false)),
            _ => false,
        }
    }

    /// Verifies that the claims match the expected issuer, if configured.
    pub fn matches_issuer(&self, expected_iss: &str) -> bool {
        self.iss.as_deref() == Some(expected_iss)
    }
}
