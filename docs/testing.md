# Testing Strategy

## Layers and Test Types

| Layer | Test type | Location | DB needed? |
|---|---|---|---|
| `domain` | Unit tests | `domain/src/**` (inline `#[cfg(test)]`) | No |
| `application` | Unit tests with mocked repositories | `application/src/**` | No |
| `infrastructure` | Integration tests | `tests/` (workspace root) or `infrastructure/tests/` | Yes (DSQL or local PG) |
| API | End-to-end | `tests/api/` | Yes |

## Commands

```bash
# All tests (unit + integration)
cargo test --workspace

# Unit tests only (no DB required)
cargo test --workspace --lib

# Integration tests (requires DB)
cargo test --workspace --test '*'

# Linting (mandatory, zero warnings)
cargo clippy -- -D warnings

# Security audit
cargo audit
```

## Mocking Repositories

In `application` tests, implement repository traits with in-memory fakes.
Prefer **fakes** (real logic, in-memory) over **mocks** (expectation-based) to avoid brittle tests.

Example pattern:
```rust
struct InMemoryEntryRepository {
    store: HashMap<Uuid, Entry>,
}
impl EntryRepository for InMemoryEntryRepository { … }
```

## Integration Test Database

- Use `DATABASE_URL` env var pointing at a real DSQL cluster or a local PostgreSQL instance
- Run `sqlx migrate run` before integration tests
- Each test should use a unique schema or clean up after itself

## Coverage

- Domain logic: aim for >90% line coverage
- Application layer: aim for >80%
- Infrastructure adapters: covered by integration tests

## CI

See `.github/workflows/` — CI runs `clippy`, `test`, and `audit` on every push.
