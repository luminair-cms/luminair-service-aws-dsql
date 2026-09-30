//! Luminair service entry point and composition root.

use infrastructure::api::create_router;
use infrastructure::composition::{AppContainer, BootstrapOutcome, ServerConfig};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    init_tracing();

    let config = ServerConfig::from_args_and_env(args)?;

    match AppContainer::bootstrap_with_mode(&config, config.mode).await? {
        BootstrapOutcome::Service(container) => {
            tracing::info!("Starting Luminair HTTP service...");
            run_server(&config, container).await?;
        }
        BootstrapOutcome::Migrated(summary) => {
            tracing::info!(
                "Migration CLI mode complete: static migrations applied, {} dynamic DDL statements executed.",
                summary.executed_statements.len()
            );
            println!("\n=== Luminair Database Migration Complete ===");
            println!("  Static Migrations:  Applied");
            println!(
                "  Dynamic Statements: {}",
                summary.executed_statements.len()
            );
            for stmt in &summary.executed_statements {
                println!("    - {stmt}");
            }
            println!("============================================\n");
        }
        BootstrapOutcome::DryRunValidated(summary) => {
            tracing::info!("Dry-run validation complete: configuration and schemas are valid.");
            println!("\n=== Luminair Dry-Run Configuration & Schema Validation ===");
            println!("  Server Address:        {}", summary.server_addr);
            println!("  Database URL:          {}", summary.database_url_masked);
            println!("  Max DB Connections:    {}", summary.max_db_connections);
            println!("  Schema Directory:      {}", summary.schema_dir.display());
            println!("  Document Types Loaded: {}", summary.document_types_count);
            if !summary.document_type_names.is_empty() {
                println!(
                    "  Document Types:        {}",
                    summary.document_type_names.join(", ")
                );
            }
            println!("  Relations Loaded:      {}", summary.relations_count);
            println!("  Default Locale:        {}", summary.default_locale);
            if !summary.available_locales.is_empty() {
                println!(
                    "  Available Locales:     {}",
                    summary.available_locales.join(", ")
                );
            }
            println!("  Target Schema Tables:  {}", summary.target_tables_count);
            println!("  Auth Mode:             {}", summary.auth_mode);
            println!("==========================================================\n");
        }
    }

    Ok(())
}

/// Prints CLI usage instructions and available environment variables.
fn print_help() {
    println!(
        r#"Luminair CMS Backend Service

Usage:
  luminair [COMMAND]

Commands:
  serve, service    Run standard HTTP service mode (default)
  migrate           Run static migrations and dynamic schema synchronization, then exit
  dry-run, check    Validate configuration, schemas, and auth settings without database connection

Options:
  -h, --help        Show this help message

Environment Variables:
  BOOTSTRAP_MODE            Set mode via environment ('service', 'migrate', 'dry-run')
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

/// Initializes structured logging and tracing using environment filter directives.
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,infrastructure=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Binds TCP listener and serves the Axum HTTP router with graceful shutdown.
async fn run_server(
    config: &ServerConfig,
    container: AppContainer,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(container.to_http_state());
    let addr = config.server_addr();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Luminair backend running on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Listens for OS termination signals (CTRL+C) for graceful connection draining.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, draining active connections...");
}
