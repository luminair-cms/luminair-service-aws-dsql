use thiserror::Error;

use crate::value_objects::{
    AccessRequestId, AttributeId, DocumentInstanceId, DocumentTypeId, LocaleId, UserId,
};

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("document type not found: {0}")]
    DocumentTypeNotFound(DocumentTypeId),

    #[error("document instance not found: {0}")]
    DocumentInstanceNotFound(DocumentInstanceId),

    #[error("SingleType already has an instance: {0}")]
    SingleTypeAlreadyExists(DocumentTypeId),

    #[error("invalid field value for attribute '{attribute_id}': {reason}")]
    InvalidFieldValue {
        attribute_id: AttributeId,
        reason: String,
    },

    #[error("unknown locale: {0}")]
    UnknownLocale(LocaleId),

    #[error("unknown attribute: {0}")]
    UnknownAttribute(AttributeId),

    #[error("access request not found: {0}")]
    AccessRequestNotFound(AccessRequestId),

    #[error("access request already exists for user: {0}")]
    AccessRequestAlreadyActive(UserId),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("invalid state transition: {reason}")]
    InvalidStateTransition { reason: String },

    #[error("storage error: {0}")]
    Storage(String),
}
