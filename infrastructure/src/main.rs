//! Luminair service entry point and composition root.

use infrastructure::api::create_router;
use infrastructure::cli::{CliOutcome, ServerConfig, print_help, run};
use infrastructure::container::AppContainer;
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

    let config = ServerConfig::from_args(args)?;

    match run(&config).await? {
        CliOutcome::Service(container) => run_server(&config, container).await?,
        CliOutcome::Migrated(summary) => print!("{summary}"),
        CliOutcome::DryRunValidated(summary) => print!("{summary}"),
    }

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
