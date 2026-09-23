# Research: Domain Model Design

- **Date**: 2026-09-17
- **Question**: What is the correct domain model for the Luminair document management system?
- **Status**: Settled decisions recorded here. Open decisions → see linked ADRs.

---

## Settled Decisions

### 1. Terminology

| Concept | Name in Luminair |
|---|---|
| Schema definition | `DocumentType` |
| A stored record | `DocumentInstance` |
| Type of document collection | `DocumentKind` |
| A field within a schema | `FieldDefinition` |
| A relation within a schema | `RelationDefinition` |
| A resolved runtime relation link | `ResolvedRelation` |

`DocumentType` and `DocumentInstance` are intentionally more general than "content type / entry" — this system is above a pure CMS.

---

### 2. DocumentKind

```rust
enum DocumentKind {
    Collection,   // many instances per type
    SingleType,   // at most one instance per type
    // open for extension (e.g. Versioned, TimeSeries, …)
}
```

`SingleType` enforcement strategy → **see [ADR-002](../adr/ADR-002-singleton-enforcement.md)**.

---

### 3. DocumentType (Schema Entity)

```rust
struct DocumentType {
    id: DocumentTypeId,         // strongly typed newtype over Uuid v7
    kind: DocumentKind,
    info: DocumentTypeInfo,
    options: DocumentTypeOptions,
    fields: IndexMap<AttributeId, FieldDefinition>,    // ordered, stable
    relations: IndexMap<AttributeId, RelationDefinition>,
}

struct DocumentTypeInfo {
    title: String,
    singular_name: String,   // API path singular
    plural_name: String,     // API path plural
    description: Option<String>,
}

struct DocumentTypeOptions {
    draft_and_publish: bool,      // controls API/UI workflow; all instances always have PublicationState
    available_locales: Vec<LocaleId>,  // scope TBD — see ADR-004
}
```

---

### 4. Field Type System

```rust
enum FieldType {
    Uid,                                 // validated slug; optional auto-generation from source field
    Uuid,
    Text,
    LocalizedText,                       // HashMap<LocaleId, String>; in MVP
    Email,                               // validated email string
    Url,                                 // validated URL string
    Integer(IntegerSize),                // i16 | i32 | i64
    Decimal { precision: u8, scale: u8 },
    Date,                                // NaiveDate
    DateTime,                            // DateTime<Utc>
    Boolean,
    Json,                                // flat HashMap<String, PrimitiveValue>; see below
}

// Json field payload — deliberately simple flat map
enum PrimitiveValue {
    Text(String),
    Uid(String),
    Uuid(Uuid),
    Integer(i64),
    Decimal(Decimal),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}
```

**Key decision**: `Json` is a flat `HashMap<String, PrimitiveValue>` — not arbitrary nested JSON (`serde_json::Value`). Full arbitrary JSON is explicitly out of scope to keep validation tractable. If arbitrary JSON is needed in the future, it requires a new `FieldType::ArbitraryJson` variant and a separate ADR.

---

### 5. FieldDefinition

```rust
struct FieldDefinition {
    id: AttributeId,
    field_type: FieldType,
    required: bool,
    unique: bool,
    constraints: Vec<FieldConstraint>,
}

enum FieldConstraint {
    Pattern(String),
    MinLength(usize),
    MaxLength(usize),
    MinInteger(i64),
    MaxInteger(i64),
    MinDecimal(Decimal),
    MaxDecimal(Decimal),
}
```

`FieldConstraint::is_applicable_for(field_type)` is validated at schema load time, not at runtime.

---

### 6. Relations — Two Separate Types

**Schema level** (what types can be related and how):
```rust
struct RelationDefinition {
    id: AttributeId,
    kind: RelationKind,
    target_type: DocumentTypeId,
    inverse_of: Option<BiDirectionalRelationId>,  // links to the paired relation if bidirectional
}

enum RelationKind {
    HasOne,
    HasMany,
    BelongsToOne,
    BelongsToMany,
}
```

**Runtime level** (which concrete instances are related):
```rust
struct ResolvedRelation {
    attribute_id: AttributeId,         // which relation field this is
    target_instance_id: DocumentInstanceId,
}
```

`DocumentInstance.relations: HashMap<AttributeId, Vec<ResolvedRelation>>`

**BiDirectionalRelation as a separate entity** → **see [ADR-003](../adr/ADR-003-bidirectional-relations.md)**.

---

### 7. DocumentInstance

```rust
struct DocumentInstance {
    id: DocumentInstanceId,             // stable Uuid v7 — the document's identity
    db_row_id: Option<DatabaseRowId>,   // None = not yet persisted to DB
    document_type_id: DocumentTypeId,
    content: DocumentContent,
    relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
    audit: AuditTrail,
}
```

`DocumentInstanceId` is the stable public identity (used in API, relations, etc.).  
`DatabaseRowId` is the internal DB PK — `None` when the instance has been constructed but not yet saved.

> **Open question**: Should `DatabaseRowId` be a separate type, or simply the same `Uuid` as `DocumentInstanceId`? If we use a single-row-per-instance model (no separate draft/published rows), they can be the same type. Needs confirmation during DB schema design.

---

### 8. DocumentContent and Values

```rust
struct DocumentContent {
    fields: HashMap<AttributeId, ContentValue>,
    publication_state: PublicationState,
}

enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}

enum DomainValue {
    Text(String),
    Uid(String),
    Uuid(Uuid),
    Email(Email),       // validated newtype
    Url(Url),           // validated newtype
    Integer(i64),
    Decimal(Decimal),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
    Boolean(bool),
    Json(HashMap<String, PrimitiveValue>),
}
```

`DomainValue` variants are now 1-to-1 with `FieldType` variants. No orphan variants.

---

### 9. Publication State (Redesigned)

All `DocumentInstance`s always carry `PublicationState`. `draft_and_publish` on `DocumentTypeOptions` only controls whether the API/UI exposes the publish workflow — it does not change what is stored.

```rust
enum PublicationState {
    Draft {
        last_published_revision: Option<u32>,
        // None  → never been published
        // Some(n) → was published n times; currently in draft after that
    },
    Published {
        revision: u32,               // how many times this instance has been published (≥ 1)
        published_at: DateTime<Utc>,
        published_by: Option<UserId>,
    },
}
```

**State transitions**:
- New instance → `Draft { last_published_revision: None }`
- Publish → `Published { revision: prev + 1, … }` (prev = last_published_revision or 0)
- Edit after publish → `Draft { last_published_revision: Some(last_revision) }`

**For types with `draft_and_publish = false`**:  
The application service auto-publishes on every save. The instance is always in `Published` state after any write. `revision` increments on every save.

---

### 10. AuditTrail (Redesigned)

`AuditTrail` is purely administrative metadata — completely independent of publication semantics.

```rust
struct AuditTrail {
    created_at: DateTime<Utc>,
    created_by: Option<UserId>,
    updated_at: DateTime<Utc>,
    updated_by: Option<UserId>,
    version: u32,    // increments on every write (edit, publish, unpublish)
}
```

| Counter | Increments on | Meaning |
|---|---|---|
| `AuditTrail.version` | every write to the instance | total number of saves |
| `PublicationState.revision` | explicit publish only | how many times published |

For types with `draft_and_publish = false`: every save is both an edit and a publish, so `version` and `revision` increment together on every write.

`AuditTrail::new(by)` → `version = 1` (first save).  
`PublicationState::new()` → `Draft { last_published_revision: None }`.

---

### 11. Uid Field Type

`Uid` is a validated URL-safe slug string (lowercase, alphanumeric, hyphens only, max 255 chars).  
Auto-generation from a source field (e.g. derive slug from `title`) is **optional behaviour** configured in `FieldDefinition`. Auto-generation details are deferred to a future spike — not in MVP.

---

### 12. Localization

- Localization is **field-level**: individual fields can be of type `LocalizedText`
- `DocumentTypeOptions.available_locales: Vec<LocaleId>` defines which locales are valid for this type
- Whether available locales are better defined at **system level** → **see [ADR-004](../adr/ADR-004-locale-scope.md)**
- `LocalizedText` values that contain locale keys not in `available_locales` are rejected at validation time

---

### 13. Authentication (Investigation Required)

The system will run on AWS (Cognito) or self-hosted (Keycloak / any OIDC provider).  
Authentication is **external** — the system validates JWTs but does not issue them.  
The system **tracks `UserId`** internally for AuditTrail and document-level R/W privileges.

Investigation: → **[`docs/research/auth-strategy.md`](./auth-strategy.md)**  
ADR: → pending auth investigation

---

## Open Questions → ADRs

| Question | ADR |
|---|---|
| How to enforce `SingleType` uniqueness? | [ADR-002](../adr/ADR-002-singleton-enforcement.md) |
| BiDirectional relation as separate entity? | [ADR-003](../adr/ADR-003-bidirectional-relations.md) |
| Available locales: system-level vs. document-type-level? | [ADR-004](../adr/ADR-004-locale-scope.md) |
| Auth strategy (external IdP + internal RBAC)? | pending auth research |
