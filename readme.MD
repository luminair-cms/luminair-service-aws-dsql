# Luminair Backend

Luminair is a schema-driven CMS backend written in Rust, exposing a Strapi-like REST API.

## Deployment

**Primary target**: AWS (Aurora DSQL, ECS / Lambda, IAM auth).

**Also works** in any local, Docker, or Kubernetes environment with a standard PostgreSQL database —
no AWS-specific code leaks into the `domain` or `application` layers.
AWS-specific adapters (DSQL IAM token refresh, etc.) are isolated in the `infrastructure` crate
and are swappable without touching business logic.

| Environment | Database | Notes |
|---|---|---|
| Local / dev | PostgreSQL (Docker) | `DATABASE_URL=postgres://...` |
| CI | PostgreSQL | `#[sqlx::test]` provisions automatically |
| Staging / prod | AWS Aurora DSQL | IAM token auth; UUID v7 PKs required |
| Self-hosted (K8s) | PostgreSQL | Same binary, standard pg connection string |

## Configuration

The service is configured entirely via environment variables parsed at startup by `ServerConfig` in the `infrastructure` composition layer ([`infrastructure/src/composition/config.rs`](infrastructure/src/composition/config.rs)).

### Environment Variables

| Variable | Type | Required | Default | Description |
|---|---|---|---|---|
| `BOOTSTRAP_MODE` | String | No | `service` | Bootstrap execution mode: `service` (default), `migrate` (migration-only), or `dry-run` (validation only). Can also be passed as CLI arguments. |
| `DATABASE_URL` | String | **Yes** | — | PostgreSQL or AWS Aurora DSQL connection URL (e.g. `postgres://user:pass@localhost:5432/luminair`). |
| `DATABASE_MAX_CONNECTIONS` | u32 | No | `20` | Maximum size of the SQLx connection pool. |
| `SCHEMA_DIR` | Path | No | `schema` | Path to directory containing schema definitions (`document-types/*.json`, `relations/*.json`). |
| `HOST` | String | No | `0.0.0.0` | IP address for the HTTP server to bind to. |
| `PORT` | u16 | No | `8080` | TCP port for the HTTP server to listen on. |
| `AUTH_SECRET` | String | Conditional* | — | Shared HMAC secret for symmetric JWT verification (`SecretTokenValidator`). Useful for local dev, test environments, and CI. |
| `AUTH_ISSUER_URL` | String | Conditional* | — | OIDC identity provider issuer URL (e.g. AWS Cognito User Pool `https://cognito-idp.{region}.amazonaws.com/{userPoolId}` or Keycloak realm) for JWKS asymmetric JWT verification (`JwksTokenValidator`). |
| `AUTH_AUDIENCE` | String | No | — | Expected JWT audience (`aud` claim). When provided, tokens with mismatched audience claims are rejected. |
| `BOOTSTRAP_ADMIN_SUB` | String | No | — | OIDC subject identifier (`sub`) for the initial system administrator. On startup, an idempotent hook assigns the system `Administrator` role to this user if not already present. |
| `BOOTSTRAP_AUTH_TYPE` | String | No | `oidc` | Descriptive auth provider identifier stored in the shadow user record for the bootstrap admin (e.g., `cognito`, `keycloak`, `oidc`). |

> **\*Authentication Requirement**: At least one of `AUTH_SECRET` or `AUTH_ISSUER_URL` must be configured. If neither is set, server initialization will fail with an error. If `AUTH_SECRET` is set, HMAC symmetric validation takes precedence.

### Bootstrap & CLI Execution Modes

Luminair supports three bootstrap modes, selectable via CLI subcommands, flags, or the `BOOTSTRAP_MODE` environment variable:

1. **Standard Service Mode (default)**:
   ```bash
   # CLI command
   luminair
   # or
   luminair serve

   # Or via environment variable
   BOOTSTRAP_MODE=service luminair
   ```
   Connects to the database pool, executes static migrations, synchronizes dynamic document schemas, initializes authentication, runs the idempotent administrator bootstrap hook, and starts the Axum HTTP REST server.

2. **Migration-Only CLI Mode**:
   ```bash
   # CLI command
   luminair migrate
   # or
   luminair --migrate

   # Or via environment variable
   BOOTSTRAP_MODE=migrate luminair
   ```
   Connects to the database pool, executes static migrations (`run_migrations`) and dynamic schema synchronization (`sync_schemas`), prints executed DDL statements, and immediately exits. Ideal for Kubernetes init-containers, AWS ECS migration tasks, and CI/CD deployment pipelines.

3. **Dry-Run CLI Mode**:
   ```bash
   # CLI command
   luminair dry-run
   # or
   luminair --dry-run
   # or
   luminair check

   # Or via environment variable
   BOOTSTRAP_MODE=dry-run luminair
   ```
   Loads and validates the configuration (`DATABASE_URL` format, auth token validator setup) and all declarative schemas from `SCHEMA_DIR` (`document-types/` and `relations/`) without connecting to or mutating the database. Ideal for pre-deployment checks, linting, and CI validation pipelines.

### Example Configurations

#### Local Development (`.env`)

```bash
# Database
DATABASE_URL=postgres://postgres:postgres@localhost:5432/luminair
DATABASE_MAX_CONNECTIONS=10

# Server
HOST=127.0.0.1
PORT=8080
SCHEMA_DIR=schema

# Authentication (Local HMAC Secret)
AUTH_SECRET=local-dev-jwt-secret-min-32-characters-long
BOOTSTRAP_ADMIN_SUB=admin-local-001
BOOTSTRAP_AUTH_TYPE=local
```

#### Production with AWS Cognito & Aurora DSQL

```bash
# Database
DATABASE_URL=postgres://app_user@dsql-cluster.dsql.us-east-1.on.aws:5432/postgres?sslmode=require
DATABASE_MAX_CONNECTIONS=50

# Server
HOST=0.0.0.0
PORT=8080
SCHEMA_DIR=/app/schema

# Authentication (Cognito OIDC / JWKS)
AUTH_ISSUER_URL=https://cognito-idp.us-east-1.amazonaws.com/us-east-1_ExamplePoolId
AUTH_AUDIENCE=2exampleclientid123456789
BOOTSTRAP_ADMIN_SUB=a1b2c3d4-e5f6-7890-abcd-ef1234567890
BOOTSTRAP_AUTH_TYPE=cognito
```

## Documentation & Planning

- [Implementation Roadmap](docs/roadmap.md) — complete milestone plan from domain model to UI
- [Architecture Guide](docs/architecture.md) — hexagonal boundaries and crate responsibilities
- [Data Model & DSQL](docs/data-model.md) — persistence schema and DSQL nuances
- [REST API Specifications](docs/api.md) — endpoint surface and conventions

