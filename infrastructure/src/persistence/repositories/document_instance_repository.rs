//! PostgreSQL / Aurora DSQL implementation of `DocumentInstanceRepository`.

use std::collections::HashMap;
use std::future::Future;

use chrono::{DateTime, Utc};
use domain::auth::UserId;
use domain::content::{
    AuditTrail, DocumentContent, DocumentInstance, DocumentInstanceId,
    DocumentInstanceRepository, FieldFilter, Page, Pagination, PublicationState, RelationMap,
    ResolvedRelation,
};
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentTypeId, RelationView, SchemaRegistry};
use sea_query::PostgresQueryBuilder;
use sea_query_sqlx::SqlxBinder;
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

use crate::persistence::naming::DocumentTableNaming;
use crate::persistence::query::{
    build_count, build_delete, build_exists, build_instances_batch_select, build_link_batch_insert,
    build_link_delete, build_link_pairs_select, build_link_select, build_published_delete,
    build_published_link_batch_insert, build_published_link_delete, build_published_select,
    build_published_upsert, build_select_by_id, build_select_by_type, build_upsert,
    read_content_value, storage_err,
};

/// Sqlx-backed repository for dynamic document instances.
#[derive(Debug, Clone)]
pub struct SqlxDocumentInstanceRepository {
    pool: PgPool,
    schema_registry: &'static SchemaRegistry,
}

impl SqlxDocumentInstanceRepository {
    pub fn new(pool: PgPool, schema_registry: &'static SchemaRegistry) -> Self {
        Self {
            pool,
            schema_registry,
        }
    }
}

impl DocumentInstanceRepository for SqlxDocumentInstanceRepository {
    fn find_by_id(
        &self,
        type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        async move {
            let doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let naming = DocumentTableNaming::new(doc_type);

            let uuid = *id.as_ref();
            let fields: Vec<_> = doc_type.fields.iter().cloned().collect();
            let select_stmt = build_select_by_id(&naming, uuid, &fields);
            let (sql, values) = select_stmt.build_sqlx(PostgresQueryBuilder);

            let row_opt = sqlx::query_with(AssertSqlSafe(&*sql), values)
                .fetch_optional(&pool)
                .await
                .map_err(storage_err)?;

            let row = match row_opt {
                Some(r) => r,
                None => return Ok(None),
            };

            // Audit metadata
            let version: i64 = row.get("version");
            let owner_id_str: String = row.get("owner_id");
            let created_at: DateTime<Utc> = row.get("created_at");
            let updated_at: DateTime<Utc> = row.get("updated_at");
            let pub_state_str: String = row.get("publication_state");

            // User declared dynamic fields
            let mut fields_map = HashMap::new();
            for field_def in &doc_type.fields {
                let attr_id = &field_def.id;
                let col_name = naming.column_name(attr_id);
                let val = read_content_value(&row, &col_name, &field_def.field_type)?;
                fields_map.insert(attr_id.clone(), val);
            }

            // Relations owned by this document type
            let mut relations = HashMap::new();
            let rel_views = schema_reg.find_relations_for(type_id);
            for rel in rel_views {
                let (attr, _kind) = match rel {
                    RelationView::Unidirectional { attr, kind, .. } => (attr, kind),
                    RelationView::OwnerSide { attr, kind, .. } => (attr, kind),
                    RelationView::InverseSide { .. } => continue,
                };

                let link_naming = naming.link_table(&attr);
                let link_stmt = build_link_select(&link_naming, uuid);
                let (link_sql, link_values) = link_stmt.build_sqlx(PostgresQueryBuilder);

                let link_rows = sqlx::query_with(AssertSqlSafe(&*link_sql), link_values)
                    .fetch_all(&pool)
                    .await
                    .map_err(storage_err)?;

                let mut resolved_list = Vec::with_capacity(link_rows.len());
                for l_row in link_rows {
                    let target_uuid: Uuid = l_row.get("target_id");
                    resolved_list.push(ResolvedRelation {
                        attribute_id: attr.clone(),
                        target_instance_id: DocumentInstanceId::new(target_uuid),
                    });
                }

                relations.insert(attr, resolved_list);
            }

            // Publication state
            let publication_state = if pub_state_str == "published" {
                if doc_type.options.draft_and_publish {
                    if let Some(pub_stmt) = build_published_select(&naming, uuid) {
                        let (pub_sql, pub_values) = pub_stmt.build_sqlx(PostgresQueryBuilder);
                        if let Some(pub_row) = sqlx::query_with(AssertSqlSafe(&*pub_sql), pub_values)
                            .fetch_optional(&pool)
                            .await
                            .map_err(storage_err)?
                        {
                            let p_ver: i64 = pub_row.get("published_version");
                            let p_at: DateTime<Utc> = pub_row.get("published_at");
                            let p_by: Option<String> = pub_row.get("published_by");
                            PublicationState::Published {
                                revision: p_ver as u32,
                                published_at: p_at,
                                published_by: p_by
                                    .map(UserId::try_new)
                                    .transpose()
                                    .map_err(storage_err)?,
                            }
                        } else {
                            PublicationState::Draft {
                                last_published_revision: None,
                            }
                        }
                    } else {
                        PublicationState::Draft {
                            last_published_revision: None,
                        }
                    }
                } else {
                    PublicationState::Draft {
                        last_published_revision: None,
                    }
                }
            } else {
                PublicationState::Draft {
                    last_published_revision: None,
                }
            };

            let instance = DocumentInstance {
                id,
                db_row_id: Some(id),
                document_type_id: type_id.clone(),
                content: DocumentContent {
                    fields: fields_map,
                    publication_state,
                },
                relations,
                populated_relations: HashMap::new(),
                audit: AuditTrail {
                    created_at,
                    created_by: UserId::try_new(owner_id_str.clone()).ok(),
                    updated_at,
                    updated_by: UserId::try_new(owner_id_str).ok(),
                    version: version as u32,
                },
            };

            Ok(Some(instance))
        }
    }

    fn find_by_type(
        &self,
        type_id: &'static DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<Page<DocumentInstance>, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        async move {
            let doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let naming = DocumentTableNaming::new(doc_type);

            // 1. Total count query
            let count_stmt = build_count(&naming, &filters);
            let (count_sql, count_values) = count_stmt.build_sqlx(PostgresQueryBuilder);
            let count_row = sqlx::query_with(AssertSqlSafe(&*count_sql), count_values)
                .fetch_one(&pool)
                .await
                .map_err(storage_err)?;
            let total: i64 = count_row.get("total");

            // 2. Select paginated items
            let offset = (pagination.page.saturating_sub(1) as u64) * (pagination.page_size as u64);
            let fields: Vec<_> = doc_type.fields.iter().cloned().collect();
            let select_stmt = build_select_by_type(
                &naming,
                &fields,
                &filters,
                pagination.page_size as u64,
                offset,
            );
            let (select_sql, select_values) = select_stmt.build_sqlx(PostgresQueryBuilder);

            let rows = sqlx::query_with(AssertSqlSafe(&*select_sql), select_values)
                .fetch_all(&pool)
                .await
                .map_err(storage_err)?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                let inst_uuid: Uuid = row.get("id");
                let inst_id = DocumentInstanceId::new(inst_uuid);

                let version: i64 = row.get("version");
                let owner_id_str: String = row.get("owner_id");
                let created_at: DateTime<Utc> = row.get("created_at");
                let updated_at: DateTime<Utc> = row.get("updated_at");
                let pub_state_str: String = row.get("publication_state");

                let mut fields_map = HashMap::new();
                for field_def in &doc_type.fields {
                    let attr_id = &field_def.id;
                    let col_name = naming.column_name(attr_id);
                    let val = read_content_value(&row, &col_name, &field_def.field_type)?;
                    fields_map.insert(attr_id.clone(), val);
                }

                let pub_state = if pub_state_str == "published" {
                    PublicationState::Published {
                        revision: 1,
                        published_at: updated_at,
                        published_by: UserId::try_new(owner_id_str.clone()).ok(),
                    }
                } else {
                    PublicationState::Draft {
                        last_published_revision: None,
                    }
                };

                items.push(DocumentInstance {
                    id: inst_id,
                    db_row_id: Some(inst_id),
                    document_type_id: type_id.clone(),
                    content: DocumentContent {
                        fields: fields_map,
                        publication_state: pub_state,
                    },
                    relations: HashMap::new(),
                    populated_relations: HashMap::new(),
                    audit: AuditTrail {
                        created_at,
                        created_by: UserId::try_new(owner_id_str.clone()).ok(),
                        updated_at,
                        updated_by: UserId::try_new(owner_id_str).ok(),
                        version: version as u32,
                    },
                });
            }

            Ok(Page {
                items,
                total: total as u64,
                page: pagination.page,
                page_size: pagination.page_size,
            })
        }
    }

    fn count(
        &self,
        type_id: &'static DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<u64, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        async move {
            let doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let naming = DocumentTableNaming::new(doc_type);

            let count_stmt = build_count(&naming, &filters);
            let (count_sql, count_values) = count_stmt.build_sqlx(PostgresQueryBuilder);
            let row = sqlx::query_with(AssertSqlSafe(&*count_sql), count_values)
                .fetch_one(&pool)
                .await
                .map_err(storage_err)?;
            let total: i64 = row.get("total");
            Ok(total as u64)
        }
    }

    fn fetch_relations(
        &self,
        type_id: &'static DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<RelationMap, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        let attributes = attributes.to_vec();
        let parent_ids = parent_ids.to_vec();

        async move {
            let mut result_map: RelationMap = HashMap::new();
            if parent_ids.is_empty() || attributes.is_empty() {
                return Ok(result_map);
            }

            let _doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;

            let parent_uuids: Vec<Uuid> = parent_ids.iter().map(|id| *id.as_ref()).collect();

            for attr in attributes {
                let rel = match schema_reg.find_relation_for_attr(type_id, &attr) {
                    Some(r) => r,
                    None => continue,
                };

                let is_inverse = &rel.target_type == type_id;
                let owner_doc_type = schema_reg
                    .find_type(&rel.owner_type)
                    .ok_or_else(|| DomainError::DocumentTypeNotFound(rel.owner_type.clone()))?;
                let owner_naming = DocumentTableNaming::new(owner_doc_type);
                let link_naming = owner_naming.link_table(&rel.owner_attr);

                let target_type_id = if is_inverse {
                    rel.owner_type.clone()
                } else {
                    rel.target_type.clone()
                };

                let parent_col = if is_inverse { "target_id" } else { "owner_id" };
                let child_col = if is_inverse { "owner_id" } else { "target_id" };

                // Query link table for pairs
                let pairs_stmt =
                    build_link_pairs_select(&link_naming, parent_col, child_col, &parent_uuids);
                let (link_sql, link_values) = pairs_stmt.build_sqlx(PostgresQueryBuilder);

                let link_rows = sqlx::query_with(AssertSqlSafe(&*link_sql), link_values)
                    .fetch_all(&pool)
                    .await
                    .map_err(storage_err)?;

                let mut child_uuids = Vec::with_capacity(link_rows.len());
                let mut pairs = Vec::with_capacity(link_rows.len());
                for l_row in link_rows {
                    let p_uuid: Uuid = l_row.get(parent_col);
                    let c_uuid: Uuid = l_row.get(child_col);
                    child_uuids.push(c_uuid);
                    pairs.push((
                        DocumentInstanceId::new(p_uuid),
                        DocumentInstanceId::new(c_uuid),
                    ));
                }

                if child_uuids.is_empty() {
                    result_map.insert(attr, HashMap::new());
                    continue;
                }

                // In-place sort and deduplicate child UUIDs without heap allocations
                child_uuids.sort_unstable();
                child_uuids.dedup();

                // Fetch child instances
                let target_dt = schema_reg
                    .find_type(&target_type_id)
                    .ok_or_else(|| DomainError::DocumentTypeNotFound(target_type_id.clone()))?;
                let target_naming = DocumentTableNaming::new(target_dt);

                let target_fields: Vec<_> = target_dt.fields.iter().cloned().collect();
                let instances_stmt =
                    build_instances_batch_select(&target_naming, &target_fields, &child_uuids);
                let (target_sql, target_values) =
                    instances_stmt.build_sqlx(PostgresQueryBuilder);

                let target_rows = sqlx::query_with(AssertSqlSafe(&*target_sql), target_values)
                    .fetch_all(&pool)
                    .await
                    .map_err(storage_err)?;

                let mut child_instances = HashMap::with_capacity(target_rows.len());

                for row in target_rows {
                    let c_id: Uuid = row.get("id");
                    let inst_id = DocumentInstanceId::new(c_id);
                    let version: i64 = row.get("version");
                    let owner_id_str: String = row.get("owner_id");
                    let created_at: DateTime<Utc> = row.get("created_at");
                    let updated_at: DateTime<Utc> = row.get("updated_at");

                    let mut fields_map = HashMap::new();
                    for c_field_def in &target_dt.fields {
                        let c_attr_id = &c_field_def.id;
                        let c_col = target_naming.column_name(c_attr_id);
                        let val = read_content_value(&row, &c_col, &c_field_def.field_type)?;
                        fields_map.insert(c_attr_id.clone(), val);
                    }

                    child_instances.insert(
                        inst_id,
                        DocumentInstance {
                            id: inst_id,
                            db_row_id: Some(inst_id),
                            document_type_id: target_type_id.clone(),
                            content: DocumentContent {
                                fields: fields_map,
                                publication_state: PublicationState::Draft {
                                    last_published_revision: None,
                                },
                            },
                            relations: HashMap::new(),
                            populated_relations: HashMap::new(),
                            audit: AuditTrail {
                                created_at,
                                created_by: UserId::try_new(owner_id_str.clone()).ok(),
                                updated_at,
                                updated_by: UserId::try_new(owner_id_str).ok(),
                                version: version as u32,
                            },
                        },
                    );
                }

                // Map child instances to their parent instance IDs
                let mut by_parent: HashMap<DocumentInstanceId, Vec<DocumentInstance>> =
                    HashMap::new();
                for (parent_id, child_id) in pairs {
                    if let Some(child_inst) = child_instances.get(&child_id) {
                        by_parent
                            .entry(parent_id)
                            .or_default()
                            .push(child_inst.clone());
                    }
                }

                result_map.insert(attr, by_parent);
            }

            Ok(result_map)
        }
    }

    fn save(
        &self,
        instance: &DocumentInstance,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        let instance = instance.clone();

        async move {
            let doc_type = schema_reg
                .find_type(&instance.document_type_id)
                .ok_or_else(|| {
                    DomainError::DocumentTypeNotFound(instance.document_type_id.clone())
                })?;
            let naming = DocumentTableNaming::new(doc_type);

            let inst_uuid = *instance.id.as_ref();

            // Begin database transaction for atomic multi-table mutation
            let mut tx = pool.begin().await.map_err(storage_err)?;

            // 1. Build and execute Upsert into working draft table
            let upsert_stmt = build_upsert(&naming, doc_type, &instance)?;
            let (upsert_sql, upsert_values) = upsert_stmt.build_sqlx(PostgresQueryBuilder);
            sqlx::query_with(AssertSqlSafe(&*upsert_sql), upsert_values)
                .execute(&mut *tx)
                .await
                .map_err(storage_err)?;

            // 2. Draft Link Tables for relations owned by this entity
            let rel_views = schema_reg.find_relations_for(&instance.document_type_id);
            for rel in &rel_views {
                let attr = match rel {
                    RelationView::Unidirectional { attr, .. } => attr,
                    RelationView::OwnerSide { attr, .. } => attr,
                    RelationView::InverseSide { .. } => continue,
                };

                let link_naming = naming.link_table(attr);

                // Clear previous links
                let del_stmt = build_link_delete(&link_naming, inst_uuid);
                let (del_sql, del_values) = del_stmt.build_sqlx(PostgresQueryBuilder);
                sqlx::query_with(AssertSqlSafe(&*del_sql), del_values)
                    .execute(&mut *tx)
                    .await
                    .map_err(storage_err)?;

                // Insert new relations in a single batch query
                if let Some(resolved) = instance.relations.get(attr) {
                    let target_uuids: Vec<Uuid> = resolved
                        .iter()
                        .map(|r| *r.target_instance_id.as_ref())
                        .collect();

                    if let Some(ins_stmt) =
                        build_link_batch_insert(&link_naming, inst_uuid, &target_uuids)
                    {
                        let (ins_sql, ins_values) = ins_stmt.build_sqlx(PostgresQueryBuilder);
                        sqlx::query_with(AssertSqlSafe(&*ins_sql), ins_values)
                            .execute(&mut *tx)
                            .await
                            .map_err(storage_err)?;
                    }
                }
            }

            // 3. Published Mirror Table & Published Link Tables
            if doc_type.options.draft_and_publish {
                match &instance.content.publication_state {
                    PublicationState::Published { .. } => {
                        if let Some(pub_stmt) =
                            build_published_upsert(&naming, doc_type, &instance)?
                        {
                            let (pub_sql, pub_values) = pub_stmt.build_sqlx(PostgresQueryBuilder);
                            sqlx::query_with(AssertSqlSafe(&*pub_sql), pub_values)
                                .execute(&mut *tx)
                                .await
                                .map_err(storage_err)?;
                        }

                        // Published Link Tables
                        for rel in &rel_views {
                            let attr = match rel {
                                RelationView::Unidirectional { attr, .. } => attr,
                                RelationView::OwnerSide { attr, .. } => attr,
                                RelationView::InverseSide { .. } => continue,
                            };

                            let link_naming = naming.link_table(attr);

                            let del_pub_stmt = build_published_link_delete(&link_naming, inst_uuid);
                            let (del_pub_sql, del_pub_vals) =
                                del_pub_stmt.build_sqlx(PostgresQueryBuilder);
                            sqlx::query_with(AssertSqlSafe(&*del_pub_sql), del_pub_vals)
                                .execute(&mut *tx)
                                .await
                                .map_err(storage_err)?;

                            if let Some(resolved) = instance.relations.get(attr) {
                                let target_uuids: Vec<Uuid> = resolved
                                    .iter()
                                    .map(|r| *r.target_instance_id.as_ref())
                                    .collect();

                                if let Some(ins_pub_stmt) = build_published_link_batch_insert(
                                    &link_naming,
                                    inst_uuid,
                                    &target_uuids,
                                ) {
                                    let (ins_pub_sql, ins_pub_vals) =
                                        ins_pub_stmt.build_sqlx(PostgresQueryBuilder);
                                    // May fail if target is not published (Variant 1: Public Filter Principle constraint)
                                    let _ = sqlx::query_with(AssertSqlSafe(&*ins_pub_sql), ins_pub_vals)
                                        .execute(&mut *tx)
                                        .await;
                                }
                            }
                        }
                    }
                    PublicationState::Draft { .. } => {
                        // If document is unpublishing, remove row from published mirror table
                        if let Some(del_pub_stmt) = build_published_delete(&naming, inst_uuid) {
                            let (del_sql, del_vals) =
                                del_pub_stmt.build_sqlx(PostgresQueryBuilder);
                            sqlx::query_with(AssertSqlSafe(&*del_sql), del_vals)
                                .execute(&mut *tx)
                                .await
                                .map_err(storage_err)?;
                        }
                    }
                }
            }

            // Commit the entire atomic transaction
            tx.commit().await.map_err(storage_err)?;

            Ok(())
        }
    }

    fn delete(
        &self,
        type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        async move {
            let doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let naming = DocumentTableNaming::new(doc_type);

            let uuid = *id.as_ref();
            let del_stmt = build_delete(&naming, uuid);
            let (sql, values) = del_stmt.build_sqlx(PostgresQueryBuilder);

            sqlx::query_with(AssertSqlSafe(&*sql), values)
                .execute(&pool)
                .await
                .map_err(storage_err)?;

            Ok(())
        }
    }

    fn exists_for_type(
        &self,
        type_id: &'static DocumentTypeId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry;
        async move {
            let doc_type = schema_reg
                .find_type(type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let naming = DocumentTableNaming::new(doc_type);

            let exists_stmt = build_exists(&naming);
            let (sql, values) = exists_stmt.build_sqlx(PostgresQueryBuilder);

            let row = sqlx::query_with(AssertSqlSafe(&*sql), values)
                .fetch_one(&pool)
                .await
                .map_err(storage_err)?;

            let exists: bool = row.get("exists");
            Ok(exists)
        }
    }
}
