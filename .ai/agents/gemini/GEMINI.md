# Gemini Agent — Overrides

> Read `AGENTS.md` at the repo root first. This file contains **Gemini-specific additions only**.

## Context Loading Order (Gemini)

Gemini automatically indexes the workspace. Still, explicitly load these at session start:
1. `AGENTS.md`
2. `.ai/context/project.md`
3. The skill relevant to your task

## MCP / Tool Hints

- Use `run_command` for `cargo clippy`, `cargo test`, `cargo audit` — do not skip these
- Use `grep_search` to navigate codebase before making edits (prefer search over assumptions)
- Prefer `replace_file_content` over full-file rewrites for surgical edits

## Memory

After completing a significant task, append a summary entry to `.ai/context/decisions.md`.
