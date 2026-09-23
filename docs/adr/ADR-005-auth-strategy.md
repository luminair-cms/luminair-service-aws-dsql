# ADR-005: Authentication and Authorization Strategy

- **Status**: Accepted
- **Date**: 2026-09-21 · Revised: 2026-09-21 (×2) · Accepted: 2026-09-21
- **Deciders**: Dmitri Astafiev
- **Research**: [docs/research/auth-strategy.md](../research/auth-strategy.md)

## Context

Luminair authenticates requests via an external OIDC provider (Cognito / AWS / Google in production,
Keycloak or any OIDC-compliant provider in self-hosted environments) and enforces document-level
read/write access control via RBAC + owner rule.

**Settled**:
- `UserId` = OIDC `sub` claim (opaque string newtype)
- Users **always have an existing IdP account** before accessing Luminair (Workflow 2 removed)
- Authorization: RBAC + owner rule
- First admin: **one-time bootstrap endpoint** (not env var)

## Decision Drivers

- No IdP-specific code in `domain` or `application`
- Users must explicitly request access; admin approves before first use
- Bootstrap must be secure and self-disabling after first use
- Portability: works with Cognito, Google, Keycloak, any OIDC provider without code changes

---

## Enrollment Workflow

User already has an account in their IdP (Cognito / Google / Keycloak).

```
User authenticates with their IdP → receives JWT (access_token)
          │
User sends:  POST /api/access-requests
             Authorization: Bearer <JWT>
             (no body required — identity is extracted from the JWT)
          │
Luminair middleware:
  1. Validates JWT signature (JWKS from IdP discovery endpoint)
  2. Checks expiry, audience, issuer
  3. Extracts sub → UserId, email, name from claims
  4. Upserts shadow_users record (infrastructure only)
          │
Application layer:
  5. Checks: does an active AccessRequest already exist for this UserId?
     - Pending  → return 200 "already pending"
     - Approved → return 200 "already approved"
     - None     → create AccessRequest { status: Pending }
          │
Returns 202 Accepted  { "status": "pending", "message": "Awaiting admin approval." }
          │
Admin:  GET  /api/admin/access-requests          ← lists pending requests (email, name, sub)
        POST /api/admin/access-requests/{id}/approve  { "role_ids": ["<editor-role-id>"] }
          │
Luminair:
  - Sets AccessRequest.status = Approved
  - Creates UserRoleAssignment(s) for assigned roles
  - Returns 200
          │
User can now make authenticated requests → proceeds normally
```

**Re-request after rejection**: a rejected user can call `POST /api/access-requests` again;
a new `AccessRequest` is created (old rejected one is preserved for audit).

---

## Bootstrap: First Admin via Config

### Problem

The very first admin has no `AccessRequest` or `UserRoleAssignment`. No one can approve
their own request because no admin exists yet. A chicken-and-egg problem.

### Solution: Config-Based Startup Seeding

The bootstrap admin's identity is declared in the deployment configuration.
At application startup, Luminair seeds the admin automatically — **no HTTP endpoint, no runtime
interaction required**.

**Config variables** (environment / config file):

```toml
# .env or deployment config
BOOTSTRAP_ADMIN_SUB  = "abc123..."   # OIDC sub of the first admin (required for bootstrap)
BOOTSTRAP_AUTH_TYPE  = "cognito"     # "cognito" | "keycloak" | "google" | "oidc"
                                     # optional — used for audit/display; defaults to "oidc"
```

**Startup sequence** (`infrastructure/src/bootstrap.rs`):

```
On application start:
  1. Read BOOTSTRAP_ADMIN_SUB from config
     Not set → skip bootstrap (normal operation)
  2. Check: does a UserRoleAssignment { user_id: sub, role: admin } already exist?
     YES → log "Bootstrap: admin already exists, skipping" → skip
     NO  → proceed:
  3. Upsert shadow_users { user_id: sub, auth_type: BOOTSTRAP_AUTH_TYPE, source: "bootstrap" }
  4. Create AccessRequest { user_id: sub, status: Approved, source: "bootstrap" }
  5. Create UserRoleAssignment { user_id: sub, role: admin, granted_by: "bootstrap" }
  6. Log "Bootstrap: admin seeded for sub=<sub>"
```

**Security properties**:
- Admin identity is set by the operator in deployment config — controlled at infra level, not by any HTTP caller
- Idempotent: re-running with the same sub is a no-op (step 2 guards)
- If `BOOTSTRAP_ADMIN_SUB` is unset after bootstrapping → no effect on existing data
- Additional admins are promoted via normal role management by the first admin
- `BOOTSTRAP_AUTH_TYPE` is purely informational — the JWT validation config (`AUTH_ISSUER_URL`, `AUTH_AUDIENCE`) controls actual authentication; `AUTH_TYPE` is stored for display in the admin UI

**Operational note**: set `BOOTSTRAP_ADMIN_SUB` on first deployment, verify admin can log in,
then optionally remove or leave the var (it is idempotent). Document the sub value in the
deployment runbook alongside the IdP credentials.

---

## `AccessRequest` Domain Entity

```rust
// domain/src/entities/auth/access_request.rs

#[nutype(validate(not_empty), derive(Debug, Clone, PartialEq, Eq, Hash))]
pub struct AccessRequestId(Uuid);

pub struct AccessRequest {
    pub id:            AccessRequestId,
    pub user_id:       UserId,              // OIDC sub
    pub email:         Option<String>,      // from JWT at request time — for admin display only
    pub name:          Option<String>,      // from JWT at request time — for admin display only
    pub requested_at:  DateTime<Utc>,
    pub status:        AccessRequestStatus,
    pub reviewed_by:   Option<UserId>,
    pub reviewed_at:   Option<DateTime<Utc>>,
    pub assigned_roles: Vec<RoleId>,        // populated on approval
}

pub enum AccessRequestStatus {
    Pending,
    Approved,
    Rejected { reason: Option<String> },
}

impl AccessRequest {
    /// Create a new pending request. Caller must verify no active request exists.
    pub fn new(user_id: UserId, email: Option<String>, name: Option<String>) -> Self { … }

    /// Approve and return the UserRoleAssignments to be persisted.
    pub fn approve(
        &mut self,
        by: UserId,
        roles: Vec<RoleId>,
        now: DateTime<Utc>,
    ) -> Result<Vec<UserRoleAssignment>, DomainError> { … }

    /// Reject the request.
    pub fn reject(&mut self, by: UserId, reason: Option<String>, now: DateTime<Utc>)
        -> Result<(), DomainError> { … }
}
```

**Invariants**:
- At most one `Pending` or `Approved` request per `UserId` at any time
- A `Rejected` request is immutable after rejection (preserved for audit)
- `assigned_roles` is only non-empty on `Approved` requests

---

## Shadow User (Infrastructure Only)

Not a domain entity. Infrastructure-layer record updated on every authenticated request.

```rust
// infrastructure/src/auth/shadow_user.rs
struct ShadowUser {
    user_id:    String,           // = OIDC sub
    email:      Option<String>,
    name:       Option<String>,
    first_seen: DateTime<Utc>,
    last_seen:  DateTime<Utc>,
}
```

The admin UI joins `shadow_users` with `access_requests` to show the approval queue with
human-readable names and emails. The domain never reads this table.

---

## Middleware Flow (Every Authenticated Request)

```
Authorization: Bearer <JWT> present?
  NO  → 401 Unauthorized

Validate JWT (signature, expiry, audience, issuer via JWKS cache)
  INVALID → 401 Unauthorized

Extract sub → UserId, email, name

Is this the bootstrap endpoint?
  YES → run bootstrap logic (see above), skip steps below

Upsert shadow_users (last_seen, email, name)

Does an Approved AccessRequest / UserRoleAssignment exist for this UserId?
  YES → inject UserId into request context → proceed to handler
  NO, Pending exists  → 403 { "code": "ACCESS_PENDING" }
  NO, Rejected exists → 403 { "code": "ACCESS_REJECTED" }
  NO, nothing exists  → 403 { "code": "ACCESS_NOT_REQUESTED",
                              "hint": "POST /api/access-requests to request access" }
```

---

## RBAC Model

```rust
// domain/src/entities/auth/

pub struct Role {
    pub id:          RoleId,
    pub name:        String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

pub enum Permission {
    ManageSchema,
    CreateDocument(Option<DocumentTypeId>),   // None = all types
    ReadDocument(Option<DocumentTypeId>),
    UpdateDocument(Option<DocumentTypeId>),
    DeleteDocument(Option<DocumentTypeId>),
    PublishDocument(Option<DocumentTypeId>),
    ManageRoles,
    ManageUsers,   // includes approving / rejecting access requests
}

pub struct UserRoleAssignment {
    pub id:         UserRoleAssignmentId,
    pub user_id:    UserId,
    pub role_id:    RoleId,
    pub granted_at: DateTime<Utc>,
    pub granted_by: UserId,
}
```

### Authorization Algorithm

```
can(user_id, action, document_instance?) → bool

1. Owner rule:
   document_instance.audit.created_by == user_id → ALLOW

2. RBAC:
   for each UserRoleAssignment of user_id:
     if role.permissions.contains(action) → ALLOW

3. → DENY
```

### Built-in Roles (seed data)

| Role | Permissions |
|---|---|
| `admin` | All |
| `editor` | Create/Read/Update/PublishDocument (all types) |
| `viewer` | ReadDocument (all types) |

### Layer Responsibilities

| Layer | What lives here |
|---|---|
| `domain` | `Permission`, `Role`, `UserRoleAssignment`, `AccessRequest`, `UserId` newtype |
| `application` | `AuthorizationService` (evaluates algorithm), use-cases: `RequestAccess`, `ApproveAccess`, `RejectAccess`, `Bootstrap` |
| `infrastructure` | JWT validation, JWKS cache, shadow_user upsert, auth middleware, repositories |

Domain and application have **zero knowledge of JWT**.

---

## Decision

*Pending human approval — status: Proposed*

## Consequences

- New domain entities: `AccessRequest`, `Role`, `UserRoleAssignment`
- New repository traits: `AccessRequestRepository`, `RoleRepository`, `UserRoleAssignmentRepository`
- Infrastructure only: `ShadowUser`, `ShadowUserRepository`, JWT middleware, `bootstrap.rs` startup hook
- DB tables: `access_requests`, `roles`, `role_permissions`, `user_role_assignments`, `shadow_users`
- Seed migration: 3 built-in roles (`admin`, `editor`, `viewer`)
- Config variables added: `BOOTSTRAP_ADMIN_SUB`, `BOOTSTRAP_AUTH_TYPE`, `AUTH_ISSUER_URL`, `AUTH_AUDIENCE`
- New endpoints (to be added to `docs/api.md`):
  - `POST /api/access-requests`
  - `GET  /api/admin/access-requests`
  - `POST /api/admin/access-requests/{id}/approve`
  - `POST /api/admin/access-requests/{id}/reject`
- No bootstrap HTTP endpoint — bootstrap is a startup-time side effect, invisible to the API surface

## Follow-up Actions

- [ ] Approve this ADR
- [ ] Add access-request endpoints to `docs/api.md`
- [ ] Define `UserId` as `nutype` newtype in `domain/src/value_objects/user_id.rs`
- [ ] Define `AuthorizationService` trait in `domain`, implement in `application`
- [ ] Add auth and bootstrap tables to infrastructure migrations
- [ ] Add `BOOTSTRAP_ADMIN_SUB` / `BOOTSTRAP_AUTH_TYPE` to `.env.example`
- [ ] Document bootstrap procedure in deployment runbook
