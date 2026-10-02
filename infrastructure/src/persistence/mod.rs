//! Database persistence adapters, schema migrations, and query generation.

pub mod container;
pub mod migration;
pub mod naming;
pub mod query;
pub mod repositories;

pub use container::PersistenceContainer;
pub use migration::{run_migrations, run_static_migrations, sync_dynamic_schemas};
pub use repositories::{
    ShadowUser, SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxRoleRepository,
    SqlxShadowUserRepository, SqlxUserRoleAssignmentRepository,
};

