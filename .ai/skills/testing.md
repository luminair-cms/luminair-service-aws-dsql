# Skill: Writing Tests for Luminair

Load this skill when asked to write or modify tests.
For the overall test strategy, see [`docs/testing.md`](../../docs/testing.md).

## Decision Tree: Which Test to Write

```
New code is in…
  domain/    → inline unit test, no DB, no async (usually)
  application/ → unit test with fake repository (InMemory*)
  infrastructure/ → integration test, real DB, #[tokio::test]
  API handler → E2E test via HTTP client (reqwest / axum TestClient)
```

## Fake Repository Pattern (Application Tests)

```rust
use std::collections::HashMap;
use uuid::Uuid;
use domain::ports::EntryRepository;
use domain::entities::Entry;

struct FakeEntryRepository {
    store: HashMap<Uuid, Entry>,
}

impl FakeEntryRepository {
    fn new() -> Self { Self { store: HashMap::new() } }
}

#[async_trait::async_trait]
impl EntryRepository for FakeEntryRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Entry>, domain::Error> {
        Ok(self.store.get(&id).cloned())
    }
    // … other methods
}
```

## Integration Test Skeleton

```rust
// infrastructure/tests/entry_repository_test.rs
use sqlx::PgPool;

#[sqlx::test(migrations = "../migrations")]
async fn test_create_entry(pool: PgPool) {
    // sqlx::test provisions a clean DB and runs migrations automatically
    let repo = SqlxEntryRepository::new(pool);
    // … arrange, act, assert
}
```

Prefer `#[sqlx::test]` over manual setup — it handles migration and teardown.

## Axum Handler Test Skeleton

```rust
use axum::http::StatusCode;
use axum_test::TestServer; // axum-test crate

#[tokio::test]
async fn test_get_entry_not_found() {
    let app = build_app(/* test state */);
    let server = TestServer::new(app).unwrap();
    let resp = server.get("/api/entries/00000000-0000-0000-0000-000000000000").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
```

## Naming Conventions

- Unit tests: `test_{method}_{scenario}` e.g. `test_create_entry_returns_id`
- Integration tests: descriptive, no prefix needed — file name provides context
- Use `assert_eq!` / `assert!(matches!(…))` — avoid string-matching errors

## What NOT to Do

- Do not call `unwrap()` in tests — use `?` with `anyhow::Result` return type
- Do not share global mutable state between tests
- Do not test infrastructure in domain test modules
