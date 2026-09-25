//! Desired Schema Builder.
//!
//! Transforms domain `SchemaRegistry` into a normalized `DatabaseSchema` AST:
//! - Applies ADR-008 and ADR-009 naming conventions
//! - Adds standard audit columns (`id`, `version`, `owner_id`, `publication_state`, `created_at`, `updated_at`)
//! - If `draft_and_publish: true`: creates published mirror table `{table}__published` with `id REFERENCES {table}(id) ON DELETE CASCADE`
//! - Maps domain `FieldType`s to `SqlColumnType`s
//! - Universal link tables (`{owner}__{attr}_link`) for all relations (`HasOne` and `HasMany`) with native foreign keys
//! - Generates indexes for unique attributes, HasOne owner uniqueness, and link targets

use domain::entities::document_type::{DocumentKind, DocumentType};
use domain::entities::relation::{OwnerRelationKind, RelationView};
use domain::services::schema_registry::SchemaRegistry;
use domain::types::field_type::{FieldType, IntegerSize, PrimitiveType};

use super::model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
use super::naming::{
    attribute_to_column_name, document_type_to_table_name, index_name, kebab_to_snake,
    link_owner_fk_name, link_owner_unique_index_name, link_table_name, link_target_fk_name,
    link_target_index_name, published_fk_name, published_link_table_name, published_table_name,
};

/// Builds the desired `DatabaseSchema` AST from the domain `SchemaRegistry`.
pub fn build_desired_schema(registry: &SchemaRegistry) -> DatabaseSchema {
    let mut schema = DatabaseSchema::new();
    let mut published_tables = Vec::new();
    let mut draft_link_tables = Vec::new();
    let mut published_link_tables = Vec::new();

    // 1. Iterate through all document types in the registry
    for name in registry.type_names() {
        if let Some(doc_type) = registry.find_type_by_name(name) {
            let (entity_table, opt_pub_table, links) =
                build_document_type_tables(doc_type, registry);
            schema.insert_table(entity_table);
            if let Some(pub_table) = opt_pub_table {
                published_tables.push(pub_table);
            }
            for link in links {
                if link.name.ends_with("__published") {
                    published_link_tables.push(link);
                } else {
                    draft_link_tables.push(link);
                }
            }
        }
    }

    // 2. Insert all published mirror tables
    for pub_table in published_tables {
        schema.insert_table(pub_table);
    }

    // 3. Insert all draft link tables
    for draft_link in draft_link_tables {
        schema.insert_table(draft_link);
    }

    // 4. Insert all published link tables
    for pub_link in published_link_tables {
        schema.insert_table(pub_link);
    }

    schema
}

fn build_document_type_tables(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
) -> (
    TableDefinition,
    Option<TableDefinition>,
    Vec<TableDefinition>,
) {
    let table_name = document_type_to_table_name(doc_type);
    let is_singleton = doc_type.kind == DocumentKind::SingleType;
    let mut entity_table = TableDefinition::new(&table_name, TableKind::Entity, is_singleton);

    // 1. Standard audit columns on the main entity table
    entity_table.columns.insert(ColumnDefinition {
        name: "id".into(),
        data_type: SqlColumnType::Uuid,
        nullable: false,
        is_primary_key: true,
        default_value: None,
        unique: true,
    });

    entity_table.columns.insert(ColumnDefinition {
        name: "version".into(),
        data_type: SqlColumnType::BigInt,
        nullable: false,
        is_primary_key: false,
        default_value: Some("1".into()),
        unique: false,
    });

    entity_table.columns.insert(ColumnDefinition {
        name: "owner_id".into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
        unique: false,
    });

    entity_table.columns.insert(ColumnDefinition {
        name: "publication_state".into(),
        data_type: SqlColumnType::Varchar(Some(50)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
        unique: false,
    });

    entity_table.columns.insert(ColumnDefinition {
        name: "created_at".into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: Some("CURRENT_TIMESTAMP".into()),
        unique: false,
    });

    entity_table.columns.insert(ColumnDefinition {
        name: "updated_at".into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: Some("CURRENT_TIMESTAMP".into()),
        unique: false,
    });

    // If SingleType, add a singleton lock column
    if is_singleton {
        entity_table.columns.insert(ColumnDefinition {
            name: "_singleton".into(),
            data_type: SqlColumnType::Boolean,
            nullable: false,
            is_primary_key: false,
            default_value: Some("TRUE".into()),
            unique: true,
        });
        entity_table.indexes.insert(IndexDefinition::new(
            format!("idx_{table_name}__singleton"),
            &table_name,
            vec!["_singleton".into()],
            true,
        ));
    }

    // 2. User-declared attribute columns on the entity table
    for (attr_id, field_def) in &doc_type.fields {
        let col_name = attribute_to_column_name(attr_id);
        let data_type = map_field_type_to_sql(&field_def.field_type);
        let nullable = !field_def.required;
        let unique = field_def.unique;

        entity_table.columns.insert(ColumnDefinition {
            name: col_name.clone(),
            data_type,
            nullable,
            is_primary_key: false,
            default_value: None,
            unique,
        });

        if unique {
            let idx_name = index_name(&table_name, &col_name);
            entity_table.indexes.insert(IndexDefinition::new(
                idx_name,
                &table_name,
                vec![col_name],
                true,
            ));
        }
    }

    // 3. Published mirror table (if draft_and_publish is enabled)
    let published_table = if doc_type.options.draft_and_publish {
        let pub_table_name = published_table_name(&table_name);
        let mut pub_table =
            TableDefinition::new(&pub_table_name, TableKind::Published, is_singleton);

        // id REFERENCES entity_table(id) ON DELETE CASCADE
        pub_table.columns.insert(ColumnDefinition {
            name: "id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: true,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "published_version".into(),
            data_type: SqlColumnType::BigInt,
            nullable: false,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "owner_id".into(),
            data_type: SqlColumnType::Varchar(Some(255)),
            nullable: false,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "created_at".into(),
            data_type: SqlColumnType::Timestamptz,
            nullable: false,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "updated_at".into(),
            data_type: SqlColumnType::Timestamptz,
            nullable: false,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "published_at".into(),
            data_type: SqlColumnType::Timestamptz,
            nullable: false,
            is_primary_key: false,
            default_value: Some("CURRENT_TIMESTAMP".into()),
            unique: false,
        });

        pub_table.columns.insert(ColumnDefinition {
            name: "published_by".into(),
            data_type: SqlColumnType::Varchar(Some(255)),
            nullable: true,
            is_primary_key: false,
            default_value: None,
            unique: false,
        });

        if is_singleton {
            pub_table.columns.insert(ColumnDefinition {
                name: "_singleton".into(),
                data_type: SqlColumnType::Boolean,
                nullable: false,
                is_primary_key: false,
                default_value: Some("TRUE".into()),
                unique: true,
            });
            pub_table.indexes.insert(IndexDefinition::new(
                format!("idx_{pub_table_name}__singleton"),
                &pub_table_name,
                vec!["_singleton".into()],
                true,
            ));
        }

        // Add user-declared attribute columns to published mirror table
        for (attr_id, field_def) in &doc_type.fields {
            let col_name = attribute_to_column_name(attr_id);
            let data_type = map_field_type_to_sql(&field_def.field_type);
            let nullable = !field_def.required;
            let unique = field_def.unique;

            pub_table.columns.insert(ColumnDefinition {
                name: col_name.clone(),
                data_type,
                nullable,
                is_primary_key: false,
                default_value: None,
                unique,
            });

            if unique {
                let idx_name = index_name(&pub_table_name, &col_name);
                pub_table.indexes.insert(IndexDefinition::new(
                    idx_name,
                    &pub_table_name,
                    vec![col_name],
                    true,
                ));
            }
        }

        // Foreign key to main entity table: id -> {table}.id ON DELETE CASCADE
        pub_table.foreign_keys.insert(ForeignKeyDefinition::new(
            published_fk_name(&pub_table_name),
            vec!["id".into()],
            &table_name,
            vec!["id".into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        ));

        Some(pub_table)
    } else {
        None
    };

    // 4. Universal Link Tables for relations
    let mut link_tables = Vec::new();
    let relations = registry.find_relations_for(&doc_type.id);
    for rel_view in relations {
        let (attr, kind, target_type) = match rel_view {
            RelationView::Unidirectional {
                attr,
                kind,
                target_type,
            } => (attr, kind, target_type),
            RelationView::OwnerSide {
                attr,
                kind,
                other_type,
            } => (attr, kind, other_type),
            RelationView::InverseSide { .. } => continue,
        };

        let target_dt = registry.find_type(&target_type);
        let target_table = if let Some(dt) = target_dt {
            document_type_to_table_name(dt)
        } else {
            kebab_to_snake(target_type.as_ref())
        };

        let link_name = link_table_name(&table_name, &attr);
        let mut link_table = TableDefinition::new(&link_name, TableKind::Link, false);

        link_table.columns.insert(ColumnDefinition {
            name: "owner_id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });

        link_table.columns.insert(ColumnDefinition {
            name: "target_id".into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
            unique: false,
        });

        // Foreign key referencing owner entity table
        link_table.foreign_keys.insert(ForeignKeyDefinition::new(
            link_owner_fk_name(&link_name),
            vec!["owner_id".into()],
            &table_name,
            vec!["id".into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        ));

        // Foreign key referencing target entity table
        link_table.foreign_keys.insert(ForeignKeyDefinition::new(
            link_target_fk_name(&link_name),
            vec!["target_id".into()],
            &target_table,
            vec!["id".into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        ));

        // If HasOne, enforce uniqueness on owner_id
        if kind == OwnerRelationKind::HasOne {
            link_table.indexes.insert(IndexDefinition::new(
                link_owner_unique_index_name(&link_name),
                &link_name,
                vec!["owner_id".into()],
                true,
            ));
        }

        // Index on target_id for fast reverse lookups
        link_table.indexes.insert(IndexDefinition::new(
            link_target_index_name(&link_name),
            &link_name,
            vec!["target_id".into()],
            false,
        ));
        link_tables.push(link_table);

        // If owner entity has draft_and_publish: true, generate published mirror link table
        // enforcing Option A (Dual Link Tables) and Variant 1 (Public Filter Principle)
        if doc_type.options.draft_and_publish {
            let pub_link_name = published_link_table_name(&link_name);
            let mut pub_link_table = TableDefinition::new(&pub_link_name, TableKind::Link, false);

            pub_link_table.columns.insert(ColumnDefinition {
                name: "owner_id".into(),
                data_type: SqlColumnType::Uuid,
                nullable: false,
                is_primary_key: true,
                default_value: None,
                unique: false,
            });

            pub_link_table.columns.insert(ColumnDefinition {
                name: "target_id".into(),
                data_type: SqlColumnType::Uuid,
                nullable: false,
                is_primary_key: true,
                default_value: None,
                unique: false,
            });

            // Owner foreign key: owner_id -> {owner_table}__published.id ON DELETE CASCADE
            let pub_owner_table = published_table_name(&table_name);
            pub_link_table
                .foreign_keys
                .insert(ForeignKeyDefinition::new(
                    link_owner_fk_name(&pub_link_name),
                    vec!["owner_id".into()],
                    &pub_owner_table,
                    vec!["id".into()],
                    ForeignKeyAction::Cascade,
                    ForeignKeyAction::NoAction,
                ));

            // Variant 1 (Public Filter Principle) for target foreign key:
            // - If target has draft_and_publish: true, target_id -> {target_table}__published.id ON DELETE CASCADE
            // - If target has draft_and_publish: false, target_id -> {target_table}.id ON DELETE CASCADE
            let target_has_draft_and_publish = target_dt
                .map(|dt| dt.options.draft_and_publish)
                .unwrap_or(false);

            let pub_target_table = if target_has_draft_and_publish {
                published_table_name(&target_table)
            } else {
                target_table.clone()
            };

            pub_link_table
                .foreign_keys
                .insert(ForeignKeyDefinition::new(
                    link_target_fk_name(&pub_link_name),
                    vec!["target_id".into()],
                    &pub_target_table,
                    vec!["id".into()],
                    ForeignKeyAction::Cascade,
                    ForeignKeyAction::NoAction,
                ));

            // If HasOne, enforce uniqueness on owner_id in published link table
            if kind == OwnerRelationKind::HasOne {
                pub_link_table.indexes.insert(IndexDefinition::new(
                    link_owner_unique_index_name(&pub_link_name),
                    &pub_link_name,
                    vec!["owner_id".into()],
                    true,
                ));
            }

            // Reverse lookup index on target_id
            pub_link_table.indexes.insert(IndexDefinition::new(
                link_target_index_name(&pub_link_name),
                &pub_link_name,
                vec!["target_id".into()],
                false,
            ));

            link_tables.push(pub_link_table);
        }
    }

    (entity_table, published_table, link_tables)
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

        // Verify tables: articles, articles__published, authors, tags, articles__author_link, articles__tags_link
        assert!(schema.find_table("articles").is_some());
        assert!(schema.find_table("articles__published").is_some());
        assert!(schema.find_table("authors").is_some());
        assert!(schema.find_table("tags").is_some());
        assert!(schema.find_table("articles__author_link").is_some());
        assert!(schema.find_table("articles__tags_link").is_some());

        // Authors and tags have draft_and_publish: false -> no published mirror tables
        assert!(schema.find_table("authors__published").is_none());
        assert!(schema.find_table("tags__published").is_none());

        let articles_table = schema.find_table("articles").unwrap();
        assert_eq!(articles_table.kind, TableKind::Entity);
        // Check audit columns
        assert!(articles_table.find_column("id").is_some());
        assert!(articles_table.find_column("version").is_some());
        assert!(articles_table.find_column("owner_id").is_some());
        assert!(articles_table.find_column("publication_state").is_some());
        // Check user columns
        assert!(articles_table.find_column("title").is_some());
        assert!(articles_table.find_column("slug").is_some());
        // Verify NO relation columns on the main entity table!
        assert!(articles_table.find_column("author_id").is_none());
        // Check unique index on slug
        assert!(articles_table.find_index("idx_articles_slug").is_some());
        assert!(
            articles_table
                .find_index("idx_articles_slug")
                .unwrap()
                .unique
        );

        // Check published mirror table
        let published_table = schema.find_table("articles__published").unwrap();
        assert_eq!(published_table.kind, TableKind::Published);
        assert!(published_table.find_column("id").is_some());
        assert!(published_table.find_column("published_version").is_some());
        assert!(published_table.find_column("owner_id").is_some());
        assert!(published_table.find_column("published_at").is_some());
        assert!(published_table.find_column("published_by").is_some());
        assert!(published_table.find_column("title").is_some());
        assert!(published_table.find_column("slug").is_some());
        assert!(
            published_table
                .find_index("idx_articles__published_slug")
                .is_some()
        );
        let pub_fk = published_table
            .find_foreign_key("fk_articles__published_id")
            .unwrap();
        assert_eq!(pub_fk.referenced_table, "articles");
        assert_eq!(pub_fk.columns, vec!["id"]);
        assert_eq!(pub_fk.referenced_columns, vec!["id"]);
        assert_eq!(pub_fk.on_delete, ForeignKeyAction::Cascade);

        // Check HasOne link table: articles__author_link
        let author_link = schema.find_table("articles__author_link").unwrap();
        assert!(author_link.is_junction());
        assert_eq!(author_link.kind, TableKind::Link);
        assert!(author_link.find_column("owner_id").is_some());
        assert!(author_link.find_column("target_id").is_some());
        // Foreign keys to articles and authors
        let owner_fk = author_link
            .find_foreign_key("fk_articles__author_link_owner")
            .unwrap();
        assert_eq!(owner_fk.referenced_table, "articles");
        let target_fk = author_link
            .find_foreign_key("fk_articles__author_link_target")
            .unwrap();
        assert_eq!(target_fk.referenced_table, "authors");
        // HasOne has unique index on owner_id
        assert!(
            author_link
                .find_index("uq_articles__author_link_owner")
                .is_some()
        );
        assert!(
            author_link
                .find_index("uq_articles__author_link_owner")
                .unwrap()
                .unique
        );
        assert!(
            author_link
                .find_index("idx_articles__author_link_target")
                .is_some()
        );

        // Check HasMany link table: articles__tags_link
        let tags_link = schema.find_table("articles__tags_link").unwrap();
        assert!(tags_link.is_junction());
        assert_eq!(tags_link.kind, TableKind::Link);
        assert!(tags_link.find_column("owner_id").is_some());
        assert!(tags_link.find_column("target_id").is_some());
        // HasMany does NOT have unique index on owner_id
        assert!(
            tags_link
                .find_index("uq_articles__tags_link_owner")
                .is_none()
        );
        assert!(
            tags_link
                .find_index("idx_articles__tags_link_target")
                .is_some()
        );

        // Check published mirror link tables (articles has draft_and_publish: true)
        assert!(
            schema
                .find_table("articles__author_link__published")
                .is_some()
        );
        assert!(
            schema
                .find_table("articles__tags_link__published")
                .is_some()
        );

        // Check articles__author_link__published:
        // owner references articles__published (id), target references authors (id) because author has draft_and_publish: false
        let pub_author_link = schema
            .find_table("articles__author_link__published")
            .unwrap();
        assert_eq!(pub_author_link.kind, TableKind::Link);
        let pub_owner_fk = pub_author_link
            .find_foreign_key("fk_articles__author_link__published_owner")
            .unwrap();
        assert_eq!(pub_owner_fk.referenced_table, "articles__published");
        assert_eq!(pub_owner_fk.columns, vec!["owner_id"]);
        assert_eq!(pub_owner_fk.referenced_columns, vec!["id"]);
        assert_eq!(pub_owner_fk.on_delete, ForeignKeyAction::Cascade);

        let pub_target_fk = pub_author_link
            .find_foreign_key("fk_articles__author_link__published_target")
            .unwrap();
        assert_eq!(pub_target_fk.referenced_table, "authors");
        assert_eq!(pub_target_fk.columns, vec!["target_id"]);
        assert_eq!(pub_target_fk.referenced_columns, vec!["id"]);
        assert_eq!(pub_target_fk.on_delete, ForeignKeyAction::Cascade);

        assert!(
            pub_author_link
                .find_index("uq_articles__author_link__published_owner")
                .is_some()
        );
        assert!(
            pub_author_link
                .find_index("idx_articles__author_link__published_target")
                .is_some()
        );

        // Check articles__tags_link__published (HasMany):
        let pub_tags_link = schema.find_table("articles__tags_link__published").unwrap();
        assert_eq!(pub_tags_link.kind, TableKind::Link);
        let pub_tags_owner_fk = pub_tags_link
            .find_foreign_key("fk_articles__tags_link__published_owner")
            .unwrap();
        assert_eq!(pub_tags_owner_fk.referenced_table, "articles__published");
        let pub_tags_target_fk = pub_tags_link
            .find_foreign_key("fk_articles__tags_link__published_target")
            .unwrap();
        assert_eq!(pub_tags_target_fk.referenced_table, "tags");
        assert!(
            pub_tags_link
                .find_index("uq_articles__tags_link__published_owner")
                .is_none()
        );
        assert!(
            pub_tags_link
                .find_index("idx_articles__tags_link__published_target")
                .is_some()
        );
    }

    #[test]
    fn test_published_link_table_with_published_target() {
        let article_type_id = DocumentTypeId::try_new("article").unwrap();
        let author_type_id = DocumentTypeId::try_new("author").unwrap();

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
            fields: IndexMap::new(),
        };

        // Author ALSO has draft_and_publish: true
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
                draft_and_publish: true,
            },
            fields: IndexMap::new(),
        };

        let author_rel = Relation {
            id: RelationId::new(Uuid::now_v7()),
            owner_type: article_type_id,
            owner_attr: AttributeId::try_new("author").unwrap(),
            owner_kind: OwnerRelationKind::HasOne,
            target_type: author_type_id,
            inverse: None,
        };

        let registry = SchemaRegistry::new(vec![article_dt, author_dt], vec![author_rel]);
        let schema = build_desired_schema(&registry);

        // articles, articles__published, authors, authors__published, articles__author_link, articles__author_link__published
        assert!(schema.find_table("articles").is_some());
        assert!(schema.find_table("articles__published").is_some());
        assert!(schema.find_table("authors").is_some());
        assert!(schema.find_table("authors__published").is_some());
        assert!(schema.find_table("articles__author_link").is_some());
        assert!(
            schema
                .find_table("articles__author_link__published")
                .is_some()
        );

        // In draft link table: target references base "authors" table
        let draft_link = schema.find_table("articles__author_link").unwrap();
        let draft_target_fk = draft_link
            .find_foreign_key("fk_articles__author_link_target")
            .unwrap();
        assert_eq!(draft_target_fk.referenced_table, "authors");

        // In published link table: Variant 1 (Public Filter Principle)
        // target references "authors__published" table!
        let pub_link = schema
            .find_table("articles__author_link__published")
            .unwrap();
        let pub_target_fk = pub_link
            .find_foreign_key("fk_articles__author_link__published_target")
            .unwrap();
        assert_eq!(pub_target_fk.referenced_table, "authors__published");
        assert_eq!(pub_target_fk.on_delete, ForeignKeyAction::Cascade);

        let pub_owner_fk = pub_link
            .find_foreign_key("fk_articles__author_link__published_owner")
            .unwrap();
        assert_eq!(pub_owner_fk.referenced_table, "articles__published");
        assert_eq!(pub_owner_fk.on_delete, ForeignKeyAction::Cascade);
    }
}
