# ADR-010: Admin Dashboard UI Architecture & Deployment Strategy

- **Status**: `Proposed`
- **Date**: 2026-09-25
- **Deciders**: Dmitri Astafiev, Antigravity
- **Research**: [`docs/research/ui-architecture-options.md`](../research/ui-architecture-options.md)
- **Related ADRs**:
  - [ADR-001: Hexagonal Architecture](./ADR-001-hexagonal-architecture.md)
  - [ADR-005: Authentication Strategy](./ADR-005-auth-strategy.md)
  - [ADR-008: Unified Naming Conventions and REST Routing Strategy](./ADR-008-naming-conventions-and-routing.md)

---

## Context

With the completion of Milestones 1 through 6, Luminair provides a fully functional, headless CMS backend with domain modeling, application use cases, static and dynamic PostgreSQL/AWS Aurora DSQL persistence, OIDC authentication, and a Strapi 5-style REST API surface.

To enable content authors, editors, and administrators to interact with the system, Luminair requires an administrative web dashboard. The key operational challenges of this UI include:
1. **Dynamic Schema-Driven Rendering**: Forms cannot be statically hardcoded because document types, attributes, constraints, and relations are defined dynamically at startup via JSON schema files and introspected via `/api/schema/document-types`.
2. **Complex Field Types & Multi-Locale Editing**: The dashboard must support localized fields (`LocalizedText`) with multi-language tabs, relational entity pickers (`HasOne` and `HasMany`), JSON structures, and draft/publish status indicators.
3. **Authentication & Onboarding**: Seamless OIDC integration supporting Cognito, Keycloak, or Auth0, handling access request lifecycles (`ACCESS_PENDING`, `ACCESS_REJECTED`, `ACCESS_NOT_REQUESTED`).
4. **AWS Serverless Alignment**: Deployment and infrastructure costs should scale to zero when idle, matching the serverless philosophy of AWS Aurora DSQL.

---

## Decision Drivers

- **Dynamic Form Ergonomics**: Rich ecosystem of schema-driven form generators, validation libraries, and data grids.
- **Architectural Integrity**: Preservation of Hexagonal Architecture boundaries—the backend must remain a pure headless REST API without leaking HTML rendering or presentation templates into the Rust crates.
- **AWS Serverless Cost & Scalability**: Zero ongoing compute cost for static asset delivery via AWS S3 and Amazon CloudFront.
- **Compilation & Toolchain Isolation**: The frontend build process must not degrade Rust workspace compilation times, dependency graphs, or CI speed.
- **Developer Velocity**: Access to mature UI component libraries (e.g. Radix UI, Shadcn UI, Tailwind CSS, TanStack Table) for rapid assembly of polished CMS interfaces.

---

## Considered Alternatives

### Option A: Decoupled Single-Page Application (SPA) (React 19 + TypeScript + Vite + Tailwind CSS / Shadcn UI) deployed to S3 / CloudFront

A modern TypeScript SPA living in a dedicated `frontend/` directory within this repository (monorepo structure with isolated package management). Communicates with the backend exclusively via HTTPS REST API calls.

**Pros**:
- Massive ecosystem of accessible UI components ([Shadcn UI](https://ui.shadcn.com/), [Radix UI](https://www.radix-ui.com/)) and battle-tested form managers ([React Hook Form](https://react-hook-form.com/), [Zod](https://zod.dev/)).
- Dynamic schema rendering is native and straightforward to implement via recursive component mapping.
- Hosted on AWS S3 behind Amazon CloudFront with Origin Access Control (OAC): zero server compute cost, instant global edge caching, and automated HTTPS certificate provisioning via ACM.
- Complete separation of concerns: Frontend and backend can be tested, linted, and deployed independently in CI/CD.
- Zero impact on Rust compiler performance or binary size.

**Cons**:
- Requires a separate frontend build toolchain (Node.js / pnpm / Vite).
- Requires configuring CORS when the API and frontend reside on different domains in development (solved via CloudFront routing or Vite dev proxy).

---

### Option B: Embedded SPA (Vite + React built and served directly by Axum binary)

Same frontend stack as Option A, but the compiled frontend assets (`dist/`) are packaged into the Rust binary or served via `tower-http::services::ServeDir` from the Axum HTTP server.

**Pros**:
- Single deployment artifact (one container or binary serves both `/api/*` and `/*`).
- Eliminates CORS issues since all requests share the same origin.

**Cons**:
- Couples Rust backend CI/CD to Node.js/pnpm build steps.
- Increases Docker image size and Axum memory/network footprint.
- Misses out on native CloudFront edge caching optimizations unless placed behind CloudFront anyway.

---

### Option C: Fullstack Rust WebAssembly (Leptos or Dioxus)

An internal UI crate (`ui/`) in the Cargo workspace compiling to WebAssembly (`wasm32-unknown-unknown`).

**Pros**:
- Unified language (100% Rust) across frontend and backend.
- Shared domain types and DTOs between crates.

**Cons**:
- Very heavy Wasm bundle sizes (typically 1.5MB to 8MB uncompressed), leading to slow initial page loads.
- Extremely limited ecosystem for complex CMS widgets (rich text WYSIWYG, nested relation pickers, complex responsive data tables).
- Substantially increases Rust compile times and CI build pipelines.
- Steep learning curve for typical frontend contributors.

---

### Option D: Server-Side Rendering (SSR) with HTMX + Askama Templates in Axum

Axum handlers render HTML pages and fragments server-side using Askama or Minijinja templates, with dynamic interactivity provided by HTMX.

**Pros**:
- Zero Node.js build tooling required; pure Cargo workflow.
- Fast initial page load with tiny JavaScript footprint.

**Cons**:
- Fundamentally conflicts with the headless API architecture: requires duplicating endpoints to return HTML fragments rather than uniform JSON envelopes and RFC 9457 error details.
- Server-side generation of dynamic, recursive schema forms with client-side multi-locale tabs is rigid and difficult to maintain.
- Stateful interactions (e.g. relation selection modals, client draft state) require significant ad-hoc JavaScript, negating the simplicity benefit.

---

## Proposed Decision

**Chosen Option: Option A (Decoupled SPA with React 19, TypeScript, Vite, Tailwind CSS, and Shadcn UI, deployed to AWS S3 + CloudFront)**, with **Vite Dev Server Proxy** for seamless local development and optional fallback support for Option B (serving static `dist/` in single-container environments):

1. **Repository Layout**:
   - The frontend will be housed in a `/frontend` directory in the repository root.
   - The root Cargo workspace remains completely decoupled from Node.js dependencies.
2. **Technology Stack**:
   - **Framework & Bundler**: React 19 + TypeScript + Vite.
   - **Design System & Styling**: Tailwind CSS + Shadcn UI (accessible Radix UI primitives + Lucide icons).
   - **State & Data Fetching**: TanStack Query (React Query) for API caching, mutation states, and automatic background refetching.
   - **Forms & Validation**: React Hook Form with dynamic schema generation derived from `/api/schema/document-types`.
   - **Tables & Pagination**: TanStack Table v8 for sorting, filtering, and paginating collection instances.
   - **Auth**: `oidc-client-ts` / `react-oidc-context` for OIDC provider integration (AWS Cognito / Keycloak).
3. **AWS Deployment Strategy**:
   - Production static assets (`index.html`, `.js`, `.css`, assets) deployed to a private AWS S3 bucket.
   - Amazon CloudFront CDN distribution with Origin Access Control (OAC), serving static files at root `/` and routing `/api/*` to the Axum backend load balancer / ECS service.
   - Eliminates CORS in production while retaining zero-compute static asset costs.

---

## Consequences

### Positive
- **Optimal CMS User Experience**: Rich, responsive, stateful client interface with smooth multi-language tab switching, modal relation pickers, and dynamic schema forms.
- **Architectural Cleanliness**: The Rust backend remains 100% headless, adhering strictly to Hexagonal Architecture. The backend API is consumable by any client (web, mobile, third-party integrations).
- **Cost Efficiency**: Zero compute cost for frontend delivery via AWS S3 and CloudFront edge caching.
- **Independent Lifecycles**: Frontend and backend can be developed, tested, and deployed independently without rebuilding the other.
- **Developer Accessibility**: Standard, modern TypeScript/React ecosystem allows any web developer to contribute without requiring Rust expertise.

### Negative / Trade-offs
- Introduces Node.js / pnpm tooling to the repository (isolated under `frontend/`).
- Requires configuring CloudFront routing rules or API CORS headers for cross-origin communication.

### Risks & Mitigations
- **Schema Drift between Backend & Frontend**:
  - *Mitigation*: The frontend does not hardcode schemas; it introspects `/api/schema/document-types` on load, ensuring that adding or modifying JSON schema files in the backend is instantly and automatically reflected in the UI upon server restart.

---

## Follow-up Actions

- [ ] Present ADR-010 to user for review and formal acceptance.
- [ ] Initialize `frontend/` directory with Vite, React 19, TypeScript, and Tailwind CSS.
- [ ] Setup Shadcn UI primitives and TanStack Query client.
- [ ] Implement Dynamic Schema Form Engine reading from `/api/schema/document-types`.
- [ ] Implement Collection and Singleton management views with Draft/Publish lifecycle.
- [ ] Implement Access Request administration and OIDC authentication flow.
