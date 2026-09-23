# ADR-003: Relation Representation

- **Status**: Accepted — Option F
- **Date**: 2026-09-17 · Revised: 2026-09-21 (×2) · Accepted: 2026-09-21
- **Deciders**: Dmitri Astafiev
- **Research**: [docs/research/domain-model-design.md](../research/domain-model-design.md)

## Context

Relations between `DocumentType`s need to be defined in the schema layer and resolved at runtime.
The evolution of this ADR:

- **Original (Options A–C)**: two `RelationDefinition`s on each side + a linking entity → three objects for one concept, rejected
- **Revised (Options D–E)**: one entity covering both sides, but D lacked unidirectional support; E kept relations embedded in `DocumentType`, creating asymmetry
- **This revision (Option F)**: user proposed making **all relations** separate entities, with `DocumentType` only holding references — generalising Option D to cover both uni- and bidirectional in a single entity type

## Decision Drivers

- One creation operation for any relation (uni or bidirectional)
- `DocumentType` aggregate is clean — no inline relation structs
- Navigable both ways: given a type, find all its relations
- Single entity type for all relation variants (uniform model)
- Clear lifecycle — create and delete as a unit
- Compatible with future relation kinds (e.g. polymorphic, self-referential)

---

## Considered Alternatives

*(Options A–E from previous revisions are archived below for history. Option F is the new proposal.)*

---

### Option F: Unified `Relation` entity — `DocumentType` holds no inline relations

One entity type covers **all** relations. The `inverse` field is `Option` — `None` means unidirectional,
`Some` means bidirectional. `DocumentType` carries **no relation fields at all**.

```rust
// domain/src/entities/relation.rs

struct Relation {
    id:          RelationId,          // Uuid v7 — own identity, own lifecycle
    // Owner side — always present (the "managing" end; for unidirectional, the only end)
    owner_type:  DocumentTypeId,
    owner_attr:  AttributeId,         // attribute name as it appears on owner_type's schema
    owner_kind:  OwnerRelationKind,   // HasOne | HasMany
    // Inverse side — present only if bidirectional
    inverse:     Option<RelationInverse>,
    created_at:  DateTime<Utc>,
}

struct RelationInverse {
    inverse_type: DocumentTypeId,
    inverse_attr: AttributeId,        // attribute name as it appears on inverse_type's schema
    // inverse_kind is always derived — never stored:
    //   HasOne  → BelongsToOne
    //   HasMany → BelongsToMany
}

enum OwnerRelationKind { HasOne, HasMany }
```

`DocumentType` has **no relation fields**. A type's full schema (fields + relations) is loaded
by querying the `Relation` aggregate:

```rust
// RelationRepository trait (domain/src/ports/relation_repository.rs)
#[async_trait]
trait RelationRepository {
    // Returns all relations where this type is the owner OR the inverse participant
    async fn find_by_participant(&self, type_id: DocumentTypeId) -> Result<Vec<Relation>>;
    async fn find_by_id(&self, id: RelationId) -> Result<Option<Relation>>;
    async fn save(&self, relation: &Relation) -> Result<()>;
    async fn delete(&self, id: RelationId) -> Result<()>;
}
```

**Loading a type's complete schema** (application layer):
```rust
let fields    = field_repo.find_by_type(type_id).await?;
let relations = relation_repo.find_by_participant(type_id).await?;
// Each Relation carries both sides; the caller computes which side belongs to this type:
let my_relations: Vec<RelationView> = relations.iter()
    .map(|r| r.view_for(type_id))   // domain method returning the correct AttributeId and kind
    .collect();
```

**`Relation::view_for(type_id) → RelationView`** (pure domain logic):
```rust
enum RelationView {
    Owner   { attr: AttributeId, kind: OwnerRelationKind,   other_type: DocumentTypeId },
    Inverse { attr: AttributeId, kind: InverseRelationKind, other_type: DocumentTypeId },
    Unidirectional { attr: AttributeId, kind: OwnerRelationKind, target_type: DocumentTypeId },
}
```

**Creation — one call**:
```rust
// application/src/use_cases/create_relation.rs
Relation::new(owner_type, owner_attr, owner_kind, inverse: Option<RelationInverse>)
```

**Deletion — one call**: removes the entity; both sides disappear atomically.

---

**Pros**

- **Uniform**: one entity type, one repository, one table for all relations
- **Truly one operation**: creating any relation (uni or bi) is a single entity creation
- **No inline relations on `DocumentType`**: the aggregate stays small, focused on schema metadata
- **Navigable**: `find_by_participant(type_id)` returns all relations a type participates in, regardless of which side
- **Clean lifecycle**: `Relation` has its own `RelationId`, its own created_at, deletable independently
- **Unidirectional is natural**: `inverse: None` — no special-case handling needed
- **Future-proof**: adding new relation variants (e.g. polymorphic) requires only new fields on `Relation` or new enum variants, not a new entity type

**Cons**

- `DocumentType` aggregate is not self-contained — loading a full schema requires two queries (fields + relations)
- The `view_for(type_id)` computation must be available wherever relations are used (domain method, low cost)
- DB query `find_by_participant` uses `OR` clause (`owner_type_id = ? OR inverse_type_id = ?`) — needs a composite index or two indexes for performance

---

### Archived Options (for history)

| Option | Summary | Why superseded |
|---|---|---|
| A | `inverse_of` field on `RelationDefinition`, two separate objects | Lifecycle implicit; cross-type reference not strongly typed |
| B | `BiDirectionalRelation` entity + two `RelationDefinition`s | Three objects for one concept; creation is a three-step operation |
| C | API-layer pairing only, no domain entity | Not enforceable at domain level |
| D | `BiDirectionalRelation` entity, no `RelationDefinition` | Unidirectional relations need a second entity type |
| E | `RelationDefinition` embeds inverse, computed projection | `DocumentType` asymmetric (owner self-contained, inverse is derived) |

---

## Recommendation

**Option F** — the user's intuition is correct. Making all relations a separate aggregate with
`DocumentType` only as a participant (not owner) is the cleanest boundary. It resolves all
previous cons: one entity type, one operation, both uni and bidirectional, uniform navigation.

## Decision

*Pending human approval — status: Proposed*

## Consequences

**If Option F is accepted**:
- New domain entity: `Relation` in `domain/src/entities/relation.rs`
- **No `RelationRepository`**: `Relation` is immutable config loaded from JSON at startup (ADR-006).
  `RelationRepository` described in the original Option F prose is **superseded** — `SchemaRegistry`
  provides `find_relations_for(type_id)` as the runtime access point (in-memory, no I/O).
- **No `relations` DB table**: removed in ADR-007 revision; drift detection uses `information_schema`
- `DocumentType` aggregate: **no changes** to its fields — only its schema-loading query changes
- `RelationView` has **three** variants: `OwnerSide`, `InverseSide`, `Unidirectional`
  (unidirectional = `inverse: None`; distinct from bidirectional owner side)
- Application layer: schema-loading via `SchemaLoader` assembles `SchemaRegistry` at startup

## Follow-up Actions

- [x] Approve Option F
- [x] `RelationRepository` superseded — `SchemaRegistry` is the sole relation access point
- [ ] Confirm: are self-referential relations (type A HasMany type A) in scope for MVP?
- [ ] Define valid kind pairs: `HasOne ↔ BelongsToOne`, `HasMany ↔ BelongsToMany`
