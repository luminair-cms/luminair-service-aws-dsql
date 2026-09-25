//! Token validation implementations (JWKS, Shared Secret, and Mock).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::Deserialize;

use super::claims::Claims;
use super::errors::AuthError;

/// Trait for validating Bearer tokens and returning verified claims.
pub trait TokenValidator: Send + Sync {
    fn validate(&self, token: &str) -> Result<Claims, AuthError>;
}

/// Token validator using a symmetric HMAC secret (HS256).
///
/// Ideal for testing, staging, and microservices with pre-shared keys.
#[derive(Clone)]
pub struct SecretTokenValidator {
    secret: Vec<u8>,
    audience: Option<String>,
    issuer: Option<String>,
}

impl SecretTokenValidator {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
            audience: None,
            issuer: None,
        }
    }

    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Helper to generate signed tokens for tests and development.
    pub fn generate_token(&self, claims: &Claims) -> Result<String, AuthError> {
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(&self.secret);
        jsonwebtoken::encode(&header, claims, &key)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))
    }
}

impl TokenValidator for SecretTokenValidator {
    fn validate(&self, token: &str) -> Result<Claims, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        if let Some(ref aud) = self.audience {
            validation.set_audience(&[aud]);
        } else {
            validation.validate_aud = false;
        }

        if let Some(ref iss) = self.issuer {
            validation.set_issuer(&[iss]);
        }

        let key = DecodingKey::from_secret(&self.secret);
        let token_data = jsonwebtoken::decode::<Claims>(token, &key, &validation).map_err(|e| {
            if matches!(e.kind(), jsonwebtoken::errors::ErrorKind::ExpiredSignature) {
                AuthError::ExpiredToken
            } else {
                AuthError::InvalidToken(e.to_string())
            }
        })?;

        Ok(token_data.claims)
    }
}

/// Token validator that verifies RSA / EC signatures against a cached JWKS (JSON Web Key Set).
///
/// Compliant with AWS Cognito, Keycloak, and standard OIDC discovery.
#[derive(Clone)]
pub struct JwksTokenValidator {
    keys: Arc<RwLock<HashMap<String, DecodingKey>>>,
    audience: Option<String>,
    issuer: Option<String>,
}

#[derive(Deserialize)]
struct JwkKey {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Deserialize)]
struct JwksPayload {
    keys: Vec<JwkKey>,
}

impl JwksTokenValidator {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            audience: None,
            issuer: None,
        }
    }

    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Loads public keys from a raw JWKS JSON string.
    pub fn load_jwks(&self, jwks_json: &str) -> Result<usize, AuthError> {
        let payload: JwksPayload = serde_json::from_str(jwks_json)
            .map_err(|e| AuthError::InvalidToken(format!("Malformed JWKS JSON: {e}")))?;

        let mut loaded = 0;
        let mut cache = self
            .keys
            .write()
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        for key in payload.keys {
            if key.kty != "RSA" {
                continue;
            }
            let (Some(kid), Some(n), Some(e)) = (key.kid, key.n, key.e) else {
                continue;
            };
            if let Ok(decoding_key) = DecodingKey::from_rsa_components(&n, &e) {
                cache.insert(kid, decoding_key);
                loaded += 1;
            }
        }

        Ok(loaded)
    }

    /// Adds a single RSA decoding key for a known `kid`.
    pub fn add_rsa_key(&self, kid: impl Into<String>, key: DecodingKey) -> Result<(), AuthError> {
        let mut cache = self
            .keys
            .write()
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;
        cache.insert(kid.into(), key);
        Ok(())
    }
}

impl Default for JwksTokenValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenValidator for JwksTokenValidator {
    fn validate(&self, token: &str) -> Result<Claims, AuthError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("Invalid JWT header: {e}")))?;

        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken("Missing 'kid' in token header".to_string()))?;

        let cache = self
            .keys
            .read()
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        let key = cache.get(&kid).ok_or_else(|| {
            AuthError::InvalidToken(format!("Unknown signing key identifier 'kid={kid}'"))
        })?;

        let mut validation = Validation::new(header.alg);
        if let Some(ref aud) = self.audience {
            validation.set_audience(&[aud]);
        } else {
            validation.validate_aud = false;
        }

        if let Some(ref iss) = self.issuer {
            validation.set_issuer(&[iss]);
        }

        let token_data = jsonwebtoken::decode::<Claims>(token, key, &validation).map_err(|e| {
            if matches!(e.kind(), jsonwebtoken::errors::ErrorKind::ExpiredSignature) {
                AuthError::ExpiredToken
            } else {
                AuthError::InvalidToken(e.to_string())
            }
        })?;

        Ok(token_data.claims)
    }
}

/// Mock validator that parses unverified claims or returns fixed claims for test suites.
#[derive(Clone, Default)]
pub struct MockTokenValidator {
    claims_override: Option<Claims>,
}

impl MockTokenValidator {
    pub fn new() -> Self {
        Self {
            claims_override: None,
        }
    }

    pub fn with_claims(claims: Claims) -> Self {
        Self {
            claims_override: Some(claims),
        }
    }
}

impl TokenValidator for MockTokenValidator {
    fn validate(&self, token: &str) -> Result<Claims, AuthError> {
        if let Some(ref c) = self.claims_override {
            return Ok(c.clone());
        }

        // Parse claims without signature verification for tests
        let claims = jsonwebtoken::dangerous::insecure_decode_claims::<Claims>(token)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        Ok(claims)
    }
}
