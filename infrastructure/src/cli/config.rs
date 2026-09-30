//! Server runtime configuration and CLI mode parsing.

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

/// Execution mode for application CLI and service bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    /// Full service mode: runs migrations, synchronizes schema, initializes auth, and starts HTTP server.
    #[default]
    Service,
    /// Migration-only CLI mode: connects to the database, executes static migrations and dynamic schema sync, then exits.
    Migrate,
    /// Dry-run CLI mode: loads and validates configuration, schema registry, and auth token validator without touching the database.
    DryRun,
}

impl std::str::FromStr for RunMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_lowercase().replace(['-', '_'], "");
        match normalized.as_str() {
            "migrate" | "m" => Ok(Self::Migrate),
            "dryrun" => Ok(Self::DryRun),
            "serve" | "service" => Ok(Self::Service),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service => write!(f, "service"),
            Self::Migrate => write!(f, "migrate"),
            Self::DryRun => write!(f, "dry-run"),
        }
    }
}

impl RunMode {
    /// Parses a mode from command-line arguments.
    pub fn from_args<I, S>(args: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        args.into_iter().find_map(|arg| arg.as_ref().parse().ok())
    }

    /// Parses a mode from the environment variable `RUN_MODE`.
    pub fn from_env() -> Option<Self> {
        std::env::var("RUN_MODE")
            .ok()
            .and_then(|val| val.parse().ok())
    }

    /// Resolves the run mode by checking CLI arguments first, then environment variable `RUN_MODE`,
    /// defaulting to `RunMode::Service`.
    pub fn from_args_or_env<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_args(args)
            .or_else(Self::from_env)
            .unwrap_or_default()
    }
}

/// Server runtime configuration loaded from environment variables and CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub mode: RunMode,
    pub database_url: String,
    pub max_db_connections: u32,
    pub schema_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub auth: AuthConfig,
    pub auth_secret: Option<String>,
}

impl ServerConfig {
    /// Loads server configuration checking command-line arguments for mode, falling back to environment.
    pub fn from_args<I, S>(args: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mode = RunMode::from_args_or_env(args);
        Self::load(mode)
    }

    /// Loads server configuration from environment variables, defaulting mode to env or Service.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mode = RunMode::from_env().unwrap_or_default();
        Self::load(mode)
    }

    fn load(mode: RunMode) -> Result<Self, ConfigError> {
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
    pub fn with_mode(mut self, mode: RunMode) -> Self {
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

/// Prints CLI usage instructions and available environment variables.
pub fn print_help() {
    println!(
        r#"Luminair CMS Backend Service

Usage:
  luminair [COMMAND]

Commands:
  serve, service    Run standard HTTP service mode (default)
  migrate           Run static migrations and dynamic schema synchronization, then exit
  dry-run           Validate configuration, schemas, and auth settings without database connection

Options:
  -h, --help        Show this help message

Environment Variables:
  RUN_MODE                  Set mode via environment ('service', 'migrate', 'dry-run')
  DATABASE_URL              PostgreSQL or AWS Aurora DSQL connection URL
  DATABASE_MAX_CONNECTIONS  Maximum database connection pool size (default 20)
  SCHEMA_DIR                Schema definitions directory (default 'schema')
  HOST                      Server host address (default '0.0.0.0')
  PORT                      Server TCP port (default 8080)
  AUTH_SECRET               HMAC secret key for symmetric JWT validation
  AUTH_ISSUER_URL           OIDC issuer URL for JWKS asymmetric JWT validation
  AUTH_AUDIENCE             Expected JWT audience (aud claim)
  BOOTSTRAP_ADMIN_SUB       OIDC sub claim for bootstrap administrator
  BOOTSTRAP_AUTH_TYPE       Auth provider descriptor (default 'oidc')
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr_formatting() {
        let config = ServerConfig {
            mode: RunMode::Service,
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
            mode: RunMode::Service,
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
            mode: RunMode::Service,
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
    fn test_run_mode_from_args() {
        assert_eq!(
            RunMode::from_args(["app", "migrate"]),
            Some(RunMode::Migrate)
        );
        assert_eq!(
            RunMode::from_args(["app", "--migrate"]),
            Some(RunMode::Migrate)
        );
        assert_eq!(RunMode::from_args(["app", "-m"]), Some(RunMode::Migrate));
        assert_eq!(
            RunMode::from_args(["app", "dry-run"]),
            Some(RunMode::DryRun)
        );
        assert_eq!(
            RunMode::from_args(["app", "--dry-run"]),
            Some(RunMode::DryRun)
        );
        assert_eq!(
            RunMode::from_args(["app", "dry_run"]),
            Some(RunMode::DryRun)
        );
        assert_eq!(RunMode::from_args(["app", "dryrun"]), Some(RunMode::DryRun));
        assert_eq!(RunMode::from_args(["app", "serve"]), Some(RunMode::Service));
        assert_eq!(
            RunMode::from_args(["app", "--service"]),
            Some(RunMode::Service)
        );
        assert_eq!(RunMode::from_args(["app", "check"]), None);
        assert_eq!(RunMode::from_args(["app", "unknown"]), None);
    }

    #[test]
    fn test_run_mode_from_str() {
        assert_eq!("migrate".parse(), Ok(RunMode::Migrate));
        assert_eq!("--migrate".parse(), Ok(RunMode::Migrate));
        assert_eq!("-m".parse(), Ok(RunMode::Migrate));
        assert_eq!("m".parse(), Ok(RunMode::Migrate));
        assert_eq!("MIGRATE".parse(), Ok(RunMode::Migrate));

        assert_eq!("dry-run".parse(), Ok(RunMode::DryRun));
        assert_eq!("--dry-run".parse(), Ok(RunMode::DryRun));
        assert_eq!("dry_run".parse(), Ok(RunMode::DryRun));
        assert_eq!("DRY_RUN".parse(), Ok(RunMode::DryRun));
        assert_eq!("dryrun".parse(), Ok(RunMode::DryRun));

        assert_eq!("serve".parse(), Ok(RunMode::Service));
        assert_eq!("--serve".parse(), Ok(RunMode::Service));
        assert_eq!("service".parse(), Ok(RunMode::Service));
        assert_eq!("--service".parse(), Ok(RunMode::Service));

        assert!("check".parse::<RunMode>().is_err());
        assert!("migration".parse::<RunMode>().is_err());
        assert!("server".parse::<RunMode>().is_err());
        assert!("unknown".parse::<RunMode>().is_err());
        assert!("".parse::<RunMode>().is_err());
        assert!("--".parse::<RunMode>().is_err());
        assert!(
            "--some-very-long-unknown-argument"
                .parse::<RunMode>()
                .is_err()
        );
    }

    #[test]
    fn test_run_mode_display() {
        assert_eq!(RunMode::Service.to_string(), "service");
        assert_eq!(RunMode::Migrate.to_string(), "migrate");
        assert_eq!(RunMode::DryRun.to_string(), "dry-run");
    }

    #[test]
    fn test_run_mode_from_args_borrowed() {
        assert_eq!(
            RunMode::from_args(["app", "migrate"]),
            Some(RunMode::Migrate)
        );
        assert_eq!(
            RunMode::from_args(["app", "--dry_run"]),
            Some(RunMode::DryRun)
        );
    }

    #[test]
    fn test_server_config_with_mode() {
        let config = ServerConfig {
            mode: RunMode::Service,
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let updated = config.with_mode(RunMode::Migrate);
        assert_eq!(updated.mode, RunMode::Migrate);
    }
}
