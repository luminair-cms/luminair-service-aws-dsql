//! Desired Schema Builder.
//!
//! Transforms domain `SchemaRegistry` into a normalized `DatabaseSchema` AST:
//! - Applies ADR-008 and ADR-009 naming conventions
//! - Adds standard audit columns (`id`, `version`, `owner_id`, `publication_state`, `created_at`, `updated_at`)
//! - Maps domain `FieldType`s to `SqlColumnType`s
//! - Generates foreign key columns (`{attr}_id UUID`) for 1:1 and N:1 relations
//! - Generates dedicated junction tables (`{owner}__{attr}`) for N:N relations
//! - Generates indexes for unique attributes, foreign keys, and junction targets

use domain::entities::document_type::{DocumentKind, DocumentType};
use domain::entities::relation::{OwnerRelationKind, RelationView};
use domain::services::schema_registry::SchemaRegistry;
use domain::types::field_type::{FieldType, IntegerSize, PrimitiveType};

use super::model::{
    ColumnDefinition, DatabaseSchema, IndexDefinition, SqlColumnType, TableDefinition,
};
use super::naming::{
    attribute_to_column_name, document_type_to_table_name, foreign_key_column_name, index_name,
    junction_table_name, junction_target_index_name,
};

/// Builds the desired `DatabaseSchema` AST from the domain `SchemaRegistry`.
pub fn build_desired_schema(registry: &SchemaRegistry) -> DatabaseSchema {
    let mut schema = DatabaseSchema::new();
    let mut junction_tables = Vec::new();

    // Iterate through all document types in the registry
    for name in registry.type_names() {
        if let Some(doc_type) = registry.find_type_by_name(name) {
            let (table_def, junctions) = build_table_definition(doc_type, registry);
            schema.insert_table(table_def);
            junction_tables.extend(junctions);
        }
    }

    // Insert all junction tables
    for junction in junction_tables {
        schema.insert_table(junction);
    }

    schema
}

fn build_table_definition(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
) -> (TableDefinition, Vec<TableDefinition>) {
    let table_name = document_type_to_table_name(doc_type);
    let is_singleton = doc_type.kind == DocumentKind::SingleType;
    let mut table = TableDefinition::new(&table_name, false, is_singleton);
    let mut junction_tables = Vec::new();

    // 1. Standard audit columns on every document table
    table.columns.insert(ColumnDefinition {
        name: "id".into(),
        data_type: SqlColumnType::Uuid,
        nullable: false,
        is_primary_key: true,
        default_value: None,
        unique: true,
    });

    table.columns.insert(ColumnDefinition {
        name: "version".into(),
        data_type: SqlColumnType::BigInt,
        nullable: false,
        is_primary_key: false,
        default_value: Some("1".into()),
        unique: false,
    });

    table.columns.insert(ColumnDefinition {
        name: "owner_id".into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
        unique: false,
    });

    table.columns.insert(ColumnDefinition {
        name: "publication_state".into(),
        data_type: SqlColumnType::Varchar(Some(50)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
        unique: false,
    });

    table.columns.insert(ColumnDefinition {
        name: "created_at".into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: Some("CURRENT_TIMESTAMP".into()),
        unique: false,
    });

    table.columns.insert(ColumnDefinition {
        name: "updated_at".into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: Some("CURRENT_TIMESTAMP".into()),
        unique: false,
    });

    // If SingleType, add a singleton lock column
    if is_singleton {
        table.columns.insert(ColumnDefinition {
            name: "_singleton".into(),
            data_type: SqlColumnType::Boolean,
            nullable: false,
            is_primary_key: false,
            default_value: Some("TRUE".into()),
            unique: true,
        });
        table.indexes.insert(IndexDefinition::new(
            format!("idx_{table_name}__singleton"),
            &table_name,
            vec!["_singleton".into()],
            true,
        ));
    }

    // 2. User-declared attribute columns
    for (attr_id, field_def) in &doc_type.fields {
        let col_name = attribute_to_column_name(attr_id);
        let data_type = map_field_type_to_sql(&field_def.field_type);
        let nullable = !field_def.required;
        let unique = field_def.unique;

        table.columns.insert(ColumnDefinition {
            name: col_name.clone(),
            data_type,
            nullable,
            is_primary_key: false,
            default_value: None,
            unique,
        });

        if unique {
            let idx_name = index_name(&table_name, &col_name);
            table.indexes.insert(IndexDefinition::new(
                idx_name,
                &table_name,
                vec![col_name],
                true,
            ));
        }
    }

    // 3. Relation columns and junction tables
    let relations = registry.find_relations_for(&doc_type.id);
    for rel_view in relations {
        match rel_view {
            RelationView::Unidirectional { attr, kind, .. }
            | RelationView::OwnerSide { attr, kind, .. } => {
                match kind {
                    OwnerRelationKind::HasOne => {
                        // 1:1 and N:1 relations persist as `{attr}_id UUID` column
                        let fk_col = foreign_key_column_name(&attr);
                        table.columns.insert(ColumnDefinition {
                            name: fk_col.clone(),
                            data_type: SqlColumnType::Uuid,
                            nullable: true,
                            is_primary_key: false,
                            default_value: None,
                            unique: false,
                        });

                        let idx = index_name(&table_name, &fk_col);
                        table.indexes.insert(IndexDefinition::new(
                            idx,
                            &table_name,
                            vec![fk_col],
                            false,
                        ));
                    }
                    OwnerRelationKind::HasMany => {
                        // N:N relations persist as junction table `{owner_table}__{owner_attr}`
                        let j_name = junction_table_name(&table_name, &attr);
                        let mut j_table = TableDefinition::new(&j_name, true, false);

                        j_table.columns.insert(ColumnDefinition {
                            name: "owner_id".into(),
                            data_type: SqlColumnType::Uuid,
                            nullable: false,
                            is_primary_key: true,
                            default_value: None,
                            unique: false,
                        });

                        j_table.columns.insert(ColumnDefinition {
                            name: "target_id".into(),
                            data_type: SqlColumnType::Uuid,
                            nullable: false,
                            is_primary_key: true,
                            default_value: None,
                            unique: false,
                        });

                        // Index on target_id for fast reverse lookups
                        let target_idx = junction_target_index_name(&j_name);
                        j_table.indexes.insert(IndexDefinition::new(
                            target_idx,
                            &j_name,
                            vec!["target_id".into()],
                            false,
                        ));

                        junction_tables.push(j_table);
                    }
                }
            }
            // InverseSide does not own columns or junction tables
            RelationView::InverseSide { .. } => {}
        }
    }

    (table, junction_tables)
}

/// Maps domain `FieldType` to physical `SqlColumnType`.
pub fn map_field_type_to_sql(ft: &FieldType) -> SqlColumnType {
    match ft {
        FieldType::Primitive(PrimitiveType::Text) => SqlColumnType::Text,
        FieldType::Primitive(PrimitiveType::Uid) => SqlColumnType::Varchar(Some(255)),
        FieldType::Primitive(PrimitiveType::Uuid) => SqlColumnType::Uuid,
        FieldType::Primitive(PrimitiveType::Boolean) => SqlColumnType::Boolean,
        FieldType::Primitive(PrimitiveType::Date) => SqlColumnType::Date,
        FieldType::Primitive(PrimitiveType::DateTime) => SqlColumnType::Timestamptz,
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => SqlColumnType::SmallInt,
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => SqlColumnType::Integer,
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)) => SqlColumnType::BigInt,
        FieldType::Primitive(PrimitiveType::Decimal { precision, scale }) => {
            SqlColumnType::Decimal(*precision, *scale)
        }
        FieldType::Email => SqlColumnType::Varchar(Some(255)),
        FieldType::Url => SqlColumnType::Text,
        FieldType::LocalizedText => SqlColumnType::Jsonb,
        FieldType::Json => SqlColumnType::Jsonb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::document_type::{DocumentTypeInfo, DocumentTypeOptions};
    use domain::entities::field_definition::FieldDefinition;
    use domain::entities::relation::Relation;
    use domain::value_objects::{AttributeId, DocumentTypeId, RelationId};
    use indexmap::IndexMap;
    use uuid::Uuid;

    #[test]
    fn test_build_desired_schema_with_relations_and_junctions() {
        let article_type_id = DocumentTypeId::try_new("article").unwrap();
        let author_type_id = DocumentTypeId::try_new("author").unwrap();
        let tag_type_id = DocumentTypeId::try_new("tag").unwrap();

        let mut article_fields = IndexMap::new();
        let title_attr = AttributeId::try_new("title").unwrap();
        article_fields.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr,
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![],
            },
        );
        let slug_attr = AttributeId::try_new("slug").unwrap();
        article_fields.insert(
            slug_attr.clone(),
            FieldDefinition {
                id: slug_attr,
                field_type: FieldType::Primitive(PrimitiveType::Uid),
                required: true,
                unique: true,
                constraints: vec![],
            },
        );

        let article_dt = DocumentType {
            id: article_type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Articles".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: article_fields,
        };

        let author_dt = DocumentType {
            id: author_type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Authors".into(),
                singular_name: "author".into(),
                plural_name: "authors".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: IndexMap::new(),
        };

        let tag_dt = DocumentType {
            id: tag_type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Tags".into(),
                singular_name: "tag".into(),
                plural_name: "tags".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: IndexMap::new(),
        };

        // 1:1 author relation on article
        let author_rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type: article_type_id.clone(),
            owner_attr: AttributeId::try_new("author").unwrap(),
            owner_kind: OwnerRelationKind::HasOne,
            target_type: author_type_id,
            inverse: None,
        };

        // N:N tags relation on article
        let tag_rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type: article_type_id,
            owner_attr: AttributeId::try_new("tags").unwrap(),
            owner_kind: OwnerRelationKind::HasMany,
            target_type: tag_type_id,
            inverse: None,
        };

        let registry = SchemaRegistry::new(
            vec![article_dt, author_dt, tag_dt],
            vec![author_rel, tag_rel],
        );

        let schema = build_desired_schema(&registry);

        // Verify tables: articles, authors, tags, articles__tags
        assert!(schema.find_table("articles").is_some());
        assert!(schema.find_table("authors").is_some());
        assert!(schema.find_table("tags").is_some());
        assert!(schema.find_table("articles__tags").is_some());

        let articles_table = schema.find_table("articles").unwrap();
        // Check audit columns
        assert!(articles_table.find_column("id").is_some());
        assert!(articles_table.find_column("version").is_some());
        assert!(articles_table.find_column("owner_id").is_some());
        assert!(articles_table.find_column("publication_state").is_some());
        // Check user columns
        assert!(articles_table.find_column("title").is_some());
        assert!(articles_table.find_column("slug").is_some());
        // Check FK column
        assert!(articles_table.find_column("author_id").is_some());
        // Check unique index on slug
        assert!(articles_table.find_index("idx_articles_slug").is_some());
        assert!(
            articles_table
                .find_index("idx_articles_slug")
                .unwrap()
                .unique
        );

        // Check junction table
        let junction = schema.find_table("articles__tags").unwrap();
        assert!(junction.is_junction);
        assert!(junction.find_column("owner_id").is_some());
        assert!(junction.find_column("target_id").is_some());
        assert!(junction.find_index("idx_articles__tags_target").is_some());
    }
}
