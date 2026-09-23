//! Luminair Domain Model
//!
//! Core business logic, entities, value objects, domain services, and repository port traits.
//! Independent of database drivers, HTTP frameworks, or external APIs.

pub mod entities;
pub mod errors;
pub mod ports;
pub mod services;
pub mod types;
pub mod value_objects;

#[cfg(test)]
pub mod test_support;

pub use entities::*;
pub use errors::*;
pub use ports::*;
pub use services::*;
pub use types::*;
pub use value_objects::*;
