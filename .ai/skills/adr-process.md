# Skill: Investigation → ADR Workflow

Load this skill when asked to **investigate a topic**, **research alternatives**, or **draft an ADR**.
This skill defines the full AI-driven process from open question to recorded decision.

---

## The Four Phases

```
1. INVESTIGATE  →  docs/research/{topic}.md
2. PROPOSE      →  docs/adr/ADR-NNN-title.md  (status: Proposed)
3. DECIDE       →  human approves → status: Accepted
4. LOG          →  .ai/context/decisions.md entry
```

Never skip phases. Never set an ADR status to `Accepted` yourself — only humans do that.

---

## Phase 1 — Investigate

**Trigger**: user says "investigate X", "research X", "what are the options for X", or you encounter an open question during implementation.

1. Search the web, read official docs, run experiments if needed
2. Create `docs/research/{kebab-case-topic}.md` using the template below
3. Write **facts only** — no opinions, no recommendations yet
4. Link to primary sources (official docs, RFCs, benchmarks)

### Research Note Template

```markdown
# Research: [Topic]

- **Date**: YYYY-MM-DD
- **Question**: What are we trying to answer?
- **Related ADR**: [link if one exists or is planned]

## Findings

### Finding 1: [Sub-topic]
…factual notes, quotes, links…

### Finding 2: [Sub-topic]
…

## Open Questions
- Question that still needs answering

## Sources
- [Title](url)
```

---

## Phase 2 — Propose

**Trigger**: investigation is complete, or user says "write an ADR for X".

1. Copy `docs/adr/TEMPLATE.md` to `docs/adr/ADR-NNN-short-title.md`
   - Pick the next available number by listing `docs/adr/`
2. Fill in **all sections** — especially *Considered Alternatives* (minimum two options)
3. Set status: `Proposed`
4. Link to the research note(s) in the `Research:` field
5. Present the draft to the user; do **not** implement the decision yet

---

## Phase 3 — Decide

**Trigger**: user explicitly approves ("go with option B", "accept this ADR").

1. Update `Status` to `Accepted` (or `Rejected` + reason)
2. If superseding an older ADR: update old ADR status to `Superseded by ADR-NNN`
3. Proceed with implementation

---

## Phase 4 — Log

After any decision (accepted or rejected):

1. Append a summary entry to `.ai/context/decisions.md`:

```markdown
## YYYY-MM-DD — [Topic]
- Decision: [one sentence]
- Alternatives rejected: [brief reason]
- See: [ADR-NNN link]
```

---

## Example Workflow

**User says**: "Investigate whether we should use Leptos or React for the UI."

```
You:
1. Create docs/research/ui-framework-options.md
   - Research Leptos (Rust/Wasm), React (separate repo), HTMX
   - Document bundle size, DSQL integration, deployment model, team skills
2. Create docs/adr/ADR-002-ui-framework.md (status: Proposed)
   - Three options: Leptos, React (separate repo), HTMX+axum
   - Recommendation: whichever the research supports
3. Present to user, wait for approval
4. On approval: update ADR status, implement, log to decisions.md
```

---

## Rules

- Research notes live in `docs/research/` and are **always committed** (valuable shared knowledge)
- ADR drafts are **immediately committed** (even as `Proposed`) so the decision process is transparent
- Never mix investigation prose into an ADR — keep ADRs crisp; link to research notes for details
- If investigation reveals the answer is obvious, it's OK to skip the research note and go straight to ADR
