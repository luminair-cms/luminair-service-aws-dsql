# ADR-006: Schema Loading Strategy — JSON Config Files at Startup

- **Status**: Accepted
- **Date**: 2026-09-21
- **Deciders**: Dmitri Astafiev
- **Research**: [docs/research/domain-model-design.md](../research/domain-model-design.md)

## Context

`DocumentType` definitions (schema: fields, relations, options) must be available at runtime so
the service can validate `DocumentInstance` content and route API calls.

The question is: are document type schemas managed dynamically via API (CRUD), or loaded from
static configuration at startup?

## Decision

**Schemas are defined in JSON configuration files and loaded once at application startup.**

There is no runtime API for creating, updating, or deleting `DocumentType` definitions.
Schema changes require updating the JSON files and restarting the service.

This matches the original design note: *"the schema is intentionally immutable after load"*.

---

## Schema File Layout

```
luminair-service-aws-dsql/
└── schema/                          # committed to git
    ├── document-types/
    │   ├── article.json             # one file per DocumentType
    │   ├── author.json
    │   └── tag.json
    └── relations/
        └── article-tag.json         # one file per Relation (Option F)
```

Each JSON file corresponds directly to a `DocumentType` or `Relation` domain entity.

### Example `schema/document-types/article.json`

```json
{
  "id": "01932c4a-...",
  "kind": "Collection",
  "info": {
    "title": "Article",
    "singular_name": "article",
    "plural_name": "articles"
  },
  "options": {
    "draft_and_publish": true
  },
  "fields": [
    { "id": "title",      "field_type": "Text",    "required": true,  "unique": false },
    { "id": "slug",       "field_type": "Uid",     "required": true,  "unique": true  },
    { "id": "content",    "field_type": "Text",    "required": false, "unique": false },
    { "id": "published",  "field_type": "DateTime","required": false, "unique": false }
  ]
}
```

### Example `schema/relations/article-tag.json`

```json
{
  "id": "01932c4b-...",
  "owner_type":  "01932c4a-...",
  "owner_attr":  "tags",
  "owner_kind":  "HasMany",
  "inverse": {
    "inverse_type": "01932c4c-...",
    "inverse_attr": "articles"
  }
}
```

UUIDs in schema files are **stable, hand-assigned v4 UUIDs** — they never change for a given
document type, even across deployments. (v7 is time-ordered and used for `DocumentInstance` PKs;
schema UUIDs are static and embedded in config.)

---

## Startup Sequence

```
infrastructure/src/schema_loader.rs

1. Read and parse all JSON files from schema/ directory
2. Validate each DocumentType:
   - FieldConstraints applicable to FieldTypes
   - No duplicate AttributeIds within a type
   - Uid field count ≤ 1 per type
   - No AttributeId collides with reserved column names
3. Validate all Relations:
   - owner_type and inverse_type exist in loaded DocumentTypes
   - No duplicate RelationIds
   - Valid kind pairs (HasMany ↔ BelongsToMany, HasOne ↔ BelongsToOne)
4. Cache schema in memory (Arc<SchemaRegistry>) — available to all request handlers
5. Log: "Schema loaded: N document types, M relations"
If ANY validation fails → startup aborts with a clear error message
```

The in-memory **`SchemaRegistry`** is an `Arc<SchemaRegistry>` injected into all application
services that need schema access. It is immutable for the lifetime of the process.

---

## Implications

### No `DocumentTypeRepository`

With `document_types` and `relations` removed from the DB, there is no repository trait for
`DocumentType` or `Relation`. They are not persisted entities — they are configuration values
loaded from JSON and held in `SchemaRegistry`.

All schema lookups go through `Arc<SchemaRegistry>` injected into application services.
No DB query is ever made for schema data at runtime.

### API Surface

No schema management endpoints are exposed. The API is purely for `DocumentInstance` CRUD.
Schema introspection (read-only) can be exposed:

```
GET /api/document-types          → list loaded types (from SchemaRegistry, no DB query)
GET /api/document-types/{id}     → get single type schema
```

These are read-only projections of the in-memory registry.

### `SchemaRegistry` Domain Service

```rust
// domain/src/services/schema_registry.rs
pub struct SchemaRegistry {
    types:     HashMap<DocumentTypeId, DocumentType>,
    by_api_id: HashMap<String, DocumentTypeId>,
    relations: HashMap<RelationId, Relation>,
}

impl SchemaRegistry {
    pub fn find_type(&self, id: DocumentTypeId) -> Option<&DocumentType>;
    pub fn find_type_by_api_id(&self, api_id: &str) -> Option<&DocumentType>;
    pub fn find_relations_for(&self, type_id: DocumentTypeId) -> Vec<&Relation>;
    pub fn validate_content(&self, type_id: DocumentTypeId, content: &DocumentContent)
        -> Result<(), Vec<ValidationError>>;
}
```

`SchemaRegistry` is constructed by the schema loader and wrapped in `Arc<SchemaRegistry>`.
It is a **domain service** (pure logic, no I/O), placed in `domain/src/services/`.

### Schema Changes Workflow

```
1. Edit JSON file in schema/
2. Run cargo test --workspace   (schema validation is covered by tests)
3. Deploy / restart service
4. Startup upserts changed DocumentTypes to DB
```

Breaking schema changes (removing a field that has data) require a data migration before
the JSON change is deployed — this is the operator's responsibility.

---

## Consequences

- `schema/` directory is a first-class project artefact — reviewed and versioned in git
- Schema UUIDs must be stable across deployments — generate once and never change
- Application startup validates the entire schema — misconfigured JSON fails fast, loudly
- `SchemaRegistry` replaces runtime `DocumentTypeRepository` reads in application services
- No schema management UI or API (reduces attack surface and complexity)
- Schema changes always require a service restart — acceptable for a backend-of-record system

## Follow-up Actions

- [x] Decision accepted
- [ ] Create `schema/` directory structure with example files
- [ ] Implement `SchemaLoader` in `infrastructure/src/schema_loader.rs`
- [ ] Define `SchemaRegistry` in `domain/src/services/schema_registry.rs`
- [ ] Add schema validation tests (startup fails cleanly on invalid JSON)
- [ ] Update `docs/data-model.md` with schema file format reference
- [ ] Document schema UUID generation convention in `CONTRIBUTING.md`
