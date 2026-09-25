# ADR-002: SingleType Enforcement Strategy

- **Status**: Superseded by [ADR-007](./ADR-007-persistence-model.md), [ADR-008](./ADR-008-naming-conventions-and-routing.md), and [ADR-009](./ADR-009-dynamic-schema-migration-and-relation-persistence.md)
- **Date**: 2026-09-17 · Revised: 2026-09-21 · Accepted: 2026-09-21 · Superseded: 2026-09-25
- **Deciders**: Dmitri Astafiev
- **Research**: [docs/research/domain-model-design.md](../research/domain-model-design.md)

> [!NOTE]
> **Superseded Context**:
> While the **Option C defence-in-depth principle** (Application Service guard + Database constraint) was accepted and remains active, the specific physical schema described in this ADR assumed a shared `document_instances(document_type_id)` table.
> Under **ADR-007**, **ADR-008**, and **ADR-009**, each document type is provisioned with its own dedicated physical table:
> - SingleTypes are mapped to dedicated tables named `{singular_name}` (e.g. `site_setting`).
> - The DB constraint is enforced via a single-row constraint (`_singleton BOOLEAN NOT NULL DEFAULT TRUE CHECK (_singleton = TRUE)` and a unique index `uq_{table}_singleton ON {table} (_singleton)`).
> - The application layer continues to perform read-before-write validation via `count_by_type` in `DocumentsService`.

## Context

`DocumentKind::SingleType` means a document type may have **at most one** `DocumentInstance`.
This invariant must be reliably enforced.

**Constraint from the user**: `DocumentType` must NOT be restructured as a sum type (`enum DocumentTypeContent`).
`DocumentType` stays a plain struct with a `kind: DocumentKind` field — identical shape regardless of kind.
The invariant is enforced at the **logic level only** (application service and/or DB).

## Decision Drivers

- `DocumentType` aggregate stays simple — same struct for `Collection` and `SingleType`
- No change to the repository interface shape for `DocumentType`
- The invariant must be impossible to violate in normal usage
- Unit-testable without a database is desirable but not mandatory
- Works on both standard PostgreSQL and AWS DSQL

## Considered Alternatives

### Option A: DB unique constraint only

A partial unique index on `document_instances(document_type_id)` filtered to `SingleType` document types.
The `DocumentInstanceRepository::create` catches the unique constraint violation and maps it to a
`DomainError::SingleTypeAlreadyExists`.

```sql
CREATE UNIQUE INDEX uix_single_type_instance
  ON document_instances (document_type_id)
  WHERE document_type_id IN (
    SELECT id FROM document_types WHERE kind = 'single_type'
  );
```

**Pros**
- Zero domain changes — `DocumentType` and its repository are untouched
- Guaranteed at the data layer regardless of code path
- Works identically on standard PG and DSQL (partial indexes are supported on both)

**Cons**
- Invariant is invisible in the `domain` crate — not expressed in code, only in a migration
- Violation discovered on DB write, not earlier (no early rejection in the application service)
- Unit tests for this invariant require a running DB

---

### Option B: Application service guard only

The application service checks before inserting:

```rust
// in application/src/use_cases/create_document_instance.rs
async fn execute(&self, cmd: CreateDocumentInstance) -> Result<DocumentInstance, AppError> {
    let doc_type = self.type_repo.find_by_id(cmd.document_type_id).await?;
    if doc_type.kind == DocumentKind::SingleType {
        let exists = self.instance_repo.exists_for_type(cmd.document_type_id).await?;
        if exists {
            return Err(AppError::SingleTypeAlreadyExists(cmd.document_type_id));
        }
    }
    // … proceed with creation
}
```

No DB constraint. The guard is a read-before-write in the application layer.

**Pros**
- Invariant is expressed in code, in the `application` crate — readable and reviewable
- Fails fast before any DB write
- Testable with a fake `DocumentInstanceRepository` (no DB required)

**Cons**
- **Race condition**: two concurrent requests can both pass the `exists` check and both insert
  (no serialisation between the check and the insert)
- Requires all creation paths to go through this specific use-case — bypassing it violates the invariant
- Not a hard guarantee; depends on discipline in the codebase

---

### Option C: Application service guard + DB unique constraint (defence in depth)

Combine Option B (early rejection, testable) with Option A (hard DB guarantee, race-safe).
`DocumentType` aggregate is unchanged — still a plain struct.

```rust
// application layer: guard as in Option B (fast, testable, user-friendly error)
// infrastructure layer: DB unique index as in Option A (catches races, direct-DB writes)
// repository: maps unique constraint violation → DomainError::SingleTypeAlreadyExists
```

**Pros**
- Application service guard: readable invariant, unit-testable, early error
- DB constraint: eliminates race conditions, protects against bypassed service layer
- `DocumentType` stays a simple struct — no aggregate restructuring

**Cons**
- Two mechanisms to maintain (but they are independent — neither depends on the other's implementation)
- DB constraint violation path is harder to test (requires a true concurrent test or raw DB insert)

---

## Recommendation

**Option C** — defence in depth without aggregate restructuring. The application guard makes the
invariant visible and testable; the DB constraint makes it unbreakable under concurrency.
Implementation cost is low: one use-case guard + one migration.

## Decision

Accepted Option C (defence in depth). Subsequently updated and superseded by ADR-007 / ADR-008 / ADR-009 for per-type table mapping.

## Consequences

- `DocumentType` struct: **no changes** — `kind: DocumentKind` field, same for all kinds.
- Application layer checks for single-instance presence before insertion.
- Database enforces single-row invariant at the physical schema level.
- `DomainError::SingleTypeAlreadyExists(DocumentTypeId)` represents violation of the singleton invariant.
