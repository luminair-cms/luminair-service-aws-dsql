//! Composition Root for Luminair Infrastructure.
//!
//! Wires together database pools, repository implementations, application services,
//! and authentication components into an `AppContainer`.

pub mod container;

pub use container::AppContainer;
