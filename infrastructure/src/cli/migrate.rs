//! CLI action for running static and dynamic database schema migrations.

use sqlx::postgres::PgPoolOptions;

use super::config::ServerConfig;
use super::runner::CliError;
use crate::migrations::run_migrations;
use crate::schema_loader::{SafetyPolicy, sync_schemas};

/// Summary of operations executed during migration CLI mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSummary {
    /// True if static system migrations were executed successfully.
    pub static_migrations_applied: bool,
    /// DDL statements executed during dynamic document schema synchronization.
    pub executed_statements: Vec<String>,
}

impl std::fmt::Display for MigrationSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n=== Luminair Database Migration Complete ===")?;
        writeln!(f, "  Static Migrations:  Applied")?;
        writeln!(
            f,
            "  Dynamic Statements: {}",
            self.executed_statements.len()
        )?;
        for stmt in &self.executed_statements {
            writeln!(f, "    - {stmt}")?;
        }
        writeln!(f, "============================================\n")
    }
}

/// Executes static system migrations and dynamic document schema synchronization without starting HTTP server.
pub async fn migrate(config: &ServerConfig) -> Result<MigrationSummary, CliError> {
    tracing::info!("Connecting to database pool for migrations...");
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

    tracing::info!(
        "Migrations completed: {} dynamic DDL statements executed.",
        sync_result.executed_statements.len()
    );

    Ok(MigrationSummary {
        static_migrations_applied: true,
        executed_statements: sync_result.executed_statements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_summary_display() {
        let summary = MigrationSummary {
            static_migrations_applied: true,
            executed_statements: vec![
                "CREATE TABLE articles (id UUID PRIMARY KEY);".into(),
                "CREATE INDEX idx_articles_status ON articles (status);".into(),
            ],
        };

        let formatted = format!("{summary}");
        assert!(formatted.contains("Luminair Database Migration Complete"));
        assert!(formatted.contains("Static Migrations:  Applied"));
        assert!(formatted.contains("Dynamic Statements: 2"));
        assert!(formatted.contains("CREATE TABLE articles"));
    }
}
