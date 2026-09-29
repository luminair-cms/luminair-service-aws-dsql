//! Luminair service entry point and composition root.

use infrastructure::api::create_router;
use infrastructure::composition::{AppContainer, ServerConfig};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    tracing::info!("Starting Luminair service...");

    let config = ServerConfig::from_env()?;
    let container = AppContainer::bootstrap(&config).await?;

    run_server(&config, container).await?;
    Ok(())
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
