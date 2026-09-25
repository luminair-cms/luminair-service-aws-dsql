# Architecture

## Overview

Luminair backend follows **Hexagonal Architecture** (Ports & Adapters) combined with **Domain-Driven Design (DDD)**.
The codebase is a Cargo workspace with three crates organised by layer:

```
luminair-service-aws-dsql/
├── Cargo.toml                 # workspace root
├── domain/                    # pure business logic — no I/O, no frameworks
├── application/               # use-cases / orchestration
└── infrastructure/            # adapters: HTTP, DB (AWS DSQL), external services
```

## Crate Responsibilities

### `domain`

- **Contains**: entities, value objects, aggregates, domain events, repository *traits* (ports), domain errors
- **Must NOT depend on**: `application`, `infrastructure`, any I/O crate (tokio, sqlx, axum …)
- **Allowed deps**: `serde` (for serialisation traits only), `thiserror`, `uuid`, `chrono`, `nutype` (for domain value objects), `email_address` and `url` (for validation predicates), `indexmap` (for preserving declared attribute order), `rust_decimal` (for exact decimal representation)

### `application`

- **Contains**: use-case structs, application services, command/query handlers (CQRS optional), application errors
- **Depends on**: `domain` only
- **Must NOT depend on**: `infrastructure`, `axum`, `sqlx`, or any adapter

### `infrastructure`

- **Contains**: HTTP handlers (axum), repository implementations (sqlx + AWS DSQL), schema migration, config loading, DI wiring
- **Depends on**: `domain`, `application`
- **This is the composition root** — the binary lives here (`infrastructure/src/main.rs`)

## Dependency Rule

```
infrastructure → application → domain
                             ↑
            (only domain traits cross this boundary)
```

No reverse dependencies. Domain traits (repository ports) are defined in `domain` and implemented in `infrastructure`.

## Key Patterns

| Pattern | Where used |
|---|---|
| Repository trait | `domain` (definition) / `infrastructure` (impl) |
| Command / Query | `application` — separate structs, no mixing |
| Domain Event | `domain` — raised by aggregates, handled in `application` |
| Error types | Each crate has its own; `infrastructure` maps all to HTTP status codes |
| Schema-driven | Content types defined via JSON schema; `domain` owns the schema entity |

## Deployment Environments

The service is **designed for AWS** (Aurora DSQL, ECS / Lambda, IAM auth) but intentionally portable.
AWS-specific code is confined entirely to the `infrastructure` crate; the `domain` and `application`
crates have no knowledge of any deployment environment.

| Environment | Database | How to run |
|---|---|---|
| Local / dev | PostgreSQL (Docker) | `DATABASE_URL=postgres://...` in `.env` |
| CI | PostgreSQL | `#[sqlx::test]` auto-provisions |
| Docker / K8s (self-hosted) | PostgreSQL | Same binary; standard pg connection string |
| Staging / prod | AWS Aurora DSQL | IAM token auth; see [`docs/research/dsql-vs-postgres.md`](./research/dsql-vs-postgres.md) |

### Portability contract

- Repository traits are defined in `domain` — they are database-agnostic
- `infrastructure` provides a single unified set of repository implementations (`SqlxDocumentInstanceRepository`, `SqlxRoleRepository`, etc.) targeting `sqlx::PgPool`
- Database differences are isolated in the connection pool factory at startup:
  - Local / CI / K8s: Standard PostgreSQL connection pool using `DATABASE_URL`
  - AWS Aurora DSQL: Dynamic connection pool with IAM authentication token rotation (15-minute token lifecycle)
- Migrations use plain SQL and `sea-query`; avoid DSQL-incompatible DDL (see research note and ADR-009) so schemas run identically on standard PG and DSQL
- The binary is configured entirely through environment variables — no AWS SDK calls outside `infrastructure`

## UI Layer (Admin Dashboard)

The UI layer is an independent **Decoupled Single-Page Application (SPA)** adhering strictly to Hexagonal Architecture boundaries:
- **Location**: Isolated `/frontend` directory in the repository (independent package management, zero impact on Cargo workspace).
- **Technology Stack**: React 19, TypeScript, Mantine v7 design system (`@mantine/*`), Zustand for global client state, TanStack Router for type-safe routing, and TanStack Query v5 for server state.
- **Dynamic Schema Forms**: Queries `/api/schema/document-types` at runtime to generate forms for all 12 field types, multi-locale editing tabs (`LocalizedText`), and relational association pickers (`HasOne` and `HasMany`).
- **Authentication**: OIDC Authorization Code Flow with PKCE (`oidc-client-ts`). Uses Dex for local development (~15MB RAM via Docker Compose) and AWS Cognito in production.
- **Deployment**: Static assets hosted on AWS S3 behind an Amazon CloudFront CDN distribution with Origin Access Control (OAC). CloudFront routes `/*` to S3 and `/api/*` to the Axum backend, eliminating CORS preflight overhead in production.
- **Specification**: See [`docs/ui-architecture.md`](./ui-architecture.md).

## Related ADRs

- [ADR-001 — Hexagonal Architecture with three-crate workspace](./adr/ADR-001-hexagonal-architecture.md)
- [ADR-005 — Authentication and Authorization Strategy](./adr/ADR-005-auth-strategy.md)
- [ADR-008 — Unified Naming Conventions and REST Routing Strategy](./adr/ADR-008-naming-conventions-and-routing.md)
- [ADR-009 — Dynamic Schema Migration and Relation Persistence](./adr/ADR-009-dynamic-schema-migration-and-relation-persistence.md)
- [ADR-010 — Admin Dashboard UI Architecture & Implementation Strategy](./adr/ADR-010-ui-architecture.md)

