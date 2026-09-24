//! Luminair Application crate.

pub mod commands;
pub mod context;
pub mod errors;
pub mod services;

/// Fake in-memory repository implementations for use in tests.
/// Only compiled when the `test-support` feature is enabled, or during `cargo test`.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use commands::{
    ApproveAccessRequestCommand, CreateDocumentCommand, DeleteDocumentCommand, FindByIdCommand,
    FindDocumentsCommand, ListSnapshotsCommand, PublishDocumentCommand,
    RejectAccessRequestCommand, SubmitAccessRequestCommand, UnpublishDocumentCommand,
};
pub use context::*;
pub use errors::*;
pub use services::access_requests::{AccessRequestsService, AccessRequestsServiceImpl};
pub use services::documents::{DocumentsService, DocumentsServiceImpl};
pub use services::system_config::{SystemConfigService, SystemConfigServiceImpl};
