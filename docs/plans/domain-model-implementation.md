# Implementation Plan: Domain Model (`domain` crate)

> [!NOTE]
> **Status**: Approved, implementation deferred.
> Created: 2026-09-22. Activate this plan when ready to begin coding.
> All decisions are final — no open questions remain before implementation can start.

Bootstraps the Cargo workspace and implements the entire `domain` crate.
`application` and `infrastructure` crates are out of scope for this plan.

> [!IMPORTANT]
> All ADRs are accepted. This plan follows the settled decisions exactly.
> Key references:
> - [ADR-002](../adr/ADR-002-singleton-enforcement.md) — Option C: singleton enforcement
> - [ADR-003](../adr/ADR-003-bidirectional-relations.md) — Option F: unified `Relation` entity
> - [ADR-004](../adr/ADR-004-locale-scope.md) — Option B: system-level locales
> - [ADR-005](../adr/ADR-005-auth-strategy.md) — OIDC auth + config bootstrap
> - [ADR-006](../adr/ADR-006-schema-loading.md) — JSON schema, `SchemaRegistry`
> - [ADR-007](../adr/ADR-007-persistence-model.md) — per-type tables, two-table publication

---

## Proposed Changes

### Phase 0 — Cargo Workspace

The workspace does not exist yet. Must be created first; all subsequent phases depend on it.

#### [NEW] [`Cargo.toml`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/Cargo.toml)
Root workspace manifest. Members: `domain`, `application`, `infrastructure`.
Shared dependency versions pinned in `[workspace.dependencies]`.

#### [NEW] [`domain/Cargo.toml`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/Cargo.toml)
Dependencies (all from workspace):
- `nutype` (features: `serde`) — value objects
- `serde` (features: `derive`)
- `uuid` (features: `v7`, `serde`)
- `chrono` (features: `serde`)
- `thiserror`
- `indexmap` (features: `serde`) — ordered field maps in `DocumentType`
- `async-trait` — repository trait definitions
- `rust_decimal` (features: `serde`) — `Decimal` for `FieldType::Decimal`

#### [NEW] [`application/Cargo.toml`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/application/Cargo.toml)
Stub only — `[dependencies] domain = { path = "../domain" }`.

#### [NEW] [`infrastructure/Cargo.toml`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/infrastructure/Cargo.toml)
Stub only — `[dependencies] application = { path = "../application" }`.

---

### Phase 1 — Value Objects (`domain/src/value_objects/`)

All types use `nutype`. No internal dependencies. Must be implemented before any entity.

#### [NEW] [`value_objects/ids.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/value_objects/ids.rs)
All UUID-based ID newtypes. One `nutype` per ID:

| Type | Inner type | Notes |
|---|---|---|
| `DocumentTypeId` | `Uuid` | Schema-level stable UUID (v4, hand-assigned from JSON) |
| `DocumentInstanceId` | `Uuid` | Runtime UUID v7 |
| `AttributeId` | `String` | Validated slug: `[a-z][a-z0-9_]*`, max 64 chars |
| `RelationId` | `Uuid` | Schema-level stable UUID (v4) |
| `SnapshotId` | `Uuid` | Runtime UUID v7 |
| `RoleId` | `Uuid` | UUID v7 |
| `UserRoleAssignmentId` | `Uuid` | UUID v7 |
| `AccessRequestId` | `Uuid` | UUID v7 |
| `SystemConfigId` | `Uuid` | UUID v7 |

All **UUID-based ID types** must derive `Display` (nutype forwards to `Uuid`'s `Display`) because
they are used in `DomainError` `#[error("…{0}…")]` messages.

> [!NOTE]
> `AttributeId` slug validation: format only (`[a-z][a-z0-9_]*`, max 64).
> Reserved SQL column name checking (`id`, `created_at`, `_singleton`, etc.) is an
> **infrastructure concern** — enforced by `SchemaLoader` at startup, not in this newtype.

#### [NEW] [`value_objects/user_id.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/value_objects/user_id.rs)
```rust
#[nutype(sanitize(trim), validate(not_empty, max_len = 255))]
pub struct UserId(String);  // OIDC sub claim
```

#### [NEW] [`value_objects/locale_id.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/value_objects/locale_id.rs)
```rust
#[nutype(sanitize(trim), validate(not_empty, max_len = 16, regex = r"^[a-z]{2,3}(-[A-Z]{2})?$"))]
pub struct LocaleId(String);  // BCP-47 subset: "en", "uk", "en-US"
```

#### [NEW] [`value_objects/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/value_objects/mod.rs)
Re-exports all value objects. Wildcard `pub use` for ergonomics within the crate.

---

### Phase 2 — Domain Errors

#### [NEW] [`errors.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/errors.rs)
```rust
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("document type not found: {0}")]
    DocumentTypeNotFound(DocumentTypeId),
    #[error("document instance not found: {0}")]
    DocumentInstanceNotFound(DocumentInstanceId),
    #[error("SingleType already has an instance: {0}")]
    SingleTypeAlreadyExists(DocumentTypeId),
    #[error("invalid field value for attribute '{attribute_id}': {reason}")]
    InvalidFieldValue { attribute_id: AttributeId, reason: String },
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
}
```

---

### Phase 3 — Field & Value Types (`domain/src/types/`)

Depends on Phase 1 (value objects) and Phase 2 (errors).

#### [NEW] [`types/field_type.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/types/field_type.rs)
```rust
pub enum FieldType {
    Uid, Uuid, Text, LocalizedText, Email, Url,
    Integer(IntegerSize),
    Decimal { precision: u8, scale: u8 },
    Date, DateTime, Boolean, Json,
}

pub enum IntegerSize { I16, I32, I64 }
```
Includes `FieldType::sql_type_name(&self) -> &str` — used by `SchemaLoader` DDL generation.

#### [NEW] [`types/primitive_value.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/types/primitive_value.rs)
Flat value set for `FieldType::Json` payloads:
```rust
pub enum PrimitiveValue { Text(String), Uid(String), Uuid(Uuid), Integer(i64),
                          Decimal(Decimal), Date(NaiveDate), DateTime(DateTime<Utc>) }
```

#### [NEW] [`types/domain_value.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/types/domain_value.rs)
1-to-1 with `FieldType`. Every variant corresponds to exactly one `FieldType`.
```rust
pub enum DomainValue {
    Text(String), Uid(String), Uuid(Uuid), Email(String), Url(String),
    Integer(i64), Decimal(Decimal), Date(NaiveDate), DateTime(DateTime<Utc>),
    Boolean(bool), Json(HashMap<String, PrimitiveValue>),
}
```
Includes `DomainValue::matches_field_type(&self, ft: &FieldType) -> bool`.

#### [NEW] [`types/content_value.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/types/content_value.rs)
```rust
pub enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}
```

#### [NEW] [`types/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/types/mod.rs)

---

### Phase 4 — `DocumentType` Entity

Depends on Phase 1 (IDs), Phase 3 (field types).

#### [NEW] [`entities/document_type.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/document_type.rs)
```rust
pub struct DocumentType {
    pub id:       DocumentTypeId,
    pub kind:     DocumentKind,
    pub info:     DocumentTypeInfo,
    pub options:  DocumentTypeOptions,
    pub fields:   IndexMap<AttributeId, FieldDefinition>,
    // Relations are NOT stored here — loaded separately via RelationRepository (ADR-003 Option F)
}

pub enum DocumentKind { Collection, SingleType }

pub struct DocumentTypeInfo {
    pub title:         String,
    pub singular_name: String,   // API path singular; used as DB table name base
    pub plural_name:   String,   // API path plural; DB table name
    pub description:   Option<String>,
}

pub struct DocumentTypeOptions {
    pub draft_and_publish: bool,
    // No available_locales here — system-level only (ADR-004 Option B)
}
```

#### [NEW] [`entities/field_definition.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/field_definition.rs)
```rust
pub struct FieldDefinition {
    pub id:          AttributeId,
    pub field_type:  FieldType,
    pub required:    bool,
    pub unique:      bool,
    pub constraints: Vec<FieldConstraint>,
}

pub enum FieldConstraint {
    Pattern(String),
    MinLength(usize), MaxLength(usize),
    MinInteger(i64),  MaxInteger(i64),
    MinDecimal(Decimal), MaxDecimal(Decimal),
}

impl FieldConstraint {
    pub fn is_applicable_for(&self, ft: &FieldType) -> bool { … }
}
```

---

### Phase 5 — `Relation` Entity (ADR-003 Option F)

Depends on Phase 1, Phase 4.

> [!NOTE]
> `Relation` (like `DocumentType`) is **immutable config** loaded from JSON at startup.
> There is **no `RelationRepository`** port and no `relations` DB table.
> All runtime access goes through `SchemaRegistry.find_relations_for(type_id)`.

#### [NEW] [`entities/relation.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/relation.rs)
```rust
pub struct Relation {
    pub id:         RelationId,
    pub owner_type: DocumentTypeId,
    pub owner_attr: AttributeId,
    pub owner_kind: OwnerRelationKind,
    pub inverse:    Option<RelationInverse>,   // None = unidirectional
    // No created_at — Relation is static config, not a runtime entity
}

pub struct RelationInverse {
    pub inverse_type: DocumentTypeId,
    pub inverse_attr: AttributeId,
    // inverse_kind is always derived — never stored
}

pub enum OwnerRelationKind { HasOne, HasMany }

pub enum InverseRelationKind { BelongsToOne, BelongsToMany }

/// Computed on read — never stored. Represents a relation from one specific type's perspective.
pub enum RelationView {
    /// This type is the owner side of a bidirectional relation
    OwnerSide   { attr: AttributeId, kind: OwnerRelationKind,   other_type: DocumentTypeId },
    /// This type is the inverse side of a bidirectional relation
    InverseSide { attr: AttributeId, kind: InverseRelationKind, other_type: DocumentTypeId },
    /// This type is the only side — no inverse (unidirectional)
    Unidirectional { attr: AttributeId, kind: OwnerRelationKind, target_type: DocumentTypeId },
}

impl Relation {
    pub fn view_for(&self, type_id: DocumentTypeId) -> Option<RelationView>;
    // Returns None if type_id is neither owner nor inverse participant
    // Returns Unidirectional (not OwnerSide) when inverse is None

    pub fn inverse_kind(&self) -> Option<InverseRelationKind>;
    // HasOne → BelongsToOne, HasMany → BelongsToMany; None if no inverse
}
```

---

### Phase 6 — `DocumentInstance` Entity

Depends on Phase 1, Phase 3, Phase 4.

#### [NEW] [`entities/document_instance.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/document_instance.rs)
```rust
pub struct DocumentInstance {
    pub id:               DocumentInstanceId,
    // db_row_id unified with id (ADR-007): None = not yet persisted
    pub db_row_id:        Option<DocumentInstanceId>,
    pub document_type_id: DocumentTypeId,
    pub content:          DocumentContent,
    // Resolved runtime relations: attribute → list of linked instance IDs
    pub relations:        HashMap<AttributeId, Vec<ResolvedRelation>>,
    pub audit:            AuditTrail,
}

pub struct DocumentContent {
    pub fields:            HashMap<AttributeId, ContentValue>,
    pub publication_state: PublicationState,
}

pub struct ResolvedRelation {
    pub attribute_id:        AttributeId,
    pub target_instance_id:  DocumentInstanceId,
}

pub enum PublicationState {
    Draft { last_published_revision: Option<u32> },
    Published { revision: u32, published_at: DateTime<Utc>, published_by: Option<UserId> },
}

pub struct AuditTrail {
    pub created_at: DateTime<Utc>,
    pub created_by: Option<UserId>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
    pub version:    u32,
}

impl DocumentInstance {
    pub fn new(type_id: DocumentTypeId, by: Option<UserId>, now: DateTime<Utc>) -> Self;

    /// Application service looks up `type_name` from `SchemaRegistry` and passes it in.
    /// `DocumentInstance` itself has no access to `DocumentType.info.plural_name`.
    pub fn publish(&mut self, type_name: &str, by: Option<UserId>, now: DateTime<Utc>)
        -> Result<PublishedSnapshot, DomainError>;

    pub fn unpublish(&mut self, now: DateTime<Utc>) -> Result<(), DomainError>;
    pub fn touch(&mut self, by: Option<UserId>, now: DateTime<Utc>);  // bumps version + updated_at
    pub fn is_owned_by(&self, user_id: &UserId) -> bool;
}
```

---

### Phase 7 — `PublishedSnapshot` Entity (ADR-007)

Depends on Phase 1, Phase 3.

#### [NEW] [`entities/published_snapshot.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/published_snapshot.rs)
```rust
pub struct PublishedSnapshot {
    pub id:           SnapshotId,
    pub instance_id:  DocumentInstanceId,
    pub type_name:    String,              // plural_name — for display/query
    pub revision:     u32,
    pub published_at: DateTime<Utc>,
    pub published_by: Option<UserId>,
    pub fields:       HashMap<AttributeId, ContentValue>,
}
```

---

### Phase 8 — `SystemConfig` Entity (ADR-004)

Depends on Phase 1.

#### [NEW] [`entities/system_config.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/system_config.rs)
```rust
pub struct SystemConfig {
    pub id:               SystemConfigId,
    pub available_locales: Vec<LocaleId>,
    pub default_locale:   LocaleId,
}

impl SystemConfig {
    pub fn contains_locale(&self, locale: &LocaleId) -> bool;
}
```

---

### Phase 9 — Auth Entities (ADR-005)

Depends on Phase 1, Phase 2.

#### [NEW] [`entities/auth/role.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/auth/role.rs)
```rust
pub struct Role {
    pub id:          RoleId,
    pub name:        String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

pub enum Permission {
    ManageSchema,
    CreateDocument(Option<DocumentTypeId>),
    ReadDocument(Option<DocumentTypeId>),
    UpdateDocument(Option<DocumentTypeId>),
    DeleteDocument(Option<DocumentTypeId>),
    PublishDocument(Option<DocumentTypeId>),
    ManageRoles,
    ManageUsers,
}

impl Permission {
    pub fn matches(&self, action: &Permission) -> bool; // None = wildcard (all types)
}
```

#### [NEW] [`entities/auth/user_role_assignment.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/auth/user_role_assignment.rs)
```rust
pub struct UserRoleAssignment {
    pub id:         UserRoleAssignmentId,
    pub user_id:    UserId,
    pub role_id:    RoleId,
    pub granted_at: DateTime<Utc>,
    pub granted_by: Option<UserId>,   // None = system/bootstrap grant
}
```

#### [NEW] [`entities/auth/access_request.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/auth/access_request.rs)
```rust
pub struct AccessRequest {
    pub id:             AccessRequestId,
    pub user_id:        UserId,
    pub email:          Option<String>,
    pub name:           Option<String>,
    pub requested_at:   DateTime<Utc>,
    pub status:         AccessRequestStatus,
    pub reviewed_by:    Option<UserId>,
    pub reviewed_at:    Option<DateTime<Utc>>,
    pub assigned_roles: Vec<RoleId>,
}

pub enum AccessRequestStatus {
    Pending,
    Approved,
    Rejected { reason: Option<String> },
}

impl AccessRequest {
    pub fn new(user_id: UserId, email: Option<String>, name: Option<String>,
               now: DateTime<Utc>) -> Self;
    pub fn approve(&mut self, by: UserId, roles: Vec<RoleId>, now: DateTime<Utc>)
        -> Result<Vec<UserRoleAssignment>, DomainError>;
    pub fn reject(&mut self, by: UserId, reason: Option<String>, now: DateTime<Utc>)
        -> Result<(), DomainError>;
    pub fn is_active(&self) -> bool;  // Pending or Approved
}
```

#### [NEW] [`entities/auth/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/auth/mod.rs)
#### [NEW] [`entities/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/entities/mod.rs)

---

### Phase 10 — Port Traits (`domain/src/ports/`)

Depends on all entity phases. These are pure trait definitions — no implementations.

> [!NOTE]
> **No `RelationRepository`**: `DocumentType` and `Relation` are immutable config loaded from
> JSON at startup. `SchemaRegistry` (Phase 11) is the only access point for schema data at
> runtime — no DB query, no port trait needed.

#### [NEW] [`ports/document_instance_repository.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/ports/document_instance_repository.rs)
```rust
pub struct Pagination { pub page: u32, pub page_size: u32 }
pub struct Page<T> { pub items: Vec<T>, pub total: u64, pub page: u32 }
pub struct FieldFilter { pub attribute_id: AttributeId, pub value: DomainValue }

#[async_trait]
pub trait DocumentInstanceRepository: Send + Sync {
    async fn find_by_id(&self, id: DocumentInstanceId)
        -> Result<Option<DocumentInstance>, DomainError>;
    async fn find_by_type(&self, type_id: DocumentTypeId, pagination: Pagination,
        filters: Vec<FieldFilter>) -> Result<Page<DocumentInstance>, DomainError>;
    async fn save(&self, instance: &DocumentInstance) -> Result<(), DomainError>;
    async fn delete(&self, id: DocumentInstanceId) -> Result<(), DomainError>;
    async fn exists_for_type(&self, type_id: DocumentTypeId) -> Result<bool, DomainError>;
}
```

#### [NEW] [`ports/snapshot_repository.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/ports/snapshot_repository.rs)
```rust
#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    async fn find_by_instance(&self, instance_id: DocumentInstanceId)
        -> Result<Vec<PublishedSnapshot>, DomainError>;
    async fn find_by_revision(&self, instance_id: DocumentInstanceId, revision: u32)
        -> Result<Option<PublishedSnapshot>, DomainError>;
    async fn save(&self, snapshot: &PublishedSnapshot) -> Result<(), DomainError>;
}
```

#### [NEW] [`ports/system_config_repository.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/ports/system_config_repository.rs)
```rust
#[async_trait]
pub trait SystemConfigRepository: Send + Sync {
    async fn load(&self) -> Result<SystemConfig, DomainError>;
    async fn save(&self, config: &SystemConfig) -> Result<(), DomainError>;
}
```

#### [NEW] [`ports/auth_repositories.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/ports/auth_repositories.rs)
Traits for `RoleRepository`, `UserRoleAssignmentRepository`, `AccessRequestRepository`.

#### [NEW] [`ports/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/ports/mod.rs)

---

### Phase 11 — Domain Services (`domain/src/services/`)

Depends on all entity phases and port traits.

#### [NEW] [`services/schema_registry.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/services/schema_registry.rs)
Pure in-memory struct (no I/O, no trait needed). Built by `SchemaLoader` in infrastructure.
```rust
pub struct SchemaRegistry {
    types:      HashMap<DocumentTypeId, DocumentType>,
    by_name:    HashMap<String, DocumentTypeId>,   // plural_name → id
    relations:  HashMap<RelationId, Relation>,
}

impl SchemaRegistry {
    pub fn find_type(&self, id: DocumentTypeId) -> Option<&DocumentType>;
    pub fn find_type_by_name(&self, plural_name: &str) -> Option<&DocumentType>;
    pub fn find_relations_for(&self, type_id: DocumentTypeId) -> Vec<RelationView>;
    pub fn validate_content(
        &self, type_id: DocumentTypeId,
        content: &DocumentContent,
        system_config: &SystemConfig,
    ) -> Result<(), Vec<DomainError>>;
    pub fn type_names(&self) -> impl Iterator<Item = &str>;
}
```

#### [NEW] [`services/authorization.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/services/authorization.rs)
```rust
pub struct AuthorizationService;

impl AuthorizationService {
    /// Owner rule → RBAC fallback → DENY
    pub fn can(
        user_id: &UserId,
        action: &Permission,
        instance: Option<&DocumentInstance>,
        roles: &[Role],
    ) -> bool;
}
```

Pure function, no state, no I/O. `roles` are pre-loaded by the application layer.

#### [NEW] [`services/mod.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/services/mod.rs)

---

### Phase 12 — Crate Root

#### [NEW] [`domain/src/lib.rs`](file:///Users/dmitri.astafiev/luminair/luminair-service-aws-dsql/domain/src/lib.rs)
Module declarations and top-level re-exports. No logic.

Must include the conditional `test_support` module declaration:

```rust
// lib.rs
pub mod errors;
pub mod value_objects;
pub mod types;
pub mod entities;
pub mod ports;
pub mod services;

#[cfg(test)]
mod test_support;   // ← required for test factories to be available across test modules
```

> [!NOTE]
> **Phase 13 (tests) is written inline, not deferred.** Tests described in Phase 13 are
> implemented inside their respective source files during each phase. Phase 13 documents
> the test plan up front — it is not a "do tests last" instruction.

---

---

### Phase 13 — Unit Tests (inline, `#[cfg(test)]`)

Tests live inside each source file they test, following the project convention.
The domain crate has **no I/O** — all tests are pure, no DB, no async runtime required
(exceptions noted below).

> [!NOTE]
> Domain tests must never use `tokio::test`, `sqlx`, or any infrastructure import.
> Port traits are tested via fake in-memory implementations only — no real DB.

#### Test helpers: `domain/src/test_support.rs` (only compiled under `#[cfg(test)]`)

Shared factories used across tests to avoid repetition:

```rust
// Available only in test builds
#[cfg(test)]
pub mod test_support {
    pub fn document_type_id() -> DocumentTypeId { … }     // deterministic test UUID
    pub fn instance_id() -> DocumentInstanceId { … }
    pub fn user_id() -> UserId { … }
    pub fn now() -> DateTime<Utc> { … }

    pub fn make_document_type(kind: DocumentKind) -> DocumentType { … }
    pub fn make_instance(type_id: DocumentTypeId) -> DocumentInstance { … }
    pub fn make_text_field(id: &str) -> FieldDefinition { … }
    pub fn make_schema_registry(types: Vec<DocumentType>, relations: Vec<Relation>)
        -> SchemaRegistry { … }
}
```

---

#### `value_objects/ids.rs` — 3 tests

| Test | Scenario |
|---|---|
| `test_attribute_id_valid_slug` | `"title"`, `"body_text"` → `Ok` |
| `test_attribute_id_invalid_uppercase` | `"Title"` → `Err` |
| `test_attribute_id_starts_with_digit` | `"1title"` → `Err` |

---

#### `value_objects/user_id.rs` — 2 tests

| Test | Scenario |
|---|---|
| `test_user_id_trims_whitespace` | `" sub123 "` → inner value `"sub123"` |
| `test_user_id_empty_rejected` | `""` → `Err` |

---

#### `value_objects/locale_id.rs` — 2 tests

| Test | Scenario |
|---|---|
| `test_locale_id_valid_bcp47` | `"en"`, `"uk"`, `"en-US"` → `Ok` |
| `test_locale_id_invalid_format` | `"EN"`, `"english"` → `Err` |

---

#### `types/field_type.rs` — 4 tests

| Test | Scenario |
|---|---|
| `test_sql_type_name_text` | `FieldType::Text` → `"TEXT"` |
| `test_sql_type_name_localized_text` | `FieldType::LocalizedText` → `"JSONB"` |
| `test_sql_type_name_decimal` | `FieldType::Decimal{p:10,s:2}` → `"NUMERIC(10, 2)"` |
| `test_sql_type_name_all_variants` | exhaustive: every variant returns a non-empty string |

---

#### `types/domain_value.rs` — 5 tests

| Test | Scenario |
|---|---|
| `test_matches_field_type_text_ok` | `DomainValue::Text` matches `FieldType::Text` |
| `test_matches_field_type_mismatch` | `DomainValue::Text` does NOT match `FieldType::Integer` |
| `test_matches_field_type_integer_all_sizes` | `DomainValue::Integer` matches any `IntegerSize` |
| `test_matches_field_type_json` | `DomainValue::Json` matches `FieldType::Json` |
| `test_matches_field_type_exhaustive` | every variant matches its correct counterpart only |

---

#### `entities/field_definition.rs` — 5 tests

| Test | Scenario |
|---|---|
| `test_constraint_applicable_pattern_on_text` | `Pattern` applicable to `Text` → true |
| `test_constraint_applicable_pattern_on_integer` | `Pattern` on `Integer` → false |
| `test_constraint_min_max_on_integer` | `MinInteger`/`MaxInteger` on `Integer` → true |
| `test_constraint_min_max_on_decimal` | `MinDecimal`/`MaxDecimal` on `Decimal` → true |
| `test_constraint_length_on_text_uid_email` | `MinLength`/`MaxLength` applicable to string-like types |

---

#### `entities/relation.rs` — 7 tests

| Test | Scenario |
|---|---|
| `test_view_for_bidirectional_owner_side` | `inverse: Some(…)`, `view_for(owner_type)` → `OwnerSide` |
| `test_view_for_bidirectional_inverse_side` | `inverse: Some(…)`, `view_for(inverse_type)` → `InverseSide` |
| `test_view_for_unidirectional` | `inverse: None`, `view_for(owner_type)` → `Unidirectional` (not `OwnerSide`) |
| `test_view_for_unrelated_type` | `view_for(other_type)` → `None` |
| `test_inverse_kind_has_many` | `HasMany` → `BelongsToMany` |
| `test_inverse_kind_has_one` | `HasOne` → `BelongsToOne` |
| `test_inverse_kind_none_when_no_inverse` | `inverse: None` → `inverse_kind()` returns `None` |

---

#### `entities/document_instance.rs` — 10 tests

| Test | Scenario |
|---|---|
| `test_new_instance_starts_as_draft` | `new()` → `Draft { last_published_revision: None }` |
| `test_publish_first_time` | draft → `Published { revision: 1 }` |
| `test_publish_increments_revision` | publish → edit → publish → `revision: 2` |
| `test_publish_returns_snapshot` | `publish()` returns `PublishedSnapshot` with correct fields |
| `test_unpublish_records_last_revision` | `Published { revision: 3 }` → `Draft { last_published_revision: Some(3) }` |
| `test_unpublish_on_draft_fails` | `unpublish()` on `Draft` → `DomainError::InvalidStateTransition` |
| `test_touch_bumps_version` | `version: 1` → after `touch()` → `version: 2` |
| `test_touch_updates_updated_at` | `updated_at` strictly after `created_at` |
| `test_is_owned_by_creator` | `created_by == user_id` → `true` |
| `test_is_owned_by_other_user` | different user → `false` |

---

#### `entities/auth/access_request.rs` — 7 tests

| Test | Scenario |
|---|---|
| `test_new_request_is_pending` | `new()` → `AccessRequestStatus::Pending` |
| `test_approve_sets_status` | `approve()` → `Approved`; `assigned_roles` populated |
| `test_approve_returns_assignments` | returned `Vec<UserRoleAssignment>` has correct `user_id` and `role_id` |
| `test_approve_already_approved_fails` | approving an already-approved request → `Err` |
| `test_reject_sets_status` | `reject()` → `Rejected { reason: Some("…") }` |
| `test_reject_approved_request_fails` | rejecting an approved request → `Err` |
| `test_is_active_pending_and_approved` | `Pending` → true; `Approved` → true; `Rejected` → false |

---

#### `entities/system_config.rs` — 3 tests

| Test | Scenario |
|---|---|
| `test_contains_locale_found` | `contains_locale("en")` → true |
| `test_contains_locale_not_found` | `contains_locale("fr")` when `fr` not in list → false |
| `test_contains_locale_default_included` | `default_locale` is always in `available_locales` (invariant) |

---

#### `services/schema_registry.rs` — 8 tests

| Test | Scenario |
|---|---|
| `test_find_type_by_id` | known ID → `Some(&DocumentType)` |
| `test_find_type_unknown_id` | unknown ID → `None` |
| `test_find_type_by_name` | `plural_name` lookup → correct type |
| `test_find_relations_for_owner` | type that owns a relation → relation appears |
| `test_find_relations_for_inverse` | type on inverse side → relation appears |
| `test_validate_content_correct` | valid field values → `Ok(())` |
| `test_validate_content_wrong_type` | `Integer` value for `Text` field → `Err` with error |
| `test_validate_content_unknown_locale` | `LocalizedText` with undeclared locale → `Err` |

---

#### `services/authorization.rs` — 6 tests

| Test | Scenario |
|---|---|
| `test_owner_always_allowed` | `created_by == user_id` → `true` regardless of roles |
| `test_rbac_explicit_permission_granted` | role has exact `ReadDocument(Some(type_id))` → `true` |
| `test_rbac_wildcard_permission` | role has `ReadDocument(None)` → `true` for any type |
| `test_rbac_wrong_permission_denied` | role has `ReadDocument` but action is `DeleteDocument` → `false` |
| `test_no_roles_denied` | empty role list, not owner → `false` |
| `test_admin_all_permissions` | `admin` role (all permissions) → `true` for any action |

---

## Verification Plan

### Gate commands (must all pass before plan is complete)

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
cargo doc --no-deps -p domain
```

### Manual Verification

```bash
cargo test --workspace -- --nocapture   # see test names as they run
cargo test -p domain -- --list          # list all test cases
```

Expected output: **~60 unit tests**, all passing, zero warnings.

---

## File Tree Summary

```
luminair-service-aws-dsql/
├── Cargo.toml                                   [NEW] workspace root
├── domain/
│   ├── Cargo.toml                               [NEW]
│   └── src/
│       ├── lib.rs                               [NEW]
│       ├── errors.rs                            [NEW] + 0 unit tests (tested indirectly)
│       ├── test_support.rs                      [NEW] cfg(test) only
│       ├── value_objects/
│       │   ├── mod.rs                           [NEW]
│       │   ├── ids.rs                           [NEW] + 6 tests
│       │   ├── user_id.rs                       [NEW] + 2 tests
│       │   └── locale_id.rs                     [NEW] + 1 test
│       ├── types/
│       │   ├── mod.rs                           [NEW]
│       │   ├── field_type.rs                    [NEW] + 4 tests
│       │   ├── primitive_value.rs               [NEW] + 0 tests (simple enum)
│       │   ├── domain_value.rs                  [NEW] + 5 tests
│       │   └── content_value.rs                 [NEW] + 0 tests (simple enum)
│       ├── entities/
│       │   ├── mod.rs                           [NEW]
│       │   ├── document_type.rs                 [NEW] + 0 tests (plain data struct)
│       │   ├── field_definition.rs              [NEW] + 5 tests
│       │   ├── relation.rs                      [NEW] + 6 tests
│       │   ├── document_instance.rs             [NEW] + 10 tests
│       │   ├── published_snapshot.rs            [NEW] + 0 tests (plain data struct)
│       │   ├── system_config.rs                 [NEW] + 3 tests
│       │   └── auth/
│       │       ├── mod.rs                       [NEW]
│       │       ├── role.rs                      [NEW] + 1 test (Permission::matches)
│       │       ├── user_role_assignment.rs      [NEW] + 0 tests (plain data struct)
│       │       └── access_request.rs            [NEW] + 7 tests
│       ├── ports/
│       │   ├── mod.rs                           [NEW]
│       │   ├── document_instance_repository.rs  [NEW] + 0 tests (trait definition)
│       │   ├── snapshot_repository.rs           [NEW] + 0 tests (trait definition)
│       │   ├── system_config_repository.rs      [NEW] + 0 tests (trait definition)
│       │   └── auth_repositories.rs             [NEW] + 0 tests (trait definition)
│       └── services/
│           ├── mod.rs                           [NEW]
│           ├── schema_registry.rs               [NEW] + 8 tests
│           └── authorization.rs                 [NEW] + 6 tests
├── application/
│   └── Cargo.toml                               [NEW] stub
└── infrastructure/
    └── Cargo.toml                               [NEW] stub
```

**Total: ~30 new files, ~64 unit tests.**

