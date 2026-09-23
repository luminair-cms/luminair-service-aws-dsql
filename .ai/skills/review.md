# Skill: Code Review Checklist

Run this checklist before finalising any code change or opening a PR.
Work through each section; fix issues before proceeding.

## 1. Automated Gates (must all pass)

```bash
cargo clippy -- -D warnings
cargo test --workspace
cargo audit
```

## 2. Architecture Boundaries

- [ ] No `infrastructure` imports in `domain` or `application` crates
- [ ] No `sqlx` / `axum` / HTTP types in `domain`
- [ ] Repository traits defined in `domain`, implemented only in `infrastructure`
- [ ] No direct DB calls in `application` (go through repository trait)

## 3. Error Handling

- [ ] No `.unwrap()` or `.expect()` outside of tests
- [ ] All new error variants use `thiserror` and have meaningful messages
- [ ] HTTP error mapping stays in the infrastructure error middleware (not scattered across handlers)

## 4. API Surface

- [ ] If any endpoint was added/changed/removed → `docs/api.md` is updated
- [ ] Response envelope matches the standard format (see `docs/api.md`)
- [ ] Errors return `application/problem+json`

## 5. Data Model

- [ ] New primary keys use `Uuid::now_v7()` — no integer sequences
- [ ] Migrations in `infrastructure/migrations/` use timestamp prefix
- [ ] No DDL inside transactions

## 6. Security

- [ ] No secrets, tokens, or credentials in committed code or docs
- [ ] Inputs validated before reaching domain logic
- [ ] No SQL string interpolation — use `sqlx` query macros or parameterised queries

## 7. Tests

- [ ] New domain logic has unit tests
- [ ] New use-cases have unit tests with fake repositories
- [ ] New endpoints have at least one integration or E2E test
- [ ] See `.ai/skills/testing.md` for conventions

## 8. Documentation

- [ ] Public types and functions have doc comments (`///`)
- [ ] Architecture changes → new ADR in `docs/adr/`
- [ ] If a non-obvious decision was made, append to `.ai/context/decisions.md`
