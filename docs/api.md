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

> **AI agents**: update this file whenever you add, modify, or remove an endpoint.
