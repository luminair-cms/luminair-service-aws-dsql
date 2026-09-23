# ADR-004: Available Locales — System Level vs. Document Type Level

- **Status**: Accepted
- **Date**: 2026-09-17 · Decision: 2026-09-21
- **Deciders**: Dmitri Astafiev
- **Research**: [docs/research/domain-model-design.md](../research/domain-model-design.md)

## Decision

**Option B accepted: System-level locales only.**

Available locales are defined once in a `SystemConfig` entity, shared across all document types.
`DocumentTypeOptions` has **no locale list**.

```rust
struct SystemConfig {
    id:               SystemConfigId,
    available_locales: Vec<LocaleId>,   // BCP 47 tags: "en", "uk", "de", …
    default_locale:   LocaleId,
}
```

`LocalizedText` field validation rejects locale keys not in `SystemConfig.available_locales`.

## Rationale

Defining locales per-document-type is repetitive and creates drift when adding a new locale.
A single system-level source of truth is simpler and sufficient for MVP.

Per-type locale restriction (Option C) can be added later by introducing
`DocumentTypeOptions.locale_subset: Option<Vec<LocaleId>>` without breaking existing data.

## Consequences

- New domain entity: `SystemConfig` in `domain/src/entities/system_config.rs`
- New repository trait: `SystemConfigRepository`
- New DB table: `system_config` (single row, enforced by application logic)
- `DocumentTypeOptions` has no locale field
- `LocalizedText` validation is a domain service that receives `Vec<LocaleId>` from `SystemConfig`
- Migration seeds `system_config` with at least one locale (`"en"` default)

## Follow-up Actions

- [x] Decision made
- [ ] Add `SystemConfig` entity to domain model implementation plan
- [ ] Define `LocaleId` as validated BCP 47 newtype
- [ ] Seed migration: insert default `SystemConfig` row
