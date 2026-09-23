# GitHub Copilot Instructions

This is a **Rust** project using **Hexagonal Architecture** with three crates: `domain`, `application`, `infrastructure`.

## Read First

Before suggesting code, refer to:
- [`AGENTS.md`](../AGENTS.md) — workflow rules and skill loading guide
- [`docs/architecture.md`](../docs/architecture.md) — crate boundaries and dependency rules
- [`.ai/skills/rust-patterns.md`](../.ai/skills/rust-patterns.md) — project Rust conventions

## Critical Rules

- **Never** add `infrastructure` crate imports to `domain` or `application`
- **Always** use `Uuid::now_v7()` for new primary keys — no integers, no `Uuid::new_v4()`
- **Always** use `thiserror` for error types; never `.unwrap()` in non-test code
- **All code** must pass `cargo clippy -- -D warnings`
- Preferred crates: `axum`, `sqlx`, `tokio`, `serde`, `uuid`, `thiserror`, `tracing`

## When Adding an Endpoint

Update [`docs/api.md`](../docs/api.md) with the new route.

## When Writing Tests

Follow [`.ai/skills/testing.md`](../.ai/skills/testing.md) — use fake repositories in `application` tests,
`#[sqlx::test]` in `infrastructure` tests.
