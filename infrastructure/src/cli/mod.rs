//! Command-line interface, configuration parsing, and bootstrap orchestration.

pub mod config;
pub mod dry_run;
pub mod migrate;
pub mod runner;

pub use config::{ConfigError, RunMode, ServerConfig, print_help};
pub use dry_run::{DryRunSummary, dry_run, mask_database_url};
pub use migrate::{MigrationSummary, migrate};
pub use runner::{CliError, CliOutcome, run, start_service};

// Backward-compatibility aliases
pub type BootstrapMode = RunMode;
pub type BootstrapOutcome = CliOutcome;
pub type BootstrapError = CliError;
