//! Topological Migration Step Planner.
//!
//! Orders DDL `MigrationStep`s into a dependency-safe `MigrationPlan`:
//!
//! Destruction Phase:
//! 1. Drop Indexes
//! 2. Drop Published Link Tables (dependent on published tables and entity tables)
//! 3. Drop Draft Link Tables (dependent on entity tables)
//! 4. Drop Published Tables (dependent on entity tables)
//! 5. Drop Entity Tables (base tables)
//! 6. Drop Columns
//!
//! Construction Phase:
//! 7. Create Entity Tables (base tables)
//! 8. Create Published Tables (dependent on entity tables)
//! 9. Create Draft Link Tables (dependent on entity tables)
//! 10. Create Published Link Tables (dependent on published tables and entity tables)
//! 11. Add Columns
//! 12. Create Indexes

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
    let mut drop_published_link_tables = Vec::new();
    let mut drop_draft_link_tables = Vec::new();
    let mut drop_published_tables = Vec::new();
    let mut drop_entity_tables = Vec::new();
    let mut drop_columns = Vec::new();

    let mut create_entity_tables = Vec::new();
    let mut create_published_tables = Vec::new();
    let mut create_draft_link_tables = Vec::new();
    let mut create_published_link_tables = Vec::new();
    let mut add_columns = Vec::new();
    let mut create_indexes = Vec::new();

    for step in raw_steps {
        match step {
            MigrationStep::DropIndex { .. } => drop_indexes.push(step),
            MigrationStep::DropTable { ref name, kind } => match kind {
                TableKind::Link => {
                    if name.ends_with("__published") {
                        drop_published_link_tables.push(step);
                    } else {
                        drop_draft_link_tables.push(step);
                    }
                }
                TableKind::Published => drop_published_tables.push(step),
                TableKind::Entity => drop_entity_tables.push(step),
            },
            MigrationStep::DropColumn { .. } => drop_columns.push(step),
            MigrationStep::CreateTable(ref table) => match table.kind {
                TableKind::Entity => create_entity_tables.push(step),
                TableKind::Published => create_published_tables.push(step),
                TableKind::Link => {
                    if table.name.ends_with("__published") {
                        create_published_link_tables.push(step);
                    } else {
                        create_draft_link_tables.push(step);
                    }
                }
            },
            MigrationStep::AddColumn { .. } => add_columns.push(step),
            MigrationStep::CreateIndex(_) => create_indexes.push(step),
        }
    }

    let mut planned = Vec::new();
    planned.extend(drop_indexes);
    planned.extend(drop_published_link_tables);
    planned.extend(drop_draft_link_tables);
    planned.extend(drop_published_tables);
    planned.extend(drop_entity_tables);
    planned.extend(drop_columns);

    planned.extend(create_entity_tables);
    planned.extend(create_published_tables);
    planned.extend(create_draft_link_tables);
    planned.extend(create_published_link_tables);
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
        let draft_link_table = TableDefinition::new("articles__tags_link", TableKind::Link, false);
        let pub_link_table =
            TableDefinition::new("articles__tags_link__published", TableKind::Link, false);
        let index_def = IndexDefinition::new("idx_test", "articles", vec!["title".into()], false);

        let raw = vec![
            MigrationStep::CreateIndex(index_def),
            MigrationStep::CreateTable(pub_link_table),
            MigrationStep::CreateTable(draft_link_table),
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
                name: "old_link__published".into(),
                kind: TableKind::Link,
            },
            MigrationStep::DropTable {
                name: "old_published".into(),
                kind: TableKind::Published,
            },
        ];

        let plan = plan_migrations(raw);
        assert_eq!(plan.steps.len(), 9);

        // 1. Drop published link table
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::DropTable {
                name,
                kind: TableKind::Link,
            } if name == "old_link__published"
        ));
        // 2. Drop draft link table
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::DropTable {
                name,
                kind: TableKind::Link,
            } if name == "old_link"
        ));
        // 3. Drop published table
        assert!(matches!(
            &plan.steps[2],
            MigrationStep::DropTable {
                kind: TableKind::Published,
                ..
            }
        ));
        // 4. Drop entity table
        assert!(matches!(
            &plan.steps[3],
            MigrationStep::DropTable {
                kind: TableKind::Entity,
                ..
            }
        ));
        // 5. Create entity table
        assert!(matches!(
            &plan.steps[4],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Entity
        ));
        // 6. Create published table
        assert!(matches!(
            &plan.steps[5],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Published
        ));
        // 7. Create draft link table
        assert!(matches!(
            &plan.steps[6],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Link && t.name == "articles__tags_link"
        ));
        // 8. Create published link table
        assert!(matches!(
            &plan.steps[7],
            MigrationStep::CreateTable(t) if t.kind == TableKind::Link && t.name == "articles__tags_link__published"
        ));
        // 9. Create index
        assert!(matches!(&plan.steps[8], MigrationStep::CreateIndex(_)));
    }
}
