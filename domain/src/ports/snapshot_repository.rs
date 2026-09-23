use async_trait::async_trait;

use crate::entities::published_snapshot::PublishedSnapshot;
use crate::errors::DomainError;
use crate::value_objects::DocumentInstanceId;

#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    async fn find_by_instance(
        &self,
        instance_id: DocumentInstanceId,
    ) -> Result<Vec<PublishedSnapshot>, DomainError>;

    async fn find_by_revision(
        &self,
        instance_id: DocumentInstanceId,
        revision: u32,
    ) -> Result<Option<PublishedSnapshot>, DomainError>;

    async fn save(&self, snapshot: &PublishedSnapshot) -> Result<(), DomainError>;
}
