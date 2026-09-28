//! Luminair Domain Model
//!
//! Core business logic, entities, value objects, domain services, and repository port traits.
//! Independent of database drivers, HTTP frameworks, or external APIs.

pub mod auth;
pub mod common;
pub mod content;
pub mod errors;
pub mod schema;
pub mod system;

#[cfg(test)]
pub mod test_support;

// Canonical top-level error export
pub use errors::DomainError;
