# Data Model & Schema Specification

## 1. Database: AWS Aurora DSQL

AWS DSQL is a distributed, serverless relational database compatible with PostgreSQL.

### Key Characteristics & Invariants

- **No Auto-Increment / SERIAL**: Primary keys always use **UUID v7** (time-ordered, generated via `uuid::Uuid::now_v7()`).
- **Distributed Transactions & OCC**: Optimistic concurrency control; retry on conflict (`409 Conflict` / code `40001` serialization failure).
- **No DDL in Transactions**: All dynamic DDL migrations execute outside transaction blocks (`-- no-transaction` in static SQL, individual autocommit statements in dynamic DDL).
- **Idempotent DDL**: Use `CREATE TABLE IF NOT EXISTS`, `ADD COLUMN IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`.
- **Foreign Keys**: AWS Aurora DSQL natively supports foreign keys (`REFERENCES ... ON DELETE CASCADE`, etc.). Static system tables (`role_permissions`, `user_role_assignments`) and dynamic tables (`{table}__published`, `{owner}__{attr}_link`) enforce referential integrity with cascading deletes at the database level.

---

## 2. Core Domain Entities

All core entities live in `domain/src/entities/` and enforce invariants via validated `nutype` value objects in `domain/src/value_objects/`.

### 2.1. `DocumentType`
Defines the schema for content instances.
- **`id`**: `DocumentTypeId` (kebab-case, 2–64 characters, regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`).
- **`kind`**: `DocumentKind::Collection` or `DocumentKind::SingleType`.
- **`info`**:
  - `title`: Human-readable display title.
  - `singular_name`: String (matches `id`).
  - `plural_name`: String (used in collection API URLs).
  - `description`: Optional text.
- **`options`**: `draft_and_publish: bool`.
- **`fields`**: `IndexMap<AttributeId, FieldDefinition>` preserving declared attribute order.

### 2.2. `FieldDefinition` & `FieldConstraint`
Defines attributes of a `DocumentType`.
- **`id`**: `AttributeId` (kebab-case, regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`).
- **`field_type`**:
  - `Primitive(PrimitiveType)`: `Text`, `Uid`, `Uuid`, `Integer(IntegerSize)` (I16, I32, I64), `Decimal { precision, scale }`, `Date`, `DateTime`, `Boolean`.
  - `Email`
  - `Url`
  - `LocalizedText` (supports multiple locales as configured in `SystemConfig`)
  - `Json`
- **`required`**: `bool`.
- **`unique`**: `bool`.
- **`constraints`**: Vector of `FieldConstraint`:
  - `MinLength(usize)` / `MaxLength(usize)`: Applicable to `Text`, `Uid`, and `LocalizedText` (evaluated per-locale on character count `chars().count()`). *Note: `Email` and `Url` are self-validating `nutype` value objects; length and pattern constraints are inapplicable.*
  - `Pattern(String)`: Regex matching for string-like fields.
  - `MinInteger(i64)` / `MaxInteger(i64)`: For `Integer` fields.
  - `MinDecimal(Decimal)` / `MaxDecimal(Decimal)`: For `Decimal` fields.

### 2.3. `Relation`
First-class domain entity modeling relationships between document types (ADR-004, ADR-009).
- **`id`**: `RelationId` (`Uuid`).
- **`owner_type`**: `DocumentTypeId`.
- **`owner_attr`**: `AttributeId`.
- **`owner_kind`**: `OwnerRelationKind::HasOne` or `OwnerRelationKind::HasMany`.
- **`target_type`**: `DocumentTypeId`.
- **`inverse`**: `Option<RelationInverse>` with `inverse_attr: AttributeId` for bidirectional relations.

### 2.4. `DocumentInstance` & Publication Model
- **`id`**: `DocumentInstanceId` (`Uuid` v7).
- **`document_type_id`**: `DocumentTypeId`.
- **`owner_id`**: `UserId`.
- **`version`**: `i64` (OCC counter).
- **`publication_state`**: `PublicationState::Draft { last_published_revision }` or `PublicationState::Published { revision }`.
- **`content`**: `DocumentContent` (`fields: HashMap<AttributeId, ContentValue>`).
- **Publication Architecture (Two-Table Model)**:
  - Working drafts reside in the primary entity table `{table}`.
  - When published, the active revision is stored in `{table}__published` with `id UUID PRIMARY KEY REFERENCES {table}(id) ON DELETE CASCADE`.
  - Exactly at most one row per instance is stored in `{table}__published` (fast GET queries without joins).
  - Deleting the instance from `{table}` cascades and deletes the published mirror row automatically.

### 2.5. RBAC & Auth Entities
- **`Role`**: Builtin (`admin`, `editor`, `viewer`) and custom roles with permissions (`resource:action`).
- **`UserRoleAssignment`**: Maps `UserId` to `RoleId`.
- **`AccessRequest`**: Self-service role request workflow (`Pending`, `Approved`, `Rejected`).
- **`ShadowUser`**: Fast local caching of IAM/OIDC authenticated users.

---

## 3. Physical Database Schema

### 3.1. System Tables (Static SQL Migrations)
Created via `infrastructure/migrations/`:
- `roles`: RBAC role definitions (`admin`, `editor`, `viewer`, custom).
- `role_permissions`: Granular and wildcard permission grants (`REFERENCES roles(id) ON DELETE CASCADE`).
- `user_role_assignments`: OIDC identity to role mapping (`REFERENCES roles(id) ON DELETE CASCADE`).
- `access_requests`: Self-service user onboarding queue.
- `shadow_users`: Local cache of verified OIDC identities.

### 3.2. Dynamic User Tables & Naming Strategy
Derived dynamically from `SchemaRegistry` (ADR-008, ADR-009):
- **Collections**: Plural name converted to snake_case $\rightarrow$ e.g. `blog-articles` $\rightarrow$ `blog_articles`.
- **Singletons**: Singular name converted to snake_case $\rightarrow$ e.g. `site-setting` $\rightarrow$ `site_setting`.
  - Singletons enforce a single row invariant via a `_singleton BOOLEAN NOT NULL DEFAULT TRUE UNIQUE` column and index.
- **Attributes / Columns**: Attribute IDs converted from kebab-case to snake_case $\rightarrow$ e.g. `header-image` $\rightarrow$ `header_image`.
- **Standard Audit Columns**: Present on all generated document tables:
  ```sql
  id UUID PRIMARY KEY,
  version BIGINT NOT NULL DEFAULT 1,
  owner_id VARCHAR(255) NOT NULL,
  publication_state VARCHAR(50) NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
  ```
- **Published Mirror Tables (`{table}__published`)**:
  Generated when `draft_and_publish: true`:
  ```sql
  CREATE TABLE {table}__published (
      id UUID PRIMARY KEY REFERENCES {table}(id) ON DELETE CASCADE,
      published_version BIGINT NOT NULL,
      owner_id VARCHAR(255) NOT NULL,
      created_at TIMESTAMPTZ NOT NULL,
      updated_at TIMESTAMPTZ NOT NULL,
      published_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
      published_by VARCHAR(255),
      -- typed attribute columns matching {table} ...
  );
  ```
- **Universal Relation Link Tables (`{owner}__{attr}_link`)**:
  All relations (`HasOne` and `HasMany`) use a dedicated link table. No relation foreign key columns are added to entity tables.
  ```sql
  CREATE TABLE {owner_table}__{owner_attr}_link (
      owner_id UUID NOT NULL REFERENCES {owner_table}(id) ON DELETE CASCADE,
      target_id UUID NOT NULL REFERENCES {target_table}(id) ON DELETE CASCADE,
      PRIMARY KEY (owner_id, target_id)
  );
  -- If HasOne: enforce single target per owner instance:
  CREATE UNIQUE INDEX uq_{owner_table}__{owner_attr}_link_owner ON {owner_table}__{owner_attr}_link (owner_id);
  -- In all cases: fast reverse lookups:
  CREATE INDEX idx_{owner_table}__{owner_attr}_link_target ON {owner_table}__{owner_attr}_link (target_id);
  ```
  *Key Advantage*: Changing a relation from `HasOne` to `HasMany` requires zero DDL changes to the link table itself—only dropping the unique index on `owner_id`.*

---

## 4. JSON Schema File Specification

Dynamic schema definitions are loaded from declarative JSON files.

### 4.1. Directory Structure

```
schema/
├── document-types/
│   ├── article.json
│   ├── category.json
│   └── site-setting.json
├── relations/
│   ├── article-author.json
│   └── article-categories.json
└── system-config.json
```

### 4.2. File Name Invariant
- For document types: the file name **MUST strictly match** the document type's `singularName` in kebab-case plus `.json`:
  `schema/document-types/{singularName}.json`
  Example: `singularName: "blog-post"` $\rightarrow$ `schema/document-types/blog-post.json`.
- A loader validation error is returned if the file stem does not equal `info.singularName`.

### 4.3. Document Type File Format (`schema/document-types/{singular_name}.json`)

Attributes preserve order through an ordered map (`IndexMap`):

```json
{
  "kind": "collection",
  "info": {
    "singularName": "article",
    "pluralName": "articles",
    "displayName": "Blog Article",
    "description": "Articles published on the company blog"
  },
  "options": {
    "draftAndPublish": true
  },
  "attributes": {
    "title": {
      "type": "text",
      "required": true,
      "unique": false,
      "constraints": [
        { "minLength": 5 },
        { "maxLength": 120 }
      ]
    },
    "slug": {
      "type": "uid",
      "required": true,
      "unique": true,
      "constraints": [
        { "pattern": "^[a-z0-9-]+$" }
      ]
    },
    "content": {
      "type": "localizedText",
      "required": false,
      "constraints": [
        { "minLength": 10 }
      ]
    },
    "view-count": {
      "type": { "integer": "int64" },
      "required": false,
      "constraints": [
        { "min": 0 }
      ]
    },
    "rating": {
      "type": { "decimal": { "precision": 3, "scale": 2 } },
      "required": false,
      "constraints": [
        { "min": "0.00" },
        { "max": "5.00" }
      ]
    },
    "contact-email": {
      "type": "email",
      "required": false
    },
    "metadata": {
      "type": "json",
      "required": false
    }
  }
}
```

#### Field Types Support Matrix

| Type in JSON | Internal `FieldType` | Notes |
|---|---|---|
| `"text"` | `Primitive(Text)` | Arbitrary string |
| `"uid"` | `Primitive(Uid)` | Unique slug/identifier |
| `"boolean"` | `Primitive(Boolean)` | True / False |
| `"date"` | `Primitive(Date)` | ISO Date (`YYYY-MM-DD`) |
| `"datetime"` | `Primitive(DateTime)` | ISO 8601 Timestamp with UTC |
| `"uuid"` | `Primitive(Uuid)` | UUID scalar |
| `"email"` | `Email` | Validated email address (self-validating `nutype`; length/pattern constraints inapplicable) |
| `"url"` | `Url` | Validated URL string (self-validating `nutype`; length/pattern constraints inapplicable) |
| `"localizedText"` | `LocalizedText` | Per-locale text map (`en`, `uk`, etc.) |
| `"json"` | `Json` | Semi-structured JSON object |
| `"integer"` / `"int32"` / `{"integer": "int32"}` | `Primitive(Integer(I32))` | 32-bit signed integer |
| `"int16"` / `{"integer": "int16"}` | `Primitive(Integer(I16))` | 16-bit signed integer |
| `"int64"` / `{"integer": "int64"}` | `Primitive(Integer(I64))` | 64-bit signed integer |
| `{"decimal": {"precision": 10, "scale": 2}}` | `Primitive(Decimal)` | Exact decimal representation |

#### Constraints Mapping

JSON constraints support clean shorthands and synonyms:
- String constraints: `{ "minLength": 5 }` (aliases: `minimalLength`, `min`), `{ "maxLength": 100 }` (aliases: `maximalLength`, `max`), `{ "pattern": "^[a-z]+$" }` (alias: `regex`).
- Numeric constraints: `{ "min": 0 }` (integer or decimal string), `{ "max": 100 }`.

### 4.4. Relation File Format (`schema/relations/{relation_name}.json`)

```json
{
  "id": "018f3a2b-1234-7000-8000-000000000001",
  "ownerType": "article",
  "ownerAttr": "categories",
  "ownerKind": "hasMany",
  "targetType": "category",
  "inverse": {
    "inverseAttr": "articles"
  }
}
```
*Note: If `id` is omitted, the loader deterministically derives a UUID v5 from `"{ownerType}:{ownerAttr}"`.*

### 4.5. System Config File Format (`schema/system-config.json`)

```json
{
  "id": "018f3a2b-0000-7000-8000-000000000000",
  "locales": ["en", "uk", "de", "fr"],
  "defaultLocale": "en"
}
```
