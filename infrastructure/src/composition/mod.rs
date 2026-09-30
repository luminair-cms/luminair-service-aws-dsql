//! Composition Root for Luminair Infrastructure.
//!
//! Wires together database pools, repository implementations, application services,
//! and authentication components into an `AppContainer`.

pub mod config;
pub mod container;

pub use config::{BootstrapMode, ConfigError, ServerConfig};
pub use container::{
    AppContainer, AppContainerBuilder, BootstrapError, BootstrapOutcome, ContainerBuildError,
    DryRunSummary, MigrationSummary, mask_database_url,
};
