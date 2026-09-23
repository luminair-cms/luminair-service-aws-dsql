# Skill: Rust Patterns for Luminair

Load this skill before writing or modifying any Rust code in this repository.

## Crate & Module Rules

- Follow the three-crate boundary strictly (see `docs/architecture.md`)
- `use` imports: group as `std` → external crates → internal crates, separated by blank lines
- Prefer `pub(crate)` over `pub` for intra-crate items; only expose what crosses crate boundaries

## Error Handling

- Use `thiserror` for all error types; one `Error` enum per crate
- Propagate with `?`; convert with `From` impls or `.map_err()`
- Never use `.unwrap()` or `.expect()` in non-test code; use `?` or handle explicitly
- In `infrastructure`, map domain/application errors to HTTP status codes in a single place (error middleware)

## Async

- Runtime: `tokio` (multi-thread)
- Annotate async functions with `#[instrument]` (tracing) for observability
- Avoid `tokio::spawn` in domain or application layers — concurrency is an infrastructure concern

## Primary Keys

- **Always use `uuid::Uuid` v7** (time-ordered): `Uuid::now_v7()`
- Never use auto-increment integers; AWS DSQL does not support sequences reliably

## Value Objects & Newtypes (DDD)

**Use [`nutype`](https://docs.rs/nutype) for all domain value objects.** This is the project standard for the newtype pattern.

`nutype` generates a validated newtype with zero boilerplate: construction validates invariants,
the inner value is only accessible after passing all rules. This enforces the DDD principle that
value objects are always valid by construction.

```rust
use nutype::nutype;

// Simple validated newtype
#[nutype(
    sanitize(trim),
    validate(not_empty, max_len = 255, regex = r"^[a-z][a-z0-9-]*$"),
    derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)
)]
pub struct DocumentTypeApiId(String);

// Numeric with range
#[nutype(
    validate(greater = 0, less_or_equal = 65535),
    derive(Debug, Clone, Copy, PartialEq, Eq)
)]
pub struct IntegerPrecision(u8);
```

**Rules**:
- Every domain identifier type (`DocumentTypeId`, `RelationId`, `AttributeId`, `UserId`, etc.) **must** use `nutype` — never use raw `String`, `Uuid`, or `i64` directly in domain signatures
- Every constrained value object (`LocaleId`, `SlugValue`, `Email`, `Url`) **must** use `nutype`
- `nutype` types live in `domain/src/value_objects/` — one file per type or grouped by concept
- The `::new(raw)` constructor returns `Result<Self, NutypeError>` — always propagate with `?`
- Do **not** use `nutype` in `infrastructure` DTOs or API request/response types — those use plain types with `validator` annotations

**Preferred `derive` set for ID types**:
```rust
derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)
```

**Preferred `derive` set for string value objects**:
```rust
derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)
```

## Serialisation

- Use `serde` with `#[serde(rename_all = "camelCase")]` on API-facing structs
- Separate domain structs from API DTOs — do not `#[derive(Serialize)]` on domain entities directly
- Use `serde_json::Value` for dynamic schema-driven fields

## Preferred Crates

| Purpose | Crate |
|---|---|
| HTTP framework | `axum` |
| Database | `sqlx` (PostgreSQL feature) |
| Async runtime | `tokio` |
| Serialisation | `serde`, `serde_json` |
| UUIDs | `uuid` (v7 feature) |
| Dates | `chrono` |
| Errors | `thiserror` (lib), `anyhow` (bins/tests only) |
| Tracing | `tracing`, `tracing-subscriber` |
| Config | `config` crate or `envy` |
| Validation | `validator` (infrastructure DTOs), `nutype` (domain value objects) |
| **Value objects / newtypes** | **`nutype`** |

## Testing

- Inline unit tests with `#[cfg(test)]` inside the module under test
- Use `tokio::test` for async tests
- See `.ai/skills/testing.md` for full testing conventions

## Clippy

Code must pass `cargo clippy -- -D warnings`. Common issues to avoid:
- Unused `mut`
- Needless borrows (`&*foo` → `foo.as_ref()`)
- `match` that could be `if let`
