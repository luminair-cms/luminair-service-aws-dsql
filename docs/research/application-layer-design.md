# Research: Application Layer Design & Architecture

- **Date**: 2026-09-23
- **Question**: What is the optimal architecture, pattern, and interface contract for the `application` layer in Luminair, given Hexagonal Architecture boundaries, pure `domain` ports, CQRS principles, and multi-repository atomic operations?
- **Related ADRs**:
  - [ADR-001 — Hexagonal Architecture](../adr/ADR-001-hexagonal-architecture.md)
  - [ADR-002 — Singleton DocumentType Enforcement](../adr/ADR-002-singleton-enforcement.md)
  - [ADR-003 — Relation Representation](../adr/ADR-003-bidirectional-relations.md)
  - [ADR-004 — System-level Locales](../adr/ADR-004-locale-scope.md)
  - [ADR-005 — Authentication & Authorization Strategy](../adr/ADR-005-auth-strategy.md)
  - [ADR-006 — Schema Loading from JSON](../adr/ADR-006-schema-loading.md)
  - [ADR-007 — Persistence Model](../adr/ADR-007-persistence-model.md)

---

## 1. Problem Statement

In Hexagonal Architecture, the `application` crate sits between the pure `domain` model and external `infrastructure` adapters (HTTP controllers, SQLx database drivers, identity providers):

```
infrastructure (Axum, SQLx, OIDC)
       │
       ▼
  application (Use cases, commands, queries, authorization enforcement)
       │
       ▼
    domain (Entities, value objects, domain services, repository ports)
```

The `domain` crate defines pure entities, invariant validations, and async port traits (`DocumentInstanceRepository`, `SnapshotRepository`, etc.). However, it deliberately executes **no I/O** and performs **no multi-repository orchestration**.

The `application` crate must:
1. Orchestrate business operations across domain entities and repositories.
2. Enforce authorization checks on every operation via `AuthorizationService`.
3. Validate document fields and relations against `SchemaRegistry`.
4. Enforce singleton pre-flight guards for `SingleType` documents (ADR-002 Option C).
5. Coordinate multi-repository workflows (e.g. updating an instance and recording a snapshot during publication).
6. Remain completely decoupled from database drivers (`sqlx`), HTTP types (`axum`), and AWS SDKs.
7. Be 100% testable in-memory using fake repositories without requiring a database or external services.

---

## 2. Findings & Architectural Investigations

### Finding 1: Organization Pattern — Handlers (CQRS) vs Cohesive Services

Two primary organizational patterns exist for the application layer in Rust DDD systems:

#### Option A: Command / Query Handler per Use Case (Targeted CQRS)
Every distinct use case is an isolated command/query struct with a dedicated handler:
```rust
pub struct CreateDocumentInstanceCommand {
    pub caller: CallerContext,
    pub document_type_id: DocumentTypeId,
    pub fields: HashMap<AttributeId, ContentValue>,
}

pub struct CreateDocumentInstanceHandler {
    instance_repo: Arc<dyn DocumentInstanceRepository>,
    schema_registry: Arc<SchemaRegistry>,
    system_config_repo: Arc<dyn SystemConfigRepository>,
}

impl CreateDocumentInstanceHandler {
    pub async fn execute(&self, cmd: CreateDocumentInstanceCommand) -> Result<DocumentInstance, ApplicationError> { ... }
}
```

* **Pros**:
  * High cohesion and single responsibility per file.
  * Each handler only depends on the exact repositories and services it needs.
  * Adding new use cases does not touch existing files (open/closed principle).
  * Easy to unit test in isolation by mocking only the needed ports.
* **Cons**:
  * Proliferation of small files (one per use case).
  * Dependency injection setup in infrastructure composition root requires wiring each handler separately.

#### Option B: Domain-Grouped Application Services
Use cases are grouped into cohesive service structs:
- `DocumentService`: handles create, update, publish, unpublish, get, list, delete.
- `AccessRequestService`: handles submit, approve, reject, list.
- `SystemConfigService`: handles get, update locales.

```rust
pub struct DocumentService {
    instance_repo: Arc<dyn DocumentInstanceRepository>,
    snapshot_repo: Arc<dyn SnapshotRepository>,
    schema_registry: Arc<SchemaRegistry>,
    system_config_repo: Arc<dyn SystemConfigRepository>,
}

impl DocumentService {
    pub async fn create_instance(&self, caller: &CallerContext, cmd: CreateDocumentInstance) -> Result<DocumentInstance, ApplicationError> { ... }
    pub async fn publish_instance(&self, caller: &CallerContext, cmd: PublishDocumentInstance) -> Result<PublishedSnapshot, ApplicationError> { ... }
    // ...
}
```

* **Pros**:
  * Simpler wiring in `infrastructure` (only 3 service instances injected into Axum state).
  * Shared helper methods (e.g. resolving type and checking instance ownership) remain internal to the service.
  * Familiar structure matching typical CMS / service architectures.
* **Cons**:
  * Service structs accumulate multiple repository dependencies.

#### Preferred Synthesis: `DocumentsService` Trait & Command Execution
To achieve maximum modularity, mockability, and alignment with the Strapi Document Service pattern, we define an async trait `DocumentsService`:
```rust
#[async_trait]
pub trait DocumentsService: Send + Sync {
    async fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> Result<(Vec<DocumentInstance>, u64), ApplicationError>;

    async fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> Result<Option<DocumentInstance>, ApplicationError>;

    async fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError>;

    async fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError>;

    async fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> Result<(), ApplicationError>;

    async fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> Result<PublishedSnapshot, ApplicationError>;

    async fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError>;
}
```
Implemented by `DefaultDocumentsService`, which encapsulates batch loading, relation enrichment, and singleton validation. See detailed research in [`docs/research/strapi-populate-and-joins.md`](./strapi-populate-and-joins.md).

---

### Finding 2: Security & Caller Context Flow

Per ADR-005:
- Authentication is handled in infrastructure middleware (extracting `sub` as `UserId`).
- Authorization is executed in the application layer using `domain::services::AuthorizationService`.
- Precedence: Owner rule -> RBAC -> Default DENY.

To execute this, every application use case needs access to the caller's identity and assigned roles:

```rust
#[derive(Debug, Clone)]
pub struct CallerContext {
    pub user_id: UserId,
    pub roles: Vec<Role>,
}
```

#### Enforcement Pattern inside Use Cases:
```rust
// 1. For instance-specific actions (e.g. UpdateDocument):
let instance = self.instance_repo.find_by_id(cmd.id).await?
    .ok_or_else(|| ApplicationError::NotFound { entity: "DocumentInstance", id: cmd.id.to_string() })?;

let action = Permission::UpdateDocument(Some(instance.document_type_id));
if !AuthorizationService::can(&caller.user_id, &action, Some(&instance), &caller.roles) {
    return Err(ApplicationError::Unauthorized {
        user_id: caller.user_id.clone(),
        action,
    });
}
```

This ensures zero business logic can bypass security, even if an HTTP endpoint forgot a middleware check.

---

### Finding 3: Multi-Repository Operations & Transaction Boundaries

Two key use cases touch multiple repositories:
1. **`publish_instance`**:
   - Updates `DocumentInstance` in `DocumentInstanceRepository` (sets `publication_state = Published`, increments version).
   - Inserts `PublishedSnapshot` in `SnapshotRepository`.
2. **`approve_access_request`**:
   - Updates `AccessRequest` in `AccessRequestRepository` (sets status = Approved).
   - Inserts new `UserRoleAssignment` records in `UserRoleAssignmentRepository`.

#### Considerations for AWS Aurora DSQL & PostgreSQL:
- Aurora DSQL uses optimistic concurrency control (OCC). Transactions that experience write conflicts fail at commit time with PostgreSQL error code `40001` (serialization failure).
- DSQL supports multi-statement transactions using standard `BEGIN ... COMMIT`.
- How should transactions be abstracted without leaking SQLx into `application`?

#### Approaches to Multi-Repository Atomicity:
- **Approach A: Sequential Application Orchestration with Idempotency**:
  - The application calls `instance_repo.save(&instance)` followed by `snapshot_repo.save(&snapshot)`.
  - If step 2 fails, the document was published but snapshot was missed, or vice versa.
  - *Risk*: Data inconsistency under crash or network partitions.
- **Approach B: UnitOfWork / Transaction Context Port**:
  - A domain port `trait UnitOfWork: Send + Sync` provides transactional scoping.
  - Or a transaction factory creates transactional instances of the repositories.
  - *Complexity*: Higher Rust type complexity with lifetimes / async transactions.
- **Approach C: Cohesive Aggregate / Focused Port Operation**:
  - In `DocumentInstanceRepository`, provide a composite method or atomic adapter where appropriate, or pass an abstract transaction handle.
  - Alternatively, in the persistence layer, implement transactional execution via a repository coordinator or provide a `run_in_transaction` helper.

---

### Finding 4: Input / Output Boundary Types (DTOs vs Domain Types)

#### Commands Input:
- Application commands should use **strongly typed domain value objects** (`DocumentTypeId`, `AttributeId`, `UserId`, `ContentValue`).
- String sanitization and basic slug formatting happen at the boundary via `nutype`.
- HTTP-specific concerns (JSON body parsing, query string parsing, HTTP headers) belong solely to `infrastructure` DTOs with `validator`. The controller maps HTTP DTOs into application command structs.

#### Handlers Output:
- Commands that create or mutate entities should return the **domain entity** or **snapshot** (`DocumentInstance`, `PublishedSnapshot`, `AccessRequest`).
- Queries should return domain entities or pagination wrappers (`Page<DocumentInstance>`).
- The HTTP layer converts domain entities into API response envelopes `{ "data": ... }` per `docs/api.md`.

---

### Finding 5: In-Memory Fake Repositories for 100% In-Memory Testing

Per `.ai/skills/testing.md`, application layer tests must run completely in-memory using fake repositories without requiring a database.

A shared set of fake repositories using thread-safe structures (`Arc<RwLock<HashMap<...>>>`):
1. `FakeDocumentInstanceRepository`: Stores instances by `DocumentInstanceId`, supports indexing by `type_id` and simple filtering.
2. `FakeSnapshotRepository`: Stores snapshots by `instance_id` and `revision`.
3. `FakeSystemConfigRepository`: Stores singleton `SystemConfig`.
4. `FakeRoleRepository`: Stores `Role`s by `RoleId` and `name`.
5. `FakeUserRoleAssignmentRepository`: Stores assignments by `user_id`.
6. `FakeAccessRequestRepository`: Stores access requests by `AccessRequestId`.

With these fakes, application tests can execute complete lifecycles (`create` -> `touch` -> `publish` -> `unpublish` -> `access request approval`) in less than 50 milliseconds.

---

## 3. Inventory of Application Use Cases

### 1. Document Management (`DocumentService`)
| Operation | Input | Business Rules | Output |
|---|---|---|---|
| `CreateDocumentInstance` | `caller, type_id, fields` | Check `CreateDocument` permission; validate against `SchemaRegistry`; if `SingleType`, check `exists_for_type` (ADR-002); validate required fields & locales against `SystemConfig` | `DocumentInstance` |
| `UpdateDocumentInstance` | `caller, id, fields` | Load instance; check `UpdateDocument` permission (or owner rule); validate fields against `SchemaRegistry`; call `touch()`; save | `DocumentInstance` |
| `PublishDocumentInstance` | `caller, id` | Load instance; check `PublishDocument` permission (or owner rule); lookup `type_name`; call `instance.publish(type_name, by, now)`; save instance and snapshot | `PublishedSnapshot` |
| `UnpublishDocumentInstance` | `caller, id` | Load instance; check `PublishDocument` permission; call `instance.unpublish(now)`; save | `DocumentInstance` |
| `GetDocumentInstance` | `caller, id` | Load instance; check `ReadDocument` permission (or owner rule) | `DocumentInstance` |
| `ListDocumentInstances` | `caller, type_id, pagination, filters` | Check `ReadDocument` permission for type; query repository | `Page<DocumentInstance>` |
| `DeleteDocumentInstance` | `caller, id` | Load instance; check `DeleteDocument` permission (or owner rule); delete from repository | `()` |

### 2. Access Request Management (`AccessRequestService`)
| Operation | Input | Business Rules | Output |
|---|---|---|---|
| `SubmitAccessRequest` | `user_id, email, name` | Verify no pending or active request already exists for user; construct `AccessRequest::new`; save | `AccessRequest` |
| `ApproveAccessRequest` | `caller, request_id, role_ids` | Check `ManageUsers` permission; load request; call `request.approve(by, role_ids, now)`; save request and save role assignments | `Vec<UserRoleAssignment>` |
| `RejectAccessRequest` | `caller, request_id, reason` | Check `ManageUsers` permission; load request; call `request.reject(by, reason, now)`; save | `AccessRequest` |
| `GetAccessRequest` | `caller, request_id` | Check `ManageUsers` permission or requester identity | `AccessRequest` |
| `ListPendingAccessRequests` | `caller` | Check `ManageUsers` permission; query pending requests | `Vec<AccessRequest>` |

### 3. System Configuration Management (`SystemConfigService`)
| Operation | Input | Business Rules | Output |
|---|---|---|---|
| `GetSystemConfig` | `caller` | Load system config; check permissions | `SystemConfig` |
| `UpdateLocales` | `caller, available_locales, default_locale` | Check `ManageSchema` permission; ensure `default_locale` is in `available_locales`; save | `SystemConfig` |

---

## 4. Open Questions for Discussion

1. **Service Structs vs Command Handlers**:
   - Do we prefer cohesive services (`DocumentService`, `AccessRequestService`, `SystemConfigService`) or granular handler structs (`CreateDocumentInstanceHandler`, etc.)?
   - *Recommendation*: Domain-grouped services (`DocumentService`, `AccessRequestService`, `SystemConfigService`). In Rust, this minimizes boilerplate in Axum state wiring while keeping use cases clearly grouped and easy to mock.

2. **Transaction Abstraction**:
   - For Phase 2 (Application), should we rely on sequential repository calls with in-memory validation, or introduce a formal `UnitOfWork` / transaction port?
   - *Recommendation*: Start with repository-level coordination in the use case. If transactional rollbacks are required across repositories for PostgreSQL/DSQL, we can introduce a lightweight `TransactionRunner` port in Phase 4 when SQLx is implemented.

3. **In-Memory Fakes Location**:
   - Should `FakeDocumentInstanceRepository`, etc., live in `application/src/test_support.rs` (under `#[cfg(test)]`), or be exported with a `test-support` cargo feature so `infrastructure` integration tests can also use them?
   - *Recommendation*: Put them in `application/src/test_support.rs` under `#[cfg(test)]`. If another crate needs them, enable via an optional feature.

---

## 5. Next Steps

1. Align on the preferred structure (grouped Application Services vs individual Command Handlers).
2. Formulate **Implementation Plan for the Application Layer** covering:
   - `application/Cargo.toml` dependencies (`domain`, `thiserror`, `tokio` for async tests).
   - `CallerContext` & `ApplicationError`.
   - `DocumentService`, `AccessRequestService`, `SystemConfigService`.
   - In-memory fake repositories in `test_support.rs`.
   - Full test coverage for all use cases and permission paths.
