# Implementation Plan: Application Layer (`application` crate)

Status: **Proposed** (awaiting approval)  
Research: [`docs/research/application-layer-design.md`](../research/application-layer-design.md), [`docs/research/strapi-populate-and-joins.md`](../research/strapi-populate-and-joins.md)  
ADR References: [ADR-001](../adr/ADR-001-hexagonal-architecture.md), [ADR-002](../adr/ADR-002-singleton-enforcement.md), [ADR-003](../adr/ADR-003-bidirectional-relations.md), [ADR-004](../adr/ADR-004-locale-scope.md), [ADR-005](../adr/ADR-005-auth-strategy.md), [ADR-006](../adr/ADR-006-schema-loading.md), [ADR-007](../adr/ADR-007-persistence-model.md)

---

## 1. Overview & Architecture Decisions

The `application` crate sits between the pure `domain` model and external `infrastructure` adapters. It contains:
- Command and query data structures.
- Caller security context (`CallerContext`).
- Application-level error types (`ApplicationError`).
- Use case service traits (`DocumentsService`, `AccessRequestsService`, `SystemConfigService`) and generic implementations.
- Two-phase batch join enrichment (`populate` support without SQL `LEFT JOIN` Cartesian explosion).
- In-memory fake repositories for 100% fast, deterministic in-memory testing.

### Dependency Constraints & Concurrency Model (Hard Rules)
- **Zero Framework Bloat**: `application` depends **only on `domain`** and pure utility crates (`thiserror`, `serde`).
- **No `tokio` in production**: Concurrency and runtime executors are strictly infrastructure concerns.
- **No `async-trait`**: Use native Rust 2024 Return Position Impl Trait in Trait (RPITIT) with explicit `+ Send` bounds:
  ```rust
  fn method(&self, ...) -> impl Future<Output = Result<T, ApplicationError>> + Send;
  ```
  Implementations use standard `async fn` and generic repository type parameters (`impl<R: DocumentInstanceRepository + 'static> DocumentsService for DocumentsServiceImpl<R>`).
- **Sequential Fetch for MVP**: In `find`, execute `find_by_type` and `count` sequentially. No `tokio::try_join!` or concurrent joining needed in the application layer.
- **No Direct DB Calls**: All persistence access occurs via domain port traits (`domain::ports`).

---

## 2. Phase-by-Phase Plan

### Phase 0: Workspace & Dependencies (`application/Cargo.toml`)

Configure `application/Cargo.toml`:
```toml
[package]
name = "application"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
domain.workspace = true
thiserror.workspace = true
serde = { workspace = true, features = ["derive"] }

[dev-dependencies]
tokio = { version = "1.53.1", features = ["rt-multi-thread", "macros"] }
```

---

### Phase 1: Application Errors (`application/src/errors.rs`)

Define `ApplicationError` enum using `thiserror`:

```rust
use domain::errors::DomainError;
use domain::types::permission::Permission;
use domain::value_objects::UserId;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("unauthorized: user '{user_id}' lacks permission for action '{action:?}'")]
    Unauthorized {
        user_id: UserId,
        action: Permission,
    },

    #[error("resource not found: {entity} with id '{id}'")]
    NotFound {
        entity: &'static str,
        id: String,
    },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),
}
```

---

### Phase 2: Caller Context (`application/src/context.rs`)

Defines the verified security context passed from authentication middleware into every use case:

```rust
use domain::entities::role::Role;
use domain::value_objects::UserId;

#[derive(Debug, Clone)]
pub struct CallerContext {
    pub user_id: UserId,
    pub roles: Vec<Role>,
}

impl CallerContext {
    pub fn new(user_id: UserId, roles: Vec<Role>) -> Self {
        Self { user_id, roles }
    }

    pub fn system() -> Self {
        Self {
            user_id: UserId::new("system-internal").expect("valid system user id"),
            roles: Vec::new(),
        }
    }
}
```

---

### Phase 3: Command & Query Data Structures (`application/src/commands/`)

Strongly typed input commands using domain value objects (`AttributeId`, `DocumentTypeId`, `DocumentInstanceId`, etc.):

#### `commands/documents.rs`
```rust
use std::collections::HashMap;
use domain::ports::document_instance_repository::{FieldFilter, Pagination};
use domain::types::content_value::ContentValue;
use domain::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId};

#[derive(Debug, Clone)]
pub struct FindDocumentsCommand {
    pub document_type: DocumentTypeId,
    pub pagination: Pagination,
    pub filters: Vec<FieldFilter>,
    pub populate: Option<Vec<AttributeId>>, // None = unpopulated, Some(vec![...]) = populate listed
}

#[derive(Debug, Clone)]
pub struct FindByIdCommand {
    pub document_type: DocumentTypeId,
    pub document_instance_id: DocumentInstanceId,
    pub populate: Option<Vec<AttributeId>>,
}

#[derive(Debug, Clone)]
pub struct CreateDocumentCommand {
    pub document_type: DocumentTypeId,
    pub fields: HashMap<AttributeId, ContentValue>,
}

#[derive(Debug, Clone)]
pub struct UpdateDocumentCommand {
    pub document_instance_id: DocumentInstanceId,
    pub fields: HashMap<AttributeId, ContentValue>,
}

#[derive(Debug, Clone)]
pub struct DeleteDocumentCommand {
    pub document_instance_id: DocumentInstanceId,
}

#[derive(Debug, Clone)]
pub struct PublishDocumentCommand {
    pub document_instance_id: DocumentInstanceId,
}

#[derive(Debug, Clone)]
pub struct UnpublishDocumentCommand {
    pub document_instance_id: DocumentInstanceId,
}
```

#### `commands/access_requests.rs`
```rust
use domain::value_objects::{AccessRequestId, RoleId, UserId};

#[derive(Debug, Clone)]
pub struct SubmitAccessRequestCommand {
    pub user_id: UserId,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApproveAccessRequestCommand {
    pub request_id: AccessRequestId,
    pub role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone)]
pub struct RejectAccessRequestCommand {
    pub request_id: AccessRequestId,
    pub reason: Option<String>,
}
```

#### `commands/system_config.rs`
```rust
use domain::value_objects::LocaleId;

#[derive(Debug, Clone)]
pub struct UpdateLocalesCommand {
    pub available_locales: Vec<LocaleId>,
    pub default_locale: LocaleId,
}
```

---

### Phase 4: `DocumentsService` Trait & Default Implementation (`application/src/services/documents.rs`)

Generic, native async service with zero macro overhead and monomorphized dispatch:

```rust
use std::future::Future;
use std::sync::Arc;
use domain::entities::document_instance::DocumentInstance;
use domain::entities::snapshot::PublishedSnapshot;
use domain::ports::{DocumentInstanceRepository, SnapshotRepository, SystemConfigRepository};
use crate::commands::documents::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

pub trait DocumentsService: Send + Sync + 'static {
    fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> impl Future<Output = Result<(Vec<DocumentInstance>, u64), ApplicationError>> + Send;

    fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, ApplicationError>> + Send;

    fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> impl Future<Output = Result<PublishedSnapshot, ApplicationError>> + Send;

    fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;
}

pub struct DocumentsServiceImpl<R, S, C> {
    pub instance_repo: Arc<R>,
    pub snapshot_repo: Arc<S>,
    pub config_repo: Arc<C>,
}
```

#### Key Implementation Details:
1. **Sequential Find & Count**:
   ```rust
   let page = self.instance_repo.find_by_type(cmd.document_type, cmd.pagination, cmd.filters.clone()).await?;
   let count = self.instance_repo.count(cmd.document_type, cmd.filters).await?;
   let enriched = self.enrich(cmd.document_type, cmd.populate, page.items).await?;
   Ok((enriched, count))
   ```
2. **Batch Relation Enrichment (`enrich`)**:
   - Collects parent IDs from page instances.
   - If `populate` contains attribute IDs, calls `self.instance_repo.fetch_relations(...)` in a single query.
   - Stitches related instances in-memory using `instance.with_populated_relations(...)`.
3. **Singleton Guard (ADR-002 Option C)**:
   - When creating a single type document, checks `self.instance_repo.exists_for_type(type_id)`.
   - If already present, rejects with `DomainError::SingleTypeAlreadyExists`.
4. **Authorization Enforcement**:
   - Checks `AuthorizationService::can(&caller.user_id, &action, instance, &caller.roles)`.

---

### Phase 5: `AccessRequestsService` Trait & Default Implementation (`application/src/services/access_requests.rs`)

```rust
use std::future::Future;
use std::sync::Arc;
use domain::entities::access_request::AccessRequest;
use domain::entities::role::UserRoleAssignment;
use domain::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::value_objects::AccessRequestId;
use crate::commands::access_requests::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

pub trait AccessRequestsService: Send + Sync + 'static {
    fn submit(
        &self,
        cmd: SubmitAccessRequestCommand,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    fn approve(
        &self,
        caller: &CallerContext,
        cmd: ApproveAccessRequestCommand,
    ) -> impl Future<Output = Result<Vec<UserRoleAssignment>, ApplicationError>> + Send;

    fn reject(
        &self,
        caller: &CallerContext,
        cmd: RejectAccessRequestCommand,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    fn get_by_id(
        &self,
        caller: &CallerContext,
        request_id: AccessRequestId,
    ) -> impl Future<Output = Result<AccessRequest, ApplicationError>> + Send;

    fn list_pending(
        &self,
        caller: &CallerContext,
    ) -> impl Future<Output = Result<Vec<AccessRequest>, ApplicationError>> + Send;
}

pub struct AccessRequestsServiceImpl<A, U, R> {
    pub access_request_repo: Arc<A>,
    pub assignment_repo: Arc<U>,
    pub role_repo: Arc<R>,
}
```

* Coordinates state updates on `AccessRequest` and persistence of `UserRoleAssignment` records.
* Enforces `Permission::ManageUsers` for approval/rejection.

---

### Phase 6: `SystemConfigService` Trait & Default Implementation (`application/src/services/system_config.rs`)

```rust
use std::future::Future;
use std::sync::Arc;
use domain::entities::system_config::SystemConfig;
use domain::ports::SystemConfigRepository;
use crate::commands::system_config::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

pub trait SystemConfigService: Send + Sync + 'static {
    fn get_config(
        &self,
        caller: &CallerContext,
    ) -> impl Future<Output = Result<SystemConfig, ApplicationError>> + Send;

    fn update_locales(
        &self,
        caller: &CallerContext,
        cmd: UpdateLocalesCommand,
    ) -> impl Future<Output = Result<SystemConfig, ApplicationError>> + Send;
}

pub struct SystemConfigServiceImpl<C> {
    pub config_repo: Arc<C>,
}
```

* Enforces `Permission::ManageSchema` for locale updates.
* Enforces invariant that `default_locale` is present in `available_locales`.

---

### Phase 7: In-Memory Fake Repositories & Test Support (`application/src/test_support.rs`)

Thread-safe fake repositories under `#[cfg(test)]` (using `std::sync::RwLock`):
- `FakeDocumentInstanceRepository`: Uses `RwLock<HashMap<DocumentInstanceId, DocumentInstance>>`. Supports `find_by_id`, `find_by_type`, `count`, `fetch_relations`, `save`, `delete`, `exists_for_type`.
- `FakeSnapshotRepository`: Uses `RwLock<Vec<PublishedSnapshot>>`.
- `FakeSystemConfigRepository`: Uses `RwLock<SystemConfig>`.
- `FakeRoleRepository`: Uses `RwLock<HashMap<RoleId, Role>>`.
- `FakeUserRoleAssignmentRepository`: Uses `RwLock<Vec<UserRoleAssignment>>`.
- `FakeAccessRequestRepository`: Uses `RwLock<HashMap<AccessRequestId, AccessRequest>>`.

---

### Phase 8: Crate Root (`application/src/lib.rs`)

Re-exports modules:
```rust
pub mod commands;
pub mod context;
pub mod errors;
pub mod services;

#[cfg(test)]
pub mod test_support;

pub use commands::*;
pub use context::*;
pub use errors::*;
pub use services::*;
```

---

### Phase 9: Comprehensive Test Suite (Inline Unit Tests & Workflows)

Target test coverage:
1. `documents`:
   - `test_find_paginated_with_count`
   - `test_find_with_batch_enrichment` (verifies `populate` attaches related records)
   - `test_find_unpopulated_returns_plain_instances`
   - `test_create_instance_valid`
   - `test_create_instance_single_type_guard` (second create fails)
   - `test_update_instance_bumps_version_and_audit`
   - `test_publish_and_unpublish_lifecycle`
   - `test_owner_can_update_without_rbac`
   - `test_unauthorized_user_denied`
2. `access_requests`:
   - `test_submit_request_creates_pending`
   - `test_approve_request_grants_roles`
   - `test_reject_request_records_reason`
   - `test_approve_already_approved_fails`
   - `test_unauthorized_approval_denied`
3. `system_config`:
   - `test_update_locales_success`
   - `test_update_locales_missing_default_fails`

---

## 3. Verification Gates

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo doc --no-deps -p application
```

