# Decisions Log

Informal running log of decisions made during development.
For major architectural decisions, a formal ADR in `docs/adr/` is also created.

---

## 2026-08-21 — Project Bootstrap

- **Hexagonal Architecture + DDD** adopted with three crates: `domain`, `application`, `infrastructure`. See ADR-001.
- **AWS Aurora DSQL** chosen as database (serverless, distributed PostgreSQL-compatible).
- **UUID v7** (time-ordered) chosen for all PKs — DSQL does not support sequences reliably.
- **Strapi-like REST API** as the API style for content management.
- **UI deferred** — decision pending investigation of options (separate React repo vs. internal Leptos/Yew/HTMX).
- AI tooling: `AGENTS.md` as single entry point; `.ai/skills/` for reusable prompts; `.ai/context/` for memory; `.github/copilot-instructions.md` for Copilot; tool-specific overrides in `.ai/agents/`.

---

## 2026-09-17 — Domain Model Design (settled decisions)

- **Terminology**: `DocumentType` / `DocumentInstance` (more general than CMS ContentType/Entry)
- **FieldType**: Uid, Uuid, Text, LocalizedText, Email, Url, Integer, Decimal, Date, DateTime, Boolean, Json
- **Json field**: flat `HashMap<String, PrimitiveValue>` — not arbitrary `serde_json::Value`
- **Relations**: `RelationDefinition` (schema) and `ResolvedRelation` (runtime) are separate types
- **PublicationState**: always present; `draft_and_publish` controls API/UI only
- **Draft.never_published**: `Draft { last_published_revision: Option<u32> }` — None = never published
- **AuditTrail**: `version` = every write; `revision` = publish-only; independent
- **Auth**: external OIDC; system tracks `UserId` internally
- See: [domain-model-design.md](../../docs/research/domain-model-design.md)

## 2026-09-17 — Open ADRs (Proposed, awaiting your decision)

- [ADR-002](../../docs/adr/ADR-002-singleton-enforcement.md): SingleType enforcement strategy
- [ADR-003](../../docs/adr/ADR-003-bidirectional-relations.md): Relation representation
- [ADR-004](../../docs/adr/ADR-004-locale-scope.md): Locale scope — system vs. document type level
- ADR-005: Auth — pending [auth-strategy.md](../../docs/research/auth-strategy.md) review

---

## 2026-09-21 — ADRs Accepted

- **ADR-002 ✅ Option C**: SingleType invariant enforced by application service guard (read-before-write) + DB partial unique index. `DocumentType` struct unchanged — no aggregate restructuring.
- **ADR-003 ✅ Option F**: All relations (unidirectional and bidirectional) are a single `Relation` entity with `inverse: Option<RelationInverse>`. `DocumentType` holds no inline relations — loads them via `RelationRepository::find_by_participant(type_id)`.
- **ADR-004 ✅ Option B**: Available locales defined at system level in `SystemConfig` entity, not per-document-type.
- **ADR-005 ✅**: External OIDC auth (`UserId = sub`), RBAC + owner rule, access request approval workflow, config-based bootstrap (`BOOTSTRAP_ADMIN_SUB` env var, idempotent on startup).

## 2026-09-21 — Schema Loading Strategy (ADR-006)

- **Schemas loaded at startup from JSON config files** (`schema/document-types/*.json`, `schema/relations/*.json`)
- Schema is **immutable at runtime** — changes require editing JSON files and restarting
- Startup sequence: parse → validate → upsert to DB → cache in `Arc<SchemaRegistry>`
- `SchemaRegistry` is a domain service injected into application services — replaces runtime DB reads for schema lookups
- Schema UUIDs are **stable, hand-assigned** (not `v7`) — embedded in JSON, never change across deployments
- No schema management API — API is purely for `DocumentInstance` CRUD
- See: [ADR-006](../../docs/adr/ADR-006-schema-loading.md)

## 2026-09-21 — Terminology Correction

The 2026-09-17 entry used `RelationDefinition` / `ResolvedRelation`. These are superseded:
- `RelationDefinition` → `Relation` entity (unified, owns both sides via `inverse: Option<RelationInverse>`)
- `ResolvedRelation` → still `ResolvedRelation` (runtime link between two `DocumentInstance`s)

## 2026-09-21 — Persistence Model (ADR-007, revised)

- **One table per DocumentType** named after `plural_name` (e.g. `articles`, `settings`) — no generic EAV table
- **Typed columns** per field: `FieldType` maps directly to a PostgreSQL column type
- **LocalizedText** → `JSONB` column inline in the same row (`{"en":"…","uk":"…"}`)
- **Json field** → `JSONB` column inline
- **Singleton unique index** (`_singleton BOOLEAN DEFAULT TRUE + UNIQUE`) created **only** for `SingleType` tables; Collection tables have no such constraint
- **Two-table publication**: per-type table (current state) + shared `document_snapshots` (JSONB, immutable)
- **Dynamic DDL at startup**: schema loader generates and executes `CREATE TABLE IF NOT EXISTS` + `ALTER TABLE ADD COLUMN` for new fields; no auto-DROP on field removal
- `DatabaseRowId` unified with `DocumentInstanceId` — one row per instance
- See: [ADR-007](../../docs/adr/ADR-007-persistence-model.md)

## 2026-09-22 — Schema Immutability (plan review finding)

- **`DocumentType` and `Relation` are both immutable config** — same lifecycle as JSON files, not runtime entities
- **No `RelationRepository`** port trait — `SchemaRegistry.find_relations_for(type_id)` replaces `find_by_participant()` (ADR-003 Consequences updated)
- **`RelationView` has three variants**: `OwnerSide` (bidirectional owner), `InverseSide` (bidirectional inverse), `Unidirectional` (`inverse: None`) — plan updated
- **`UserRoleAssignment.granted_by: Option<UserId>`** — `None` = system/bootstrap grant
- **`AttributeId` slug validation is format-only** in the domain newtype; SQL reserved-name check moves to `SchemaLoader` in infrastructure
- **`Relation` has no `created_at`** — static config has no runtime creation timestamp
- **ID types derive `Display`** — required for `thiserror` `#[error("{0}")]` messages

## 2026-09-24 — Application Layer Architecture & Concurrency Model

- **Native Async Traits (RPITIT + Send)**: All port traits in `domain::ports` and service traits in `application::services` use native Rust 2024 `fn ... -> impl Future<Output = Result<...>> + Send` without `async-trait`. This avoids heap allocations (`Pin<Box<dyn Future>>`), eliminates macro dependencies, and enables LLVM monomorphization.
- **Zero Runtime Dependencies**: `application` crate depends strictly on `domain`, `thiserror`, `serde`, `uuid`, and `chrono`. `tokio` is strictly a test runner dependency in `[dev-dependencies]`.
- **Sequential Fetch for MVP**: In `DocumentsService::find`, `find_by_type` and `count` are executed sequentially, avoiding runtime concurrency join overhead (`tokio::try_join!`) in the application layer.
- **Two-Phase Batch Relation Enrichment (`enrich`)**: Strapi 5-style `populate` avoids SQL `LEFT JOIN` Cartesian explosion. Parent IDs are collected from the queried page, relations are fetched in a single batch query, and related documents are stitched in-memory.
- **Static SystemConfig**: `SystemConfig` (supported locales, default locale) is loaded once at startup alongside the schema (ADR-004, ADR-006). It is immutable at runtime, eliminating the need for `UpdateLocalesCommand` or runtime locale mutation APIs.
- **Shared In-Memory Test Fakes**: `application::test_support` exposes fast, thread-safe in-memory repositories using `std::sync::RwLock` for deterministic testing across the workspace.

---

> **AI agents**: when you make a non-obvious decision during implementation, append an entry here.
> Format: `## YYYY-MM-DD — Topic` followed by bullet points.

