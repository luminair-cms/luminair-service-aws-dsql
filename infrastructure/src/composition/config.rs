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

/// Execution mode for application bootstrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BootstrapMode {
    /// Full service mode: runs migrations, synchronizes schema, initializes auth, and starts HTTP server.
    #[default]
    Service,
    /// Migration-only CLI mode: connects to the database, executes static migrations and dynamic schema sync, then exits.
    Migrate,
    /// Dry-run CLI mode: loads and validates configuration, schema registry, and auth token validator without touching the database.
    DryRun,
}

impl BootstrapMode {
    /// Parses a mode from command-line arguments.
    ///
    /// Recognizes:
    /// - `"migrate"`, `"--migrate"`, `"-m"`, `"migration"` -> `BootstrapMode::Migrate`
    /// - `"dry-run"`, `"--dry-run"`, `"dry_run"`, `"dryrun"`, `"check"`, `"--check"` -> `BootstrapMode::DryRun`
    /// - `"serve"`, `"--serve"`, `"service"`, `"--service"`, `"server"`, `"--server"` -> `BootstrapMode::Service`
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Option<Self> {
        for arg in args {
            let s = arg.trim();
            if s.eq_ignore_ascii_case("migrate")
                || s.eq_ignore_ascii_case("--migrate")
                || s == "-m"
                || s.eq_ignore_ascii_case("migration")
            {
                return Some(Self::Migrate);
            }
            if s.eq_ignore_ascii_case("dry-run")
                || s.eq_ignore_ascii_case("--dry-run")
                || s.eq_ignore_ascii_case("dry_run")
                || s.eq_ignore_ascii_case("dryrun")
                || s.eq_ignore_ascii_case("check")
                || s.eq_ignore_ascii_case("--check")
            {
                return Some(Self::DryRun);
            }
            if s.eq_ignore_ascii_case("serve")
                || s.eq_ignore_ascii_case("--serve")
                || s.eq_ignore_ascii_case("service")
                || s.eq_ignore_ascii_case("--service")
                || s.eq_ignore_ascii_case("server")
                || s.eq_ignore_ascii_case("--server")
            {
                return Some(Self::Service);
            }
        }
        None
    }

    /// Parses a mode from environment variables `BOOTSTRAP_MODE`, `LUMINAIR_MODE`, or `APP_MODE`.
    pub fn from_env() -> Option<Self> {
        let val = std::env::var("BOOTSTRAP_MODE")
            .or_else(|_| std::env::var("LUMINAIR_MODE"))
            .or_else(|_| std::env::var("APP_MODE"))
            .ok()?;

        let s = val.trim();
        if s.eq_ignore_ascii_case("migrate")
            || s.eq_ignore_ascii_case("--migrate")
            || s == "-m"
            || s.eq_ignore_ascii_case("migration")
        {
            return Some(Self::Migrate);
        }
        if s.eq_ignore_ascii_case("dry-run")
            || s.eq_ignore_ascii_case("--dry-run")
            || s.eq_ignore_ascii_case("dry_run")
            || s.eq_ignore_ascii_case("dryrun")
            || s.eq_ignore_ascii_case("check")
            || s.eq_ignore_ascii_case("--check")
        {
            return Some(Self::DryRun);
        }
        if s.eq_ignore_ascii_case("serve")
            || s.eq_ignore_ascii_case("--serve")
            || s.eq_ignore_ascii_case("service")
            || s.eq_ignore_ascii_case("--service")
            || s.eq_ignore_ascii_case("server")
            || s.eq_ignore_ascii_case("--server")
        {
            return Some(Self::Service);
        }
        None
    }

    /// Resolves the bootstrap mode by checking CLI arguments first, then environment variables,
    /// defaulting to `BootstrapMode::Service`.
    pub fn from_args_or_env(args: impl IntoIterator<Item = String>) -> Self {
        Self::from_args(args)
            .or_else(Self::from_env)
            .unwrap_or_default()
    }
}

/// Server runtime configuration loaded from environment variables and CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub mode: BootstrapMode,
    pub database_url: String,
    pub max_db_connections: u32,
    pub schema_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub auth: AuthConfig,
    pub auth_secret: Option<String>,
}

impl ServerConfig {
    /// Loads server configuration from environment variables, defaulting mode to env or Service.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mode = BootstrapMode::from_env().unwrap_or_default();
        Self::from_env_with_mode(mode)
    }

    /// Loads server configuration checking command-line arguments for mode, falling back to environment.
    pub fn from_args_and_env(args: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let mode = BootstrapMode::from_args_or_env(args);
        Self::from_env_with_mode(mode)
    }

    /// Loads server configuration from environment variables with an explicit mode.
    pub fn from_env_with_mode(mode: BootstrapMode) -> Result<Self, ConfigError> {
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
            mode,
            database_url,
            max_db_connections,
            schema_dir,
            host,
            port,
            auth,
            auth_secret,
        })
    }

    /// Overrides the execution mode for this configuration.
    pub fn with_mode(mut self, mode: BootstrapMode) -> Self {
        self.mode = mode;
        self
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
            mode: BootstrapMode::Service,
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
            mode: BootstrapMode::Service,
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
            mode: BootstrapMode::Service,
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

    #[test]
    fn test_bootstrap_mode_from_args() {
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "migrate".into()]),
            Some(BootstrapMode::Migrate)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "--migrate".into()]),
            Some(BootstrapMode::Migrate)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "dry-run".into()]),
            Some(BootstrapMode::DryRun)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "--dry-run".into()]),
            Some(BootstrapMode::DryRun)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "check".into()]),
            Some(BootstrapMode::DryRun)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "serve".into()]),
            Some(BootstrapMode::Service)
        );
        assert_eq!(
            BootstrapMode::from_args(vec!["app".into(), "unknown".into()]),
            None
        );
    }

    #[test]
    fn test_server_config_with_mode() {
        let config = ServerConfig {
            mode: BootstrapMode::Service,
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let updated = config.with_mode(BootstrapMode::Migrate);
        assert_eq!(updated.mode, BootstrapMode::Migrate);
    }
}
