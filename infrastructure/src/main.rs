//! Luminair service entry point and composition root.

use std::path::Path;
use std::sync::Arc;

use infrastructure::api::create_router;
use infrastructure::auth::{
    AuthConfig, JwksTokenValidator, SecretTokenValidator, TokenValidator, run_bootstrap,
};
use infrastructure::composition::AppContainer;
use infrastructure::migrations::run_migrations;
use infrastructure::schema_loader::{SafetyPolicy, sync_schemas};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize observability / tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,infrastructure=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Luminair service...");

    // 2. Load configuration from environment
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable must be set")?;
    let schema_dir = std::env::var("SCHEMA_DIR").unwrap_or_else(|_| "schema".into());
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .map_err(|e| format!("Invalid PORT environment variable: {e}"))?;
    let auth_config = AuthConfig::from_env();

    // 3. Connect to database
    tracing::info!("Connecting to database pool...");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {e}"))?;

    // 4. Run static migrations (system tables, roles, shadow users)
    tracing::info!("Running static system migrations...");
    run_migrations(&pool)
        .await
        .map_err(|e| format!("Failed to run static migrations: {e}"))?;

    // 5. Dynamic schema synchronization
    tracing::info!("Synchronizing document schemas from '{schema_dir}'...");
    let schema_path = Path::new(&schema_dir);
    let sync_result = sync_schemas(&pool, schema_path, SafetyPolicy::AdditiveOnly)
        .await
        .map_err(|e| format!("Failed to synchronize schemas: {e}"))?;

    // 6. Initialize token validator
    let auth_secret = std::env::var("AUTH_SECRET").ok();
    let validator: Arc<dyn TokenValidator> = if let Some(secret) = auth_secret {
        let mut v = SecretTokenValidator::new(secret.as_bytes());
        if let Some(aud) = &auth_config.audience {
            v = v.with_audience(aud);
        }
        if let Some(iss) = &auth_config.issuer_url {
            v = v.with_issuer(iss);
        }
        Arc::new(v)
    } else if let Some(issuer) = &auth_config.issuer_url {
        let mut v = JwksTokenValidator::new().with_issuer(issuer);
        if let Some(aud) = &auth_config.audience {
            v = v.with_audience(aud);
        }
        Arc::new(v)
    } else {
        return Err("Either AUTH_SECRET or AUTH_ISSUER_URL must be set in environment".into());
    };

    // 7. Assemble composition root container
    let container = AppContainer::new(
        pool.clone(),
        validator,
        Arc::new(sync_result.registry),
        Arc::new(sync_result.system_config),
    );

    // 8. Execute administrator bootstrap hook
    tracing::info!("Checking administrator bootstrap hook...");
    run_bootstrap(
        &pool,
        &auth_config,
        container.assignment_repo.as_ref(),
        container.access_request_repo.as_ref(),
    )
    .await
    .map_err(|e| format!("Bootstrap error: {e}"))?;

    // 9. Build Axum HTTP router
    let app = create_router(container.to_http_state());

    // 10. Bind TCP listener and serve
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {addr}: {e}"))?;
    tracing::info!("Luminair backend running on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, draining active connections...");
}
