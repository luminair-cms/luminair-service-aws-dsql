# ADR-001: Hexagonal Architecture with Three-Crate Workspace

- **Status**: Accepted
- **Date**: 2026-08-21
- **Deciders**: Dmitri Astafiev

## Context

Luminair backend needs to be:
- Testable at the domain level without a running database
- Evolvable (e.g., swap AWS DSQL for another DB, or add a second transport)
- Maintainable by both humans and AI agents with clear layer boundaries

## Decision

Adopt **Hexagonal Architecture** (Ports & Adapters) implemented as a Cargo workspace with three crates:

| Crate | Role |
|---|---|
| `domain` | Core business logic; no I/O |
| `application` | Use-case orchestration; depends on `domain` |
| `infrastructure` | Adapters, binary; depends on both |

Repository traits (ports) are defined in `domain` and implemented in `infrastructure`.
The dependency rule: `infrastructure → application → domain` — never reversed.

## Consequences

- **Positive**: Domain and application layers are unit-testable without a DB; framework swap requires only `infrastructure` changes
- **Positive**: Clear guidance for AI agents — domain logic must never import infrastructure crates
- **Negative**: More boilerplate for simple CRUD (trait + impl + fake); accepted as worthwhile for a growing CMS backend
- **Negative**: Cross-crate refactors require touching multiple `Cargo.toml` files

## Alternatives Considered

- **Single crate with module hierarchy**: simpler, but harder to enforce boundaries (Rust visibility rules are insufficient)
- **Four crates with a separate `ports` crate**: adds indirection with little benefit at this scale
