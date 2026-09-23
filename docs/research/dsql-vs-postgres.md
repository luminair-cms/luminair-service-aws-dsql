# Research: AWS DSQL vs Standard PostgreSQL

- **Date**: 2026-08-21
- **Question**: What are the differences between AWS Aurora DSQL and standard PostgreSQL that affect application code, migrations, and the ORM layer?
- **Related ADR**: ADR-001 (DSQL chosen as database)

---

## Findings

### 1. Primary Keys — No Sequences / SERIAL

AWS DSQL does not support PostgreSQL sequences or `SERIAL` / `BIGSERIAL` columns.

> *"DSQL does not support sequences."* — AWS DSQL documentation

**Impact**: All PKs must be client-generated. Use **UUID v7** (`uuid::Uuid::now_v7()`) — time-ordered, so B-tree index performance is similar to auto-increment integers.

Never use:
```sql
id SERIAL PRIMARY KEY
id BIGSERIAL PRIMARY KEY
CREATE SEQUENCE …
```

Always use:
```sql
id UUID PRIMARY KEY DEFAULT gen_random_uuid()
-- or generate UUID v7 in application code
```

### 2. DDL and Transactions

DDL statements (`CREATE TABLE`, `ALTER TABLE`, `DROP TABLE`, etc.) **cannot be run inside a transaction** in DSQL. Standard PostgreSQL allows transactional DDL.

**Impact**: `sqlx` migrations that wrap DDL in `BEGIN … COMMIT` will fail. Each DDL migration file must be a standalone statement, not wrapped in a transaction block.

sqlx migration setting to use: `#[allow_non_transactional_migrations]` or split DDL into separate migration files.

### 3. Optimistic Concurrency Control (OCC)

DSQL uses OCC instead of traditional pessimistic locking. Concurrent writes to the same row may result in a **transaction conflict error** (HTTP 409 / `OccConflictException`).

**Impact**: The infrastructure layer must detect this error and **retry with exponential backoff**. Do not propagate OCC conflicts to the API client as 409 — retry transparently up to N times, then return 503.

Relevant sqlx/PostgreSQL error code to catch: `40001` (serialization failure).

### 4. Connection Authentication — IAM Token

DSQL uses **short-lived IAM authentication tokens** (valid ~15 minutes) instead of static passwords.

**Impact**:
- Connection string password must be refreshed before expiry
- The connection pool must support token rotation without full restart
- Use the AWS SDK to generate tokens: `dsql_auth_token(endpoint, region)`

### 5. PostgreSQL Feature Compatibility

DSQL is **not 100% PostgreSQL-compatible**. Known unsupported or limited features (verify against current AWS docs):

| Feature | DSQL support |
|---|---|
| Sequences / SERIAL | ❌ Not supported |
| Transactional DDL | ❌ Not supported |
| `LISTEN` / `NOTIFY` | ❌ Not supported |
| Foreign keys | ⚠️ Supported but cross-shard FKs have constraints |
| Full-text search | ⚠️ Limited — verify before use |
| `pg_catalog` views | ⚠️ Partial |
| Stored procedures | ✅ Basic support |
| JSONB | ✅ Supported |
| UUID | ✅ Supported |
| Standard DML | ✅ Full support |

> **Always verify against current AWS DSQL release notes** — the service is actively evolving.

### 6. sqlx Compatibility

`sqlx` works with DSQL via the PostgreSQL driver. Known considerations:

- Use `sqlx::query!` macro with `DATABASE_URL` pointing at DSQL endpoint
- Compile-time query checking (`sqlx prepare`) works if DSQL is reachable at build time; otherwise use offline mode (`.sqlx/` directory)
- `#[sqlx::test]` requires a standard PostgreSQL instance for CI (DSQL may not be available in CI environment — use a local PG for unit/integration tests, DSQL only in staging/prod)

### 7. Distributed Transactions

DSQL supports distributed transactions across shards but with higher latency than single-shard operations.

**Impact**: Keep related entities in the same logical group where possible to minimise cross-shard transactions. Avoid very large transactions.

---

## Open Questions

- Does `sqlx migrate run` need any flags to skip transaction wrapping on DDL migrations?
- What is the exact error code/type returned by sqlx for OCC conflicts?
- Are foreign keys between `content_types` and `entries` cross-shard in practice?

---

## Sources

- [AWS DSQL Developer Guide](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/what-is-aurora-dsql.html)
- [DSQL Limitations](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-aurora-dsql-limitations.html)
- [DSQL IAM Auth](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/security-iam.html)
- [sqlx offline mode](https://docs.rs/sqlx/latest/sqlx/macro.query.html#offline-mode)
