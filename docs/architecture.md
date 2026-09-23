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
- **Allowed deps**: `serde` (for serialisation traits only), `thiserror`, `uuid`, `chrono`

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
- `infrastructure` provides two concrete implementations: `SqlxPgRepository` (standard PG) and `DsqlRepository` (DSQL with IAM token rotation); selected via config at startup
- Migrations use plain SQL; avoid DSQL-incompatible DDL (see research note) so the same migration files run on both standard PG and DSQL
- The binary is configured entirely through environment variables — no AWS SDK calls outside `infrastructure`

## UI (TBD)

The UI layer is **not decided yet**. Options under investigation:
- Separate repository deployed independently on AWS (React or similar)
- Internal UI crate in this workspace (Leptos / Yew / HTMX)

**Do not** add UI dependencies to any existing crate until this decision is recorded in an ADR.

## Related ADRs

- [ADR-001 — Hexagonal Architecture with three-crate workspace](./adr/ADR-001-hexagonal-architecture.md)
