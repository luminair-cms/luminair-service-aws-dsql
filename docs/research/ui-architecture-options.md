# Research: UI Architecture Options for Admin Dashboard

- **Date**: 2026-09-25
- **Question**: Which architecture and frontend technology stack should be used for the Luminair admin dashboard, and how should it be deployed in AWS?
- **Related ADR**: [ADR-010: UI Architecture](../adr/ADR-010-ui-architecture.md)

---

## Context & Functional Requirements

The Luminair backend exposes a clean REST API (Axum) adhering to [RFC 9457 Problem Details](https://datatracker.ietf.org/doc/html/rfc9457) and Strapi 5-style conventions.
The admin dashboard is the primary human interface for content editors and system administrators.

### Core Functional Requirements
1. **Dynamic Schema-Driven Form Generation**:
   - The admin panel does not have hardcoded forms for content types.
   - At runtime, it queries `/api/schema/document-types` to introspect registered entities, attributes, and constraints.
   - It dynamically renders forms for all supported field types: `Text`, `LocalizedText` (multi-locale tabs/inputs from `SystemConfig`), `Uid`, `Uuid`, `Integer`, `Decimal`, `Date`, `DateTime`, `Boolean`, `Email`, `Url`, and `Json`.
   - Client-side validation against schema constraints (`min`, `max`, `pattern`, `required`).
2. **Collection & Singleton Management**:
   - Collections: Paginated list views (`/api/{plural_name}`), sorting, search/filter, item detail/edit view (`/api/{plural_name}/{id}`).
   - Singletons: Direct singleton edit view (`/api/{singular_name}`) without instance IDs or pagination.
3. **Publication Workflows**:
   - Visual badges for draft and published states (`Draft`, `Published`, `Modified`).
   - One-click actions for `Publish` (`POST /api/.../publish`) and `Unpublish` (`POST /api/.../unpublish`).
   - Revision history inspection (`GET /api/.../snapshots`).
4. **Relational Content Picker**:
   - Interactive modal or autocomplete picker to link related entities for `HasOne` and `HasMany` relations.
   - Display populated bidirectional relationships without recursive loops.
5. **Authentication & User Onboarding (OIDC)**:
   - Integration with external OIDC providers (AWS Cognito, Keycloak, Auth0, Google).
   - JWT storage (in-memory / secure cookies / Web Workers) and Bearer header injection.
   - Handling of onboarding states: `ACCESS_PENDING` (waiting approval screen), `ACCESS_REJECTED` (notice screen), `ACCESS_NOT_REQUESTED` (onboarding request form).
6. **Access Request Administration**:
   - Administrative review table (`/api/admin/access-requests`) for approving/rejecting user requests and assigning roles.

---

## Architectural Options Evaluated

### Option 1: Decoupled Single-Page Application (SPA) (React + TypeScript + Tailwind CSS / Vite)

#### Overview
A standalone frontend project located either in a separate subdirectory (e.g. `frontend/` or `admin-ui/`) or separate repository. Built using Vite, TypeScript, React 19, and Tailwind CSS (or Shadcn UI / Radix primitives). Deployed as static assets to AWS S3 and served globally via CloudFront.

#### Key Findings
- **Component Ecosystem**: React has the world's most mature ecosystem for dynamic form generation ([React Hook Form](https://react-hook-form.com/), [Zod](https://zod.dev/), [@tanstack/react-table](https://tanstack.com/table/latest), [TipTap](https://tiptap.dev/) / rich text editors, Monaco Editor for JSON).
- **Dynamic Schema Forms**: JSON Schema / custom schema interpreters in TypeScript can dynamically instantiate form schemas and validation rules on the fly with zero server roundtrips.
- **AWS Deployment**: Pure static assets hosted on S3 with CloudFront CDN distribution.
  - Cost: Fractions of a cent per month (effectively free under AWS free tier).
  - Scalability: Infinite scalability with zero compute overhead on the Axum backend.
  - Security: S3 bucket completely private behind Origin Access Control (OAC). Content Security Policy (CSP) and security headers managed at CloudFront viewer response level.
- **Independence & Crate Boundaries**: Zero impact on the Rust workspace compile times or binary size. Backend and frontend can be versioned, tested, and deployed independently in CI/CD pipelines.
- **Developer Talent**: Extensive availability of frontend engineers proficient in React/TypeScript.

---

### Option 2: Embedded SPA (React + TypeScript served directly by Axum binary)

#### Overview
Same frontend stack as Option 1 (Vite + React + TS), but during the Docker/release build process, the compiled static dist assets (`index.html`, `assets/*.js`, `assets/*.css`) are copied into the Docker container or embedded into the Axum binary via `rust-embed` or `tower-http::services::ServeDir`.

#### Key Findings
- **Operational Simplicity**: Single deployable artifact (one Docker container or ECS task containing both backend API and admin dashboard).
- **CORS Elimination**: Since the API (`/api/*`) and UI (`/*`) share the same domain and origin, CORS configuration is unnecessary.
- **Build Coupling**: Rust release builds require Node.js/pnpm installed in the build pipeline or a multi-stage Docker build (`node:alpine` stage $\rightarrow$ `cargo-chef` / `rust:alpine` stage).
- **Binary Size & Caching**: Assets served by Axum consume backend network bandwidth and memory/disk, whereas CloudFront edge caching is optimized for static file delivery.

---

### Option 3: Fullstack Rust WebAssembly (Leptos or Dioxus)

#### Overview
Frontend written in Rust and compiled to WebAssembly (`wasm32-unknown-unknown`). UI components use reactive signals (`leptos`) or JSX-like syntax (`dioxus`).

#### Key Findings
- **Single Language Stack**: 100% Rust across the entire project. Common data contracts (DTOs) can theoretically be shared via a shared crate.
- **Wasm Binary Size**: Typical Leptos/Dioxus Wasm bundles range from 1.5MB to 8MB (uncompressed) or 400KB - 1.5MB (gzipped/brotli). Initial page load ("time to interactive") is noticeably slower on mobile or constrained networks compared to tree-shaken JS.
- **Ecosystem Maturity for CMS UI**:
  - Extremely limited ecosystem for complex, off-the-shelf admin widgets (e.g. rich text markdown/WYSIWYG editors, complex hierarchical tree tables, drag-and-drop sortable lists, date-range pickers with localization).
  - Integrating third-party JavaScript libraries requires tedious `wasm-bindgen` / `web-sys` FFI wrappers.
- **Build Times**: Adding a Wasm frontend crate significantly increases CI build times and local incremental compilation overhead.

---

### Option 4: Server-Side Rendering (SSR) with HTMX + Axum (Askama / Minijinja)

#### Overview
Axum handlers render HTML server-side using templating engines (Askama or Minijinja). User interactions (pagination, modals, form submission) are driven by HTMX (`hx-get`, `hx-post`, `hx-swap`).

#### Key Findings
- **Simplicity for Static Forms**: Very fast initial page loads, zero JavaScript build tooling (no Node.js/Vite/webpack required).
- **Mismatch with Dynamic Schema CMS**:
  - The Luminair backend is designed as an API-first headless service. An SSR admin panel requires writing duplicate server endpoints returning HTML fragments alongside the JSON API endpoints.
  - Multi-locale editing (`LocalizedText`) requires client-side tab switching and state management without losing unsaved changes in other tabs.
  - Dynamic schema forms: Building a server-rendered recursive form generator for schemas loaded at runtime is significantly harder and more rigid to maintain than client-side component trees.
  - Rich interactive features (e.g. JSON tree editors, drag-and-drop relation linkers) inevitably require substantial vanilla JavaScript or Alpine.js, defeating the pure SSR benefit.

---

## Comparative Matrix

| Evaluation Dimension | Option 1: Decoupled SPA (React/TS/Vite) | Option 2: Embedded SPA (Vite in Axum) | Option 3: Fullstack Rust (Leptos Wasm) | Option 4: SSR + HTMX (Askama) |
|---|---|---|---|---|
| **CMS UI Widget Ecosystem** | **Excellent** (Thousands of battle-tested components) | **Excellent** (Same as Option 1) | **Poor** (Very few mature UI widgets) | **Limited** (Requires bespoke JS for rich widgets) |
| **Dynamic Schema Form Support** | **Native** (React Hook Form, dynamic JSON forms) | **Native** (Same as Option 1) | **Moderate** (Manual signal wiring required) | **Difficult** (Server-side dynamic template logic) |
| **Architectural Separation** | **Strict** (Adheres strictly to Hexagonal Ports/Adapters) | **Moderate** (API isolated, but packaging coupled) | **Moderate** (Rust-coupled) | **Poor** (Blurs headless API boundary with HTML handlers) |
| **AWS Serverless Cost** | **Lowest** (S3 + CloudFront static hosting) | **Low** (Container compute serves assets) | **Lowest / Low** (S3 or container) | **Medium** (Every interaction hits backend compute) |
| **Build & Toolchain Impact** | **Zero impact on Rust** (Separated CI workflows) | **Coupled CI** (Requires Node + Cargo in build) | **Heavy** (Drastic increase in Rust compilation times) | **Zero Node** (Cargo-only) |
| **Initial Load Performance** | **High** (Small JS chunking, CDN cached) | **High** (CDN or container cached) | **Moderate / Low** (Large Wasm binary download & parse) | **Highest** (Plain HTML) |
| **OIDC / JWT Integration** | **Native** (`oidc-client-ts` / `react-oidc-context`) | **Native** (Same as Option 1) | **Complex** (Manual JS interop for web crypto/storage) | **Complex** (Requires backend session cookie proxy) |

---

## Open Questions

1. **Monorepo vs Separate Repository**:
   - Should the admin dashboard live in a `/frontend` or `/admin-ui` directory in this monorepo, or in a dedicated git repository?
   - *Consideration*: Keeping it in this repository under `frontend/` preserves atomic versioning and end-to-end integration tests, while keeping the Cargo workspace untouched.
2. **Component Library Choice**:
   - Tailwind CSS + [Shadcn UI](https://ui.shadcn.com/) (Radix primitives + Lucide icons) vs Mantine vs Ant Design.
   - *Consideration*: Shadcn UI provides copy-paste, zero-lock-in accessible components with clean Tailwind CSS styling, ideal for modern CMS dashboard UX.

---

## Sources

- [React 19 Documentation](https://react.dev/)
- [Vite Next Generation Frontend Tooling](https://vitejs.dev/)
- [Shadcn UI Component Architecture](https://ui.shadcn.com/docs)
- [React Hook Form Dynamic Schema Validation](https://react-hook-form.com/advanced-usage#DynamicFields)
- [TanStack Table v8 Headless Datagrids](https://tanstack.com/table/v8)
- [AWS Hosting Modern Static Web Applications with CloudFront and S3](https://docs.aws.amazon.com/whitepapers/latest/best-practices-wordpress/hosting-on-s3-and-cloudfront.html)
- [Leptos WebAssembly Framework](https://leptos.dev/)
- [HTMX Dynamic Web Interfaces](https://htmx.org/)
