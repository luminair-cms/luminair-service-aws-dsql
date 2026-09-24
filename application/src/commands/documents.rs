//! Commands for document instance operations.

use std::collections::HashMap;

use domain::ports::document_instance_repository::{FieldFilter, Pagination};
use domain::types::content_value::ContentValue;
use domain::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId};

/// Query command to find documents matching criteria with pagination and optional relation enrichment.
#[derive(Debug, Clone, PartialEq)]
pub struct FindDocumentsCommand {
    /// Target document type to query.
    pub document_type: DocumentTypeId,
    /// Pagination parameters (page number and page size).
    pub pagination: Pagination,
    /// Field filters to apply.
    pub filters: Vec<FieldFilter>,
    /// Optional list of relation attribute IDs to populate in the returned documents.
    pub populate: Option<Vec<AttributeId>>,
}

impl FindDocumentsCommand {
    pub fn new(document_type: DocumentTypeId, pagination: Pagination) -> Self {
        Self {
            document_type,
            pagination,
            filters: Vec::new(),
            populate: None,
        }
    }

    pub fn with_filters(mut self, filters: Vec<FieldFilter>) -> Self {
        self.filters = filters;
        self
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Query command to find a single document instance by identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindByIdCommand {
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
    /// Unique identifier of the document instance.
    pub document_instance_id: DocumentInstanceId,
    /// Optional list of relation attribute IDs to populate in the returned document.
    pub populate: Option<Vec<AttributeId>>,
}

impl FindByIdCommand {
    pub fn new(document_type: DocumentTypeId, document_instance_id: DocumentInstanceId) -> Self {
        Self {
            document_type,
            document_instance_id,
            populate: None,
        }
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Command to create a new draft document instance.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateDocumentCommand {
    /// Target document type to create an instance for.
    pub document_type: DocumentTypeId,
    /// Initial field values keyed by attribute ID.
    pub fields: HashMap<AttributeId, ContentValue>,
}

impl CreateDocumentCommand {
    pub fn new(
        document_type: DocumentTypeId,
        fields: HashMap<AttributeId, ContentValue>,
    ) -> Self {
        Self {
            document_type,
            fields,
        }
    }
}

/// Command to update field values of an existing document instance.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateDocumentCommand {
    /// Identifier of the document instance to update.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
    /// Updated field values keyed by attribute ID.
    pub fields: HashMap<AttributeId, ContentValue>,
}

impl UpdateDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: DocumentTypeId,
        fields: HashMap<AttributeId, ContentValue>,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
            fields,
        }
    }
}

/// Command to delete a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteDocumentCommand {
    /// Identifier of the document instance to delete.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
}

impl DeleteDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
        }
    }
}

/// Command to publish a draft document instance into a published snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishDocumentCommand {
    /// Identifier of the document instance to publish.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
}

impl PublishDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
        }
    }
}

/// Command to unpublish a published document instance back to draft status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpublishDocumentCommand {
    /// Identifier of the document instance to unpublish.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
}

impl UnpublishDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
        }
    }
}

/// Query command to list all published snapshots (revision history) for a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSnapshotsCommand {
    /// Document type of the target document.
    pub document_type: DocumentTypeId,
    /// Identifier of the document instance whose snapshots to retrieve.
    pub document_instance_id: DocumentInstanceId,
}

impl ListSnapshotsCommand {
    pub fn new(
        document_type: DocumentTypeId,
        document_instance_id: DocumentInstanceId,
    ) -> Self {
        Self {
            document_type,
            document_instance_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_find_documents_command_builder() {
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let attr = AttributeId::try_new("author").unwrap();
        let cmd = FindDocumentsCommand::new(type_id, Pagination::default())
            .with_populate(vec![attr.clone()]);

        assert_eq!(cmd.document_type, type_id);
        assert_eq!(cmd.pagination, Pagination::default());
        assert_eq!(cmd.populate, Some(vec![attr]));
        assert!(cmd.filters.is_empty());
    }

    #[test]
    fn test_find_by_id_command_builder() {
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let id = DocumentInstanceId::new(Uuid::now_v7());
        let attr = AttributeId::try_new("category").unwrap();

        let cmd = FindByIdCommand::new(type_id, id).with_populate(vec![attr.clone()]);
        assert_eq!(cmd.document_type, type_id);
        assert_eq!(cmd.document_instance_id, id);
        assert_eq!(cmd.populate, Some(vec![attr]));
    }

    #[test]
    fn test_document_lifecycle_commands() {
        let type_id = DocumentTypeId::new(Uuid::now_v7());
        let id = DocumentInstanceId::new(Uuid::now_v7());

        let create = CreateDocumentCommand::new(type_id, HashMap::new());
        assert_eq!(create.document_type, type_id);
        assert!(create.fields.is_empty());

        let update = UpdateDocumentCommand::new(id, type_id, HashMap::new());
        assert_eq!(update.document_instance_id, id);
        assert_eq!(update.document_type, type_id);

        let delete = DeleteDocumentCommand::new(id, type_id);
        assert_eq!(delete.document_instance_id, id);
        assert_eq!(delete.document_type, type_id);

        let pub_cmd = PublishDocumentCommand::new(id, type_id);
        assert_eq!(pub_cmd.document_instance_id, id);

        let unpub_cmd = UnpublishDocumentCommand::new(id, type_id);
        assert_eq!(unpub_cmd.document_instance_id, id);
    }
}
