use std::future::Future;

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
pub type RelationMap = std::collections::HashMap<
    AttributeId,
    std::collections::HashMap<DocumentInstanceId, Vec<DocumentInstance>>,
>;

pub trait DocumentInstanceRepository: Send + Sync {
    /// Loads a single document instance by its type's table and its unique ID.
    ///
    /// `type_id` is required to resolve the per-type table name (ADR-007).
    fn find_by_id(
        &self,
        type_id: DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send;

    fn find_by_type(
        &self,
        type_id: DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<Page<DocumentInstance>, DomainError>> + Send;

    fn count(
        &self,
        type_id: DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<u64, DomainError>> + Send;

    /// Batch-loads relations for a set of parent instance IDs.
    /// Returns a nested map: AttributeId -> (ParentInstanceId -> Vec<RelatedInstance>)
    fn fetch_relations(
        &self,
        type_id: DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<RelationMap, DomainError>> + Send;

    fn save(
        &self,
        instance: &DocumentInstance,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Deletes a document instance from the type's table.
    ///
    /// `type_id` is required to resolve the per-type table name (ADR-007).
    fn delete(
        &self,
        type_id: DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn exists_for_type(
        &self,
        type_id: DocumentTypeId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send;
}
