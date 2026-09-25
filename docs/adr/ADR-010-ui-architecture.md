# ADR-010: Admin Dashboard UI Architecture & Implementation Strategy

- **Status**: `Accepted`
- **Date**: 2026-09-25
- **Deciders**: Dmitri Astafiev, Antigravity
- **Research**: [`docs/research/ui-architecture-options.md`](../research/ui-architecture-options.md)
- **Technical Specification**: [`docs/ui-architecture.md`](../ui-architecture.md)
- **Related ADRs**:
  - [ADR-001: Hexagonal Architecture](./ADR-001-hexagonal-architecture.md)
  - [ADR-005: Authentication Strategy](./ADR-005-auth-strategy.md)
  - [ADR-008: Unified Naming Conventions and REST Routing Strategy](./ADR-008-naming-conventions-and-routing.md)

---

## Context

With Milestones 1 through 6 complete, Luminair exposes a headless CMS REST API backed by AWS Aurora DSQL / PostgreSQL. The system requires an administration web panel for content editors and administrators.

Key requirements for this interface:
1. **Dynamic Schema-Driven Form Generation**: The UI must dynamically inspect registered document types at `/api/schema/document-types` and generate forms for 12 attribute types (`Text`, `LocalizedText`, `Uid`, `Uuid`, `Integer`, `Decimal`, `Date`, `DateTime`, `Boolean`, `Email`, `Url`, `Json`) and relational links (`HasOne`, `HasMany`) with client-side constraint validation.
2. **Draft & Publish Lifecycle Controls**: Visual state indicators (`Draft`, `Published`, `Modified`) and one-click publish, unpublish, and revision history inspection.
3. **Multi-Locale Editing**: Seamless editing of `LocalizedText` across all configured system locales (`SystemConfig`) without losing in-flight draft state.
4. **OIDC Authentication & Onboarding**: Seamless OIDC integration (AWS Cognito in production, Dex for lightweight local development), supporting the user onboarding lifecycle (`ACCESS_NOT_REQUESTED`, `ACCESS_PENDING`, `ACCESS_REJECTED`).
5. **Zero-Compute AWS Serverless Hosting**: Frontend delivery must be cost-efficient, scaling to zero via AWS S3 and Amazon CloudFront.

---

## Decision

We adopt **Option A: Decoupled Single-Page Application (SPA)** using **React 19, TypeScript, Mantine v7, Zustand, and TanStack Router/Query**, deployed to **AWS S3 + Amazon CloudFront** in production, paired with **Dex** as the ultra-lightweight local OIDC identity provider.

### Core Technology Stack

| Layer / Responsibility | Technology Choice | Rationale |
|---|---|---|
| **Language & Runtime** | **TypeScript 5.8+ & React 19** | Industry-standard for CMS dashboards; maximum library ecosystem and typing ergonomics. |
| **Build & Dev Tooling** | **Vite 6** | Sub-second cold start, instant Hot Module Replacement (HMR), optimized Rollup tree-shaking for static S3 hosting. |
| **UI Component System** | **Mantine v7** (`@mantine/*`) | All-in-one, fully typed, accessible design system. Includes core components, form engine (`@mantine/form`), notifications, modals, dates (`@mantine/dates`), and rich-text editing (`@mantine/tiptap`). Eliminates fragmented UI dependencies. |
| **Global & Client State** | **Zustand** (`zustand`) | Ultra-lightweight, unopinionated, hook-based state management. Manages authentication session, active locale, sidebar layout, and in-memory draft buffers with `persist` middleware. |
| **Server State & Caching** | **TanStack Query v5** (`@tanstack/react-query`) | Handles caching, optimistic updates (publish/unpublish), background refetching, and query invalidation for REST resources. |
| **Routing & Navigation** | **TanStack Router** (`@tanstack/react-router`) | 100% type-safe routing, typed search parameter parsing with Zod (for pagination and filtering), nested route layouts, and route loaders. |
| **HTTP Client** | **Axios** with Interceptors | Centralized Bearer token injection, automatic OIDC token refresh handling, and RFC 9457 `application/problem+json` error mapping. |
| **OIDC / PKCE Auth** | **`oidc-client-ts`** | Standards-compliant Authorization Code Flow with PKCE for public SPA clients. Zero server secrets needed. |
| **Local OIDC IdP** | **Dex** (`ghcr.io/dexidp/dex`) | ~15 MB RAM, sub-second boot, single static YAML file for mock users (`admin@luminair.dev`, `editor@luminair.dev`) and SPA client config. |

---

## Architectural Principles

1. **Strict Hexagonal Separation**:
   - The frontend lives in an isolated `/frontend` directory in the repository.
   - It maintains its own `package.json` and toolchain, having **zero impact** on Cargo workspace build times, dependency trees, or CI pipelines.
   - The Rust backend remains a 100% headless REST API.
2. **Schema-Driven Presentation**:
   - The UI never hardcodes document schemas or attribute lists.
   - On startup, the UI loads `/api/schema/document-types` and `/api/system/config`, dynamically configuring its navigation menus, tables, and form inputs.
   - Any schema modification in the backend JSON files is immediately reflected in the admin dashboard upon browser refresh.
3. **Public Client Security Model (OAuth 2.0 PKCE)**:
   - The SPA registers as a Public Client without a secret.
   - Tokens are kept in memory and passed via `Authorization: Bearer <access_token>`.
   - The backend validates tokens statelessly via cached JWKS public keys.
4. **CloudFront Single-Origin Routing (Production)**:
   - CloudFront distributes static assets from a private S3 bucket (`/*`) and proxies `/api/*` requests to the Axum backend load balancer/ECS container.
   - Eliminates cross-origin preflight (`OPTIONS`) latency and CORS complexities in production.

---

## Consequences

### Positive
- **Complete, Cohesive UI Suite**: Mantine provides all required CMS controls (rich text, date pickers, modals, notifications, tabbed locale inputs) under a single cohesive design system.
- **Fast & Predictable State**: Zustand provides clean, boilerplate-free state management without React context re-render thrashing.
- **Zero Ongoing Compute Costs**: Static asset hosting on S3/CloudFront costs fractions of a cent per month and scales infinitely.
- **Ultra-Fast Local Development**: Dex starts instantaneously in ~15MB RAM via Docker Compose, eliminating Keycloak's JVM overhead.

### Trade-offs & Mitigations
- **Node.js Toolchain**: Introduces Node.js and `pnpm` to the repository. Isolated strictly to `/frontend`.
- **CORS in Local Dev**: In local development, the Vite dev server runs on port `5173` while Axum runs on `3000`. Solved transparently via Vite dev proxy (`server.proxy`).

---

## Implementation Roadmap (Phase 7)

- [ ] **Milestone 7A**: Local Environment & Frontend Scaffolding (`docker-compose.dev.yml` with Dex + Vite + React 19 + Mantine setup).
- [ ] **Milestone 7B**: Authentication, PKCE & Onboarding Flow (Dex/Cognito login, callback, `useAuthStore`, `AuthGuard`, onboarding requests).
- [ ] **Milestone 7C**: App Shell, Navigation & Dynamic Schema Engine (Mantine AppShell, dynamic form renderer for all 12 field types + `LocalizedText` tabs).
- [ ] **Milestone 7D**: Content Management Views (Collections list with pagination/filter, singleton direct edit, draft/publish workflow, revision snapshots).
- [ ] **Milestone 7E**: Relational Link Picker & User Management (`HasOne`/`HasMany` modal picker, admin access requests review panel).
