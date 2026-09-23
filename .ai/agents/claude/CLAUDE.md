# Claude Agent — Overrides

> Read `AGENTS.md` at the repo root first. This file contains **Claude-specific additions only**.

## Context Loading Order (Claude)

Claude (claude.ai Projects / Claude Code) should be configured with these as project documents:
1. `AGENTS.md`
2. `.ai/context/project.md`
3. `docs/architecture.md`

Load skill files on demand per task.

## Behaviour Notes

- Always run `cargo clippy -- -D warnings` before presenting code — do not skip
- When uncertain about crate boundaries, re-read `docs/architecture.md` rather than guessing
- Prefer explicit `From` impl conversions over `.into()` when the target type is ambiguous

## Memory

After completing a task involving a non-obvious decision, append to `.ai/context/decisions.md`.
