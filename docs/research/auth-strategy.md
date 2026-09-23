# Research: Authentication & Authorization Strategy

- **Date**: 2026-09-17
- **Question**: How should Luminair handle authentication (who are you?) and authorization (what can you do?) given it must work with both AWS Cognito and self-hosted OIDC providers (Keycloak, etc.)?
- **Related ADR**: ADR-005 (to be created after this investigation)

---

## Problem Statement

The system has two conflicting constraints:
1. **Portability**: must run on AWS with Cognito, on K8s with Keycloak, and potentially with other OIDC providers
2. **Document-level access control**: must track `userId` and enforce R/W privileges per document

This means authentication is external (we cannot own the identity store) but authorization is internal (we must own the access control logic).

---

## Findings

### 1. Authentication — External, via OIDC/JWT

Both Cognito and Keycloak (and most modern IdPs) support **OIDC** and issue **JWT access tokens**.

The service should:
- Accept a `Bearer <JWT>` on every request
- Validate the JWT signature using the IdP's public keys (fetched from `/.well-known/jwks.json`)
- Extract claims: `sub` (the stable user identity), `email`, custom claims
- Map `sub` → internal `UserId` (either via a local user registry or using `sub` directly as `UserId`)

The infrastructure layer handles token validation. The domain layer only knows `UserId` (an opaque identifier).

**Key question**: should `UserId` be the raw OIDC `sub` claim, or a system-internal UUID? See options in the ADR section.

### 2. Authorization — Internal RBAC / Document-level ACL

The system needs to answer: "Can user X read/write document instance Y?"

Two standard models:

#### Model A: Role-Based Access Control (RBAC)

Users have roles (e.g. `Admin`, `Editor`, `Viewer`). Roles have permissions on document types (not individual instances).

```
Role: Editor → can create/update any DocumentInstance of type "Article"
Role: Viewer → can read any DocumentInstance
```

**Pros**: Simple, well-understood, easy to cache  
**Cons**: Cannot restrict access to individual instances (e.g. "editor can only edit their own articles")

#### Model B: Document-level ACL

Each `DocumentInstance` has an access control list: `HashMap<UserId, Permission>` where `Permission = Read | Write | Admin`.

**Pros**: Fine-grained control per instance  
**Cons**: Complex — ACL entries must be stored, queried, cached; large data sets have performance implications

#### Model C: Ownership + Role hybrid

Each `DocumentInstance` tracks `created_by: UserId` (owner). Rules:
- Owner always has full write access
- Global roles apply as a fallback
- Optional: additional explicit grants per instance

**Pros**: Practical and covers 80% of use cases  
**Cons**: Still need to define what "global roles" mean

### 3. UserId — External sub vs. Internal UUID

#### Use OIDC `sub` directly as UserId

`sub` is stable and globally unique within an IdP. Use it as a string `UserId` internally.

**Pros**: No user registry needed; works immediately  
**Cons**: If IdP changes (e.g. migrate from Cognito to Keycloak), all `sub` values change and historical audit data becomes orphaned

#### Maintain an internal User Registry

On first authenticated request, register the user (map `sub` + IdP → internal `UserId: Uuid v7`).
Store: `(id, sub, idp_issuer, email, created_at)`.

**Pros**: IdP-agnostic — migration possible without data corruption; `UserId` is stable across IdP changes  
**Cons**: Requires a `User` entity and migration; first-request latency for new users

### 4. Token Validation Infrastructure

- Use a JWT validation library (e.g. `jsonwebtoken` crate)
- JWKS endpoint: fetch and cache public keys; refresh on key rotation (typically using `kid` header)
- Configuration: `AUTH_ISSUER_URL`, `AUTH_AUDIENCE` env vars — works for both Cognito and Keycloak
- Cognito: issuer = `https://cognito-idp.{region}.amazonaws.com/{user_pool_id}`
- Keycloak: issuer = `https://{host}/realms/{realm}`

Both follow the same OIDC discovery pattern (`/.well-known/openid-configuration`).

### 5. Multi-tenancy Interaction

Multi-tenancy is deferred, but the auth design should not block it. If a tenant concept is introduced later:
- JWT may carry a `tenant_id` claim (Cognito custom attribute or Keycloak attribute)
- Internal `UserId` registry would link users to tenants

This does not require design decisions now but should not be excluded.

---

## Open Questions → ADR-005

1. Should `UserId` be the OIDC `sub` directly, or an internal UUID from a user registry?
2. Which authorization model: RBAC, Document ACL, or Ownership+Role hybrid?
3. Should authorization rules live in `domain` (as pure logic over value objects) or in `application` (as a service)?
4. What is the minimum viable auth for the first release?

---

## Sources

- [OIDC Core Specification](https://openid.net/specs/openid-connect-core-1_0.html)
- [AWS Cognito JWT validation](https://docs.aws.amazon.com/cognito/latest/developerguide/amazon-cognito-user-pools-using-tokens-verifying-a-jwt.html)
- [Keycloak OIDC documentation](https://www.keycloak.org/docs/latest/securing_apps/index.html#_oidc)
- [jsonwebtoken crate](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/)
- [JWKS crate for Rust](https://docs.rs/jwks-client-rs/latest/jwks_client_rs/)
