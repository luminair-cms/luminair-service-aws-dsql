# Contributing to Luminair Service (AWS DSQL)

Welcome — contributions from humans and AI agents are both expected here.

## For Humans

1. Fork the repo, create a feature branch from `main`
2. Follow the Rust conventions in [`.ai/skills/rust-patterns.md`](.ai/skills/rust-patterns.md) — these apply to everyone
3. Run `cargo clippy -- -D warnings` and `cargo test --workspace` locally before pushing
4. Open a PR; CI must be green

## For AI Agents

See [`AGENTS.md`](./AGENTS.md) for the canonical AI workflow. In short:

1. Read `AGENTS.md` → load relevant skills → implement
2. Self-review using `.ai/skills/review.md` before finalising
3. Update `docs/api.md` if you touched the API surface
4. If you made an architectural decision, append to `.ai/context/decisions.md` and consider opening an ADR

## PR Checklist (humans and AI)

- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo audit` has no critical advisories
- [ ] Docs updated if public API or architecture changed
- [ ] No secrets or personal tokens in committed files

## Architecture Boundaries

Changes that cross crate boundaries (`domain` ↔ `application` ↔ `infrastructure`) require extra scrutiny.
Read [`docs/architecture.md`](./docs/architecture.md) before touching inter-crate interfaces.
