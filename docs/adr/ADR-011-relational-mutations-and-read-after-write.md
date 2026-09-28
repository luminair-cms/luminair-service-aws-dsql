# ADR-011: Relational Mutation Operations and Read-After-Write Command Orchestration

- **Status**: `Accepted`
- **Date**: 2026-09-28
- **Deciders**: Dmitri Astafiev, Antigravity
- **Related ADRs**:
  - [ADR-001: Hexagonal Architecture](./ADR-001-hexagonal-architecture.md)
  - [ADR-003: Bidirectional Relations](./ADR-003-bidirectional-relations.md)
  - [ADR-007: Persistence Model](./ADR-007-persistence-model.md)
  - [ADR-008: Unified Naming Conventions and REST Routing Strategy](./ADR-008-naming-conventions-and-routing.md)
  - [ADR-009: Dynamic Schema Migration and Relation Persistence](./ADR-009-dynamic-schema-migration-and-relation-persistence.md)
  - [ADR-010: Admin Dashboard UI Architecture & Implementation Strategy](./ADR-010-ui-architecture.md)

---

## Context

In [ADR-001](./ADR-001-hexagonal-architecture.md) and [ADR-008](./ADR-008-naming-conventions-and-routing.md), Luminair adopted Domain-Driven Design (DDD) with a RESTful API layer. In [ADR-009](./ADR-009-dynamic-schema-migration-and-relation-persistence.md), relational associations between document types (`HasOne`, `HasMany`) are persisted in dual draft/published link tables (e.g. `_rel_articles_tags`).

In the initial implementation, `CreateDocumentCommand` and `UpdateDocumentCommand` handled only scalar content fields. Relational link associations were not exposed in write commands.

As we introduce relational mutations (Strapi-style `connect`, `disconnect`, `set`, `unset`), an architectural tension emerges between:
1. **Academic CQRS / CQS**: Commands should be void (`Result<(), Error>`) or return at most a resource identifier (`DocumentInstanceId`). Returning data projections from commands violates strict Command Query Separation.
2. **REST Conventions (RFC 9110 §9.3.2)**: `POST` (`201 Created`) and `PUT` (`200 OK`) should return the representation of the created/updated resource to avoid redundant client `GET` roundtrips.
3. **Single Page Application (SPA) Latency & Cache Safety**: In the React 19 / TanStack Query UI layer ([ADR-010](./ADR-010-ui-architecture.md)), if write commands return `void` or unpopulated resource stubs, the UI either has to perform a secondary `GET` request (doubling network latency on AWS CloudFront / Lambda / Aurora DSQL) or risks overwriting its cache with empty relation lists (the "missing relations" cache-clobbering trap).

Furthermore, relational mutations are **relative delta operations** (`connect`, `disconnect`). The server is the sole source of truth for the resulting relation set after set arithmetic, foreign key validation, and cardinality constraint enforcement.

---

## Decision Drivers

1. **REST Protocol Compliance & Network Efficiency**: Support RFC 9110 `201 Created` and `200 OK` with full resource representation in a single HTTP roundtrip.
2. **Strapi-Compatible Relational Mutations**: Support intuitive delta operations (`connect`, `disconnect`, `set`, `unset`) as well as convenient shorthand syntax.
3. **TanStack Query Cache Safety**: Guarantee that write responses return the authoritative relation tree so client-side cache updates do not clobber existing populated relations.
4. **Pragmatic Scope Control for MVP**: Explicitly postpone nested document creation (creating new child documents inline) to post-MVP; MVP focuses strictly on connecting/disconnecting *existing* documents.
5. **ACID Transaction Boundary**: All mutations to parent document columns and child link table rows must commit atomically within a single database transaction.
6. **Domain Invariant Enforcement**: Protect relational cardinality (`HasOne` vs `HasMany`) at the domain and application boundaries.

---

## Considered Alternatives

### Variant A: Canonical / Pure CQRS
- **Concept**: `create` returns `DocumentInstanceId`; `update` returns `()`.
- **Pros**: 100% textbook CQRS purity. Zero projection logic in write handlers.
- **Cons**: Every create/update requires an immediate secondary `GET` from the UI. Doubles network latency and database queries; creates visible UI loading flickers.

### Variant B: Naive Pragmatic CQRS (Return Aggregate Without Population)
- **Concept**: Commands mutate link tables and return `DocumentInstance` directly, but `populated_relations` remains empty.
- **Pros**: Fast, single database transaction.
- **Cons**: Severe UI bug: writing to an article clears the populated author and category badges in the frontend cache unless the frontend manually ignores relation fields in the response.

### Variant C: Mutation Receipts / Event Notifications
- **Concept**: Commands return a diff receipt: `{ id, connected: [...], disconnected: [...] }`.
- **Pros**: Explicit and clean for backend message queues.
- **Cons**: Incompatible with standard Headless CMS REST frontends. Forces complex client-side relation graph reconciliation.

### Variant D: Orchestrated Read-After-Write (Adopted)
- **Concept**: The write command executes the atomic database transaction. If the caller requests relation population (via `?populate=...`), the application service immediately invokes `enrich()` in-memory and returns the fully populated `DocumentInstance`.
- **Pros**:
  - 1 HTTP roundtrip (optimal UX).
  - Exact match for Strapi 5, Prisma, and Contentful behavior.
  - Safe TanStack Query cache updates.
  - Clear separation between the transactional write phase and the read-after-write enrichment phase.
- **Cons**:
  - Combines write orchestration with read enrichment in the application use-case layer.

---

## Decision

**Chosen Option: Variant D (Orchestrated Read-After-Write)** with the explicit MVP constraint that **nested document creation is postponed to post-MVP**.

### 1. Relational Action Enum (`application::commands::documents`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationAction {
    /// Replaces all existing relations for this attribute with the given target IDs.
    Set(Vec<DocumentInstanceId>),
    /// Appends the specified target IDs to existing relations without removing current links.
    Connect(Vec<DocumentInstanceId>),
    /// Removes the specified target IDs from existing relations.
    Disconnect(Vec<DocumentInstanceId>),
    /// Clears all relations for this attribute.
    Unset,
}
```

*Note on MVP Scope*: A future variant `Create(Vec<CreateDocumentCommand>)` for nested document creation is deferred to Post-MVP.

### 2. Command Extensions

`CreateDocumentCommand` and `UpdateDocumentCommand` are extended to include relational actions and optional populate directives:

```rust
pub struct CreateDocumentCommand {
    pub type_id: DocumentTypeId,
    pub fields: HashMap<AttributeId, ContentValue>,
    pub relations: HashMap<AttributeId, RelationAction>,
    pub populate: Option<Vec<AttributeId>>,
}

pub struct UpdateDocumentCommand {
    pub id: DocumentInstanceId,
    pub type_id: DocumentTypeId,
    pub fields: HashMap<AttributeId, ContentValue>,
    pub relations: HashMap<AttributeId, RelationAction>,
    pub populate: Option<Vec<AttributeId>>,
}
```

### 3. Application Service Orchestration (`DocumentsServiceImpl`)

In `create` and `update`:
1. **Validate**:
   - Verify caller permissions.
   - Verify relation attributes exist on `DocumentTypeId` via `SchemaRegistry`.
   - Enforce cardinality: if relation is `HasOne`, reject actions that result in more than 1 linked target.
2. **Execute Transactional Write**:
   - Mutate document entity attributes and update `instance.relations`.
   - Persist to PostgreSQL / Aurora DSQL (saving the document table row and synchronizing the `_rel_*` draft link table).
3. **Read-After-Write Enrichment**:
   - If `cmd.populate` is `Some(attrs)`:
     - Invoke `self.enrich(type_id, Some(attrs), vec![instance])`.
     - Return the fully enriched `DocumentInstance` with populated relations.
   - If `cmd.populate` is `None`:
     - Return `instance` with populated `instance.relations` (IDs).

### 4. HTTP API & DTO Conventions

#### Request Payloads (Strapi Parity)
Clients can submit relational mutations using explicit action objects or convenient shorthands:

```json
{
  "title": "Getting Started with Aurora DSQL",
  "category": {
    "connect": ["01920000-0000-7000-8000-000000000001"]
  },
  "tags": {
    "set": [
      "01920000-0000-7000-8000-000000000010",
      "01920000-0000-7000-8000-000000000020"
    ]
  }
}
```

Shorthand syntax:
- `"category": "01920000-0000-7000-8000-000000000001"` -> parsed as `RelationAction::Set(vec![id])`
- `"tags": ["0192...", "0192..."]` -> parsed as `RelationAction::Set(vec![id1, id2])`
- `"category": null` -> parsed as `RelationAction::Unset`

#### Query Parameters
- `POST /api/{collection}?populate=category,tags`
- `PUT /api/{collection}/{id}?populate=category,tags`

The endpoint returns `201 Created` / `200 OK` with the populated representation in `SingleResponse<T>`.

---

## Consequences

### Positive
- **Optimal UX & Performance**: Single HTTP roundtrip for creating/updating documents and receiving populated relations.
- **Cache Integrity**: TanStack Query cache in the React UI can be safely updated in-place without losing populated relations or triggering loading spinners.
- **Robust Invariant Protection**: Server enforces `HasOne` vs `HasMany` limits and link table referential integrity.
- **Strapi 5 Compatibility**: Frictionless transition for developers and frontend SDKs familiar with Strapi REST conventions.
- **Deterministic Scope**: By deferring nested creation, MVP avoids complex cascade transaction rollbacks and circular reference resolution.

### Negative / Trade-offs
- **Command Query Coupling in Use Case**: `DocumentsService` performs query-side enrichment after write commands when `populate` is requested.
- **Slightly Higher Write Latency when `?populate` is Used**: The database write is followed immediately by the relation enrichment query within the same HTTP request.

---

## Follow-up Actions

- [ ] Implement `RelationAction` and extend `CreateDocumentCommand` / `UpdateDocumentCommand` in `application`.
- [ ] Implement relation manipulation methods on `domain::content::DocumentInstance`.
- [ ] Implement relation action validation and orchestration in `DocumentsServiceImpl`.
- [ ] Implement JSON parsing for relational actions and populate parameter extraction in `infrastructure::api`.
- [ ] Add comprehensive integration tests in `infrastructure/tests/api_test.rs` covering all relation actions.
