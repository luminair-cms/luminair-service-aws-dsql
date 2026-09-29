//! Server configuration and environment variable parsing.

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

use crate::auth::{AuthConfig, JwksTokenValidator, SecretTokenValidator, TokenValidator};

/// Configuration parsing and validation errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(&'static str),

    #[error("Invalid value for environment variable {0}: {1}")]
    InvalidEnvVar(&'static str, String),

    #[error("Authentication configuration error: {0}")]
    Auth(String),
}

/// Server runtime configuration loaded from environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub database_url: String,
    pub max_db_connections: u32,
    pub schema_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub auth: AuthConfig,
    pub auth_secret: Option<String>,
}

impl ServerConfig {
    /// Loads server configuration from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnvVar("DATABASE_URL"))?;

        let max_db_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(20);

        let schema_dir = std::env::var("SCHEMA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("schema"));

        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());

        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse()
            .map_err(|e| ConfigError::InvalidEnvVar("PORT", format!("{e}")))?;

        let auth = AuthConfig::from_env();
        let auth_secret = std::env::var("AUTH_SECRET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            database_url,
            max_db_connections,
            schema_dir,
            host,
            port,
            auth,
            auth_secret,
        })
    }

    /// Returns the formatted `host:port` socket address string.
    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Initializes the configured token validator based on secret or JWKS issuer.
    pub fn init_token_validator(&self) -> Result<Arc<dyn TokenValidator>, ConfigError> {
        if let Some(secret) = &self.auth_secret {
            let mut v = SecretTokenValidator::new(secret.as_bytes());
            if let Some(aud) = &self.auth.audience {
                v = v.with_audience(aud);
            }
            if let Some(iss) = &self.auth.issuer_url {
                v = v.with_issuer(iss);
            }
            Ok(Arc::new(v))
        } else if let Some(issuer) = &self.auth.issuer_url {
            let mut v = JwksTokenValidator::new().with_issuer(issuer);
            if let Some(aud) = &self.auth.audience {
                v = v.with_audience(aud);
            }
            Ok(Arc::new(v))
        } else {
            Err(ConfigError::Auth(
                "Either AUTH_SECRET or AUTH_ISSUER_URL must be set in environment".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr_formatting() {
        let config = ServerConfig {
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        assert_eq!(config.server_addr(), "127.0.0.1:3000");
    }

    #[test]
    fn test_init_token_validator_secret() {
        let config = ServerConfig {
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        assert!(config.init_token_validator().is_ok());
    }

    #[test]
    fn test_init_token_validator_missing() {
        let config = ServerConfig {
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: AuthConfig::default(),
            auth_secret: None,
        };

        assert!(config.init_token_validator().is_err());
    }
}
