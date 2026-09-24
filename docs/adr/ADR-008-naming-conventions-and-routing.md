# ADR-008: Unified Identifier Naming Conventions, Validation Rules, and REST Routing Strategy

- **Status**: `Accepted`
- **Date**: 2026-09-24
- **Deciders**: Dmitri Astafiev
- **Related ADRs**: [ADR-002](./ADR-002-singleton-enforcement.md), [ADR-006](./ADR-006-schema-loading.md), [ADR-007](./ADR-007-persistence-model.md)

---

## Context

Headless CMS architectures frequently suffer from naming convention fragmentation. In systems like Strapi, developers routinely encounter mixed conventions in the same project:
- Document types: `singularName = "partner-booking-category"` (`kebab-case`), `pluralName = "partner-booking-categories"` (`kebab-case`)
- Component & field keys: `titleHeader` (`camelCase`)
- Relation inverses: `inversedBy = "partner_booking_category"` (`snake_case`)
- Database tables: `partner_booking_categories` (`snake_case`)

This inconsistent mixing of hyphens (`-`) and underscores (`_`) causes developer confusion, subtle mapping bugs, and friction in client SDKs.

Furthermore, `DocumentTypeId` was originally defined as an opaque `UUID v7`. In [ADR-006](./ADR-006-schema-loading.md), this forced schemas in `schema/document-types/*.json` to declare arbitrary, hand-assigned UUIDs, disconnecting the technical type identifier from the human-readable schema name.

Finally, the REST API must provide clear, ergonomic conventions for both **Collections** (0 to $N$ entries) and **SingleTypes / Singletons** (exactly 0 or 1 entry, e.g. `homepage`, `site-settings`).

---

## Decision Drivers

1. **Absolute Consistency**: Zero tolerance for mixing `-` and `_` within schema declarations, domain identifiers, and API payloads.
2. **Readability & Ergonomics**: `DocumentTypeId` should be human-readable and directly derived from the document's `singular_name`.
3. **Clean REST URLs**: Collections use plural names; Singletons use clean singular URLs without redundant instance UUIDs in the path.
4. **Collision Safety**: Guarantee that no Collection route can collide with a Singleton route.
5. **SQL Parity**: Deterministic, transparent mapping between domain identifiers and PostgreSQL / AWS DSQL identifiers.

---

## Decisions

### 1. `DocumentTypeId` is `DocumentTypeId(String)` Derived from `singular_name`

`DocumentTypeId` is no longer a UUID. It is a validated string newtype (`nutype`) that is identical to the document type's `singular_name`:
- Document Type: `partner-booking-category`
- `DocumentTypeId`: `"partner-booking-category"`
- `singular_name`: `"partner-booking-category"`
- `plural_name`: `"partner-booking-categories"`
- File path: `schema/document-types/partner-booking-category.json`

Schema JSON files no longer declare an arbitrary `id: "01932c4a-..."` UUID. The identity is the `singular_name` itself.

---

### 2. Strict Domain & Schema Kebab-Case Standard

Across the entire domain, schema configuration, and API surface, **all user-defined identifiers must be strictly lowercase kebab-case**:

```regex
^[a-z][a-z0-9]*(-[a-z0-9]+)*$
```

#### Rules:
1. **Characters**: Lowercase ASCII letters (`a-z`), digits (`0-9`), and single hyphens (`-`).
2. **Forbidden**: Underscores (`_`), uppercase letters (`A-Z`), and whitespace are strictly rejected.
3. **Boundaries**: Must begin with a letter (`[a-z]`) and end with an alphanumeric character (`[a-z0-9]`).
4. **No Double Hyphens**: Consecutive hyphens (`--`) are rejected.
5. **Length**: Minimum 2 characters, maximum 64 characters.

#### Scope of Application:
- `DocumentTypeId` & `singular_name` (e.g. `article`, `partner-booking-category`)
- `plural_name` (e.g. `articles`, `partner-booking-categories`)
- `AttributeId` / Field keys (e.g. `title`, `slug`, `title-header`, `published-at`)
- Relation attributes (`owner_attr`, `inverse_attr`)
- Relation IDs (`RelationId`)

---

### 3. Startup Anti-Collision & Invariant Validations

During schema loading at startup (`infrastructure::schema_loader`):
1. **Singular != Plural**: For every type, `info.singular_name != info.plural_name` must hold.
2. **Cross-Kind Anti-Collision**: No Collection type's `plural_name` may equal any Singleton type's `singular_name`. (e.g. if a Singleton is named `news`, no Collection can have `plural_name = "news"`).
3. **Reserved Identifiers Blacklist**: The following words cannot be used as document type names or attribute names:
   ```
   id, version, status, audit, publication, system, schema, admin,
   singletons, collections, content, metadata
   ```

---

### 4. REST API Routing Strategy

We adopt **Clean REST URLs** with distinct conventions for Collections, Singletons, and Schema Introspection:

#### A. Schema Introspection Endpoints
- `GET /api/schema/document-types` — Lists all registered document type definitions.
- `GET /api/schema/document-types/{id}` — Fetches schema definition for a specific type, where `{id}` is `singular_name` (e.g. `/api/schema/document-types/partner-booking-category`).

#### B. Collection Types (`kind = Collection`)
Collection endpoints always use the `{plural_name}`:
| Method | Path | Description |
|---|---|---|
| `GET` | `/api/{plural_name}` | List entries (paginated, filterable, sortable) |
| `POST` | `/api/{plural_name}` | Create draft entry |
| `GET` | `/api/{plural_name}/{id}` | Get entry by instance UUID |
| `PUT` | `/api/{plural_name}/{id}` | Update entry fields |
| `DELETE` | `/api/{plural_name}/{id}` | Delete entry and cascade snapshots |
| `POST` | `/api/{plural_name}/{id}/publish` | Publish entry to new snapshot |
| `POST` | `/api/{plural_name}/{id}/unpublish` | Revert entry to draft |
| `GET` | `/api/{plural_name}/{id}/snapshots` | List snapshot revision history |

#### C. Single Types / Singletons (`kind = SingleType`)
Singletons use the clean `{singular_name}`. Because a Singleton has at most one instance, **no instance UUID appears in the URL path**:
| Method | Path | Description |
|---|---|---|
| `GET` | `/api/{singular_name}` | Get the singleton entry directly |
| `PUT` | `/api/{singular_name}` | Upsert / update the singleton entry |
| `DELETE` | `/api/{singular_name}` | Clear / delete the singleton entry |
| `POST` | `/api/{singular_name}/publish` | Publish singleton to new snapshot |
| `POST` | `/api/{singular_name}/unpublish` | Revert singleton to draft |
| `GET` | `/api/{singular_name}/snapshots` | List singleton revision history |

*Response Envelope*:
- Collection lists return `{"data": [...], "meta": {"pagination": {...}}}`.
- Singletons return `{"data": {...}}` directly without pagination metadata.

---

### 5. Database Persistence Mapping (PostgreSQL & AWS DSQL)

In PostgreSQL and AWS DSQL, unquoted identifiers cannot contain hyphens. The persistence layer (`infrastructure::repositories`) performs a deterministic 1:1 conversion:

| Layer | Convention | Example |
|---|---|---|
| **Domain / Schema / REST** | `kebab-case` | `partner-booking-categories`, `title-header` |
| **SQL Table / Column** | `snake_case` | `partner_booking_categories`, `title_header` |

- **Table names**: `plural_name.replace('-', '_')` (e.g. `articles`, `partner_booking_categories`, `site_settings`).
- **Column names**: `attribute_id.replace('-', '_')` (e.g. `title_header`, `view_count`).
- **Snapshot type names**: Stored as the canonical kebab-case `plural_name` in `document_snapshots.type_name`.

---

## Consequences

### Positive
- **Eliminates Mixed Casing**: No more guessing between `-` and `_`. Kebab-case everywhere in schemas and API.
- **Human-Readable Identifiers**: `DocumentTypeId` is `partner-booking-category`, not an opaque UUID.
- **Clean REST Endpoints**: Singletons don't force clients to query an ID before updating (`PUT /api/homepage` just works).
- **Safe from Collision**: Startup validation guarantees collection plurals never collide with singleton singulars.
- **SQL Idiomatic**: Automatic translation to `snake_case` ensures clean, unquoted SQL queries in DSQL and PostgreSQL.

### Trade-offs / Adjustments Required
- `DocumentTypeId` in `domain` must be converted from `nutype(Uuid)` to `nutype(String)` with kebab-case validation.
- Unit and integration tests that generated random UUIDs for document types will now use clean semantic slugs (e.g. `"article"`, `"author"`).
- `role_permissions.document_type_id` in SQL becomes `VARCHAR(64)` instead of `UUID`.

---

## Follow-up Actions

- [x] Record decision in ADR-008.
- [ ] Update `docs/api.md` with singleton routing and kebab-case schema endpoints.
- [ ] Log entry in `.ai/context/decisions.md`.
- [ ] Update `domain/src/value_objects/ids.rs` (`DocumentTypeId` and `AttributeId`).
- [ ] Update `domain/src/entities/document_type.rs` validation.
- [ ] Update `application` commands and services to take string `DocumentTypeId`.
- [ ] Update `plan_3a_static_sql_migrations.md` (`role_permissions.document_type_id` as `VARCHAR(64)`).
