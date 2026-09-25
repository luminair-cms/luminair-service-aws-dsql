//! SQLx PostgreSQL and AWS Aurora DSQL repository implementations.

pub mod access_request_repository;
pub mod document_instance_repository;
pub mod role_repository;
pub mod snapshot_repository;
pub mod user_role_assignment_repository;

pub use access_request_repository::SqlxAccessRequestRepository;
pub use document_instance_repository::SqlxDocumentInstanceRepository;
pub use role_repository::SqlxRoleRepository;
pub use snapshot_repository::SqlxSnapshotRepository;
pub use user_role_assignment_repository::SqlxUserRoleAssignmentRepository;
