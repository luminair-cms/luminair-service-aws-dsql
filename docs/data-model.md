# Data Model

## Database: AWS Aurora DSQL

AWS DSQL is a serverless, distributed SQL database compatible with PostgreSQL wire protocol.

### Key characteristics relevant to this project

- **No explicit sequences / SERIAL** — use `UUID v7` (time-ordered) for all primary keys
- **Distributed transactions** — optimistic concurrency; handle `409 Conflict` (OCC conflict) with retry in `infrastructure`
- **No DDL in transactions** — run migrations outside transaction blocks
- **Connection**: IAM-based token auth; token refreshed every 15 min (handle in connection pool config)

## Core Entities (Domain Layer)

> Fill in as domain is designed. Each entity lives in `domain/src/entities/`.

### `ContentType`

Represents a schema definition (analogous to a Strapi content type).

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` (v7) | PK |
| `name` | `String` | unique, kebab-case singular |
| `plural` | `String` | unique, used in API paths |
| `schema` | `JsonValue` | JSON Schema definition of fields |
| `created_at` | `DateTime<Utc>` | |
| `updated_at` | `DateTime<Utc>` | |

### `Entry`

A single record belonging to a `ContentType`.

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` (v7) | PK |
| `content_type_id` | `Uuid` | FK → ContentType |
| `data` | `JsonValue` | Validated against schema on write |
| `created_at` | `DateTime<Utc>` | |
| `updated_at` | `DateTime<Utc>` | |

## Migrations

- Tool: **sqlx-cli** (`sqlx migrate run`)
- Location: `infrastructure/migrations/`
- Naming: `{timestamp}_{description}.sql`
- Never write DDL migrations that create sequences or use `SERIAL`

## Repository Traits

Repository traits are defined in `domain/src/ports/` and implemented in `infrastructure/src/repositories/`.
See [`architecture.md`](./architecture.md) for the dependency rule.
