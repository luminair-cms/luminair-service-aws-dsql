//! CLI action for offline configuration, auth, and schema validation.

use super::config::{ConfigError, ServerConfig};
use super::runner::CliError;
use crate::schema_loader::{build_desired_schema, load_schema_registry};

/// Helper to mask sensitive password credentials in database connection URLs.
pub fn mask_database_url(url: &str) -> String {
    if let Some(scheme_idx) = url.find("://") {
        let after_scheme = &url[scheme_idx + 3..];
        if let Some(at_idx) = after_scheme.find('@') {
            let user_info = &after_scheme[..at_idx];
            let rest = &after_scheme[at_idx..];
            let masked_user_info = if let Some(colon_idx) = user_info.find(':') {
                format!("{}:****", &user_info[..colon_idx])
            } else {
                "****".to_string()
            };
            return format!("{}{}{}", &url[..scheme_idx + 3], masked_user_info, rest);
        }
    }
    url.to_string()
}

/// Summary of configuration and schema validation executed during dry-run mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunSummary {
    /// Configured host:port server bind address.
    pub server_addr: String,
    /// Masked database connection URL (e.g. postgres://user:****@host:5432/db).
    pub database_url_masked: String,
    /// Maximum database connection pool size.
    pub max_db_connections: u32,
    /// Schema directory path.
    pub schema_dir: std::path::PathBuf,
    /// Number of loaded document types.
    pub document_types_count: usize,
    /// Names of loaded document types.
    pub document_type_names: Vec<String>,
    /// Number of loaded relations.
    pub relations_count: usize,
    /// Default locale identifier.
    pub default_locale: String,
    /// Available locale identifiers.
    pub available_locales: Vec<String>,
    /// Number of target database tables in the desired schema.
    pub target_tables_count: usize,
    /// Human-readable token validation mode (e.g. "HMAC Secret" or "OIDC JWKS").
    pub auth_mode: String,
}

impl std::fmt::Display for DryRunSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "\n=== Luminair Dry-Run Configuration & Schema Validation ==="
        )?;
        writeln!(f, "  Server Address:        {}", self.server_addr)?;
        writeln!(f, "  Database URL:          {}", self.database_url_masked)?;
        writeln!(f, "  Max DB Connections:    {}", self.max_db_connections)?;
        writeln!(f, "  Schema Directory:      {}", self.schema_dir.display())?;
        writeln!(f, "  Document Types Loaded: {}", self.document_types_count)?;
        if !self.document_type_names.is_empty() {
            writeln!(
                f,
                "  Document Types:        {}",
                self.document_type_names.join(", ")
            )?;
        }
        writeln!(f, "  Relations Loaded:      {}", self.relations_count)?;
        writeln!(f, "  Default Locale:        {}", self.default_locale)?;
        if !self.available_locales.is_empty() {
            writeln!(
                f,
                "  Available Locales:     {}",
                self.available_locales.join(", ")
            )?;
        }
        writeln!(f, "  Target Schema Tables:  {}", self.target_tables_count)?;
        writeln!(f, "  Auth Mode:             {}", self.auth_mode)?;
        writeln!(
            f,
            "==========================================================\n"
        )
    }
}

/// Validates environment configuration, authentication, and schemas without database connection.
pub fn dry_run(config: &ServerConfig) -> Result<DryRunSummary, CliError> {
    tracing::info!("Validating server configuration and schemas (dry-run)...");

    // 1. Validate database URL format
    if !config.database_url.starts_with("postgres://")
        && !config.database_url.starts_with("postgresql://")
    {
        return Err(ConfigError::InvalidEnvVar(
            "DATABASE_URL",
            "database URL must start with 'postgres://' or 'postgresql://'".into(),
        )
        .into());
    }

    // 2. Validate token validator initialization
    let _validator = config.init_token_validator()?;
    let auth_mode = if config.auth_secret.is_some() {
        "Secret HMAC token validator (symmetric)".to_string()
    } else if let Some(issuer) = &config.auth.issuer_url {
        format!("OIDC JWKS token validator (issuer: {issuer})")
    } else {
        "Unknown token validator".to_string()
    };

    // 3. Load and validate declarative schemas from disk
    tracing::info!(
        "Loading and testing schema definitions from '{}'...",
        config.schema_dir.display()
    );
    let (registry, system_config) = load_schema_registry(&config.schema_dir)?;

    // 4. Validate desired database schema AST construction
    let desired_schema = build_desired_schema(&registry);

    let document_type_names = registry
        .all_types()
        .map(|t| t.id.as_str().to_string())
        .collect::<Vec<_>>();

    let available_locales = system_config
        .available_locales
        .iter()
        .map(|l| l.as_str().to_string())
        .collect::<Vec<_>>();

    let summary = DryRunSummary {
        server_addr: config.server_addr(),
        database_url_masked: mask_database_url(&config.database_url),
        max_db_connections: config.max_db_connections,
        schema_dir: config.schema_dir.clone(),
        document_types_count: document_type_names.len(),
        document_type_names,
        relations_count: registry.all_relations().count(),
        default_locale: system_config.default_locale.as_str().to_string(),
        available_locales,
        target_tables_count: desired_schema.tables.len(),
        auth_mode,
    };

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config::RunMode;
    use uuid::Uuid;

    #[test]
    fn test_mask_database_url() {
        assert_eq!(
            mask_database_url("postgres://postgres:secret@localhost:5432/test"),
            "postgres://postgres:****@localhost:5432/test"
        );
        assert_eq!(
            mask_database_url("postgres://user@localhost:5432/test"),
            "postgres://****@localhost:5432/test"
        );
        assert_eq!(mask_database_url("sqlite::memory:"), "sqlite::memory:");
    }

    #[test]
    fn test_dry_run_success() {
        let temp_dir =
            std::env::temp_dir().join(format!("luminair_dry_run_test_{}", Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        let relations_dir = temp_dir.join("relations");
        std::fs::create_dir_all(&doc_types_dir).unwrap();
        std::fs::create_dir_all(&relations_dir).unwrap();

        let article_json = r#"{
            "kind": "collection",
            "info": {
                "displayName": "Article",
                "singularName": "article",
                "pluralName": "articles"
            },
            "options": { "draftAndPublish": true },
            "attributes": {
                "title": { "type": "text", "required": true }
            }
        }"#;
        std::fs::write(doc_types_dir.join("article.json"), article_json).unwrap();

        let config = ServerConfig {
            mode: RunMode::DryRun,
            database_url: "postgres://postgres:secret@localhost:5432/test".into(),
            max_db_connections: 10,
            schema_dir: temp_dir.clone(),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("test-jwt-secret-min-32-chars-long".into()),
        };

        let summary = dry_run(&config).expect("dry run should succeed");
        assert_eq!(summary.server_addr, "127.0.0.1:3000");
        assert_eq!(
            summary.database_url_masked,
            "postgres://postgres:****@localhost:5432/test"
        );
        assert_eq!(summary.document_types_count, 1);
        assert_eq!(summary.document_type_names, vec!["article"]);
        assert!(summary.target_tables_count >= 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_dry_run_invalid_database_url() {
        let config = ServerConfig {
            mode: RunMode::DryRun,
            database_url: "mysql://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: std::path::PathBuf::from("schema"),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let err = dry_run(&config).unwrap_err();
        match err {
            CliError::Config(ConfigError::InvalidEnvVar("DATABASE_URL", _)) => {}
            other => panic!("expected InvalidEnvVar for DATABASE_URL, got {:?}", other),
        }
    }

    #[test]
    fn test_dry_run_summary_display() {
        let summary = DryRunSummary {
            server_addr: "0.0.0.0:8080".into(),
            database_url_masked: "postgres://user:****@localhost:5432/db".into(),
            max_db_connections: 20,
            schema_dir: std::path::PathBuf::from("schema"),
            document_types_count: 2,
            document_type_names: vec!["article".into(), "author".into()],
            relations_count: 1,
            default_locale: "en-US".into(),
            available_locales: vec!["en-US".into(), "fr-FR".into()],
            target_tables_count: 4,
            auth_mode: "Secret HMAC token validator (symmetric)".into(),
        };

        let formatted = format!("{summary}");
        assert!(formatted.contains("Dry-Run Configuration & Schema Validation"));
        assert!(formatted.contains("0.0.0.0:8080"));
        assert!(formatted.contains("postgres://user:****@localhost:5432/db"));
        assert!(formatted.contains("Document Types:        article, author"));
        assert!(formatted.contains("Available Locales:     en-US, fr-FR"));
    }
}
