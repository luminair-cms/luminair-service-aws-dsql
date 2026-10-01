//! SeaQuery statement builders for dynamic document instances and relational link tables.

use domain::content::{DocumentInstance, FieldFilter, PublicationState};
use domain::schema::{DocumentKind, DocumentType, FieldDefinition};
use sea_query::{
    Alias, Condition, DeleteStatement, DynIden, Expr, ExprTrait, Func, InsertStatement, IntoIden,
    OnConflict, Order, Query, SelectStatement,
};
use uuid::Uuid;

use crate::persistence::naming::{
    BaseSystemColumn, DocumentTableNaming, LinkColumn, LinkTableNaming, PublishedSystemColumn,
};
use super::codec::to_sea_value;
use super::filters::filter_to_condition;

/// Builds a `SELECT` statement to fetch a single document instance by primary key.
pub fn build_select_by_id(
    naming: &DocumentTableNaming<'_>,
    id: Uuid,
    fields: &[FieldDefinition],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(naming.table_iden()).columns([
        BaseSystemColumn::Id,
        BaseSystemColumn::Version,
        BaseSystemColumn::OwnerId,
        BaseSystemColumn::PublicationState,
        BaseSystemColumn::CreatedAt,
        BaseSystemColumn::UpdatedAt,
    ]);

    if naming.doc_type.map(|d| d.kind == DocumentKind::SingleType).unwrap_or(false) {
        stmt.column(BaseSystemColumn::Singleton);
    }

    for field in fields {
        stmt.column(naming.column_iden(&field.id));
    }

    stmt.and_where(Expr::col(BaseSystemColumn::Id).eq(id));
    stmt
}

/// Builds a `SELECT` statement for paginated querying with filters.
pub fn build_select_by_type(
    naming: &DocumentTableNaming<'_>,
    fields: &[FieldDefinition],
    filters: &[FieldFilter],
    limit: u64,
    offset: u64,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(naming.table_iden()).columns([
        BaseSystemColumn::Id,
        BaseSystemColumn::Version,
        BaseSystemColumn::OwnerId,
        BaseSystemColumn::PublicationState,
        BaseSystemColumn::CreatedAt,
        BaseSystemColumn::UpdatedAt,
    ]);

    if naming.doc_type.map(|d| d.kind == DocumentKind::SingleType).unwrap_or(false) {
        stmt.column(BaseSystemColumn::Singleton);
    }

    for field in fields {
        stmt.column(naming.column_iden(&field.id));
    }

    if !filters.is_empty() {
        let mut cond = Condition::all();
        for filter in filters {
            cond = cond.add(filter_to_condition(naming, filter));
        }
        stmt.cond_where(cond);
    }

    stmt.order_by(BaseSystemColumn::CreatedAt, Order::Desc)
        .limit(limit)
        .offset(offset);

    stmt
}

/// Builds a `SELECT COUNT(*)` statement with filters.
pub fn build_count(
    naming: &DocumentTableNaming<'_>,
    filters: &[FieldFilter],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(naming.table_iden())
        .expr_as(Func::count(Expr::col(sea_query::Asterisk)), Alias::new("total"));

    if !filters.is_empty() {
        let mut cond = Condition::all();
        for filter in filters {
            cond = cond.add(filter_to_condition(naming, filter));
        }
        stmt.cond_where(cond);
    }

    stmt
}

/// Builds a `SELECT EXISTS(...)` statement.
pub fn build_exists(naming: &DocumentTableNaming<'_>) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.expr_as(
        Expr::cust(format!(
            "EXISTS(SELECT 1 FROM \"{}\" LIMIT 1)",
            naming.table_name()
        )),
        Alias::new("exists"),
    );
    stmt
}

/// Builds a `DELETE` statement for a document by ID.
pub fn build_delete(naming: &DocumentTableNaming<'_>, id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(naming.table_iden())
        .and_where(Expr::col(BaseSystemColumn::Id).eq(id));
    stmt
}

/// Builds an upsert (`INSERT ... ON CONFLICT (id) DO UPDATE SET ...`) for a working draft instance.
pub fn build_upsert(
    naming: &DocumentTableNaming<'_>,
    doc_type: &DocumentType,
    instance: &DocumentInstance,
) -> Result<InsertStatement, domain::errors::DomainError> {
    let id = *instance.id.as_ref();
    let version = instance.audit.version as i64;
    let owner_id = instance
        .audit
        .created_by
        .as_ref()
        .map(|u| u.as_ref().to_string())
        .unwrap_or_else(|| "system".into());
    let publication_state = match &instance.content.publication_state {
        PublicationState::Draft { .. } => "draft",
        PublicationState::Published { .. } => "published",
    };
    let created_at = instance.audit.created_at;
    let updated_at = instance.audit.updated_at;

    let mut stmt = Query::insert();
    stmt.into_table(naming.table_iden());

    let mut columns: Vec<DynIden> = vec![
        BaseSystemColumn::Id.into_iden(),
        BaseSystemColumn::Version.into_iden(),
        BaseSystemColumn::OwnerId.into_iden(),
        BaseSystemColumn::PublicationState.into_iden(),
        BaseSystemColumn::CreatedAt.into_iden(),
        BaseSystemColumn::UpdatedAt.into_iden(),
    ];

    let mut values: Vec<Expr> = vec![
        Expr::val(id),
        Expr::val(version),
        Expr::val(owner_id),
        Expr::val(publication_state.to_string()),
        Expr::val(created_at),
        Expr::val(updated_at),
    ];

    if doc_type.kind == DocumentKind::SingleType {
        columns.push(BaseSystemColumn::Singleton.into_iden());
        values.push(Expr::val(true));
    }

    let mut update_columns: Vec<DynIden> = vec![
        BaseSystemColumn::Version.into_iden(),
        BaseSystemColumn::OwnerId.into_iden(),
        BaseSystemColumn::PublicationState.into_iden(),
        BaseSystemColumn::UpdatedAt.into_iden(),
    ];

    for field_def in &doc_type.fields {
        let col_iden = naming.column_iden(&field_def.id);
        columns.push(col_iden.clone().into_iden());
        update_columns.push(col_iden.into_iden());

        let val_opt = instance.content.fields.get(&field_def.id);
        let sea_val = to_sea_value(val_opt, &field_def.field_type)?;
        values.push(Expr::val(sea_val));
    }

    stmt.columns(columns).values_panic(values);

    let on_conflict = OnConflict::column(BaseSystemColumn::Id)
        .update_columns(update_columns)
        .to_owned();
    stmt.on_conflict(on_conflict);

    Ok(stmt)
}

/// Builds a `SELECT` statement to fetch target relation IDs for a single owner instance.
pub fn build_link_select(link_naming: &LinkTableNaming, owner_id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(link_naming.table_iden())
        .column(LinkColumn::TargetId)
        .and_where(Expr::col(LinkColumn::OwnerId).eq(owner_id))
        .order_by(LinkColumn::TargetId, Order::Asc);
    stmt
}

/// Builds a `DELETE` statement to remove existing relation links for an owner instance.
pub fn build_link_delete(link_naming: &LinkTableNaming, owner_id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(link_naming.table_iden())
        .and_where(Expr::col(LinkColumn::OwnerId).eq(owner_id));
    stmt
}

/// Builds a batch `INSERT ... ON CONFLICT DO NOTHING` statement for relational links.
pub fn build_link_batch_insert(
    link_naming: &LinkTableNaming,
    owner_id: Uuid,
    target_ids: &[Uuid],
) -> Option<InsertStatement> {
    if target_ids.is_empty() {
        return None;
    }

    let mut stmt = Query::insert();
    stmt.into_table(link_naming.table_iden())
        .columns([LinkColumn::OwnerId, LinkColumn::TargetId]);

    for target_id in target_ids {
        stmt.values_panic([Expr::val(owner_id), Expr::val(*target_id)]);
    }

    stmt.on_conflict(OnConflict::new().do_nothing().to_owned());
    Some(stmt)
}

/// Builds a batch `SELECT` query for link table pairs across multiple parent instance IDs.
pub fn build_link_pairs_select(
    link_naming: &LinkTableNaming,
    parent_col: &str,
    child_col: &str,
    parent_uuids: &[Uuid],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(link_naming.table_iden())
        .columns([Alias::new(parent_col), Alias::new(child_col)])
        .and_where(
            Expr::col(Alias::new(parent_col))
                .is_in(parent_uuids.iter().copied().map(Expr::val)),
        );
    stmt
}

/// Builds a batch `SELECT` query for target instances given unique target UUIDs.
pub fn build_instances_batch_select(
    target_naming: &DocumentTableNaming<'_>,
    fields: &[FieldDefinition],
    target_uuids: &[Uuid],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(target_naming.table_iden()).columns([
        BaseSystemColumn::Id,
        BaseSystemColumn::Version,
        BaseSystemColumn::OwnerId,
        BaseSystemColumn::PublicationState,
        BaseSystemColumn::CreatedAt,
        BaseSystemColumn::UpdatedAt,
    ]);

    if target_naming.doc_type.map(|d| d.kind == DocumentKind::SingleType).unwrap_or(false) {
        stmt.column(BaseSystemColumn::Singleton);
    }

    for field in fields {
        stmt.column(target_naming.column_iden(&field.id));
    }

    stmt.and_where(
        Expr::col(BaseSystemColumn::Id).is_in(target_uuids.iter().copied().map(Expr::val)),
    );
    stmt
}

/// Builds a `SELECT` statement to query published metadata from the published mirror table.
pub fn build_published_select(
    naming: &DocumentTableNaming<'_>,
    id: Uuid,
) -> Option<SelectStatement> {
    let pub_table = naming.published_table_iden()?;
    let mut stmt = Query::select();
    stmt.from(pub_table)
        .columns([
            PublishedSystemColumn::PublishedVersion,
            PublishedSystemColumn::PublishedAt,
            PublishedSystemColumn::PublishedBy,
        ])
        .and_where(Expr::col(PublishedSystemColumn::Id).eq(id));
    Some(stmt)
}

/// Builds an upsert (`INSERT ... ON CONFLICT (id) DO UPDATE SET ...`) for the published mirror table.
pub fn build_published_upsert(
    naming: &DocumentTableNaming<'_>,
    doc_type: &DocumentType,
    instance: &DocumentInstance,
) -> Result<Option<InsertStatement>, domain::errors::DomainError> {
    let pub_table = match naming.published_table_iden() {
        Some(t) => t,
        None => return Ok(None),
    };

    let (revision, published_at, published_by) = match &instance.content.publication_state {
        PublicationState::Published {
            revision,
            published_at,
            published_by,
        } => (*revision, *published_at, published_by),
        PublicationState::Draft { .. } => return Ok(None),
    };

    let id = *instance.id.as_ref();
    let owner_id = instance
        .audit
        .created_by
        .as_ref()
        .map(|u| u.as_ref().to_string())
        .unwrap_or_else(|| "system".into());
    let p_by_str = published_by
        .as_ref()
        .map(|u| u.as_ref().to_string())
        .or_else(|| Some(owner_id.clone()));
    let created_at = instance.audit.created_at;
    let updated_at = instance.audit.updated_at;

    let mut stmt = Query::insert();
    stmt.into_table(pub_table);

    let mut columns: Vec<DynIden> = vec![
        PublishedSystemColumn::Id.into_iden(),
        PublishedSystemColumn::PublishedVersion.into_iden(),
        PublishedSystemColumn::OwnerId.into_iden(),
        PublishedSystemColumn::CreatedAt.into_iden(),
        PublishedSystemColumn::UpdatedAt.into_iden(),
        PublishedSystemColumn::PublishedAt.into_iden(),
        PublishedSystemColumn::PublishedBy.into_iden(),
    ];

    let mut values: Vec<Expr> = vec![
        Expr::val(id),
        Expr::val(revision as i64),
        Expr::val(owner_id),
        Expr::val(created_at),
        Expr::val(updated_at),
        Expr::val(published_at),
        Expr::val(p_by_str),
    ];

    if doc_type.kind == DocumentKind::SingleType {
        columns.push(PublishedSystemColumn::Singleton.into_iden());
        values.push(Expr::val(true));
    }

    let mut update_columns: Vec<DynIden> = vec![
        PublishedSystemColumn::PublishedVersion.into_iden(),
        PublishedSystemColumn::OwnerId.into_iden(),
        PublishedSystemColumn::UpdatedAt.into_iden(),
        PublishedSystemColumn::PublishedAt.into_iden(),
        PublishedSystemColumn::PublishedBy.into_iden(),
    ];

    for field_def in &doc_type.fields {
        let col_iden = naming.column_iden(&field_def.id);
        columns.push(col_iden.clone().into_iden());
        update_columns.push(col_iden.into_iden());

        let val_opt = instance.content.fields.get(&field_def.id);
        let sea_val = to_sea_value(val_opt, &field_def.field_type)?;
        values.push(Expr::val(sea_val));
    }

    stmt.columns(columns).values_panic(values);

    let on_conflict = OnConflict::column(PublishedSystemColumn::Id)
        .update_columns(update_columns)
        .to_owned();
    stmt.on_conflict(on_conflict);

    Ok(Some(stmt))
}

/// Builds a `DELETE` statement to remove an instance from the published mirror table.
pub fn build_published_delete(
    naming: &DocumentTableNaming<'_>,
    id: Uuid,
) -> Option<DeleteStatement> {
    let pub_table = naming.published_table_iden()?;
    let mut stmt = Query::delete();
    stmt.from_table(pub_table)
        .and_where(Expr::col(PublishedSystemColumn::Id).eq(id));
    Some(stmt)
}

/// Builds a `DELETE` statement to remove published relation links for an owner instance.
pub fn build_published_link_delete(
    link_naming: &LinkTableNaming,
    owner_id: Uuid,
) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(link_naming.published_table_iden())
        .and_where(Expr::col(LinkColumn::OwnerId).eq(owner_id));
    stmt
}

/// Builds a batch `INSERT ... ON CONFLICT DO NOTHING` statement for published link tables.
pub fn build_published_link_batch_insert(
    link_naming: &LinkTableNaming,
    owner_id: Uuid,
    target_ids: &[Uuid],
) -> Option<InsertStatement> {
    if target_ids.is_empty() {
        return None;
    }

    let mut stmt = Query::insert();
    stmt.into_table(link_naming.published_table_iden())
        .columns([LinkColumn::OwnerId, LinkColumn::TargetId]);

    for target_id in target_ids {
        stmt.values_panic([Expr::val(owner_id), Expr::val(*target_id)]);
    }

    stmt.on_conflict(OnConflict::new().do_nothing().to_owned());
    Some(stmt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::{
        AttributeId, DocumentTypeId, DocumentTypeInfo, DocumentTypeOptions, FieldType,
        PrimitiveType,
    };
    use indexmap::IndexSet;
    use sea_query::PostgresQueryBuilder;

    fn sample_doc_type() -> DocumentType {
        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: Vec::new(),
        });
        DocumentType {
            id: DocumentTypeId::try_new("article").unwrap(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Article".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
        }
    }

    #[test]
    fn test_build_select_by_id() {
        let dt = sample_doc_type();
        let naming = DocumentTableNaming::new(&dt);
        let id = Uuid::nil();
        let fields: Vec<_> = dt.fields.iter().cloned().collect();
        let stmt = build_select_by_id(&naming, id, &fields);

        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains(r#"SELECT "id", "version", "owner_id", "publication_state", "created_at", "updated_at", "title" FROM "articles" WHERE "id" = "#));
    }

    #[test]
    fn test_build_link_batch_insert() {
        let dt = sample_doc_type();
        let naming = DocumentTableNaming::new(&dt);
        let attr = AttributeId::try_new("tags").unwrap();
        let link_naming = naming.link_table(&attr);
        let owner_id = Uuid::nil();
        let target_ids = vec![Uuid::nil(), Uuid::max()];

        let stmt = build_link_batch_insert(&link_naming, owner_id, &target_ids).unwrap();
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains(r#"INSERT INTO "articles__tags_link" ("owner_id", "target_id") VALUES"#));
        assert!(sql.contains("DO NOTHING"));
    }
}

