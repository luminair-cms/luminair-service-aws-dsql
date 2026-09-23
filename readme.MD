# Luminair Backend

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.

## Deployment

**Primary target**: AWS (Aurora DSQL, ECS / Lambda, IAM auth).

**Also works** in any local, Docker, or Kubernetes environment with a standard PostgreSQL database —
no AWS-specific code leaks into the `domain` or `application` layers.
AWS-specific adapters (DSQL IAM token refresh, etc.) are isolated in the `infrastructure` crate
and are swappable without touching business logic.

| Environment | Database | Notes |
|---|---|---|
| Local / dev | PostgreSQL (Docker) | `DATABASE_URL=postgres://...` |
| CI | PostgreSQL | `#[sqlx::test]` provisions automatically |
| Staging / prod | AWS Aurora DSQL | IAM token auth; UUID v7 PKs required |
| Self-hosted (K8s) | PostgreSQL | Same binary, standard pg connection string |

## Documentation & Planning

- [Implementation Roadmap](docs/roadmap.md) — complete milestone plan from domain model to UI
- [Architecture Guide](docs/architecture.md) — hexagonal boundaries and crate responsibilities
- [Data Model & DSQL](docs/data-model.md) — persistence schema and DSQL nuances
- [REST API Specifications](docs/api.md) — endpoint surface and conventions

