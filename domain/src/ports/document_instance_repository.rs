use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::entities::document_instance::DocumentInstance;
use crate::errors::DomainError;
use crate::types::domain_value::DomainValue;
use crate::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldFilter {
    pub attribute_id: AttributeId,
    pub value: DomainValue,
}

/// A mapping of relation attribute to a map of parent instance IDs and their related instances:
/// AttributeId -> (ParentInstanceId -> Vec<RelatedInstance>)
pub type RelationMap =
    std::collections::HashMap<AttributeId, std::collections::HashMap<DocumentInstanceId, Vec<DocumentInstance>>>;

#[async_trait]
pub trait DocumentInstanceRepository: Send + Sync {
    async fn find_by_id(
        &self,
        id: DocumentInstanceId,
    ) -> Result<Option<DocumentInstance>, DomainError>;

    async fn find_by_type(
        &self,
        type_id: DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError>;

    async fn count(
        &self,
        type_id: DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> Result<u64, DomainError>;

    /// Batch-loads relations for a set of parent instance IDs.
    /// Returns a nested map: AttributeId -> (ParentInstanceId -> Vec<RelatedInstance>)
    async fn fetch_relations(
        &self,
        type_id: DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> Result<RelationMap, DomainError>;

    async fn save(&self, instance: &DocumentInstance) -> Result<(), DomainError>;

    async fn delete(&self, id: DocumentInstanceId) -> Result<(), DomainError>;

    async fn exists_for_type(&self, type_id: DocumentTypeId) -> Result<bool, DomainError>;
}
