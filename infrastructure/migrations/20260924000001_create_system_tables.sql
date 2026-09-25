-- no-transaction
-- Migration: 20260924000001_create_system_tables.sql
-- Description: Create all static system tables for Luminair (DSQL-compatible)

-- 1. Roles (RBAC definitions, ADR-005)
CREATE TABLE IF NOT EXISTS roles (
    id          UUID PRIMARY KEY,
    name        VARCHAR(64) NOT NULL,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_roles_name UNIQUE (name)
);

-- 3. Role Permissions (Granular and wildcard permission grants)
CREATE TABLE IF NOT EXISTS role_permissions (
    id               UUID PRIMARY KEY,
    role_id          UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    action           VARCHAR(64) NOT NULL CHECK (
        action IN (
            'ManageSchema',
            'ManageRoles',
            'ManageUsers',
            'CreateDocument',
            'ReadDocument',
            'UpdateDocument',
            'DeleteDocument',
            'PublishDocument'
        )
    ),
    document_type_id VARCHAR(64),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_role_permissions UNIQUE NULLS NOT DISTINCT (role_id, action, document_type_id)
);

CREATE INDEX IF NOT EXISTS idx_role_permissions_role_id
    ON role_permissions (role_id);

-- 4. User Role Assignments (OIDC identity to role mapping)
CREATE TABLE IF NOT EXISTS user_role_assignments (
    id         UUID PRIMARY KEY,
    user_id    VARCHAR(255) NOT NULL,
    role_id    UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL,
    granted_by VARCHAR(255),
    CONSTRAINT uq_user_role_assignments UNIQUE (user_id, role_id)
);

CREATE INDEX IF NOT EXISTS idx_user_role_assignments_user_id
    ON user_role_assignments (user_id);

CREATE INDEX IF NOT EXISTS idx_user_role_assignments_role_id
    ON user_role_assignments (role_id);

-- 5. Access Requests (User onboarding queue, ADR-005)
CREATE TABLE IF NOT EXISTS access_requests (
    id               UUID PRIMARY KEY,
    user_id          VARCHAR(255) NOT NULL,
    email            VARCHAR(320),
    name             VARCHAR(255),
    requested_at     TIMESTAMPTZ NOT NULL,
    status           VARCHAR(32) NOT NULL CHECK (status IN ('pending', 'approved', 'rejected')),
    rejection_reason TEXT,
    reviewed_by      VARCHAR(255),
    reviewed_at      TIMESTAMPTZ,
    assigned_roles   JSONB NOT NULL DEFAULT '[]'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_access_requests_user_id
    ON access_requests (user_id);

CREATE INDEX IF NOT EXISTS idx_access_requests_status
    ON access_requests (status);

-- Partial unique index: at most one active (pending or approved) request per user
CREATE UNIQUE INDEX IF NOT EXISTS uq_access_requests_active_user
    ON access_requests (user_id)
    WHERE status IN ('pending', 'approved');

-- 6. Shadow Users (Local cache of verified OIDC identities, ADR-005)
CREATE TABLE IF NOT EXISTS shadow_users (
    user_id    VARCHAR(255) PRIMARY KEY,
    email      VARCHAR(320),
    name       VARCHAR(255),
    auth_type  VARCHAR(64),
    first_seen TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_shadow_users_email
    ON shadow_users (email);
