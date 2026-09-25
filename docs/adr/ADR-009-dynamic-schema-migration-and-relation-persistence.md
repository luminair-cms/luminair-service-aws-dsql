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

### 2. Two-Table Publication Model & Relation Persistence

#### A. Two-Table Model for Document Types
For every document type:
- **Main Entity Table `{table}`**: Holds current working drafts.
  - Audit columns: `id UUID PRIMARY KEY`, `version BIGINT NOT NULL DEFAULT 1`, `owner_id VARCHAR(255) NOT NULL`, `publication_state VARCHAR(50) NOT NULL`, `created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP`, `updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP`.
  - User attribute columns (e.g. `title TEXT NOT NULL`, `slug VARCHAR(255) NOT NULL`).
  - No relation foreign key columns are placed on `{table}`.
- **Published Mirror Table `{table}__published`**:
  - Generated when `draft_and_publish: true`.
  - Primary key: `id UUID PRIMARY KEY REFERENCES {table}(id) ON DELETE CASCADE`.
  - Holds at most **one row** per instance (the currently active published revision).
  - Audit columns: `published_version BIGINT NOT NULL`, `owner_id VARCHAR(255) NOT NULL`, `created_at TIMESTAMPTZ NOT NULL`, `updated_at TIMESTAMPTZ NOT NULL`, `published_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP`, `published_by VARCHAR(255)`.
  - User attribute columns mirror `{table}`, enabling direct, high-performance `GET` queries without joins or JSONB unpacking.
  - Deleting an instance from `{table}` cascades and deletes the published row automatically.

#### B. Universal Link Tables (`{owner_table}__{owner_attr}_link`)
All relations (`HasOne` and `HasMany`, unidirectional and bidirectional) are persisted in dedicated universal link tables:
- **Link Table Name**: `{owner_table}__{owner_attr}_link`
  - Suffix `_link` explicitly distinguishes relation link tables from entity tables and published mirror tables.
- **Columns & Foreign Keys**:
  - `owner_id UUID NOT NULL REFERENCES {owner_table}(id) ON DELETE CASCADE`
  - `target_id UUID NOT NULL REFERENCES {target_table}(id) ON DELETE CASCADE`
  - `PRIMARY KEY (owner_id, target_id)`
- **Indexes**:
  - `HasOne`: Enforces at most one target per owner via `CREATE UNIQUE INDEX uq_{link_table}_owner ON {link_table} (owner_id);`.
  - Reverse lookups: Supported in all cases via `CREATE INDEX idx_{link_table}_target ON {link_table} (target_id);`.
- **Zero-Migration Advantage**: Changing a relation type between `HasOne` and `HasMany` requires zero DDL changes to the link table itself; only the unique index on `owner_id` is created or dropped.

#### C. Dual Link Tables for Draft & Publish (`{owner}__{attr}_link__published`)
To support relations under the Two-Table model, we adopt **Option A (Dual Link Tables)** with **Variant 1 (Public Filter Principle)**:
- When the owner entity has `draft_and_publish: true`, a mirror published link table `{owner_table}__{owner_attr}_link__published` is generated.
- **Foreign Key Definitions**:
  - `owner_id UUID NOT NULL REFERENCES {owner_table}__published(id) ON DELETE CASCADE`
  - `target_id UUID NOT NULL REFERENCES {target_ref_table}(id) ON DELETE CASCADE` where:
    - If target has `draft_and_publish: true`: references `{target_table}__published(id) ON DELETE CASCADE`.
    - If target has `draft_and_publish: false`: references `{target_table}(id) ON DELETE CASCADE`.
- **Public Filter Principle**: Enforcing the target foreign key at the database constraint level guarantees that an unpublished target entity can **never** be linked or visible in published state.
- **Automatic Cleanup**: When an entity is unpublished (deleted from `{owner_table}__published` or `{target_table}__published`) or deleted entirely, native Aurora DSQL cascading deletes automatically remove the published relation link.

### 3. Database Catalog Introspection

The actual schema is inspected by querying `information_schema.columns`, `information_schema.tables`, `information_schema.table_constraints`, and `pg_catalog.pg_indexes` for the `public` schema. Static system tables (`_sqlx_migrations`, `roles`, `role_permissions`, `user_role_assignments`, `access_requests`, `shadow_users`, `document_snapshots`) are explicitly filtered out.

### 4. Diffing Engine & Migration Steps

Comparing `ActualSchema` against `DesiredSchema` produces an ordered set of atomic `MigrationStep` variants:

```rust
pub enum MigrationStep {
    CreateTable(TableDefinition),
    AddColumn { table: String, column: ColumnDefinition },
    CreateIndex(IndexDefinition),
    DropIndex { name: String, table: String },
    DropColumn { table: String, column: String },
    DropTable { name: String, kind: TableKind },
}
```

### 5. Safety Policy

- **`SafetyPolicy::AdditiveOnly` (Default for Production)**:
  - Executes: `CreateTable`, `AddColumn`, `CreateIndex`.
  - Fails when encountering destructive drift (`DropColumn`, `DropTable`, `DropIndex`).
- **`SafetyPolicy::AllowDestructive` (Opt-in for CI/Test environments)**:
  - Allows dropping orphan tables, columns, or indexes.

### 6. Dependency Graph & Topologically Ordered Execution

Operations are ordered topologically by dependency phase to satisfy all foreign key references:

**Destruction Phase (Reverse Dependency Order)**:
1. Drop Indexes
2. Drop Published Link Tables (`TableKind::Link` ending in `__published`)
3. Drop Draft Link Tables (`TableKind::Link` not ending in `__published`)
4. Drop Published Mirror Tables (`TableKind::Published`)
5. Drop Entity Tables (`TableKind::Entity`)
6. Drop Columns

**Construction Phase (Forward Dependency Order)**:
7. Create Entity Tables (`TableKind::Entity`)
8. Create Published Mirror Tables (`TableKind::Published`, references Entity table PK)
9. Create Draft Link Tables (`TableKind::Link` not ending in `__published`, references Entity tables)
10. Create Published Link Tables (`TableKind::Link` ending in `__published`, references Published/Entity tables)
11. Add Columns
12. Create Indexes

Each step generates a PostgreSQL DDL string via `sea-query::PostgresQueryBuilder` and executes independently outside of a transaction block.

---

## Consequences

### Positive
- **Native Aurora DSQL Foreign Keys**: Referential integrity is enforced with cascading deletes at the database engine level.
- **High-Performance Public Queries**: Published rows are stored in typed columns in `{table}__published`, allowing simple single-table `SELECT` queries without JSONB deserialization overhead.
- **Clean Table Evolution**: Changing relation cardinality (`HasOne` $\leftrightarrow$ `HasMany`) does not migrate or alter physical link tables.
- **Memory Optimized**: Replaces bloated maps with compact, order-preserving `IndexSet<T>` with $O(1)$ lookups.
- **Production Safe**: Additive-only safety policy guards against accidental data loss.

### Negative / Trade-offs
- Two tables per document type (`{table}` and `{table}__published`) increases table count in the database schema.
- Non-transactional DDL execution requires all statements to remain strictly idempotent (`IF NOT EXISTS`, `IF EXISTS`).
