# REST API

Luminair exposes a **Strapi-like REST API** — content types are schema-driven and discovered at runtime.

## Conventions

- Base path: `/api`
- Content-type header: `application/json`
- Errors follow RFC 9457 Problem Details (`application/problem+json`)
- Pagination: `?page[offset]=0&page[limit]=25`
- Filtering: `?filters[field][$eq]=value` (Strapi v4 style)
- Sorting: `?sort=field:asc`

## Generic Collection Endpoints

For any registered content type `{plural}`:

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/{plural}` | List entries (paginated, filterable) |
| `POST` | `/api/{plural}` | Create entry |
| `GET` | `/api/{plural}/{id}` | Get single entry |
| `PUT` | `/api/{plural}/{id}` | Full update |
| `PATCH` | `/api/{plural}/{id}` | Partial update |
| `DELETE` | `/api/{plural}/{id}` | Delete entry |

## System Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Liveness probe |
| `GET` | `/ready` | Readiness probe (DB connectivity) |
| `GET` | `/api/content-types` | List registered schema definitions |

## Response Envelope

```json
{
  "data": { ... },
  "meta": {
    "pagination": { "page": 1, "pageSize": 25, "total": 100 }
  }
}
```

## Error Response

```json
{
  "type": "https://luminair.io/errors/not-found",
  "title": "Resource not found",
  "status": 404,
  "detail": "Entry with id '…' does not exist"
}
```

---

> **AI agents**: update this file whenever you add, modify, or remove an endpoint.
