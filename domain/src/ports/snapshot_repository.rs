use std::future::Future;

use crate::entities::published_snapshot::PublishedSnapshot;
use crate::errors::DomainError;
use crate::value_objects::DocumentInstanceId;

pub trait SnapshotRepository: Send + Sync {
    fn find_by_instance(
        &self,
        instance_id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Vec<PublishedSnapshot>, DomainError>> + Send;

    fn find_by_revision(
        &self,
        instance_id: DocumentInstanceId,
        revision: u32,
    ) -> impl Future<Output = Result<Option<PublishedSnapshot>, DomainError>> + Send;

    fn save(
        &self,
        snapshot: &PublishedSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Deletes all snapshots for a given document instance.
    ///
    /// Must be called before deleting the parent instance to enforce
    /// application-level referential integrity (ADR-007).
    fn delete_by_instance(
        &self,
        instance_id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
}
