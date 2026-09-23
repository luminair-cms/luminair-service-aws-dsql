# ADR-007: Persistence Model — Per-Type Tables with Typed Columns

- **Status**: Accepted
- **Date**: 2026-09-21 · Revised: 2026-09-21
- **Deciders**: Dmitri Astafiev
- **Supersedes**: ADR-007 v1 (EAV approach — discarded)
- **Research**: [docs/research/domain-model-design.md](../research/domain-model-design.md)

## Context

Two persistence model decisions were made before implementing the domain model:

1. How `DocumentContent.fields` are stored
2. How draft vs. published state is stored

The initial proposal (EAV table) was replaced with a cleaner per-type table approach.

## Decisions

### Decision 1: One table per DocumentType, typed columns

Each `DocumentType` gets its own DB table. The table has:
- System columns (id, publication, audit) — identical across all tables
- One typed column per `FieldDefinition` — type determined by `FieldType`
- `LocalizedText` fields: a single `JSONB` column storing `{"locale_key": "text_value"}`
- `Json` fields: a single `JSONB` column storing the flat `PrimitiveValue` map

**No generic EAV table. No separate localized-fields sub-table.**

### Decision 2: Two-table publication model (unchanged)

- `{plural_name}` — current working state (draft or published); one row per instance
- `document_snapshots` — immutable published revisions; shared across all types (JSONB payload)

### Decision 3: Unique index only for SingleType tables

A `UNIQUE` constraint enforcing "at most one row" is created **only** on tables for
`DocumentKind::SingleType`. Collection tables have no such constraint.

---

## Table Schema

### `{plural_name}` — Per-Type Instance Table

Generated at startup by the schema loader from the `DocumentType` definition.
Table name = `DocumentTypeInfo.plural_name` (e.g. `articles`, `authors`, `settings`).

```sql
-- Example: DocumentType "article" (Collection)
CREATE TABLE IF NOT EXISTS articles (
    -- Identity
    id                      UUID PRIMARY KEY,   -- DocumentInstanceId (Uuid v7)

    -- Publication state (PublicationState enum)
    publication_status      VARCHAR(16) NOT NULL
                            CHECK (publication_status IN ('draft', 'published')),
    last_published_revision INTEGER,             -- NULL = never published
    published_at            TIMESTAMPTZ,         -- NULL when draft
    published_by            VARCHAR(255),        -- UserId (OIDC sub); NULL when draft

    -- Audit trail
    version                 INTEGER NOT NULL DEFAULT 1,
    created_at              TIMESTAMPTZ NOT NULL,
    created_by              VARCHAR(255),
    updated_at              TIMESTAMPTZ NOT NULL,
    updated_by              VARCHAR(255),

    -- User-defined fields (example for "article" type):
    title                   TEXT,               -- FieldType::Text
    slug                    VARCHAR(255),        -- FieldType::Uid (validated slug)
    view_count              BIGINT,              -- FieldType::Integer(i64)
    published_on            DATE,               -- FieldType::Date
    is_featured             BOOLEAN,            -- FieldType::Boolean
    summary                 JSONB,              -- FieldType::LocalizedText → {"en":"…","uk":"…"}
    meta                    JSONB               -- FieldType::Json → flat PrimitiveValue map
);
```

For `DocumentKind::SingleType`, one additional column + unique constraint:
```sql
-- Added only for SingleType tables (e.g. "settings")
CREATE TABLE IF NOT EXISTS settings (
    id UUID PRIMARY KEY,
    -- … system columns …
    -- … user field columns …

    -- Singleton guard: enforces at most one row
    _singleton BOOLEAN NOT NULL DEFAULT TRUE CHECK (_singleton = TRUE)
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_settings_singleton ON settings (_singleton);
```

This replaces the application-level guard from ADR-002 Option C at the DB layer for the
per-type table approach. The application service guard (read-before-write) is still kept
as the first line of defence (fast fail before hitting the DB).

---

## FieldType → PostgreSQL Column Type Mapping

| `FieldType` | PostgreSQL column type | Notes |
|---|---|---|
| `Uid` | `VARCHAR(255)` | Pattern-constrained slug |
| `Uuid` | `UUID` | |
| `Text` | `TEXT` | |
| `Email` | `VARCHAR(320)` | Max RFC 5321 email length |
| `Url` | `TEXT` | |
| `Integer(i16)` | `SMALLINT` | |
| `Integer(i32)` | `INTEGER` | |
| `Integer(i64)` | `BIGINT` | |
| `Decimal { p, s }` | `NUMERIC(p, s)` | p and s from field definition |
| `Date` | `DATE` | |
| `DateTime` | `TIMESTAMPTZ` | Always UTC |
| `Boolean` | `BOOLEAN` | |
| `LocalizedText` | `JSONB` | `{"en": "text", "uk": "text"}` |
| `Json` | `JSONB` | Flat `{key: PrimitiveValue}` map |

All field columns are `NULL`-able by default; `required: true` adds `NOT NULL`.
`unique: true` adds `UNIQUE` to the column.

---

## Column Naming and Reserved Names

Field column name = `AttributeId` value (already a validated slug from `nutype`).

**Reserved column names** — forbidden as `AttributeId` values; validated at schema load:

```
id, publication_status, last_published_revision, published_at, published_by,
version, created_at, created_by, updated_at, updated_by, _singleton
```

The `SchemaRegistry` validation step rejects any `FieldDefinition` whose `AttributeId`
matches a reserved name, aborting startup with a clear error.

---

## `document_snapshots` — Shared Immutable Snapshot Table

Snapshots are kept in a single shared table (not per-type) because they are immutable
JSONB records — no need for typed columns on historical data.

```sql
CREATE TABLE IF NOT EXISTS document_snapshots (
    id           UUID PRIMARY KEY,          -- SnapshotId (Uuid v7, generated at publish time)
    type_name    VARCHAR(255) NOT NULL,      -- DocumentTypeInfo.plural_name (for human readability)
    instance_id  UUID NOT NULL,             -- DocumentInstanceId (FK handled at app level)
    revision     INTEGER NOT NULL CHECK (revision >= 1),
    published_at TIMESTAMPTZ NOT NULL,
    published_by VARCHAR(255),
    snapshot_data JSONB NOT NULL,
    UNIQUE (instance_id, revision)
);
CREATE INDEX ON document_snapshots (instance_id);
```

`snapshot_data` format (same as before):
```json
{
  "fields": {
    "title":   { "type": "Text",          "value": "Hello World" },
    "summary": { "type": "LocalizedText", "value": { "en": "…", "uk": "…" } },
    "count":   { "type": "Integer",       "value": 42 }
  }
}
```

No FK constraint from `document_snapshots.instance_id` to the per-type table — the type name
varies and SQL cross-table FK to a dynamic table name is impractical. Referential integrity
is enforced at the application service level (delete snapshots before deleting instance).

---

## Schema Loader — Dynamic DDL at Startup

The schema loader (ADR-006) is extended to perform DDL in addition to data upsert:

```
infrastructure/src/schema_loader.rs — startup sequence:

1. Parse and validate JSON schema files
2. For each DocumentType:
   a. Generate CREATE TABLE IF NOT EXISTS SQL from field definitions
   b. Execute DDL (outside any transaction — DSQL constraint, ADR-006 notes)
   c. If table already exists: compare existing columns to current schema via information_schema.columns
      - New fields → ALTER TABLE … ADD COLUMN …
      - Removed fields → no automatic DROP (safety); log a warning
      - Changed field types → abort startup with error (requires manual migration)
   d. For SingleType: ensure _singleton column and unique index exist
3. Execute CREATE TABLE IF NOT EXISTS for document_snapshots (once, not per-type)
4. Cache SchemaRegistry in Arc<SchemaRegistry>
```

**Schema evolution rules** (enforced at startup):
- ✅ Adding a nullable field → safe (ALTER TABLE ADD COLUMN)
- ✅ Adding a NOT NULL field with a default → safe
- ❌ Removing a field → startup warning only; manual DROP COLUMN required
- ❌ Changing a field type → startup abort; manual migration required

---

## Domain Model Impact

### Simpler `DocumentInstance` assembly

No multi-table join or EAV aggregation. Loading an instance:
```sql
SELECT * FROM articles WHERE id = $1;
```
One query, one row, typed columns map directly to `ContentValue`.

### `DatabaseRowId` unified with `DocumentInstanceId`

Confirmed: one row per instance in the per-type table. `db_row_id: Option<DocumentInstanceId>`
(`None` = not yet persisted, `Some(id)` = PK in the type's table).

### Updated Repository Trait

```rust
// domain/src/ports/document_instance_repository.rs
#[async_trait]
pub trait DocumentInstanceRepository {
    async fn find_by_id(&self, id: DocumentInstanceId) -> Result<Option<DocumentInstance>>;
    async fn find_by_type(
        &self,
        type_id: DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,  // maps to WHERE attribute = value on typed columns
    ) -> Result<Page<DocumentInstance>>;
    async fn save(&self, instance: &DocumentInstance) -> Result<()>;
    async fn delete(&self, id: DocumentInstanceId) -> Result<()>;
    async fn exists_for_type(&self, type_id: DocumentTypeId) -> Result<bool>; // SingleType guard
}
```

`FieldFilter` enables SQL-native filtering by field value — the primary motivation for per-type typed columns over JSONB.

### `PublishedSnapshot` Domain Entity (unchanged)

```rust
pub struct PublishedSnapshot {
    pub id:           SnapshotId,
    pub instance_id:  DocumentInstanceId,
    pub type_name:    String,
    pub revision:     u32,
    pub published_at: DateTime<Utc>,
    pub published_by: Option<UserId>,
    pub fields:       HashMap<AttributeId, ContentValue>,
}
```

---

## Consequences

### Pros
- **Readable schema**: each document type has its own named table and typed columns — visible in any DB client
- **Native SQL filtering**: `WHERE slug = 'my-post'` instead of EAV joins
- **No assembly complexity**: one row = one `DocumentInstance` — simple serialization
- **LocalizedText inline**: `JSONB` column in the same row — no join, no sub-table
- **Singleton enforcement is trivial**: one `UNIQUE` constraint on `_singleton`
- **No schema reference tables**: `document_types` / `relations` eliminated — JSON is the single source of truth

### Cons
- **Dynamic DDL at startup**: schema loader must generate and execute DDL — more complex loader
- **Schema evolution is explicit**: removing or changing fields requires manual migration step
- **No FK from snapshots to per-type tables**: referential integrity is application-level
- **Table proliferation**: one table per document type — manageable for typical CMS workloads (tens of types)

## Full Table Inventory

```
Per-type instance tables (DDL generated at startup from JSON schema):
  {plural_name}           ← one per DocumentType (e.g. articles, authors, settings)

Shared snapshot table (static migration):
  document_snapshots

Auth / system tables (static migrations):
  system_config
  roles
  role_permissions
  user_role_assignments
  access_requests
  shadow_users
```

## Follow-up Actions

- [x] Decisions made
- [ ] Implement `SchemaLoader::generate_table_ddl(doc_type: &DocumentType) -> String` in infrastructure
- [ ] Implement column-diff logic (existing columns vs. current schema) for schema evolution
- [ ] Implement `DocumentInstanceRepository` using dynamic table names (from `SchemaRegistry`)
- [ ] Implement EAV-free row ↔ `DocumentInstance` serialization
- [ ] Write static migrations for auth/system tables
- [ ] Validate `AttributeId` blacklist against reserved column names in `SchemaRegistry`
- [ ] Document schema evolution runbook in `CONTRIBUTING.md`
