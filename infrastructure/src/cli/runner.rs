//! CLI runner orchestration for Service, Migrate, and DryRun modes.

use domain::system::SystemContext;
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

use super::config::{ConfigError, RunMode, ServerConfig};
use super::dry_run::{DryRunSummary, dry_run};
use super::migrate::{MigrationSummary, migrate};
use crate::auth::{AuthError, run_bootstrap};
use crate::container::AppContainer;
use crate::migrations::run_migrations;
use crate::schema_loader::{SafetyPolicy, SchemaLoaderError, SchemaSyncError, sync_schemas};

/// Errors encountered during application CLI execution or service bootstrapping.
#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("Database connection error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Dynamic schema sync error: {0}")]
    SchemaSync(#[from] SchemaSyncError),

    #[error("Schema loader error: {0}")]
    SchemaLoader(#[from] SchemaLoaderError),

    #[error("Administrator bootstrap error: {0}")]
    AuthBootstrap(#[from] AuthError),
}

/// The result outcome of executing the application CLI.
pub enum CliOutcome {
    /// Full service mode: application container initialized and ready to serve HTTP requests.
    Service(AppContainer),
    /// Migration-only CLI mode: database migrations and schema sync finished successfully.
    Migrated(MigrationSummary),
    /// Dry-run CLI mode: configuration and schemas validated successfully.
    DryRunValidated(DryRunSummary),
}

impl std::fmt::Debug for CliOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service(_) => f.debug_tuple("Service").finish(),
            Self::Migrated(m) => f.debug_tuple("Migrated").field(m).finish(),
            Self::DryRunValidated(d) => f.debug_tuple("DryRunValidated").field(d).finish(),
        }
    }
}

/// Initializes the complete application infrastructure in Service mode:
/// 1. Connects to the database pool
/// 2. Runs static database migrations (MIGRATOR)
/// 3. Synchronizes declarative JSON document schemas (sync_schemas)
/// 4. Initializes the token validator (JWKS or secret)
/// 5. Assembles AppContainer
/// 6. Executes the administrator bootstrap hook (run_bootstrap)
pub async fn start_service(config: &ServerConfig) -> Result<AppContainer, CliError> {
    tracing::info!("Connecting to database pool...");
    let pool = PgPoolOptions::new()
        .max_connections(config.max_db_connections)
        .connect(&config.database_url)
        .await?;

    tracing::info!("Running static system migrations...");
    run_migrations(&pool).await?;

    tracing::info!(
        "Synchronizing document schemas from '{}'...",
        config.schema_dir.display()
    );
    let sync_result = sync_schemas(&pool, &config.schema_dir, SafetyPolicy::AdditiveOnly).await?;

    let validator = config.init_token_validator()?;

    let system_context: &'static SystemContext = Box::leak(Box::new(SystemContext {
        schema: sync_result.registry,
        config: sync_result.system_config,
    }));

    let container = AppContainer::new(pool.clone(), validator, system_context);

    tracing::info!("Checking administrator bootstrap hook...");
    run_bootstrap(
        &pool,
        &config.auth,
        &container.persistence.assignment_repo,
        &container.persistence.access_request_repo,
    )
    .await?;

    Ok(container)
}

/// Dispatches execution based on `config.mode`:
/// - `RunMode::Service`: executes static migrations, dynamic schema sync, admin bootstrap hook, and returns the assembled container.
/// - `RunMode::Migrate`: executes static migrations and dynamic schema sync, then returns migration summary.
/// - `RunMode::DryRun`: validates configuration, auth, and schema definitions without connecting to the database.
pub async fn run(config: &ServerConfig) -> Result<CliOutcome, CliError> {
    match config.mode {
        RunMode::Service => {
            let container = start_service(config).await?;
            Ok(CliOutcome::Service(container))
        }
        RunMode::Migrate => {
            let summary = migrate(config).await?;
            Ok(CliOutcome::Migrated(summary))
        }
        RunMode::DryRun => {
            let summary = dry_run(config)?;
            Ok(CliOutcome::DryRunValidated(summary))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_run_dry_run() {
        let temp_dir =
            std::env::temp_dir().join(format!("luminair_run_dry_run_test_{}", Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        let relations_dir = temp_dir.join("relations");
        std::fs::create_dir_all(&doc_types_dir).unwrap();
        std::fs::create_dir_all(&relations_dir).unwrap();

        let config = ServerConfig {
            mode: RunMode::DryRun,
            database_url: "postgres://localhost/test".into(),
            max_db_connections: 10,
            schema_dir: temp_dir.clone(),
            host: "127.0.0.1".into(),
            port: 3000,
            auth: crate::auth::AuthConfig::default(),
            auth_secret: Some("secret123".into()),
        };

        let outcome = run(&config).await.expect("dry run outcome");

        match outcome {
            CliOutcome::DryRunValidated(summary) => {
                assert_eq!(summary.document_types_count, 0);
            }
            _ => panic!("expected DryRunValidated"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
