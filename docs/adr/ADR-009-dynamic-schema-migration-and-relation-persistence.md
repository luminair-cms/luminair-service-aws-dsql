# ADR-009: Dynamic Schema Migration, Drift Detection, and Relation Persistence Model

- **Status**: `Accepted`
- **Date**: 2026-09-24
- **Deciders**: Dmitri Astafiev, Architecture & Core Engineering
- **Consulted**: ADR-006 (Startup Schema Loading), ADR-007 (Persistence Model), ADR-008 (Naming Conventions & Routing)

---

## Context

Following [ADR-006](./ADR-006-schema-loading.md) and [ADR-007](./ADR-007-persistence-model.md), Luminair treats JSON schema files (`schema/document-types/*.json`, `schema/relations/*.json`) as the single source of truth for the domain model.

While static system tables (`roles`, `user_role_assignments`, `access_requests`, `shadow_users`, `document_snapshots`) are provisioned via versioned static migrations ([Milestone 3A](./ADR-007-persistence-model.md)), document type instances are stored in dedicated per-type physical tables (e.g. `articles`, `authors`, `partner_booking_categories`). As schemas evolve:
1. New document types are added.
2. New fields are declared or field types/constraints are modified.
3. Relations (1:1, 1:N, N:N) are established or updated.
4. Physical database schemas in AWS Aurora DSQL / PostgreSQL may drift from the JSON declarations.

Furthermore, AWS Aurora DSQL introduces specific architectural characteristics:
- **Foreign Key Constraints (Supported August 2026)**: While Aurora DSQL supports foreign keys (`CASCADE`, `RESTRICT`, `SET NULL`, etc.), it validates them via commit-time `KEY SHARE` checks. High write concurrency on referencing rows can increase OCC serialization conflicts (`40001`). For dynamic schema relations, using indexed ID columns (`{attr}_id`) and junction tables avoids cross-shard lock amplification during mass CMS imports.
- **No DDL inside Transactions**: `CREATE TABLE`, `ALTER TABLE`, and `CREATE INDEX` cannot execute inside a `BEGIN ... COMMIT` block.
- **Client-Generated UUID v7**: Primary keys must be UUID v7; sequences (`SERIAL`, `BIGSERIAL`) are unsupported.

We must define:
1. How relations (1:1, 1:N, N:N) map to physical tables and columns.
2. How to model the desired vs actual database schemas in Rust without redundant allocations.
3. How to introspect existing database tables and detect schema drift.
4. How to generate, topologically order, and safely execute dynamic DDL migration steps.

---

## Decision Drivers

- **AWS DSQL Compatibility**: Non-transactional DDL execution, minimized cross-table OCC write contention, zero sequences.
- **Zero Memory & Allocation Redundancy**: Avoid duplicating string identifiers between map keys and internal struct fields (`TableDefinition.name`, `FieldDefinition.id`).
- **Insertion Order Preservation**: In a CMS, field and column order defined in JSON schemas must be deterministic and preserved in UI and database definitions.
- **Collision-Free Relation Persistence**: Consistent, collision-free naming for relation columns and junction tables.
- **Production Safety (Safe by Default)**: Additive migrations by default; destructive alterations (drop table, drop column) must be guarded.
- **Type-Safe DDL Generation**: Avoid fragile string concatenation templates for SQL generation.

---

## Considered Alternatives

### Option A: Raw String Formatting with Ad-Hoc Drift Check
Generate SQL DDL via string templates (`format!("CREATE TABLE IF NOT EXISTS {} ...", name)`), inspect `information_schema.columns` with ad-hoc string comparisons, and use `BTreeMap<String, TableDefinition>`.

- **Pros**: Minimal new abstractions.
- **Cons**: High risk of SQL syntax/quoting errors; `BTreeMap` duplicates identifier strings in memory and destroys insertion order; ad-hoc drift checking becomes brittle when handling nullability, defaults, partial indexes, and junction tables.

### Option B: Heavyweight ORM Migration Engine (e.g. SeaORM / Diesel CLI)
Delegate dynamic migrations to an external ORM migration tool or library.

- **Pros**: Pre-packaged migration commands.
- **Cons**: Most ORM migration frameworks assume conventional RDBMS features (foreign keys, transaction blocks around DDL), which fail on AWS Aurora DSQL. They also mandate compile-time entity macros, conflicting with Luminair's runtime JSON-driven document types.

### Option C: Intermediate Schema AST with `IndexSet<T>` + `Borrow<Q>`, Catalog Introspection, and `sea-query` DDL Generation (Chosen)
1. Represent schemas using a strongly-typed `DatabaseSchema` AST where collections use `IndexSet<T>` implementing `Borrow<Q>`.
2. Convert `SchemaRegistry` into a `DesiredSchema`.
3. Introspect `information_schema` and `pg_catalog` into an `ActualSchema`.
4. Run a pure `DiffEngine` generating a `Vec<MigrationStep>`.
5. Topologically sort steps through a Dependency Graph and execute them via `sea-query` outside transaction blocks.

---

## Decision

We adopt **Option C**. Specifically:

### 1. In-Memory AST Optimization via `IndexSet<T>` & `Borrow<Q>`

To eliminate identifier duplication and preserve deterministic schema ordering, collections within the schema models use `indexmap::IndexSet<T>` with `std::borrow::Borrow<Q>`:

- `TableDefinition` implements `Borrow<str>` and equality/hash based on `name`.
- `ColumnDefinition` implements `Borrow<str>` and equality/hash based on `name`.
- `IndexDefinition` implements `Borrow<str>` and equality/hash based on `name`.
- `FieldDefinition` (in domain `DocumentType`) implements `Borrow<AttributeId>` and equality/hash based on `id`.

**Benefits**:
- Zero memory duplication: table names and attribute IDs are stored once.
- $O(1)$ lookup by borrowed key (`table.columns.get("title_header")`).
- Exact insertion order preservation from JSON schema to DDL execution.

### 2. Relation Persistence & Naming Model

Following [ADR-008](./ADR-008-naming-conventions-and-routing.md), domain identifiers in `kebab-case` map deterministically to SQL `snake_case`:

#### A. Single-Column Relations (1:1 and N:1)
Stored directly as a column on the owner table:
- **Column Name**: `{to_snake_case(attribute_id)}_id UUID`
  - Example: `category` $\rightarrow$ `category_id UUID`
  - Example: `featured-image` $\rightarrow$ `featured_image_id UUID`
- **1:1 Constraint**: `CREATE UNIQUE INDEX uq_{table}_{col} ON {table} ({col});`
- **N:1 Constraint**: `CREATE INDEX idx_{table}_{col} ON {table} ({col});`

#### B. Virtual Inverse Side (1:N)
The target document type does **not** store an array of IDs in its table. Instead, it is resolved at query time by filtering the owner table: `WHERE {owner_attr}_id = $1`.

#### C. Many-to-Many Relations (N:N)
Stored in a dedicated junction table:
- **Junction Table Name**: `{owner_plural}__{owner_attr}`
  - Uses double underscore (`__`) to prevent collisions with entity tables that contain single underscores.
  - Example: `articles` $\leftrightarrow$ `tags` via `tags` $\rightarrow$ `articles__tags`
  - Example: `partner_booking_categories` $\leftrightarrow$ `partner_categories` via `categories` $\rightarrow$ `partner_booking_categories__categories`
- **Columns**:
  - `{owner_singular}_id UUID NOT NULL` (e.g. `article_id`)
  - `{target_singular}_id UUID NOT NULL` (e.g. `tag_id`)
- **Indexes**:
  - `PRIMARY KEY ({owner_singular}_id, {target_singular}_id)`
  - Secondary Index: `CREATE INDEX idx_{junction}_{target_singular}_id ON {junction} ({target_singular}_id);`

### 3. Database Catalog Introspection

The actual schema is inspected by querying `information_schema.columns`, `information_schema.tables`, and `pg_catalog.pg_index` for the `public` schema. Static and system tables (`_sqlx_migrations`, `document_snapshots`, `roles`, `role_permissions`, `user_role_assignments`, `access_requests`, `shadow_users`) are explicitly filtered out.

### 4. Diffing Engine & Migration Steps

Comparing `ActualSchema` against `DesiredSchema` produces an ordered set of atomic `MigrationStep` variants:

```rust
pub enum MigrationStep {
    CreateTable(TableDefinition),
    AddColumn { table: String, column: ColumnDefinition },
    AlterColumnType { table: String, column: String, from: SqlColumnType, to: SqlColumnType },
    AlterColumnNullability { table: String, column: String, nullable: bool },
    CreateIndex(IndexDefinition),
    DropIndex { table: String, name: String },
    DropColumn { table: String, column: String },
    DropTable { table: String },
}
```

### 5. Safety Policy

- **`SafetyPolicy::AdditiveOnly` (Default for Production)**:
  - Executes: `CreateTable`, `AddColumn`, `CreateIndex`.
  - Warns or fails when encountering destructive drift (`DropColumn`, `DropTable`, narrowing column types).
- **`SafetyPolicy::AllowDestructive` (Opt-in for CI/Test environments)**:
  - Allows dropping orphan tables, columns, or indexes.

### 6. Dependency Graph & Topologically Ordered Execution

Operations are ordered topologically by dependency phase:
1. `CreateTable` (entity tables)
2. `CreateTable` (junction tables)
3. `AddColumn`
4. `AlterColumnType` / `AlterColumnNullability`
5. `CreateIndex`
6. Destructive steps (`DropIndex` $\rightarrow$ `DropColumn` $\rightarrow$ `DropTable`, if permitted).

Each step generates a PostgreSQL DDL string via `sea-query::PostgresQueryBuilder` and executes independently outside of a transaction block.

---

## Consequences

### Positive
- **DSQL & PostgreSQL Ready**: Fully adheres to DSQL non-transactional DDL and UUID v7 PKs while avoiding cross-table OCC lock contention.
- **Memory Optimized**: Replaces bloated `HashMap<Key, StructWithKey>` with compact, order-preserving `IndexSet<T>`.
- **Zero Ambiguity**: Clear naming rules for all relation types; no collisions due to `__` junction table standard.
- **Production Safe**: Prevents accidental data loss through strict additive safety policy by default.
- **Robust DDL Generation**: Leverages `sea-query` to eliminate manual SQL string formatting bugs.

### Negative / Trade-offs
- Non-transactional DDL execution means that if a failure occurs halfway through multi-step migrations, previously executed statements remain applied. All generated DDL statements must therefore remain strictly idempotent (`IF NOT EXISTS`, `IF EXISTS`).
