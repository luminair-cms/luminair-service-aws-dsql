//! PostgreSQL / Aurora DSQL implementation of `DocumentInstanceRepository`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use domain::entities::document_instance::{
    AuditTrail, DocumentContent, DocumentInstance, PublicationState, ResolvedRelation,
};
use domain::entities::document_type::DocumentKind;
use domain::entities::relation::RelationView;
use domain::errors::DomainError;
use domain::ports::DocumentInstanceRepository;
use domain::ports::document_instance_repository::{FieldFilter, Page, Pagination, RelationMap};
use domain::services::schema_registry::SchemaRegistry;
use domain::types::content_value::ContentValue;
use domain::types::domain_value::DomainValue;
use domain::types::field_type::{FieldType, IntegerSize, PrimitiveType};
use domain::types::primitive_value::PrimitiveValue;
use domain::value_objects::{
    AttributeId, DocumentInstanceId, DocumentTypeId, Email, LocaleId, Url, UserId,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::schema_loader::naming::{
    attribute_to_column_name, document_type_to_table_name, link_table_name,
    published_link_table_name, published_table_name,
};

/// Sqlx-backed repository for dynamic document instances.
#[derive(Debug, Clone)]
pub struct SqlxDocumentInstanceRepository {
    pool: PgPool,
    schema_registry: Arc<SchemaRegistry>,
}

impl SqlxDocumentInstanceRepository {
    pub fn new(pool: PgPool, schema_registry: Arc<SchemaRegistry>) -> Self {
        Self {
            pool,
            schema_registry,
        }
    }
}

pub(crate) fn read_content_value(
    row: &sqlx::postgres::PgRow,
    col_name: &str,
    ft: &FieldType,
) -> Result<ContentValue, DomainError> {
    match ft {
        FieldType::Primitive(PrimitiveType::Text) => {
            let s: Option<String> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                s.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(s))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::Uid) => {
            let s: Option<String> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                s.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uid(s))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::Uuid) => {
            let u: Option<Uuid> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                u.map(|u| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uuid(u))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::Integer(size)) => {
            let val: Option<i64> = match size {
                IntegerSize::I16 => row
                    .try_get::<Option<i16>, _>(col_name)
                    .map_err(|e| DomainError::Storage(e.to_string()))?
                    .map(|i| i as i64),
                IntegerSize::I32 => row
                    .try_get::<Option<i32>, _>(col_name)
                    .map_err(|e| DomainError::Storage(e.to_string()))?
                    .map(|i| i as i64),
                IntegerSize::I64 => row
                    .try_get::<Option<i64>, _>(col_name)
                    .map_err(|e| DomainError::Storage(e.to_string()))?,
            };
            Ok(val
                .map(|i| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
            let d: Option<rust_decimal::Decimal> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                d.map(|d| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(d))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::Boolean) => {
            let b: Option<bool> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                b.map(|b| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Boolean(b))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::Date) => {
            let d: Option<chrono::NaiveDate> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(
                d.map(|d| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Date(d))))
                    .unwrap_or(ContentValue::Null),
            )
        }
        FieldType::Primitive(PrimitiveType::DateTime) => {
            let dt: Option<DateTime<Utc>> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            Ok(dt
                .map(|dt| {
                    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::DateTime(dt)))
                })
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Email => {
            let s: Option<String> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            match s {
                Some(s) => {
                    let email =
                        Email::try_new(s).map_err(|e| DomainError::Storage(e.to_string()))?;
                    Ok(ContentValue::Scalar(DomainValue::Email(email)))
                }
                None => Ok(ContentValue::Null),
            }
        }
        FieldType::Url => {
            let s: Option<String> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            match s {
                Some(s) => {
                    let url = Url::try_new(s).map_err(|e| DomainError::Storage(e.to_string()))?;
                    Ok(ContentValue::Scalar(DomainValue::Url(url)))
                }
                None => Ok(ContentValue::Null),
            }
        }
        FieldType::LocalizedText => {
            let json_opt: Option<serde_json::Value> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            match json_opt {
                Some(json) => {
                    let map: HashMap<LocaleId, String> = serde_json::from_value(json)
                        .map_err(|e| DomainError::Storage(e.to_string()))?;
                    Ok(ContentValue::LocalizedText(map))
                }
                None => Ok(ContentValue::Null),
            }
        }
        FieldType::Json => {
            let json_opt: Option<serde_json::Value> = row
                .try_get(col_name)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            match json_opt {
                Some(json) => {
                    let map: HashMap<String, PrimitiveValue> = serde_json::from_value(json)
                        .map_err(|e| DomainError::Storage(e.to_string()))?;
                    Ok(ContentValue::Scalar(DomainValue::Json(map)))
                }
                None => Ok(ContentValue::Null),
            }
        }
    }
}

fn bind_content_value(
    qb: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    val: Option<&ContentValue>,
    ft: &FieldType,
) -> Result<(), DomainError> {
    match (val, ft) {
        (None | Some(ContentValue::Null), _) => {
            qb.push("NULL");
        }
        (Some(ContentValue::LocalizedText(map)), FieldType::LocalizedText) => {
            let json =
                serde_json::to_value(map).map_err(|e| DomainError::Storage(e.to_string()))?;
            qb.push_bind(json);
        }
        (Some(ContentValue::Scalar(DomainValue::Json(map))), FieldType::Json) => {
            let json =
                serde_json::to_value(map).map_err(|e| DomainError::Storage(e.to_string()))?;
            qb.push_bind(json);
        }
        (Some(ContentValue::Scalar(DomainValue::Email(e))), FieldType::Email) => {
            qb.push_bind(e.as_ref().to_string());
        }
        (Some(ContentValue::Scalar(DomainValue::Url(u))), FieldType::Url) => {
            qb.push_bind(u.as_ref().to_string());
        }
        (Some(ContentValue::Scalar(DomainValue::Primitive(p))), _) => match p {
            PrimitiveValue::Text(s) => {
                qb.push_bind(s.clone());
            }
            PrimitiveValue::Uid(s) => {
                qb.push_bind(s.clone());
            }
            PrimitiveValue::Uuid(u) => {
                qb.push_bind(*u);
            }
            PrimitiveValue::Integer(i) => match ft {
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => {
                    qb.push_bind(*i as i16);
                }
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => {
                    qb.push_bind(*i as i32);
                }
                _ => {
                    qb.push_bind(*i);
                }
            },
            PrimitiveValue::Decimal(d) => {
                qb.push_bind(*d);
            }
            PrimitiveValue::Boolean(b) => {
                qb.push_bind(*b);
            }
            PrimitiveValue::Date(d) => {
                qb.push_bind(*d);
            }
            PrimitiveValue::DateTime(dt) => {
                qb.push_bind(*dt);
            }
        },
        _ => {
            qb.push("NULL");
        }
    }
    Ok(())
}

fn apply_field_filter(qb: &mut sqlx::QueryBuilder<sqlx::Postgres>, filter: &FieldFilter) {
    let col = attribute_to_column_name(&filter.attribute_id);
    qb.push(" AND \"");
    qb.push(&col);
    qb.push("\" = ");

    match &filter.value {
        DomainValue::Primitive(PrimitiveValue::Text(s)) => {
            qb.push_bind(s.clone());
        }
        DomainValue::Primitive(PrimitiveValue::Uid(s)) => {
            qb.push_bind(s.clone());
        }
        DomainValue::Primitive(PrimitiveValue::Uuid(u)) => {
            qb.push_bind(*u);
        }
        DomainValue::Primitive(PrimitiveValue::Integer(i)) => {
            qb.push_bind(*i);
        }
        DomainValue::Primitive(PrimitiveValue::Decimal(d)) => {
            qb.push_bind(*d);
        }
        DomainValue::Primitive(PrimitiveValue::Boolean(b)) => {
            qb.push_bind(*b);
        }
        DomainValue::Primitive(PrimitiveValue::Date(d)) => {
            qb.push_bind(*d);
        }
        DomainValue::Primitive(PrimitiveValue::DateTime(dt)) => {
            qb.push_bind(*dt);
        }
        DomainValue::Email(e) => {
            qb.push_bind(e.as_ref().to_string());
        }
        DomainValue::Url(u) => {
            qb.push_bind(u.as_ref().to_string());
        }
        DomainValue::Json(map) => {
            let json = serde_json::to_value(map).unwrap_or(serde_json::Value::Null);
            qb.push_bind(json);
        }
    }
}

impl DocumentInstanceRepository for SqlxDocumentInstanceRepository {
    fn find_by_id(
        &self,
        type_id: DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        async move {
            let doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let table_name = document_type_to_table_name(doc_type);

            let uuid = *id.as_ref();
            let mut qb = sqlx::QueryBuilder::new("SELECT * FROM \"");
            qb.push(&table_name);
            qb.push("\" WHERE id = ");
            qb.push_bind(uuid);

            let row_opt = qb
                .build()
                .fetch_optional(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            let row = match row_opt {
                Some(r) => r,
                None => return Ok(None),
            };

            // Audit
            let version: i64 = row.get("version");
            let owner_id_str: String = row.get("owner_id");
            let created_at: DateTime<Utc> = row.get("created_at");
            let updated_at: DateTime<Utc> = row.get("updated_at");
            let pub_state_str: String = row.get("publication_state");

            // User declared fields
            let mut fields = HashMap::new();
            for (attr_id, field_def) in &doc_type.fields {
                let col_name = attribute_to_column_name(attr_id);
                let val = read_content_value(&row, &col_name, &field_def.field_type)?;
                fields.insert(attr_id.clone(), val);
            }

            // Relations owned by this document type
            let mut relations = HashMap::new();
            let rel_views = schema_reg.find_relations_for(&type_id);
            for rel in rel_views {
                let (attr, _kind) = match rel {
                    RelationView::Unidirectional { attr, kind, .. } => (attr, kind),
                    RelationView::OwnerSide { attr, kind, .. } => (attr, kind),
                    RelationView::InverseSide { .. } => continue,
                };

                let link_name = link_table_name(&table_name, &attr);
                let mut link_qb = sqlx::QueryBuilder::new("SELECT target_id FROM \"");
                link_qb.push(&link_name);
                link_qb.push("\" WHERE owner_id = ");
                link_qb.push_bind(uuid);
                link_qb.push(" ORDER BY target_id ASC");

                let link_rows = link_qb
                    .build()
                    .fetch_all(&pool)
                    .await
                    .map_err(|e| DomainError::Storage(e.to_string()))?;

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
                    let pub_table = published_table_name(&table_name);
                    let mut pub_qb = sqlx::QueryBuilder::new(
                        "SELECT published_version, published_at, published_by FROM \"",
                    );
                    pub_qb.push(&pub_table);
                    pub_qb.push("\" WHERE id = ");
                    pub_qb.push_bind(uuid);

                    if let Some(pub_row) = pub_qb
                        .build()
                        .fetch_optional(&pool)
                        .await
                        .map_err(|e| DomainError::Storage(e.to_string()))?
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
                                .map_err(|e| DomainError::Storage(e.to_string()))?,
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
                document_type_id: type_id,
                content: DocumentContent {
                    fields,
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
        type_id: DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<Page<DocumentInstance>, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        async move {
            let doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let table_name = document_type_to_table_name(doc_type);

            // 1. Count query
            let mut count_qb = sqlx::QueryBuilder::new("SELECT COUNT(*) AS total FROM \"");
            count_qb.push(&table_name);
            count_qb.push("\" WHERE 1=1 ");
            for filter in &filters {
                apply_field_filter(&mut count_qb, filter);
            }

            let count_row = count_qb
                .build()
                .fetch_one(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            let total: i64 = count_row.get("total");

            // 2. Select query
            let mut select_qb = sqlx::QueryBuilder::new("SELECT * FROM \"");
            select_qb.push(&table_name);
            select_qb.push("\" WHERE 1=1 ");
            for filter in &filters {
                apply_field_filter(&mut select_qb, filter);
            }
            select_qb.push(" ORDER BY created_at DESC");

            let offset = (pagination.page.saturating_sub(1) as i64) * (pagination.page_size as i64);
            select_qb.push(" LIMIT ");
            select_qb.push_bind(pagination.page_size as i64);
            select_qb.push(" OFFSET ");
            select_qb.push_bind(offset);

            let rows = select_qb
                .build()
                .fetch_all(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                let inst_uuid: Uuid = row.get("id");
                let inst_id = DocumentInstanceId::new(inst_uuid);

                let version: i64 = row.get("version");
                let owner_id_str: String = row.get("owner_id");
                let created_at: DateTime<Utc> = row.get("created_at");
                let updated_at: DateTime<Utc> = row.get("updated_at");
                let pub_state_str: String = row.get("publication_state");

                let mut fields = HashMap::new();
                for (attr_id, field_def) in &doc_type.fields {
                    let col_name = attribute_to_column_name(attr_id);
                    let val = read_content_value(&row, &col_name, &field_def.field_type)?;
                    fields.insert(attr_id.clone(), val);
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
                        fields,
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
        type_id: DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<u64, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        async move {
            let doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let table_name = document_type_to_table_name(doc_type);

            let mut qb = sqlx::QueryBuilder::new("SELECT COUNT(*) AS total FROM \"");
            qb.push(&table_name);
            qb.push("\" WHERE 1=1 ");
            for filter in &filters {
                apply_field_filter(&mut qb, filter);
            }

            let row = qb
                .build()
                .fetch_one(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            let total: i64 = row.get("total");
            Ok(total as u64)
        }
    }

    fn fetch_relations(
        &self,
        type_id: DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<RelationMap, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        let attributes = attributes.to_vec();
        let parent_ids = parent_ids.to_vec();

        async move {
            let mut result_map: RelationMap = HashMap::new();
            if parent_ids.is_empty() || attributes.is_empty() {
                return Ok(result_map);
            }

            let _doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;

            let parent_uuids: Vec<Uuid> = parent_ids.iter().map(|id| *id.as_ref()).collect();

            for attr in attributes {
                let rel = match schema_reg.find_relation_for_attr(&type_id, &attr) {
                    Some(r) => r,
                    None => continue,
                };

                let is_inverse = rel.target_type == type_id;
                let owner_doc_type = schema_reg
                    .find_type(&rel.owner_type)
                    .ok_or_else(|| DomainError::DocumentTypeNotFound(rel.owner_type.clone()))?;
                let owner_table = document_type_to_table_name(owner_doc_type);
                let link_name = link_table_name(&owner_table, &rel.owner_attr);
                let target_type_id = if is_inverse {
                    rel.owner_type.clone()
                } else {
                    rel.target_type.clone()
                };

                let parent_col = if is_inverse { "target_id" } else { "owner_id" };
                let child_col = if is_inverse { "owner_id" } else { "target_id" };

                // Query link table for pairs
                let mut link_qb = sqlx::QueryBuilder::new("SELECT \"");
                link_qb.push(parent_col);
                link_qb.push("\", \"");
                link_qb.push(child_col);
                link_qb.push("\" FROM \"");
                link_qb.push(&link_name);
                link_qb.push("\" WHERE \"");
                link_qb.push(parent_col);
                link_qb.push("\" = ANY(");
                link_qb.push_bind(&parent_uuids);
                link_qb.push(")");

                let link_rows = link_qb
                    .build()
                    .fetch_all(&pool)
                    .await
                    .map_err(|e| DomainError::Storage(e.to_string()))?;

                let mut child_uuids = Vec::new();
                let mut pairs: Vec<(DocumentInstanceId, DocumentInstanceId)> = Vec::new();
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

                // Deduplicate child IDs
                let unique_child_uuids: Vec<Uuid> = child_uuids
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                // Fetch child instances
                let target_dt = schema_reg
                    .find_type(&target_type_id)
                    .ok_or_else(|| DomainError::DocumentTypeNotFound(target_type_id.clone()))?;
                let target_table = document_type_to_table_name(target_dt);

                let mut target_qb = sqlx::QueryBuilder::new("SELECT * FROM \"");
                target_qb.push(&target_table);
                target_qb.push("\" WHERE id = ANY(");
                target_qb.push_bind(&unique_child_uuids);
                target_qb.push(")");

                let target_rows = target_qb
                    .build()
                    .fetch_all(&pool)
                    .await
                    .map_err(|e| DomainError::Storage(e.to_string()))?;

                let mut child_instances: HashMap<DocumentInstanceId, DocumentInstance> =
                    HashMap::with_capacity(target_rows.len());

                for row in target_rows {
                    let c_id: Uuid = row.get("id");
                    let inst_id = DocumentInstanceId::new(c_id);
                    let version: i64 = row.get("version");
                    let owner_id_str: String = row.get("owner_id");
                    let created_at: DateTime<Utc> = row.get("created_at");
                    let updated_at: DateTime<Utc> = row.get("updated_at");

                    let mut fields = HashMap::new();
                    for (c_attr_id, c_field_def) in &target_dt.fields {
                        let c_col = attribute_to_column_name(c_attr_id);
                        let val = read_content_value(&row, &c_col, &c_field_def.field_type)?;
                        fields.insert(c_attr_id.clone(), val);
                    }

                    child_instances.insert(
                        inst_id,
                        DocumentInstance {
                            id: inst_id,
                            db_row_id: Some(inst_id),
                            document_type_id: target_type_id.clone(),
                            content: DocumentContent {
                                fields,
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

                // Map to parent instance IDs
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
        let schema_reg = self.schema_registry.clone();
        let instance = instance.clone();

        async move {
            let doc_type = schema_reg
                .find_type(&instance.document_type_id)
                .ok_or_else(|| {
                    DomainError::DocumentTypeNotFound(instance.document_type_id.clone())
                })?;
            let table_name = document_type_to_table_name(doc_type);

            let inst_uuid = *instance.id.as_ref();
            let owner_id_str = instance
                .audit
                .created_by
                .as_ref()
                .map(|u| u.as_ref().to_string())
                .unwrap_or_else(|| "system".into());

            let pub_state_str = match &instance.content.publication_state {
                PublicationState::Draft { .. } => "draft",
                PublicationState::Published { .. } => "published",
            };

            // 1. Build Upsert into working draft table {table}
            let mut qb = sqlx::QueryBuilder::new("INSERT INTO \"");
            qb.push(&table_name);
            qb.push("\" (id, version, owner_id, publication_state, created_at, updated_at");

            if doc_type.kind == DocumentKind::SingleType {
                qb.push(", _singleton");
            }

            for attr_id in doc_type.fields.keys() {
                let col = attribute_to_column_name(attr_id);
                qb.push(", \"");
                qb.push(&col);
                qb.push("\"");
            }

            qb.push(") VALUES (");
            qb.push_bind(inst_uuid);
            qb.push(", ");
            qb.push_bind(instance.audit.version as i64);
            qb.push(", ");
            qb.push_bind(&owner_id_str);
            qb.push(", ");
            qb.push_bind(pub_state_str);
            qb.push(", ");
            qb.push_bind(instance.audit.created_at);
            qb.push(", ");
            qb.push_bind(instance.audit.updated_at);

            if doc_type.kind == DocumentKind::SingleType {
                qb.push(", TRUE");
            }

            for (attr_id, field_def) in &doc_type.fields {
                qb.push(", ");
                let val = instance.content.fields.get(attr_id);
                bind_content_value(&mut qb, val, &field_def.field_type)?;
            }

            qb.push(") ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version, owner_id = EXCLUDED.owner_id, publication_state = EXCLUDED.publication_state, updated_at = EXCLUDED.updated_at");

            for attr_id in doc_type.fields.keys() {
                let col = attribute_to_column_name(attr_id);
                qb.push(", \"");
                qb.push(&col);
                qb.push("\" = EXCLUDED.\"");
                qb.push(&col);
                qb.push("\"");
            }

            qb.build()
                .execute(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            // 2. Draft Link Tables for relations owned by this entity
            let rel_views = schema_reg.find_relations_for(&instance.document_type_id);
            for rel in &rel_views {
                let attr = match rel {
                    RelationView::Unidirectional { attr, .. } => attr,
                    RelationView::OwnerSide { attr, .. } => attr,
                    RelationView::InverseSide { .. } => continue,
                };

                let link_name = link_table_name(&table_name, attr);

                // Clear previous links
                let mut del_qb = sqlx::QueryBuilder::new("DELETE FROM \"");
                del_qb.push(&link_name);
                del_qb.push("\" WHERE owner_id = ");
                del_qb.push_bind(inst_uuid);
                del_qb
                    .build()
                    .execute(&pool)
                    .await
                    .map_err(|e| DomainError::Storage(e.to_string()))?;

                // Insert new relations
                if let Some(resolved) = instance.relations.get(attr) {
                    for r in resolved {
                        let target_uuid = *r.target_instance_id.as_ref();
                        let mut ins_qb = sqlx::QueryBuilder::new("INSERT INTO \"");
                        ins_qb.push(&link_name);
                        ins_qb.push("\" (owner_id, target_id) VALUES (");
                        ins_qb.push_bind(inst_uuid);
                        ins_qb.push(", ");
                        ins_qb.push_bind(target_uuid);
                        ins_qb.push(") ON CONFLICT DO NOTHING");

                        ins_qb
                            .build()
                            .execute(&pool)
                            .await
                            .map_err(|e| DomainError::Storage(e.to_string()))?;
                    }
                }
            }

            // 3. Published Mirror Table & Published Link Tables
            if doc_type.options.draft_and_publish {
                let pub_table = published_table_name(&table_name);

                match &instance.content.publication_state {
                    PublicationState::Published {
                        revision,
                        published_at,
                        published_by,
                    } => {
                        let p_by_str = published_by
                            .as_ref()
                            .map(|u| u.as_ref().to_string())
                            .or_else(|| Some(owner_id_str.clone()));

                        let mut pub_qb = sqlx::QueryBuilder::new("INSERT INTO \"");
                        pub_qb.push(&pub_table);
                        pub_qb.push("\" (id, published_version, owner_id, created_at, updated_at, published_at, published_by");

                        if doc_type.kind == DocumentKind::SingleType {
                            pub_qb.push(", _singleton");
                        }

                        for attr_id in doc_type.fields.keys() {
                            let col = attribute_to_column_name(attr_id);
                            pub_qb.push(", \"");
                            pub_qb.push(&col);
                            pub_qb.push("\"");
                        }

                        pub_qb.push(") VALUES (");
                        pub_qb.push_bind(inst_uuid);
                        pub_qb.push(", ");
                        pub_qb.push_bind(*revision as i64);
                        pub_qb.push(", ");
                        pub_qb.push_bind(&owner_id_str);
                        pub_qb.push(", ");
                        pub_qb.push_bind(instance.audit.created_at);
                        pub_qb.push(", ");
                        pub_qb.push_bind(instance.audit.updated_at);
                        pub_qb.push(", ");
                        pub_qb.push_bind(*published_at);
                        pub_qb.push(", ");
                        pub_qb.push_bind(p_by_str);

                        if doc_type.kind == DocumentKind::SingleType {
                            pub_qb.push(", TRUE");
                        }

                        for (attr_id, field_def) in &doc_type.fields {
                            pub_qb.push(", ");
                            let val = instance.content.fields.get(attr_id);
                            bind_content_value(&mut pub_qb, val, &field_def.field_type)?;
                        }

                        pub_qb.push(") ON CONFLICT (id) DO UPDATE SET published_version = EXCLUDED.published_version, owner_id = EXCLUDED.owner_id, updated_at = EXCLUDED.updated_at, published_at = EXCLUDED.published_at, published_by = EXCLUDED.published_by");

                        for attr_id in doc_type.fields.keys() {
                            let col = attribute_to_column_name(attr_id);
                            pub_qb.push(", \"");
                            pub_qb.push(&col);
                            pub_qb.push("\" = EXCLUDED.\"");
                            pub_qb.push(&col);
                            pub_qb.push("\"");
                        }

                        pub_qb
                            .build()
                            .execute(&pool)
                            .await
                            .map_err(|e| DomainError::Storage(e.to_string()))?;

                        // Published Link Tables
                        for rel in &rel_views {
                            let attr = match rel {
                                RelationView::Unidirectional { attr, .. } => attr,
                                RelationView::OwnerSide { attr, .. } => attr,
                                RelationView::InverseSide { .. } => continue,
                            };

                            let draft_link = link_table_name(&table_name, attr);
                            let pub_link = published_link_table_name(&draft_link);

                            let mut del_pub_link = sqlx::QueryBuilder::new("DELETE FROM \"");
                            del_pub_link.push(&pub_link);
                            del_pub_link.push("\" WHERE owner_id = ");
                            del_pub_link.push_bind(inst_uuid);
                            del_pub_link
                                .build()
                                .execute(&pool)
                                .await
                                .map_err(|e| DomainError::Storage(e.to_string()))?;

                            if let Some(resolved) = instance.relations.get(attr) {
                                for r in resolved {
                                    let target_uuid = *r.target_instance_id.as_ref();
                                    let mut ins_pub_link =
                                        sqlx::QueryBuilder::new("INSERT INTO \"");
                                    ins_pub_link.push(&pub_link);
                                    ins_pub_link.push("\" (owner_id, target_id) VALUES (");
                                    ins_pub_link.push_bind(inst_uuid);
                                    ins_pub_link.push(", ");
                                    ins_pub_link.push_bind(target_uuid);
                                    ins_pub_link.push(") ON CONFLICT DO NOTHING");

                                    // May fail if target is not published (Variant 1: Public Filter Principle constraint)
                                    let _ = ins_pub_link.build().execute(&pool).await;
                                }
                            }
                        }
                    }
                    PublicationState::Draft { .. } => {
                        // If document is unpublishing, remove row from published table
                        let mut del_pub = sqlx::QueryBuilder::new("DELETE FROM \"");
                        del_pub.push(&pub_table);
                        del_pub.push("\" WHERE id = ");
                        del_pub.push_bind(inst_uuid);
                        del_pub
                            .build()
                            .execute(&pool)
                            .await
                            .map_err(|e| DomainError::Storage(e.to_string()))?;
                    }
                }
            }

            Ok(())
        }
    }

    fn delete(
        &self,
        type_id: DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        async move {
            let doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let table_name = document_type_to_table_name(doc_type);

            let uuid = *id.as_ref();
            let mut qb = sqlx::QueryBuilder::new("DELETE FROM \"");
            qb.push(&table_name);
            qb.push("\" WHERE id = ");
            qb.push_bind(uuid);

            qb.build()
                .execute(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            Ok(())
        }
    }

    fn exists_for_type(
        &self,
        type_id: DocumentTypeId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send {
        let pool = self.pool.clone();
        let schema_reg = self.schema_registry.clone();
        async move {
            let doc_type = schema_reg
                .find_type(&type_id)
                .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
            let table_name = document_type_to_table_name(doc_type);

            let mut qb = sqlx::QueryBuilder::new("SELECT EXISTS(SELECT 1 FROM \"");
            qb.push(&table_name);
            qb.push("\" LIMIT 1) AS exists");

            let row = qb
                .build()
                .fetch_one(&pool)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;

            let exists: bool = row.get("exists");
            Ok(exists)
        }
    }
}
