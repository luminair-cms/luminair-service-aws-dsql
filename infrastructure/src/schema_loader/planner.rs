//! Topological Migration Step Planner.
//!
//! Orders DDL `MigrationStep`s into a dependency-safe `MigrationPlan`:
//!
//! Destruction Phase:
//! 1. Drop Indexes
//! 2. Drop Junction Tables (dependent tables)
//! 3. Drop Entity Tables (base tables)
//! 4. Drop Columns
//!
//! Construction Phase:
//! 5. Create Entity Tables
//! 6. Add Columns
//! 7. Create Junction Tables
//! 8. Create Indexes

use super::diff::MigrationStep;

/// An ordered execution plan of DDL statements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
}

impl MigrationPlan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

/// Organizes and sorts raw diff steps into a topologically sound execution order.
pub fn plan_migrations(raw_steps: Vec<MigrationStep>) -> MigrationPlan {
    let mut drop_indexes = Vec::new();
    let mut drop_junction_tables = Vec::new();
    let mut drop_entity_tables = Vec::new();
    let mut drop_columns = Vec::new();

    let mut create_entity_tables = Vec::new();
    let mut add_columns = Vec::new();
    let mut create_junction_tables = Vec::new();
    let mut create_indexes = Vec::new();

    for step in raw_steps {
        match step {
            MigrationStep::DropIndex { .. } => drop_indexes.push(step),
            MigrationStep::DropTable { is_junction, .. } => {
                if is_junction {
                    drop_junction_tables.push(step);
                } else {
                    drop_entity_tables.push(step);
                }
            }
            MigrationStep::DropColumn { .. } => drop_columns.push(step),
            MigrationStep::CreateTable(ref table) => {
                if table.is_junction {
                    create_junction_tables.push(step);
                } else {
                    create_entity_tables.push(step);
                }
            }
            MigrationStep::AddColumn { .. } => add_columns.push(step),
            MigrationStep::CreateIndex(_) => create_indexes.push(step),
        }
    }

    let mut planned = Vec::new();
    planned.extend(drop_indexes);
    planned.extend(drop_junction_tables);
    planned.extend(drop_entity_tables);
    planned.extend(drop_columns);

    planned.extend(create_entity_tables);
    planned.extend(add_columns);
    planned.extend(create_junction_tables);
    planned.extend(create_indexes);

    MigrationPlan { steps: planned }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_loader::model::{IndexDefinition, TableDefinition};

    #[test]
    fn test_topological_ordering() {
        let entity_table = TableDefinition::new("articles", false, false);
        let junction_table = TableDefinition::new("articles__tags", true, false);
        let index_def = IndexDefinition::new("idx_test", "articles", vec!["title".into()], false);

        let raw = vec![
            MigrationStep::CreateIndex(index_def),
            MigrationStep::CreateTable(junction_table),
            MigrationStep::DropTable {
                name: "old_entity".into(),
                is_junction: false,
            },
            MigrationStep::CreateTable(entity_table),
            MigrationStep::DropTable {
                name: "old_junction".into(),
                is_junction: true,
            },
        ];

        let plan = plan_migrations(raw);
        assert_eq!(plan.steps.len(), 5);

        // 1. Drop junction table
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::DropTable {
                is_junction: true,
                ..
            }
        ));
        // 2. Drop entity table
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::DropTable {
                is_junction: false,
                ..
            }
        ));
        // 3. Create entity table
        assert!(matches!(
            &plan.steps[2],
            MigrationStep::CreateTable(t) if !t.is_junction
        ));
        // 4. Create junction table
        assert!(matches!(
            &plan.steps[3],
            MigrationStep::CreateTable(t) if t.is_junction
        ));
        // 5. Create index
        assert!(matches!(&plan.steps[4], MigrationStep::CreateIndex(_)));
    }
}
