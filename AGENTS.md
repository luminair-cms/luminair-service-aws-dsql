# AGENTS.md — AI Agent Entry Point

> This file is the **single entry point** for any AI coding agent working in this repository.
> Read it first, follow the links in order, then begin your task.

## 1. Understand the Project

Read in this order:
1. [`README.md`](./README.md) — what, why, quick-start
2. [`docs/roadmap.md`](./docs/roadmap.md) — complete milestone plan & dependencies
3. [`docs/architecture.md`](./docs/architecture.md) — crate layout, hexagonal architecture, DDD
4. [`docs/data-model.md`](./docs/data-model.md) — entities, schema, AWS DSQL specifics
5. [`docs/api.md`](./docs/api.md) — REST API surface and conventions

## 2. Load Skills Before Acting

| Task | Skill to load |
|---|---|
| Writing or modifying Rust code | [`.ai/skills/rust-patterns.md`](.ai/skills/rust-patterns.md) |
| Writing or modifying tests | [`.ai/skills/testing.md`](.ai/skills/testing.md) |
| Reviewing a PR / self-reviewing code | [`.ai/skills/review.md`](.ai/skills/review.md) |
| Investigating a topic / drafting an ADR | [`.ai/skills/adr-process.md`](.ai/skills/adr-process.md) |

## 3. Memory (Persistent Context)

- [`.ai/context/project.md`](.ai/context/project.md) — one-paragraph seed for cold-start sessions
- [`.ai/context/decisions.md`](.ai/context/decisions.md) — running log of key decisions (informal complement to ADRs)
- [`docs/research/`](./docs/research/) — investigation/spike notes (raw findings, no opinions)
- [`docs/adr/`](./docs/adr/) — Architecture Decision Records; use [`TEMPLATE.md`](./docs/adr/TEMPLATE.md) for new ones

## 4. Mandatory Workflow Rules

- **Before every PR / patch**: run the review checklist in `.ai/skills/review.md`
- **All code must pass**: `cargo clippy -- -D warnings` and `cargo test --workspace`
- **New endpoints**: add an entry to `docs/api.md`
- **Architecture changes**: create a new ADR in `docs/adr/` (see [`ADR-001`](./docs/adr/ADR-001-hexagonal-architecture.md) as template)
- **Never put domain logic in `infrastructure` crate** — see architecture doc

## 5. Sensitive / Local Files

`.ai/local/` is gitignored. Put any session-specific secrets, tokens, or personal context there.
Never commit anything from `.ai/local/`.
