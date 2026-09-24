-- no-transaction
-- Migration: 20260924000002_seed_builtin_roles.sql
-- Description: Seed built-in roles (admin, editor, viewer) and permissions

-- 1. Insert built-in roles
INSERT INTO roles (id, name, description, created_at, updated_at)
VALUES
    ('01920000-0000-7000-8000-000000000001', 'admin', 'Administrator with full system privileges', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('01920000-0000-7000-8000-000000000002', 'editor', 'Content editor capable of authoring and publishing content', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('01920000-0000-7000-8000-000000000003', 'viewer', 'Read-only viewer for document instances', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT (id) DO NOTHING;

-- 2. Insert admin permissions (all system and document actions)
INSERT INTO role_permissions (id, role_id, action, document_type_id)
VALUES
    ('01920000-0000-7000-8000-000000000010', '01920000-0000-7000-8000-000000000001', 'ManageSchema', NULL),
    ('01920000-0000-7000-8000-000000000011', '01920000-0000-7000-8000-000000000001', 'ManageRoles', NULL),
    ('01920000-0000-7000-8000-000000000012', '01920000-0000-7000-8000-000000000001', 'ManageUsers', NULL),
    ('01920000-0000-7000-8000-000000000013', '01920000-0000-7000-8000-000000000001', 'CreateDocument', NULL),
    ('01920000-0000-7000-8000-000000000014', '01920000-0000-7000-8000-000000000001', 'ReadDocument', NULL),
    ('01920000-0000-7000-8000-000000000015', '01920000-0000-7000-8000-000000000001', 'UpdateDocument', NULL),
    ('01920000-0000-7000-8000-000000000016', '01920000-0000-7000-8000-000000000001', 'DeleteDocument', NULL),
    ('01920000-0000-7000-8000-000000000017', '01920000-0000-7000-8000-000000000001', 'PublishDocument', NULL)
ON CONFLICT (id) DO NOTHING;

-- 3. Insert editor permissions (create, read, update, publish for all types)
INSERT INTO role_permissions (id, role_id, action, document_type_id)
VALUES
    ('01920000-0000-7000-8000-000000000020', '01920000-0000-7000-8000-000000000002', 'CreateDocument', NULL),
    ('01920000-0000-7000-8000-000000000021', '01920000-0000-7000-8000-000000000002', 'ReadDocument', NULL),
    ('01920000-0000-7000-8000-000000000022', '01920000-0000-7000-8000-000000000002', 'UpdateDocument', NULL),
    ('01920000-0000-7000-8000-000000000023', '01920000-0000-7000-8000-000000000002', 'PublishDocument', NULL)
ON CONFLICT (id) DO NOTHING;

-- 4. Insert viewer permissions (read for all types)
INSERT INTO role_permissions (id, role_id, action, document_type_id)
VALUES
    ('01920000-0000-7000-8000-000000000030', '01920000-0000-7000-8000-000000000003', 'ReadDocument', NULL)
ON CONFLICT (id) DO NOTHING;
