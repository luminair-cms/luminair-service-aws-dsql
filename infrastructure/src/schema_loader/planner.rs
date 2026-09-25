//! Topological Migration Step Planner.
//!
//! Orders DDL `MigrationStep`s into a dependency-safe `MigrationPlan`:
//!
//! Destruction Phase:
//! 1. Drop Indexes
//! 2. Drop Link Tables (dependent on entity tables)
//! 3. Drop Published Tables (dependent on entity tables)
//! 4. Drop Entity Tables (base tables)
//! 5. Drop Columns
//!
//! Construction Phase:
//! 6. Create Entity Tables (base tables)
//! 7. Create Published Tables (dependent on entity tables)
//! 8. Create Link Tables (dependent on entity tables)
//! 9. Add Columns
//! 10. Create Indexes

use super::diff::MigrationStep;
use super::model::TableKind;

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
    let mut drop_link_tables = Vec::new();
    let mut drop_published_tables = Vec::new();
    let mut drop_entity_tables = Vec::new();
    let mut drop_columns = Vec::new();

    let mut create_entity_tables = Vec::new();
    let mut create_published_tables = Vec::new();
    let mut create_link_tables = Vec::new();
    let mut add_columns = Vec::new();
    let mut create_indexes = Vec::new();

    for step in raw_steps {
        match step {
            MigrationStep::DropIndex { .. } => drop_indexes.push(step),
            MigrationStep::DropTable { kind, .. } => match kind {
                TableKind::Link => drop_link_tables.push(step),
                TableKind::Published => drop_published_tables.push(step),
                TableKind::Entity => drop_entity_tables.push(step),
            },
            MigrationStep::DropColumn { .. } => drop_columns.push(step),
            MigrationStep::CreateTable(ref table) => match table.kind {
                TableKind::Entity => create_entity_tables.push(step),
                TableKind::Published => create_published_tables.push(step),
                TableKind::Link => create_link_tables.push(step),
            },
            MigrationStep::AddColumn { .. } => add_columns.push(step),
            MigrationStep::CreateIndex(_) => create_indexes.push(step),
        }
    }

    let mut planned = Vec::new();
    planned.extend(drop_indexes);
    planned.extend(drop_link_tables);
    planned.extend(drop_published_tables);
    planned.extend(drop_entity_tables);
    planned.extend(drop_columns);

    planned.extend(create_entity_tables);
    planned.extend(create_published_tables);
    planned.extend(create_link_tables);
    planned.extend(add_columns);
    planned.extend(create_indexes);

    MigrationPlan { steps: planned }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_loader::model::{IndexDefinition, TableDefinition, TableKind};

    #[test]
    fn test_topological_ordering() {
        let entity_table = TableDefinition::new("articles", TableKind::Entity, false);
        let published_table =
            TableDefinition::new("articles__published", TableKind::Published, false);
        let link_table = TableDefinition::new("articles__tags_link", TableKind::Link, false);
        let index_def = IndexDefinition::new("idx_test", "articles", vec!["title".into()], false);

        let raw = vec![
            MigrationStep::CreateIndex(index_def),
            MigrationStep::CreateTable(link_table),
            MigrationStep::DropTable {
                name: "old_entity".into(),
                kind: TableKind::Entity,
            },
            MigrationStep::CreateTable(published_table),
            MigrationStep::CreateTable(entity_table),
            MigrationStep::DropTable {
                name: "old_link".into(),
                kind: TableKind::Link,
            },
            MigrationStep::DropTable {
                name: "old_published".into(),
                kind: TableKind::Published,
            },
        ];

        let plan = plan_migrations(raw);
        assert_eq!(plan.steps.len(), 7);

        // 1. Drop link table
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::DropTable {
                kind: TableKind::Link,
                ..
            }
        ));
        // 2. Drop published table
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::DropTable {
                kind: TableKind::Published,
                ..
            }
        ));
        // 3. Drop entity table
        assert!(matches!(
            &plan.steps[2],
            MigrationStep::DropTable {
                kind: TableKind::Entity,
                ..
            }
        ));
        // 4. Create entity table
        assert!(matches!(
            &plan.steps[3],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Entity
        ));
        // 5. Create published table
        assert!(matches!(
            &plan.steps[4],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Published
        ));
        // 6. Create link table
        assert!(matches!(
            &plan.steps[5],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Link
        ));
        // 7. Create index
        assert!(matches!(&plan.steps[6], MigrationStep::CreateIndex(_)));
    }
}
