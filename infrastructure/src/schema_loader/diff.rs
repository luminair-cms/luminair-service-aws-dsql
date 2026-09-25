//! Schema Drift Detection and Diffing Engine.
//!
//! Compares `ActualSchema` against `DesiredSchema` and generates a list of `MigrationStep`s.
//! Enforces `SafetyPolicy` (`AdditiveOnly` vs `AllowDestructive`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::model::{ColumnDefinition, DatabaseSchema, IndexDefinition, TableDefinition};

/// Safety policy governing schema migration step generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyPolicy {
    /// In production: blocks any destructive schema drop operations.
    AdditiveOnly,
    /// In testing / development / explicit CLI: allows dropping unmanaged tables, columns, or indexes.
    AllowDestructive,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DiffError {
    #[error("Destructive operation blocked by SafetyPolicy::AdditiveOnly: {operation}")]
    DestructiveOperationBlocked { operation: String },
}

/// A discrete DDL migration operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStep {
    CreateTable(TableDefinition),
    DropTable {
        name: String,
        is_junction: bool,
    },
    AddColumn {
        table: String,
        column: ColumnDefinition,
    },
    DropColumn {
        table: String,
        column: String,
    },
    CreateIndex(IndexDefinition),
    DropIndex {
        name: String,
        table: String,
    },
}

/// Computes the diff between actual and desired schema.
pub fn compute_diff(
    actual: &DatabaseSchema,
    desired: &DatabaseSchema,
    policy: SafetyPolicy,
) -> Result<Vec<MigrationStep>, DiffError> {
    let mut steps = Vec::new();

    // 1. Identify new tables to create
    for desired_table in &desired.tables {
        match actual.find_table(&desired_table.name) {
            None => {
                steps.push(MigrationStep::CreateTable(desired_table.clone()));
            }
            Some(actual_table) => {
                // Table exists in both -> check for column and index drift
                // 1a. New columns in desired
                for desired_col in &desired_table.columns {
                    if actual_table.find_column(&desired_col.name).is_none() {
                        steps.push(MigrationStep::AddColumn {
                            table: desired_table.name.clone(),
                            column: desired_col.clone(),
                        });
                    }
                }

                // 1b. Dropped columns in actual
                for actual_col in &actual_table.columns {
                    if desired_table.find_column(&actual_col.name).is_none() {
                        let op = format!("DROP COLUMN {}.{}", desired_table.name, actual_col.name);
                        match policy {
                            SafetyPolicy::AdditiveOnly => {
                                return Err(DiffError::DestructiveOperationBlocked {
                                    operation: op,
                                });
                            }
                            SafetyPolicy::AllowDestructive => {
                                steps.push(MigrationStep::DropColumn {
                                    table: desired_table.name.clone(),
                                    column: actual_col.name.clone(),
                                });
                            }
                        }
                    }
                }

                // 1c. New indexes in desired
                for desired_idx in &desired_table.indexes {
                    if actual_table.find_index(&desired_idx.name).is_none() {
                        steps.push(MigrationStep::CreateIndex(desired_idx.clone()));
                    }
                }

                // 1d. Dropped indexes in actual
                for actual_idx in &actual_table.indexes {
                    if desired_table.find_index(&actual_idx.name).is_none() {
                        let op = format!("DROP INDEX {}", actual_idx.name);
                        match policy {
                            SafetyPolicy::AdditiveOnly => {
                                return Err(DiffError::DestructiveOperationBlocked {
                                    operation: op,
                                });
                            }
                            SafetyPolicy::AllowDestructive => {
                                steps.push(MigrationStep::DropIndex {
                                    name: actual_idx.name.clone(),
                                    table: actual_table.name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Identify tables in actual that are no longer in desired
    for actual_table in &actual.tables {
        if desired.find_table(&actual_table.name).is_none() {
            let op = format!("DROP TABLE {}", actual_table.name);
            match policy {
                SafetyPolicy::AdditiveOnly => {
                    return Err(DiffError::DestructiveOperationBlocked { operation: op });
                }
                SafetyPolicy::AllowDestructive => {
                    steps.push(MigrationStep::DropTable {
                        name: actual_table.name.clone(),
                        is_junction: actual_table.is_junction,
                    });
                }
            }
        }
    }

    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_loader::model::{ColumnDefinition, SqlColumnType, TableDefinition};

    #[test]
    fn test_compute_diff_new_table() {
        let actual = DatabaseSchema::new();
        let mut desired = DatabaseSchema::new();
        let mut table = TableDefinition::new("articles", false, false);
        table.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        desired.insert_table(table);

        let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], MigrationStep::CreateTable(_)));
    }

    #[test]
    fn test_compute_diff_add_column() {
        let mut actual = DatabaseSchema::new();
        let mut table_act = TableDefinition::new("articles", false, false);
        table_act.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        actual.insert_table(table_act);

        let mut desired = DatabaseSchema::new();
        let mut table_des = TableDefinition::new("articles", false, false);
        table_des.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        table_des.columns.insert(ColumnDefinition::new(
            "views",
            SqlColumnType::BigInt,
            true,
            false,
        ));
        desired.insert_table(table_des);

        let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
        assert_eq!(steps.len(), 1);
        match &steps[0] {
            MigrationStep::AddColumn { table, column } => {
                assert_eq!(table, "articles");
                assert_eq!(column.name, "views");
            }
            _ => panic!("expected AddColumn"),
        }
    }

    #[test]
    fn test_compute_diff_destructive_blocked_in_additive_only() {
        let mut actual = DatabaseSchema::new();
        let table = TableDefinition::new("old_table", false, false);
        actual.insert_table(table);

        let desired = DatabaseSchema::new();

        let err = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap_err();
        assert!(matches!(err, DiffError::DestructiveOperationBlocked { .. }));
    }

    #[test]
    fn test_compute_diff_destructive_allowed_in_allow_destructive() {
        let mut actual = DatabaseSchema::new();
        let table = TableDefinition::new("old_table", false, false);
        actual.insert_table(table);

        let desired = DatabaseSchema::new();

        let steps = compute_diff(&actual, &desired, SafetyPolicy::AllowDestructive).unwrap();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], MigrationStep::DropTable { .. }));
    }
}
