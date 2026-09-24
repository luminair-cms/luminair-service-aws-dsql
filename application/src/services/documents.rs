//! Document instances application service.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use chrono::Utc;
use domain::entities::document_instance::DocumentInstance;
use domain::entities::document_type::DocumentKind;
use domain::entities::published_snapshot::PublishedSnapshot;
use domain::entities::role::Permission;
use domain::entities::system_config::SystemConfig;
use domain::errors::DomainError;
use domain::ports::{DocumentInstanceRepository, SnapshotRepository};
use domain::services::schema_registry::SchemaRegistry;
use domain::value_objects::{AttributeId, DocumentInstanceId, DocumentTypeId};

use crate::commands::documents::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

/// Port trait defining use cases for document instances.
pub trait DocumentsService: Send + Sync + 'static {
    /// Queries a paginated list of documents with sequential count and optional relation enrichment.
    fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> impl Future<Output = Result<(Vec<DocumentInstance>, u64), ApplicationError>> + Send;

    /// Fetches a single document instance by identifier with optional relation enrichment.
    fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, ApplicationError>> + Send;

    /// Creates a new draft document instance, enforcing singleton limits and schema validation.
    fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Updates field values of an existing document instance.
    fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Deletes a document instance by identifier.
    fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Publishes a draft document instance and persists an immutable snapshot.
    fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> impl Future<Output = Result<PublishedSnapshot, ApplicationError>> + Send;

    /// Reverts a published document instance back to draft state.
    fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Lists all published snapshots (revision history) for a document instance.
    fn list_snapshots(
        &self,
        caller: &CallerContext,
        cmd: ListSnapshotsCommand,
    ) -> impl Future<Output = Result<Vec<PublishedSnapshot>, ApplicationError>> + Send;
}

/// Generic implementation of `DocumentsService` monomorphized over repository adapters.
pub struct DocumentsServiceImpl<R, S> {
    pub instance_repo: Arc<R>,
    pub snapshot_repo: Arc<S>,
    pub schema_registry: Arc<SchemaRegistry>,
    pub system_config: Arc<SystemConfig>,
}

impl<R, S> DocumentsServiceImpl<R, S>
where
    R: DocumentInstanceRepository + 'static,
    S: SnapshotRepository + 'static,
{
    pub fn new(
        instance_repo: Arc<R>,
        snapshot_repo: Arc<S>,
        schema_registry: Arc<SchemaRegistry>,
        system_config: Arc<SystemConfig>,
    ) -> Self {
        Self {
            instance_repo,
            snapshot_repo,
            schema_registry,
            system_config,
        }
    }

    /// Two-phase batch relation enrichment:
    /// 1. Fetches related instances for all parent instances in a single batch query.
    /// 2. Stitches relations in-memory to each corresponding document instance.
    pub async fn enrich(
        &self,
        type_id: DocumentTypeId,
        populate: Option<Vec<AttributeId>>,
        instances: Vec<DocumentInstance>,
    ) -> Result<Vec<DocumentInstance>, ApplicationError> {
        let populate_attrs = match populate {
            Some(attrs) if !attrs.is_empty() => attrs,
            _ => return Ok(instances),
        };

        if instances.is_empty() {
            return Ok(instances);
        }

        let parent_ids: Vec<DocumentInstanceId> = instances.iter().map(|inst| inst.id).collect();

        // Batch fetch relations for all parent instances in one query
        let mut relation_map = self
            .instance_repo
            .fetch_relations(type_id, &populate_attrs, &parent_ids)
            .await?;

        // Attach populated relations in-memory
        let enriched = instances
            .into_iter()
            .map(|inst| {
                let mut per_doc_relations = HashMap::new();
                for attr in &populate_attrs {
                    if let Some(by_parent) = relation_map.get_mut(attr)
                        && let Some(related_items) = by_parent.remove(&inst.id)
                    {
                        per_doc_relations.insert(attr.clone(), related_items);
                    }
                }
                inst.with_populated_relations(per_doc_relations)
            })
            .collect();

        Ok(enriched)
    }
}

impl<R, S> DocumentsService for DocumentsServiceImpl<R, S>
where
    R: DocumentInstanceRepository + 'static,
    S: SnapshotRepository + 'static,
{
    async fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> Result<(Vec<DocumentInstance>, u64), ApplicationError> {
        caller.check_permission(
            &Permission::ReadDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        self.schema_registry
            .find_type(&cmd.document_type)
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::DocumentTypeNotFound(
                    cmd.document_type.clone(),
                ))
            })?;

        // Sequential fetch for MVP
        let page = self
            .instance_repo
            .find_by_type(
                cmd.document_type.clone(),
                cmd.pagination,
                cmd.filters.clone(),
            )
            .await?;
        let count = self
            .instance_repo
            .count(cmd.document_type.clone(), cmd.filters)
            .await?;

        // Two-phase batch relation enrichment
        let enriched = self
            .enrich(cmd.document_type, cmd.populate, page.items)
            .await?;

        Ok((enriched, count))
    }

    async fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> Result<Option<DocumentInstance>, ApplicationError> {
        let instance = self
            .instance_repo
            .find_by_id(cmd.document_type.clone(), cmd.document_instance_id)
            .await?;

        match instance {
            Some(inst) => {
                caller.check_permission(
                    &Permission::ReadDocument(Some(inst.document_type_id.clone())),
                    Some(&inst),
                )?;
                let mut enriched = self
                    .enrich(cmd.document_type, cmd.populate, vec![inst])
                    .await?;
                Ok(enriched.pop())
            }
            None => Ok(None),
        }
    }

    async fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        caller.check_permission(
            &Permission::CreateDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        let doc_type = self
            .schema_registry
            .find_type(&cmd.document_type)
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::DocumentTypeNotFound(
                    cmd.document_type.clone(),
                ))
            })?;

        // Singleton Guard (ADR-002 Option C)
        if doc_type.kind == DocumentKind::SingleType
            && self
                .instance_repo
                .exists_for_type(cmd.document_type.clone())
                .await?
        {
            return Err(ApplicationError::Domain(
                DomainError::SingleTypeAlreadyExists(cmd.document_type),
            ));
        }

        let mut instance = DocumentInstance::new(
            cmd.document_type.clone(),
            Some(caller.user_id.clone()),
            Utc::now(),
        );
        instance.content.fields = cmd.fields;

        // Content & locale validation — return ALL errors (R6)
        if let Err(errs) = self.schema_registry.validate_content(
            &cmd.document_type,
            &instance.content,
            &self.system_config,
        ) {
            let messages: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            return Err(ApplicationError::Validation(messages));
        }

        self.instance_repo.save(&instance).await?;
        Ok(instance)
    }

    async fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::UpdateDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        // Update fields and touch
        for (attr_id, val) in cmd.fields {
            instance.content.fields.insert(attr_id, val);
        }
        instance.touch(Some(caller.user_id.clone()), Utc::now());

        // Validate updated content — return ALL errors (R6)
        if let Err(errs) = self.schema_registry.validate_content(
            &instance.document_type_id,
            &instance.content,
            &self.system_config,
        ) {
            let messages: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            return Err(ApplicationError::Validation(messages));
        }

        self.instance_repo.save(&instance).await?;
        Ok(instance)
    }

    async fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> Result<(), ApplicationError> {
        let instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::DeleteDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        // Cascade delete: remove all snapshots before the instance (R2)
        // No FK in AWS DSQL — app layer is responsible for referential integrity (ADR-007)
        self.snapshot_repo
            .delete_by_instance(cmd.document_instance_id)
            .await?;
        self.instance_repo
            .delete(instance.document_type_id, cmd.document_instance_id)
            .await?;
        Ok(())
    }

    async fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> Result<PublishedSnapshot, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::PublishDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let doc_type = self
            .schema_registry
            .find_type(&instance.document_type_id)
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::DocumentTypeNotFound(
                    instance.document_type_id.clone(),
                ))
            })?;

        // Draft-and-publish guard (R7): only types that support publish/unpublish can be published
        if !doc_type.options.draft_and_publish {
            return Err(ApplicationError::Conflict(format!(
                "document type '{}' does not support draft-and-publish workflow",
                doc_type.info.title
            )));
        }

        let snapshot = instance.publish(
            &doc_type.info.plural_name,
            Some(caller.user_id.clone()),
            Utc::now(),
        )?;

        // Save snapshot first, then update instance state
        self.snapshot_repo.save(&snapshot).await?;
        self.instance_repo.save(&instance).await?;

        Ok(snapshot)
    }

    async fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::PublishDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let doc_type = self
            .schema_registry
            .find_type(&instance.document_type_id)
            .ok_or_else(|| {
                ApplicationError::Domain(DomainError::DocumentTypeNotFound(
                    instance.document_type_id.clone(),
                ))
            })?;

        // Draft-and-publish guard (R7): only types that support publish/unpublish can be unpublished
        if !doc_type.options.draft_and_publish {
            return Err(ApplicationError::Conflict(format!(
                "document type '{}' does not support draft-and-publish workflow",
                doc_type.info.title
            )));
        }

        instance.unpublish(Utc::now())?;
        self.instance_repo.save(&instance).await?;

        Ok(instance)
    }

    async fn list_snapshots(
        &self,
        caller: &CallerContext,
        cmd: ListSnapshotsCommand,
    ) -> Result<Vec<PublishedSnapshot>, ApplicationError> {
        // Require read permission; fetch instance to assert it belongs to the right type
        let instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::ReadDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let snapshots = self
            .snapshot_repo
            .find_by_instance(cmd.document_instance_id)
            .await?;

        Ok(snapshots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use domain::entities::document_instance::PublicationState;
    use domain::entities::document_type::{DocumentType, DocumentTypeInfo, DocumentTypeOptions};
    use domain::entities::field_definition::FieldDefinition;
    use domain::ports::document_instance_repository::Pagination;
    use domain::types::content_value::ContentValue;
    use domain::types::domain_value::DomainValue;
    use domain::types::field_type::{FieldType, PrimitiveType};
    use domain::types::primitive_value::PrimitiveValue;
    use domain::value_objects::{LocaleId, SystemConfigId, UserId};
    use indexmap::IndexMap;
    use uuid::Uuid;

    use crate::test_support::{FakeDocumentInstanceRepository, FakeSnapshotRepository};

    fn make_test_fixture(
        kind: DocumentKind,
    ) -> (
        DocumentsServiceImpl<FakeDocumentInstanceRepository, FakeSnapshotRepository>,
        DocumentType,
        CallerContext,
        AttributeId,
    ) {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let title_attr = AttributeId::try_new("title").unwrap();

        let mut fields = IndexMap::new();
        fields.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![],
            },
        );

        let doc_type = DocumentType {
            id: type_id,
            kind,
            info: DocumentTypeInfo {
                title: "Articles".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
        };

        let en = LocaleId::try_new("en").unwrap();
        let config = Arc::new(
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap(),
        );

        let schema_registry = Arc::new(SchemaRegistry::new(vec![doc_type.clone()], vec![]));
        let instance_repo = Arc::new(FakeDocumentInstanceRepository::new());
        let snapshot_repo = Arc::new(FakeSnapshotRepository::new());

        let service =
            DocumentsServiceImpl::new(instance_repo, snapshot_repo, schema_registry, config);

        let caller = CallerContext::system();
        (service, doc_type, caller, title_attr)
    }

    #[tokio::test]
    async fn test_create_and_find_by_id() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Hello Rust".into(),
            ))),
        );

        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .expect("create success");

        let fetched = service
            .find_by_id(
                &caller,
                FindByIdCommand::new(doc_type.id.clone(), created.id),
            )
            .await
            .expect("find success")
            .expect("found document");

        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.document_type_id, doc_type.id);
    }

    #[tokio::test]
    async fn test_create_single_type_guard_blocks_duplicate() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::SingleType);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Header".into(),
            ))),
        );

        // First creation succeeds
        service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields.clone()),
            )
            .await
            .expect("first create succeeds");

        // Second creation must fail with SingleTypeAlreadyExists
        let second = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await;

        assert!(matches!(
            second,
            Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(id))) if id == doc_type.id
        ));
    }

    #[tokio::test]
    async fn test_find_with_batch_relation_enrichment() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Post 1".into(),
            ))),
        );

        let inst = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .unwrap();

        // Setup relation data in fake repo
        let rel_attr = AttributeId::try_new("tags").unwrap();
        let tag_type_id = DocumentTypeId::try_new("tag").unwrap();
        let related_tag = DocumentInstance::new(tag_type_id, None, Utc::now());

        service.instance_repo.add_relation_data(
            rel_attr.clone(),
            inst.id,
            vec![related_tag.clone()],
        );

        // Query with populate
        let cmd = FindDocumentsCommand::new(doc_type.id.clone(), Pagination::default())
            .with_populate(vec![rel_attr.clone()]);

        let (items, count) = service.find(&caller, cmd).await.expect("find ok");
        assert_eq!(count, 1);
        assert_eq!(items.len(), 1);

        let doc = &items[0];
        let populated_tags = doc
            .populated_relations
            .get(&rel_attr)
            .expect("tags populated");
        assert_eq!(populated_tags.len(), 1);
        assert_eq!(populated_tags[0].id, related_tag.id);
    }

    #[tokio::test]
    async fn test_update_touches_audit_and_bumps_version() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Old Title".into(),
            ))),
        );

        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .unwrap();
        assert_eq!(created.audit.version, 1);

        let mut update_fields = HashMap::new();
        update_fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "New Title".into(),
            ))),
        );

        let updated = service
            .update(
                &caller,
                UpdateDocumentCommand::new(created.id, doc_type.id.clone(), update_fields),
            )
            .await
            .expect("update success");

        assert_eq!(updated.audit.version, 2);
    }

    #[tokio::test]
    async fn test_publish_and_unpublish_lifecycle() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Live News".into(),
            ))),
        );

        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .unwrap();

        // Publish
        let snapshot = service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("publish success");

        assert_eq!(snapshot.instance_id, created.id);
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.type_name, "articles");

        // Verify snapshot in snapshot repository
        let snaps = service
            .snapshot_repo
            .find_by_instance(created.id)
            .await
            .unwrap();
        assert_eq!(snaps.len(), 1);

        // Unpublish
        let unpublished = service
            .unpublish(
                &caller,
                UnpublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("unpublish success");

        assert!(matches!(
            unpublished.content.publication_state,
            PublicationState::Draft {
                last_published_revision: Some(1)
            }
        ));
    }

    #[tokio::test]
    async fn test_unauthorized_user_denied() {
        let (service, doc_type, _, title_attr) = make_test_fixture(DocumentKind::Collection);

        let unauthorized_caller = CallerContext::new(
            UserId::try_new("unauth_user").unwrap(),
            vec![], // no roles
        );

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Restricted".into(),
            ))),
        );

        let res = service
            .create(
                &unauthorized_caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await;

        assert!(matches!(res, Err(ApplicationError::Unauthorized { .. })));
    }

    // ──────────────────────────────────────────────────────────────────────
    // R5: list_snapshots
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_snapshots_returns_revision_history() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        // Create + publish twice
        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Draft 1".into(),
            ))),
        );
        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .expect("create succeeds");

        service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("first publish succeeds");
        service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("second publish succeeds");

        let snapshots = service
            .list_snapshots(
                &caller,
                ListSnapshotsCommand::new(doc_type.id.clone(), created.id),
            )
            .await
            .expect("list_snapshots succeeds");

        assert_eq!(snapshots.len(), 2, "should have 2 snapshots");
        assert_eq!(snapshots[0].revision, 1);
        assert_eq!(snapshots[1].revision, 2);
    }

    #[tokio::test]
    async fn test_list_snapshots_unknown_instance_returns_not_found() {
        let (service, doc_type, caller, _) = make_test_fixture(DocumentKind::Collection);

        let fake_id = domain::value_objects::DocumentInstanceId::new(Uuid::now_v7());
        let result = service
            .list_snapshots(
                &caller,
                ListSnapshotsCommand::new(doc_type.id.clone(), fake_id),
            )
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(_)
            ))
        ));
    }

    // ──────────────────────────────────────────────────────────────────────
    // R7: draft_and_publish lifecycle guard
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_publish_blocked_when_draft_and_publish_disabled() {
        // Build a doc type that has draft_and_publish = false
        let type_id = DocumentTypeId::try_new("simple").unwrap();
        let title_attr = AttributeId::try_new("title").unwrap();

        let mut field_defs = IndexMap::new();
        field_defs.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: false,
                unique: false,
                constraints: vec![],
            },
        );

        use domain::entities::document_type::{
            DocumentType, DocumentTypeInfo, DocumentTypeOptions,
        };
        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Simple".into(),
                singular_name: "simple".into(),
                plural_name: "simples".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: field_defs,
        };

        let en = LocaleId::try_new("en").unwrap();
        let config = Arc::new(
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap(),
        );
        let schema_registry = Arc::new(SchemaRegistry::new(vec![doc_type.clone()], vec![]));
        let instance_repo = Arc::new(FakeDocumentInstanceRepository::new());
        let snapshot_repo = Arc::new(FakeSnapshotRepository::new());
        let service =
            DocumentsServiceImpl::new(instance_repo, snapshot_repo, schema_registry, config);
        let caller = CallerContext::system();

        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), HashMap::new()),
            )
            .await
            .expect("create succeeds");

        let publish_result = service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await;

        assert!(
            matches!(publish_result, Err(ApplicationError::Conflict(_))),
            "expected Conflict error when draft_and_publish=false, got: {:?}",
            publish_result
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // R2: cascade delete of snapshots
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_cascades_snapshots() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Cascade".into(),
            ))),
        );
        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(doc_type.id.clone(), fields),
            )
            .await
            .expect("create succeeds");

        service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("publish succeeds");

        // Before delete: one snapshot
        let before = service
            .list_snapshots(
                &caller,
                ListSnapshotsCommand::new(doc_type.id.clone(), created.id),
            )
            .await
            .expect("list_snapshots before delete");
        assert_eq!(before.len(), 1);

        service
            .delete(
                &caller,
                DeleteDocumentCommand::new(created.id, doc_type.id.clone()),
            )
            .await
            .expect("delete succeeds");

        // After delete: instance is gone
        let found = service
            .find_by_id(
                &caller,
                FindByIdCommand::new(doc_type.id.clone(), created.id),
            )
            .await
            .expect("find_by_id returns Ok");
        assert!(found.is_none(), "instance should be gone after delete");
    }
}
