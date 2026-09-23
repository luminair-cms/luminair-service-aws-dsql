# Project: Luminair Backend (AWS DSQL)

**One-paragraph summary for AI cold-start sessions.**

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.
It uses **Hexagonal Architecture** with three Cargo crates: `domain` (pure business logic, no I/O),
`application` (use-case orchestration), and `infrastructure` (axum HTTP handlers, sqlx repository
implementations, binary entry-point). **Primary deployment target is AWS** (Aurora DSQL, IAM auth),
but the service runs on any standard PostgreSQL environment (local, Docker, K8s) — AWS-specific
code is isolated in `infrastructure` only. All primary keys are **UUID v7** (no sequences).
UI is not yet decided. The canonical entry points for an AI agent are `AGENTS.md` (workflow rules)
and `docs/architecture.md` (design). Test strategy: unit tests with fake repos in `application`,
integration tests with `#[sqlx::test]` against standard PostgreSQL in `infrastructure`.
