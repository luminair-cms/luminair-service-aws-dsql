//! Luminair Domain Model
//!
//! Core business logic, entities, value objects, domain services, and repository port traits.
//! Independent of database drivers, HTTP frameworks, or external APIs.

// Canonical subdomains
pub mod auth;
pub mod common;
pub mod content;
pub mod errors;
pub mod schema;
pub mod system;

#[cfg(test)]
pub mod test_support;

// Canonical top-level error export
pub use errors::*;

// Backward-compatible module shims for legacy paths
#[doc(hidden)]
pub mod entities {
    pub use crate::auth::{
        AccessRequest, AccessRequestStatus, Permission, Role, UserRoleAssignment,
    };
    pub use crate::content::{
        AuditTrail, DocumentContent, DocumentInstance, PublicationState, PublishedSnapshot,
        ResolvedRelation,
    };
    pub use crate::schema::{
        DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions, FieldConstraint,
        FieldDefinition, InverseRelationKind, OwnerRelationKind, Relation, RelationInverse,
        RelationView,
    };
    pub use crate::system::SystemConfig;

    pub mod auth {
        pub use crate::auth::{
            AccessRequest, AccessRequestStatus, Permission, Role, UserRoleAssignment,
        };
        pub mod access_request {
            pub use crate::auth::{AccessRequest, AccessRequestStatus};
        }
        pub mod role {
            pub use crate::auth::{Permission, Role};
        }
        pub mod user_role_assignment {
            pub use crate::auth::UserRoleAssignment;
        }
    }

    pub mod access_request {
        pub use crate::auth::{AccessRequest, AccessRequestStatus};
    }
    pub mod role {
        pub use crate::auth::{Permission, Role};
    }
    pub mod user_role_assignment {
        pub use crate::auth::UserRoleAssignment;
    }
    pub mod document_instance {
        pub use crate::content::{
            AuditTrail, DocumentContent, DocumentInstance, PublicationState, ResolvedRelation,
        };
    }
    pub mod published_snapshot {
        pub use crate::content::PublishedSnapshot;
    }
    pub mod document_type {
        pub use crate::schema::{
            DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions,
        };
    }
    pub mod field_definition {
        pub use crate::schema::{FieldConstraint, FieldDefinition};
    }
    pub mod relation {
        pub use crate::schema::{
            InverseRelationKind, OwnerRelationKind, Relation, RelationInverse, RelationView,
        };
    }
    pub mod system_config {
        pub use crate::system::SystemConfig;
    }
}

#[doc(hidden)]
pub mod ports {
    pub use crate::auth::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
    pub use crate::content::{
        DocumentInstanceRepository, FieldFilter, Page, Pagination, RelationMap, SnapshotRepository,
    };
    pub use crate::system::SystemConfigRepository;

    pub mod auth_repositories {
        pub use crate::auth::{
            AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository,
        };
    }
    pub mod access_request_repository {
        pub use crate::auth::AccessRequestRepository;
    }
    pub mod role_repository {
        pub use crate::auth::RoleRepository;
    }
    pub mod user_role_assignment_repository {
        pub use crate::auth::UserRoleAssignmentRepository;
    }
    pub mod document_instance_repository {
        pub use crate::content::{
            DocumentInstanceRepository, FieldFilter, Page, Pagination, RelationMap,
        };
    }
    pub mod snapshot_repository {
        pub use crate::content::SnapshotRepository;
    }
    pub mod system_config_repository {
        pub use crate::system::SystemConfigRepository;
    }
}

#[doc(hidden)]
pub mod services {
    pub use crate::auth::AuthorizationService;
    pub use crate::schema::SchemaRegistry;

    pub mod authorization {
        pub use crate::auth::AuthorizationService;
    }
    pub mod schema_registry {
        pub use crate::schema::SchemaRegistry;
    }
}

#[doc(hidden)]
pub mod types {
    pub use crate::content::{ContentValue, DomainValue, PrimitiveValue};
    pub use crate::schema::{FieldType, IntegerSize, PrimitiveType};

    pub mod content_value {
        pub use crate::content::ContentValue;
    }
    pub mod domain_value {
        pub use crate::content::DomainValue;
    }
    pub mod primitive_value {
        pub use crate::content::PrimitiveValue;
    }
    pub mod field_type {
        pub use crate::schema::{FieldType, IntegerSize, PrimitiveType};
    }
}

#[doc(hidden)]
pub mod value_objects {
    pub use crate::auth::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};
    pub use crate::common::{Email, Url};
    pub use crate::content::{DocumentInstanceId, SnapshotId};
    pub use crate::schema::{AttributeId, DocumentTypeId, RelationId};
    pub use crate::system::{LocaleId, SystemConfigId};

    pub mod ids {
        pub use crate::auth::{AccessRequestId, RoleId, UserId, UserRoleAssignmentId};
        pub use crate::content::{DocumentInstanceId, SnapshotId};
        pub use crate::schema::{AttributeId, DocumentTypeId, RelationId};
        pub use crate::system::{LocaleId, SystemConfigId};
    }
    pub mod email {
        pub use crate::common::Email;
    }
    pub mod url {
        pub use crate::common::Url;
    }
    pub mod user_id {
        pub use crate::auth::UserId;
    }
    pub mod locale_id {
        pub use crate::system::LocaleId;
    }
}

// Legacy top-level re-exports for flat access
pub use entities::*;
pub use ports::*;
pub use services::*;
pub use types::*;
pub use value_objects::*;
