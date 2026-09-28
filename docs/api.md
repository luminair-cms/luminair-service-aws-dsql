# REST API

Luminair exposes a schema-driven REST API discovered at runtime.

## Conventions

- **Base path**: `/api`
- **Naming format**: Strictly `kebab-case` (`^[a-z][a-z0-9]*(-[a-z0-9]+)*$`) for all document type names, attributes, and query parameters.
- **Headers**: `Content-Type: application/json`, `Accept: application/json`
- **Errors**: RFC 9457 Problem Details (`application/problem+json`)
- **Pagination**: `?page=1&page_size=25`
- **Filtering**: `?filters[field][$eq]=value`
- **Populate (Relations)**: `?populate=tags,category`

---

## 1. Schema Introspection Endpoints

Endpoints for inspecting active document types loaded from `schema/`:

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/schema/document-types` | List all registered document types |
| `GET` | `/api/schema/document-types/{id}` | Get detailed schema for a type (`{id}` is `singular_name`) |

---

## 2. Collection Endpoints (`kind = Collection`)

Collection types manage 0 to $N$ document instances and use the `{plural_name}`:

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/{plural_name}` | List entries (paginated, filterable, sortable) |
| `POST` | `/api/{plural_name}` | Create a new draft entry |
| `GET` | `/api/{plural_name}/{id}` | Get single entry by its UUID v7 |
| `PUT` | `/api/{plural_name}/{id}` | Update entry fields |
| `DELETE` | `/api/{plural_name}/{id}` | Delete entry (cascades snapshots) |
| `POST` | `/api/{plural_name}/{id}/publish` | Publish draft to an immutable snapshot |
| `POST` | `/api/{plural_name}/{id}/unpublish` | Revert entry to draft state |
| `GET` | `/api/{plural_name}/{id}/snapshots` | List revision history / published snapshots |

---

## 3. Single Type Endpoints (`kind = SingleType`)

Single types (e.g. `homepage`, `site-settings`) represent unique singletons with at most one instance. They use the clean `{singular_name}` without redundant UUIDs in the path:

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/{singular_name}` | Get the singleton entry directly |
| `PUT` | `/api/{singular_name}` | Upsert / update singleton fields |
| `DELETE` | `/api/{singular_name}` | Delete / clear the singleton entry |
| `POST` | `/api/{singular_name}/publish` | Publish singleton to an immutable snapshot |
| `POST` | `/api/{singular_name}/unpublish` | Revert singleton to draft state |
| `GET` | `/api/{singular_name}/snapshots` | List singleton revision history |

---

## 4. Access Requests Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/access-requests` | Submit access request (OIDC user) |
| `GET` | `/api/admin/access-requests` | List pending access requests (admin only) |
| `POST` | `/api/admin/access-requests/{id}/approve` | Approve request & grant roles (admin only) |
| `POST` | `/api/admin/access-requests/{id}/reject` | Reject request with reason (admin only) |

---

## 5. System Health Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Liveness probe |
| `GET` | `/ready` | Readiness probe (DB connectivity) |

---

## 6. Response Envelopes

### Collection List Response
```json
{
  "data": [
    {
      "id": "01920000-0000-7000-8000-000000000001",
      "title": "Getting Started with Luminair",
      "slug": "getting-started",
      "publication-status": "published"
    }
  ],
  "meta": {
    "pagination": {
      "page": 1,
      "page_size": 25,
      "total": 1
    }
  }
}
```

### Single Resource Response (Item or Singleton)
```json
{
  "data": {
    "id": "01920000-0000-7000-8000-000000000001",
    "hero-title": "Welcome to Luminair",
    "publication-status": "published"
  }
}
```

### Error Response (RFC 9457)
```json
{
  "type": "https://luminair.io/errors/not-found",
  "title": "Resource Not Found",
  "status": 404,
  "detail": "Document instance with id '01920000-...' was not found"
}
```

---

## 7. Relational Mutations & Read-After-Write (ADR-011)

Luminair supports Strapi-compatible relational mutations on write commands (`POST` and `PUT`), with optional immediate read-after-write enrichment via `?populate=...`.

### Request Payloads

#### Explicit Action Objects
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

Supported actions:
- `connect`: Appends target UUIDs to existing relations without removing current links (`HasMany` or `HasOne` if currently empty).
- `disconnect`: Removes target UUIDs from existing relations.
- `set`: Replaces all relations for the attribute with the given target UUIDs.
- `unset`: `{"unset": true}` clears all relations for the attribute.

#### Shorthand Syntax
- `"category": "01920000-0000-7000-8000-000000000001"` -> Equivalent to `set: [uuid]`
- `"category": { "id": "01920000-0000-7000-8000-000000000001" }` -> Equivalent to `set: [uuid]`
- `"tags": ["0192...", "0192..."]` -> Equivalent to `set: [uuid1, uuid2]`
- `"category": null` -> Equivalent to `unset: true`

### Read-After-Write (`?populate=...`)

When creating (`POST /api/{plural_name}?populate=category,tags`) or updating (`PUT /api/{plural_name}/{id}?populate=tags`) documents, specifying `?populate` instructs the server to enrich and return the populated relational representation in the `201 Created` or `200 OK` response envelope in a single HTTP roundtrip.

---

> **AI agents**: update this file whenever you add, modify, or remove an endpoint.

