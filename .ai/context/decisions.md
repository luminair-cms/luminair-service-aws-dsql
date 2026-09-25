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

## 2026-09-24 — Unified Naming Conventions & REST Routing Strategy (ADR-008)

- **`DocumentTypeId` is `DocumentTypeId(String)`**: Replaces `Uuid`. Identity is derived directly from `singular_name` (e.g. `"partner-booking-category"`). Schema JSON files no longer need arbitrary hand-generated UUIDs.
- **Strict Kebab-Case Standard**: All user-defined domain identifiers (`DocumentTypeId`, `singular_name`, `plural_name`, `AttributeId`, relation attributes) must strictly match `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`. Underscores (`_`), uppercase letters, and double hyphens (`--`) are rejected.
- **Clean REST URLs**:
  - Collections use `{plural_name}`: `/api/{plural_name}` and `/api/{plural_name}/{id}`.
  - Singletons use `{singular_name}`: `/api/{singular_name}` (no instance UUID in path; direct object envelope without pagination).
  - Schema introspection uses `singular_name` as ID: `/api/schema/document-types/{id}`.
- **Startup Anti-Collision Guard**: Validates that no collection `plural_name` can ever collide with a singleton `singular_name`.
- **SQL Persistence Mapping**: Domain `kebab-case` maps deterministically to SQL `snake_case` (e.g. `partner-booking-categories` -> `partner_booking_categories`, `title-header` -> `title_header`).

## 2026-09-24 — Dynamic Schema Migration, Drift Detection, and Relation Persistence Model (ADR-009)

- **`IndexSet<T>` + `Borrow<Q>` AST Optimization**: Replaces redundant map structures (`BTreeMap<String, TableDefinition>`, `HashMap<AttributeId, FieldDefinition>`) with `IndexSet<T>`. Custom implementations of `Borrow<str>` / `Borrow<AttributeId>` and ID-based hashing eliminate key duplication while preserving exact insertion order for DDL generation.
- **Relation Persistence Model**:
  - **1:1 and N:1**: Stored as foreign key column on the owner table as `{to_snake_case(attribute_id)}_id UUID` (with unique index for 1:1, non-unique index for N:1).
  - **1:N**: Virtual inverse in domain; queried at runtime via `WHERE {owner_attr}_id = $1`.
  - **N:N**: Stored in dedicated junction table `{owner_plural}__{owner_attr}` with `(owner_id UUID, target_id UUID)` and double-underscore anti-collision prefix.
- **Drift Detection & Diffing**: Introspects `information_schema` and `pg_catalog` (ignoring system/static tables) and diffs against `DesiredSchema`, producing an ordered list of `MigrationStep`s.
- **Topologically Ordered DDL Planning**: Sorts steps into phases (Entity Tables $\rightarrow$ Junction Tables $\rightarrow$ Columns $\rightarrow$ Constraints/Indexes).
- **Safety Policy**: Production runs in `SafetyPolicy::AdditiveOnly` (drops are prohibited/flagged); destructive alterations require explicit `SafetyPolicy::AllowDestructive`.
- **`sea-query` DDL Execution**: Generates PostgreSQL-compliant DDL via `sea-query::PostgresQueryBuilder` and executes statements individually outside transaction blocks for AWS DSQL compatibility.

## 2026-09-25 — Self-Validating Email/Url Value Objects & Schema Constraint Ergonomics

- **Dedicated Nutype Value Objects for Email and Url**:
  - `DomainValue::Email(Email)` and `DomainValue::Url(Url)` replace raw string wrappers.
  - Constructed via `nutype` with predicate validators using `email_address::EmailAddress` and `url::Url`.
  - Self-validating by construction: `FieldConstraint::MinLength`, `FieldConstraint::MaxLength`, and `FieldConstraint::Pattern` are inapplicable to `Email` and `Url` fields and are rejected during schema loading.
- **JSON Schema Constraints Ergonomics (`min`/`max` Shortcuts)**:
  - JSON schema attributes support `{ "min": X, "max": Y }` shortcuts for strings (`text`, `uid`, `localizedText`), automatically mapping to `MinLength(X)` and `MaxLength(Y)`.
  - Serde aliases support `minimalLength`/`maximalLength`, `minimalInteger`/`maximalInteger`, `minimalDecimal`/`maximalDecimal`, and `minimum`/`maximum`.
  - Duplicate constraint definitions are safely deduplicated.

## 2026-09-25 — Documentation Alignment & AWS Aurora DSQL Foreign Key Support

- **Aurora DSQL Foreign Key Support (August 2026 Release)**:
  - Aurora DSQL natively supports foreign keys (`CASCADE`, `RESTRICT`, `SET NULL`, `NO ACTION`, deferrable).
  - Validation occurs via snapshot checks and commit-time `KEY SHARE`. Referencing writes contend with referenced modifications, increasing serialization conflict rates (`40001 OCC`).
  - Static system tables (`role_permissions`, `user_role_assignments`) enforce integrity via physical SQL FKs (`REFERENCES roles(id) ON DELETE CASCADE`).
  - Dynamic user document tables use indexed UUID columns (`{attr}_id`) and composite primary keys in junction tables to prevent cross-shard write contention and migration locks.
- **Unified SQLx Repository Strategy**:
  - `infrastructure` provides a single unified set of repositories targeting `sqlx::PgPool`. Database variations are isolated in the connection pool factory at startup (IAM token refresher for DSQL vs static connection string for PostgreSQL).
- **ADR-002 Superseded & ADR-009 Accepted**:
  - ADR-002 marked as superseded by ADR-007, ADR-008, and ADR-009 (singleton enforcement via dedicated per-type tables with single-row constraint index).
  - ADR-009 marked as Accepted.

## 2026-09-25 — Schema Redesign: Two-Table Publication Model & Universal `_link` Tables with Aurora DSQL Foreign Keys

- **Elimination of Global `document_snapshots` Table**:
  - The shared JSONB `document_snapshots` table is completely removed from static migrations (Milestone 3A).
  - Replaced by per-document-type published mirror tables with strongly-typed columns.
- **Two-Table Publication Model (MVP)**:
  - For each document type, a primary table `{table}` holds working draft records.
  - When `draft_and_publish: true`, a mirror table `{table}__published` is generated with `id UUID PRIMARY KEY REFERENCES {table}(id) ON DELETE CASCADE`.
  - Exactly at most one row per instance is stored in `{table}__published`, allowing direct `GET /api/{type}` queries on published content without JSONB extraction overhead or table joins.
  - Deleting a document instance cascades and removes the published mirror row at the database level.
- **Universal `_link` Tables for All Relations**:
  - All relations (`HasOne` and `HasMany`) use a dedicated link table named `{owner_table}__{owner_attr}_link`.
  - No relation foreign key columns are added to entity tables (`{table}`).
  - Columns: `owner_id UUID NOT NULL REFERENCES {owner_table}(id) ON DELETE CASCADE`, `target_id UUID NOT NULL REFERENCES {target_table}(id) ON DELETE CASCADE`, `PRIMARY KEY (owner_id, target_id)`.
  - For `HasOne`: uniqueness is enforced via `CREATE UNIQUE INDEX uq_{link}_owner ON {link} (owner_id)`.
  - For `HasMany`: no uniqueness constraint on `owner_id`.
  - Reverse lookups: indexed via `CREATE INDEX idx_{link}_target ON {link} (target_id)`.
  - **Zero-Migration Changing**: Changing relation type between `HasOne` and `HasMany` requires zero physical alterations to the link table itself; only the unique index on `owner_id` is created or dropped.
- **Topologically Ordered Migration Planning**:
  - Destruction order: drop indexes $\rightarrow$ drop link tables $\rightarrow$ drop published mirror tables $\rightarrow$ drop entity tables $\rightarrow$ drop columns.
  - Construction order: create entity tables $\rightarrow$ create published mirror tables $\rightarrow$ create link tables $\rightarrow$ add columns $\rightarrow$ create indexes.

## 2026-09-25 — Dual Link Tables & Public Filter Principle for Draft and Publish Relations

- **Dual Link Tables Model (Option A)**:
  - For every relation `{owner}__{attr}_link`, if the owner entity has `draft_and_publish: true`, generate a mirror published link table `{owner}__{attr}_link__published`.
  - Draft link table `{owner}__{attr}_link` persists working relationships between base/draft entity records.
  - Published link table `{owner}__{attr}_link__published` persists published relationships between active published instances.
- **Public Filter Principle (Variant 1)**:
  - `owner_id UUID NOT NULL REFERENCES {owner_table}__published(id) ON DELETE CASCADE`
  - Target foreign key:
    - If target has `draft_and_publish: true`: references `{target_table}__published(id) ON DELETE CASCADE`.
    - If target has `draft_and_publish: false`: references `{target_table}(id) ON DELETE CASCADE`.
  - Database constraint enforcement: An unpublished target entity can never be linked in published state, eliminating broken/draft links in public queries with zero runtime check overhead.
  - Cascading cleanup: When an entity is unpublished or deleted, native Aurora DSQL cascading deletes automatically remove the published relation link.
- **Refined 12-Step Topological Migration Lifecycle**:
  - Destruction: drop indexes $\rightarrow$ drop published link tables $\rightarrow$ drop draft link tables $\rightarrow$ drop published tables $\rightarrow$ drop entity tables $\rightarrow$ drop columns.
  - Construction: create entity tables $\rightarrow$ create published tables $\rightarrow$ create draft link tables $\rightarrow$ create published link tables $\rightarrow$ add columns $\rightarrow$ create indexes.

## 2026-09-25 — Milestone 4: Persistence Layer Implementation (`infrastructure/src/repositories/`)

- **Static System Repositories**:
  - `SqlxRoleRepository`: Full CRUD for `roles` and `role_permissions` with all permission variants, transactional role updates with atomic permission synchronization.
  - `SqlxUserRoleAssignmentRepository`: CRUD for `user_role_assignments` with role filtering and admin existence check.
  - `SqlxAccessRequestRepository`: Full lifecycle management for `access_requests` (creation, pending retrieval, approval, rejection).
- **Dynamic Document Repository (`SqlxDocumentInstanceRepository`)**:
  - Dynamically binds and decodes all schema-driven field types: Text, Uid, Uuid, Integer (I16, I32, I64), Decimal, Boolean, Date, DateTime, Email, Url, LocalizedText (JSONB), Json (JSONB).
  - **SingleType Singleton Enforcement**: Automatically handles singleton row update without creating duplicate rows.
  - **Two-Table Publication Lifecycle**: Base draft row persisted to `{table}`; when published, upserted to `{table}__published`; when returned to draft, mirror row in `{table}__published` is removed (cascading deletes any published links via Aurora DSQL FK constraints).
  - **Dual Link Tables**: Maintains working draft relations in `{owner}__{attr}_link` and published relations in `{owner}__{attr}_link__published` according to Option A and Variant 1 (Public Filter Principle).
  - **Two-Phase Batch Relation Loading (`fetch_relations`)**: Efficiently loads relations for any set of parent documents by querying the universal link tables using `WHERE owner_id = ANY($1)` (or `WHERE target_id = ANY($1)` for inverse side) and batch-fetching child instances using `WHERE id = ANY($2)`, avoiding N+1 queries and Cartesian join explosions.
  - **Querying & Pagination**: Dynamic query building with parameter binding for field equality filters and pagination.
  - Comprehensive integration test suites in `tests/repositories_test.rs` and `tests/document_repository_test.rs`.

## 2026-09-25 — Milestone 5: Authentication, Security & Bootstrap (`infrastructure/src/auth/`)

- **Pure-Rust Cryptographic Architecture (`rust_crypto`)**:
  - `jsonwebtoken` configured with `default-features = false, features = ["rust_crypto", "use_pem"]`.
  - Avoids C library compilation (`aws-lc-sys`) and native toolchain issues, ensuring cross-platform stability and zero C dependencies.
- **Pluggable Token Validation (`TokenValidator`)**:
  - `JwksTokenValidator`: Thread-safe cached JWKS key rotation (`Arc<RwLock<HashMap<String, DecodingKey>>>`) for AWS Cognito and Keycloak with audience and issuer verification.
  - `SecretTokenValidator`: Symmetric HMAC-SHA256 validator and token generator for development, tests, and inter-service authentication.
  - `MockTokenValidator`: Unverified claims extraction (`jsonwebtoken::dangerous::insecure_decode_claims`) and overrides for unit test isolation.
- **Shadow Users (`SqlxShadowUserRepository`)**:
  - Automatically records and updates verified OIDC identity claims (`user_id`, `email`, `name`, `auth_type`, `last_seen`) in PostgreSQL / AWS Aurora DSQL on every authenticated request.
- **Idempotent Administrator Bootstrap (`run_bootstrap`)**:
  - Seeds initial administrator identity from `BOOTSTRAP_ADMIN_SUB` and `BOOTSTRAP_AUTH_TYPE` on application startup.
  - Idempotently guards against duplicate role grants: skips if an admin role assignment already exists for the user.
  - Seeds `shadow_users`, creates an approved `AccessRequest` audit record, and assigns `ROLE_ADMIN_ID` with `granted_by: None`.
- **Axum Request Extractors**:
  - `AuthenticatedClaims`: Open extractor for verified OIDC tokens without requiring active permissions (used for onboarding at `POST /api/access-requests`).
  - `AuthUser`: Protected extractor enforcing ADR-005 enrollment states:
    - Approved role assignments -> returns `AuthUser` carrying `CallerContext`.
    - Pending request -> returns 403 `ACCESS_PENDING`.
    - Rejected request -> returns 403 `ACCESS_REJECTED`.
    - No request submitted -> returns 403 `ACCESS_NOT_REQUESTED`.

## 2026-09-25 — Milestone 6: REST API Layer (`infrastructure/src/api/`)

- **Strapi 5-style Unified REST Routing**:
  - Dynamically routes requests based on registered `DocumentType` metadata in `SchemaRegistry`:
    - Collections (`kind = Collection`): uses `{plural_name}`: `/api/{plural_name}` (GET list, POST create) and `/api/{plural_name}/{id}` (GET, PUT, DELETE, publish, unpublish, snapshots).
    - Singletons (`kind = SingleType`): uses `{singular_name}`: `/api/{singular_name}` (GET direct, PUT upsert, DELETE clear, publish, unpublish, snapshots) with zero redundant instance UUIDs.
- **Two-Table MVP Snapshot Synchronization (`SqlxSnapshotRepository`)**:
  - `SnapshotRepository` implementation queries the per-type `{table}__published` mirror table directly, adhering to the single published revision MVP model.
  - Deletion cascades automatically through native Aurora DSQL foreign keys (`ON DELETE CASCADE`).
- **RFC 9457 Problem Details (`application/problem+json`)**:
  - Unified HTTP error response mapping for all domain, application, auth, and parameter validation failures.
  - Exposes standardized `type`, `title`, `status`, and `detail` fields without leaking internal database errors.
- **Envelope Standardization**:
  - Single resource endpoints wrap responses in `{ "data": ... }`.
  - Collection endpoints wrap responses in `{ "data": [ ... ], "meta": { "pagination": { "page", "page_size", "total" } } }`.
- **Axum Extractor Composition**:
  - `AppState` implements `FromRef<AppState> for AuthAppState`, seamlessly wiring `AuthUser` and `AuthenticatedClaims` extractors without manual middleware bridges.

## 2026-09-25 — Admin Dashboard UI Architecture (ADR-010 Accepted)

- **Architecture**: Decoupled Single-Page Application (Option A) housed under `/frontend`, strictly preserving Hexagonal Architecture and headless API boundaries.
- **Frontend Stack**: React 19 + TypeScript + Vite 6 + Mantine v7 design system (`@mantine/core`, `@mantine/form`, `@mantine/notifications`, `@mantine/modals`, `@mantine/dates`, `@mantine/tiptap`).
- **State Architecture**:
  - Client / Global State: Zustand (`useAuthStore`, `useUiStore`, `useDraftStore`) with persistence middleware.
  - Server State: TanStack Query v5 for remote data caching, query invalidation, and optimistic mutations.
- **Routing**: TanStack Router with 100% type-safe routes, Zod search param validation, and `AuthGuard` enrollment routing.
- **Dynamic Schema Forms**: Runtime form generator mapping `/api/schema/document-types` metadata to Mantine inputs for all 12 field types, multi-locale editing tabs (`LocalizedText`), and relation pickers (`HasOne`, `HasMany`).
- **Authentication**: OIDC Authorization Code Flow with PKCE via `oidc-client-ts`.
- **Local Dev IdP**: Dex (`ghcr.io/dexidp/dex`) configured via `docker-compose.dev.yml` and `docker/dex/dex-config.yaml` (~15MB RAM, instant boot, pre-seeded admin/editor users).
- **Production AWS Deployment**: Static assets deployed to AWS S3 behind Amazon CloudFront with Origin Access Control (OAC). CloudFront serves static SPA at `/*` and proxies `/api/*` to the Axum backend (eliminating CORS in production).
- **Alternatives Rejected**:
  - Embedded SPA in Axum (coupled build pipelines and container asset serving).
  - Fullstack Rust Wasm / Leptos (heavy bundle size 1.5MB+, scarce CMS component ecosystem, slow compilation).
  - Server-Side Rendering with HTMX + Askama (awkward recursive schema form generation, rigid multi-locale tabs, blurs headless API boundary).
- See: [ADR-010](../../docs/adr/ADR-010-ui-architecture.md) and [ui-architecture.md](../../docs/ui-architecture.md).

---

> **AI agents**: when you make a non-obvious decision during implementation, append an entry here.
> Format: `## YYYY-MM-DD — Topic` followed by bullet points.







