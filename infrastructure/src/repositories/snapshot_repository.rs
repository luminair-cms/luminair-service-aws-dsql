//! PostgreSQL / Aurora DSQL implementation of `SnapshotRepository`.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use domain::entities::published_snapshot::PublishedSnapshot;
use domain::errors::DomainError;
use domain::ports::SnapshotRepository;
use domain::services::schema_registry::SchemaRegistry;
use domain::value_objects::{DocumentInstanceId, SnapshotId, UserId};
use sqlx::{PgPool, Row};

use super::document_instance_repository::read_content_value;
use crate::schema_loader::naming::{
    attribute_to_column_name, document_type_to_table_name, published_table_name,
};

/// SQLx implementation of `SnapshotRepository` querying `{table}__published` tables.
#[derive(Debug, Clone)]
pub struct SqlxSnapshotRepository {
    pool: PgPool,
    schema_registry: Arc<SchemaRegistry>,
}

impl SqlxSnapshotRepository {
    pub fn new(pool: PgPool, schema_registry: Arc<SchemaRegistry>) -> Self {
        Self {
            pool,
            schema_registry,
        }
    }
}

impl SnapshotRepository for SqlxSnapshotRepository {
    async fn save(&self, _snapshot: &PublishedSnapshot) -> Result<(), DomainError> {
        // In the Two-Table model, SqlxDocumentInstanceRepository::save directly
        // maintains {table}__published on publish / unpublish.
        Ok(())
    }

    async fn find_by_instance(
        &self,
        instance_id: DocumentInstanceId,
    ) -> Result<Vec<PublishedSnapshot>, DomainError> {
        let uuid = *instance_id.as_ref();
        let mut snapshots = Vec::new();

        for doc_type in self.schema_registry.all_types() {
            if !doc_type.options.draft_and_publish {
                continue;
            }

            let table_name = document_type_to_table_name(doc_type);
            let pub_table = published_table_name(&table_name);

            let mut qb = sqlx::QueryBuilder::new("SELECT * FROM \"");
            qb.push(&pub_table);
            qb.push("\" WHERE id = ");
            qb.push_bind(uuid);

            if let Some(row) = qb
                .build()
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?
            {
                let published_version: i64 = row.get("published_version");
                let published_at: DateTime<Utc> = row.get("published_at");
                let published_by_opt: Option<String> = row.get("published_by");
                let published_by = published_by_opt.and_then(|s| UserId::try_new(s).ok());

                let mut fields = HashMap::new();
                for (attr_id, field_def) in &doc_type.fields {
                    let col_name = attribute_to_column_name(attr_id);
                    let val = read_content_value(&row, &col_name, &field_def.field_type)?;
                    fields.insert(attr_id.clone(), val);
                }

                snapshots.push(PublishedSnapshot {
                    id: SnapshotId::new(uuid),
                    instance_id,
                    type_name: doc_type.info.plural_name.clone(),
                    revision: published_version as u32,
                    published_at,
                    published_by,
                    fields,
                });
                break;
            }
        }

        Ok(snapshots)
    }

    async fn find_by_revision(
        &self,
        instance_id: DocumentInstanceId,
        revision: u32,
    ) -> Result<Option<PublishedSnapshot>, DomainError> {
        let snapshots = self.find_by_instance(instance_id).await?;
        Ok(snapshots.into_iter().find(|s| s.revision == revision))
    }

    async fn delete_by_instance(
        &self,
        _instance_id: DocumentInstanceId,
    ) -> Result<(), DomainError> {
        // Handled via foreign key ON DELETE CASCADE from {table} to {table}__published
        Ok(())
    }
}
