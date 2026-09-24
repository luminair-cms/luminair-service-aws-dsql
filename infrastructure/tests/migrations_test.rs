//! Tests for static SQL migrations and AWS DSQL compatibility rules.

use std::fs;
use std::path::Path;

use infrastructure::migrations::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, run_migrations,
};
use sqlx::PgPool;

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

#[test]
fn test_migration_files_exist_and_readable() {
    let m1 = Path::new(MIGRATIONS_DIR).join("20260924000001_create_system_tables.sql");
    let m2 = Path::new(MIGRATIONS_DIR).join("20260924000002_seed_builtin_roles.sql");

    assert!(
        m1.exists(),
        "20260924000001_create_system_tables.sql must exist"
    );
    assert!(
        m2.exists(),
        "20260924000002_seed_builtin_roles.sql must exist"
    );

    let content_m1 = fs::read_to_string(&m1).expect("readable m1");
    let content_m2 = fs::read_to_string(&m2).expect("readable m2");

    assert!(!content_m1.trim().is_empty(), "m1 must not be empty");
    assert!(!content_m2.trim().is_empty(), "m2 must not be empty");
}

#[test]
fn test_all_migrations_have_no_transaction_directive() {
    let entries = fs::read_dir(MIGRATIONS_DIR).expect("read migrations dir");

    for entry in entries {
        let entry = entry.expect("valid entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            let content = fs::read_to_string(&path).expect("read sql file");
            let first_line = content
                .lines()
                .find(|l| !l.trim().is_empty())
                .expect("has non-empty line");

            assert_eq!(
                first_line.trim(),
                "-- no-transaction",
                "Migration file {:?} must start with '-- no-transaction' for AWS DSQL compatibility",
                path.file_name()
            );
        }
    }
}

#[test]
fn test_dsql_compatibility_forbidden_keywords() {
    let entries = fs::read_dir(MIGRATIONS_DIR).expect("read migrations dir");

    let forbidden_patterns = ["SERIAL", "BIGSERIAL", "CREATE SEQUENCE", "SMALLSERIAL"];

    for entry in entries {
        let entry = entry.expect("valid entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            let content = fs::read_to_string(&path).expect("read sql file");
            let upper = content.to_uppercase();

            for forbidden in forbidden_patterns {
                assert!(
                    !upper.contains(forbidden),
                    "Migration {:?} contains forbidden keyword '{}'. DSQL does not support sequences or serial types; use client-generated UUID v7.",
                    path.file_name(),
                    forbidden
                );
            }
        }
    }
}

#[test]
fn test_ddl_and_seed_idempotency_patterns() {
    let m1 = Path::new(MIGRATIONS_DIR).join("20260924000001_create_system_tables.sql");
    let content_m1 = fs::read_to_string(m1).expect("read m1");

    for line in content_m1.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("CREATE TABLE") {
            assert!(
                trimmed.contains("IF NOT EXISTS"),
                "CREATE TABLE statement must include IF NOT EXISTS for idempotency: '{line}'"
            );
        }
        if trimmed.starts_with("CREATE INDEX") || trimmed.starts_with("CREATE UNIQUE INDEX") {
            assert!(
                trimmed.contains("IF NOT EXISTS"),
                "CREATE INDEX statement must include IF NOT EXISTS for idempotency: '{line}'"
            );
        }
    }

    let m2 = Path::new(MIGRATIONS_DIR).join("20260924000002_seed_builtin_roles.sql");
    let content_m2 = fs::read_to_string(m2).expect("read m2");
    assert!(
        content_m2.contains("ON CONFLICT (id) DO NOTHING"),
        "Seed migration must use ON CONFLICT (id) DO NOTHING for idempotency"
    );
}

#[test]
fn test_embedded_migrator_contains_all_migrations() {
    let migrations = &MIGRATOR.migrations;
    assert_eq!(migrations.len(), 2, "Expected exactly 2 static migrations");

    assert_eq!(migrations[0].version, 20260924000001);
    assert_eq!(migrations[0].description, "create system tables");

    assert_eq!(migrations[1].version, 20260924000002);
    assert_eq!(migrations[1].description, "seed builtin roles");
}

#[test]
fn test_deterministic_role_constants() {
    // Assert all role constants are valid UUID v7
    assert_eq!(ROLE_ADMIN_ID.get_version_num(), 7);
    assert_eq!(ROLE_EDITOR_ID.get_version_num(), 7);
    assert_eq!(ROLE_VIEWER_ID.get_version_num(), 7);

    // Assert all IDs are distinct
    assert_ne!(ROLE_ADMIN_ID, ROLE_EDITOR_ID);
    assert_ne!(ROLE_ADMIN_ID, ROLE_VIEWER_ID);
    assert_ne!(ROLE_EDITOR_ID, ROLE_VIEWER_ID);
}

#[tokio::test]
async fn test_db_migrations_execution_and_idempotency_if_database_available() {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!("Skipping live database test: DATABASE_URL is not set");
            return;
        }
    };

    let pool = PgPool::connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // 1. Run migrations first time
    run_migrations(&pool)
        .await
        .expect("Migrations failed on initial run");

    // 2. Verify all static tables exist
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name::TEXT
        FROM information_schema.tables
        WHERE table_schema = 'public'
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query information_schema.tables");

    let table_names: Vec<String> = rows.into_iter().map(|r| r.0).collect();

    assert!(table_names.contains(&"document_snapshots".to_string()));
    assert!(table_names.contains(&"roles".to_string()));
    assert!(table_names.contains(&"role_permissions".to_string()));
    assert!(table_names.contains(&"user_role_assignments".to_string()));
    assert!(table_names.contains(&"access_requests".to_string()));
    assert!(table_names.contains(&"shadow_users".to_string()));

    // Verify system_config table is NOT created (ADR-006 / startup config decision)
    assert!(
        !table_names.contains(&"system_config".to_string()),
        "system_config table must not exist in SQL schema"
    );

    // 3. Re-run migrations to test idempotency
    run_migrations(&pool)
        .await
        .expect("Migrations must be idempotent on second run");
}
