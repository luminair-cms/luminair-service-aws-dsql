//! Comprehensive end-to-end integration and workflow tests for the Application layer.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use domain::entities::document_instance::{DocumentInstance, PublicationState};
use domain::entities::document_type::{
    DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions,
};
use domain::entities::field_definition::FieldDefinition;
use domain::entities::role::{Permission, Role};
use domain::entities::system_config::SystemConfig;
use domain::errors::DomainError;
use domain::ports::document_instance_repository::Pagination;
use domain::services::schema_registry::SchemaRegistry;
use domain::types::content_value::ContentValue;
use domain::types::domain_value::DomainValue;
use domain::types::field_type::{FieldType, PrimitiveType};
use domain::types::primitive_value::PrimitiveValue;
use domain::value_objects::{
    AttributeId, DocumentTypeId, LocaleId, RoleId, SystemConfigId, UserId,
};
use indexmap::IndexMap;
use uuid::Uuid;

use application::commands::*;
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{
    AccessRequestsService, AccessRequestsServiceImpl, DocumentsService, DocumentsServiceImpl,
    SystemConfigService, SystemConfigServiceImpl,
};
use application::test_support::{
    FakeAccessRequestRepository, FakeDocumentInstanceRepository, FakeRoleRepository,
    FakeSnapshotRepository, FakeUserRoleAssignmentRepository,
};

/// Test harness packaging all interconnected application services and fake repositories.
#[allow(dead_code)]
struct TestAppHarness {
    pub documents_service:
        DocumentsServiceImpl<FakeDocumentInstanceRepository, FakeSnapshotRepository>,
    pub access_requests_service: AccessRequestsServiceImpl<
        FakeAccessRequestRepository,
        FakeUserRoleAssignmentRepository,
        FakeRoleRepository,
    >,
    pub system_config_service: SystemConfigServiceImpl,
    pub instance_repo: Arc<FakeDocumentInstanceRepository>,
    pub snapshot_repo: Arc<FakeSnapshotRepository>,
    pub access_request_repo: Arc<FakeAccessRequestRepository>,
    pub assignment_repo: Arc<FakeUserRoleAssignmentRepository>,
    pub role_repo: Arc<FakeRoleRepository>,
    pub editor_role: Role,
    pub article_type: DocumentType,
    pub single_type: DocumentType,
    pub title_attr: AttributeId,
    pub body_attr: AttributeId,
    pub admin_context: CallerContext,
}

impl TestAppHarness {
    fn setup() -> Self {
        // 1. System Config (en, uk)
        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let system_config = Arc::new(
            SystemConfig::new(
                SystemConfigId::new(Uuid::now_v7()),
                vec![en.clone(), uk],
                en,
            )
            .unwrap(),
        );

        // 2. Document Types
        let article_type_id = DocumentTypeId::new(Uuid::now_v7());
        let title_attr = AttributeId::try_new("title").unwrap();
        let body_attr = AttributeId::try_new("body").unwrap();

        let mut article_fields = IndexMap::new();
        article_fields.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![],
            },
        );
        article_fields.insert(
            body_attr.clone(),
            FieldDefinition {
                id: body_attr.clone(),
                field_type: FieldType::LocalizedText,
                required: false,
                unique: false,
                constraints: vec![],
            },
        );

        let article_type = DocumentType {
            id: article_type_id,
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

        let single_type_id = DocumentTypeId::new(Uuid::now_v7());
        let mut single_fields = IndexMap::new();
        single_fields.insert(
            title_attr.clone(),
            FieldDefinition {
                id: title_attr.clone(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                required: true,
                unique: false,
                constraints: vec![],
            },
        );

        let single_type = DocumentType {
            id: single_type_id,
            kind: DocumentKind::SingleType,
            info: DocumentTypeInfo {
                title: "Homepage".into(),
                singular_name: "homepage".into(),
                plural_name: "homepages".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: single_fields,
        };

        // 3. Schema Registry
        let schema_registry = Arc::new(SchemaRegistry::new(
            vec![article_type.clone(), single_type.clone()],
            vec![],
        ));

        // 4. Repositories
        let instance_repo = Arc::new(FakeDocumentInstanceRepository::new());
        let snapshot_repo = Arc::new(FakeSnapshotRepository::new());
        let access_request_repo = Arc::new(FakeAccessRequestRepository::new());
        let assignment_repo = Arc::new(FakeUserRoleAssignmentRepository::new());

        let editor_role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "editor".to_string(),
            description: Some("Can create, read, update, publish articles".into()),
            permissions: vec![
                Permission::CreateDocument(Some(article_type_id)),
                Permission::ReadDocument(Some(article_type_id)),
                Permission::UpdateDocument(Some(article_type_id)),
                Permission::PublishDocument(Some(article_type_id)),
            ],
        };
        let role_repo = Arc::new(FakeRoleRepository::new().with_role(editor_role.clone()));

        // 5. Services
        let documents_service = DocumentsServiceImpl::new(
            instance_repo.clone(),
            snapshot_repo.clone(),
            schema_registry,
            system_config.clone(),
        );

        let access_requests_service = AccessRequestsServiceImpl::new(
            access_request_repo.clone(),
            assignment_repo.clone(),
            role_repo.clone(),
        );

        let system_config_service = SystemConfigServiceImpl::new(system_config);
        let admin_context = CallerContext::system();

        Self {
            documents_service,
            access_requests_service,
            system_config_service,
            instance_repo,
            snapshot_repo,
            access_request_repo,
            assignment_repo,
            role_repo,
            editor_role,
            article_type,
            single_type,
            title_attr,
            body_attr,
            admin_context,
        }
    }
}

#[tokio::test]
async fn test_end_to_end_user_enrollment_and_content_authoring() {
    let harness = TestAppHarness::setup();

    // 1. User Alice arrives via IdP with no Luminair roles
    let alice_id = UserId::try_new("alice-sub-001").unwrap();
    let alice_unauthorized_ctx = CallerContext::new(alice_id.clone(), vec![]);

    // Alice cannot create documents initially
    let mut fields = HashMap::new();
    fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Alice's First Article".into(),
        ))),
    );
    let early_create = harness
        .documents_service
        .create(
            &alice_unauthorized_ctx,
            CreateDocumentCommand::new(harness.article_type.id, fields.clone()),
        )
        .await;
    assert!(matches!(
        early_create,
        Err(ApplicationError::Unauthorized { .. })
    ));

    // 2. Alice submits access request
    let submit_cmd = SubmitAccessRequestCommand::new(
        alice_id.clone(),
        Some("alice@example.com".into()),
        Some("Alice Smith".into()),
    );
    let request = harness
        .access_requests_service
        .submit(submit_cmd)
        .await
        .expect("submit succeeds");

    // 3. Admin inspects pending queue and approves Alice's request with editor role
    let pending = harness
        .access_requests_service
        .list_pending(&harness.admin_context)
        .await
        .expect("admin can list pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, request.id);

    let approve_cmd =
        ApproveAccessRequestCommand::new(request.id, vec![harness.editor_role.id]);
    let assignments = harness
        .access_requests_service
        .approve(&harness.admin_context, approve_cmd)
        .await
        .expect("approval succeeds");
    assert_eq!(assignments.len(), 1);

    // 4. Alice now has the editor role in her CallerContext
    let alice_ctx = CallerContext::new(alice_id.clone(), vec![harness.editor_role.clone()]);

    // 5. Alice creates article with localized content
    let mut loc_body = HashMap::new();
    loc_body.insert(LocaleId::try_new("en").unwrap(), "English body text".into());
    loc_body.insert(
        LocaleId::try_new("uk").unwrap(),
        "Текст українською".into(),
    );
    fields.insert(
        harness.body_attr.clone(),
        ContentValue::LocalizedText(loc_body),
    );

    let article = harness
        .documents_service
        .create(
            &alice_ctx,
            CreateDocumentCommand::new(harness.article_type.id, fields),
        )
        .await
        .expect("Alice can create document");
    assert_eq!(article.audit.version, 1);
    assert_eq!(article.audit.created_by, Some(alice_id.clone()));

    // 6. Alice updates the article, bumping version
    let mut update_fields = HashMap::new();
    update_fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Alice's Updated Article".into(),
        ))),
    );
    let updated = harness
        .documents_service
        .update(
            &alice_ctx,
            UpdateDocumentCommand::new(article.id, harness.article_type.id, update_fields),
        )
        .await
        .expect("update succeeds");
    assert_eq!(updated.audit.version, 2);

    // 7. Alice publishes the article -> produces snapshot revision 1
    let snapshot_v1 = harness
        .documents_service
        .publish(
            &alice_ctx,
            PublishDocumentCommand::new(article.id, harness.article_type.id),
        )
        .await
        .expect("publish succeeds");
    assert_eq!(snapshot_v1.revision, 1);
    assert_eq!(snapshot_v1.type_name, "articles");

    // 8. Alice unpublishes article -> returns to Draft with last_published_revision = 1
    let unpub = harness
        .documents_service
        .unpublish(
            &alice_ctx,
            UnpublishDocumentCommand::new(article.id, harness.article_type.id),
        )
        .await
        .expect("unpublish succeeds");
    assert!(matches!(
        unpub.content.publication_state,
        PublicationState::Draft {
            last_published_revision: Some(1)
        }
    ));

    // 9. Alice republishes -> advances to revision 2
    let snapshot_v2 = harness
        .documents_service
        .publish(
            &alice_ctx,
            PublishDocumentCommand::new(article.id, harness.article_type.id),
        )
        .await
        .expect("republish succeeds");
    assert_eq!(snapshot_v2.revision, 2);

    // 10. Another user Bob cannot update Alice's article
    let bob_id = UserId::try_new("bob-sub-002").unwrap();
    let bob_ctx = CallerContext::new(bob_id, vec![]);
    let bob_update = harness
        .documents_service
        .update(
            &bob_ctx,
            UpdateDocumentCommand::new(article.id, harness.article_type.id, HashMap::new()),
        )
        .await;
    assert!(matches!(
        bob_update,
        Err(ApplicationError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn test_single_type_singleton_enforcement_workflow() {
    let harness = TestAppHarness::setup();

    let mut fields = HashMap::new();
    fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Welcome Home".into(),
        ))),
    );

    // First creation succeeds
    let first = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(harness.single_type.id, fields.clone()),
        )
        .await
        .expect("first creation succeeds");
    assert_eq!(first.document_type_id, harness.single_type.id);

    // Second creation of SingleType must fail with SingleTypeAlreadyExists
    let second = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(harness.single_type.id, fields),
        )
        .await;

    assert!(matches!(
        second,
        Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(id))) if id == harness.single_type.id
    ));

    // Updating existing instance works
    let mut update_fields = HashMap::new();
    update_fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Welcome to Luminair".into(),
        ))),
    );
    let updated = harness
        .documents_service
        .update(
            &harness.admin_context,
            UpdateDocumentCommand::new(first.id, harness.single_type.id, update_fields),
        )
        .await
        .expect("update succeeds");
    assert_eq!(updated.audit.version, 2);

    // Deleting the instance succeeds
    harness
        .documents_service
        .delete(
            &harness.admin_context,
            DeleteDocumentCommand::new(first.id, harness.single_type.id),
        )
        .await
        .expect("delete succeeds");

    // After deletion, creating again is allowed
    let recreated = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(
                harness.single_type.id,
                HashMap::from([(
                    harness.title_attr.clone(),
                    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                        "Fresh Homepage".into(),
                    ))),
                )]),
            ),
        )
        .await
        .expect("recreation succeeds after deletion");
    assert_eq!(recreated.document_type_id, harness.single_type.id);
}

#[tokio::test]
async fn test_two_phase_batch_populate_workflow() {
    let harness = TestAppHarness::setup();

    // Create 3 articles
    let mut created_ids = Vec::new();
    for i in 1..=3 {
        let fields = HashMap::from([(
            harness.title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(format!(
                "Article #{i}"
            )))),
        )]);

        let doc = harness
            .documents_service
            .create(
                &harness.admin_context,
                CreateDocumentCommand::new(harness.article_type.id, fields),
            )
            .await
            .unwrap();
        created_ids.push(doc.id);
    }

    // Attach related tags in the relation store
    let tag_attr = AttributeId::try_new("tags").unwrap();
    let tag_type_id = DocumentTypeId::new(Uuid::now_v7());

    let tag1 = DocumentInstance::new(tag_type_id, None, Utc::now());
    let tag2 = DocumentInstance::new(tag_type_id, None, Utc::now());

    harness
        .instance_repo
        .add_relation_data(tag_attr.clone(), created_ids[0], vec![tag1.clone()]);
    harness.instance_repo.add_relation_data(
        tag_attr.clone(),
        created_ids[1],
        vec![tag1.clone(), tag2.clone()],
    );
    // created_ids[2] has no tags

    // Find with populate: ["tags"]
    let cmd = FindDocumentsCommand::new(harness.article_type.id, Pagination::default())
        .with_populate(vec![tag_attr.clone()]);
    let (items, count) = harness
        .documents_service
        .find(&harness.admin_context, cmd)
        .await
        .expect("find with populate succeeds");

    assert_eq!(count, 3);
    assert_eq!(items.len(), 3);

    // Verify relations populated correctly
    let doc0 = items.iter().find(|d| d.id == created_ids[0]).unwrap();
    assert_eq!(doc0.populated_relations.get(&tag_attr).unwrap().len(), 1);

    let doc1 = items.iter().find(|d| d.id == created_ids[1]).unwrap();
    assert_eq!(doc1.populated_relations.get(&tag_attr).unwrap().len(), 2);

    let doc2 = items.iter().find(|d| d.id == created_ids[2]).unwrap();
    assert!(!doc2.populated_relations.contains_key(&tag_attr));

    // Find without populate -> populated_relations remains empty
    let unpopulated_cmd =
        FindDocumentsCommand::new(harness.article_type.id, Pagination::default());
    let (plain_items, _) = harness
        .documents_service
        .find(&harness.admin_context, unpopulated_cmd)
        .await
        .expect("find unpopulated succeeds");

    for doc in plain_items {
        assert!(doc.populated_relations.is_empty());
    }
}

#[tokio::test]
async fn test_schema_and_locale_validation_workflow() {
    let harness = TestAppHarness::setup();

    // 1. Missing required field -> validation error with all violations collected
    let missing_required = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(harness.article_type.id, HashMap::new()),
        )
        .await;
    // Returns Validation(Vec<String>) since service now collects all errors
    assert!(
        matches!(&missing_required, Err(ApplicationError::Validation(msgs)) if
            msgs.iter().any(|m| m.contains("required") || m.contains("missing"))),
        "expected Validation error mentioning 'required', got: {:?}",
        missing_required
    );

    // 2. Unsupported locale ('es' not in en, uk) -> validation error
    let mut invalid_locale_fields = HashMap::new();
    invalid_locale_fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Valid".into()))),
    );
    let mut es_loc = HashMap::new();
    es_loc.insert(
        LocaleId::try_new("es").unwrap(),
        "Texto en espanol".into(),
    );
    invalid_locale_fields.insert(
        harness.body_attr.clone(),
        ContentValue::LocalizedText(es_loc),
    );

    let invalid_locale = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(harness.article_type.id, invalid_locale_fields),
        )
        .await;
    assert!(
        matches!(&invalid_locale, Err(ApplicationError::Validation(msgs)) if
            msgs.iter().any(|m| m.contains("locale") || m.contains("unknown"))),
        "expected Validation error mentioning unknown locale, got: {:?}",
        invalid_locale
    );

    // 3. Undeclared attribute -> validation error
    let fake_attr = AttributeId::try_new("hacker_field").unwrap();
    let mut undeclared_attr_fields = HashMap::new();
    undeclared_attr_fields.insert(
        harness.title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Valid".into()))),
    );
    undeclared_attr_fields.insert(
        fake_attr,
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Danger".into()))),
    );

    let undeclared = harness
        .documents_service
        .create(
            &harness.admin_context,
            CreateDocumentCommand::new(harness.article_type.id, undeclared_attr_fields),
        )
        .await;
    assert!(
        matches!(&undeclared, Err(ApplicationError::Validation(msgs)) if
            msgs.iter().any(|m| m.contains("unknown attribute") || m.contains("undeclared"))),
        "expected Validation error mentioning unknown attribute, got: {:?}",
        undeclared
    );

    // 4. Verify system_config_service reflects static configuration
    assert!(harness
        .system_config_service
        .is_locale_supported(&LocaleId::try_new("en").unwrap()));
    assert!(harness
        .system_config_service
        .is_locale_supported(&LocaleId::try_new("uk").unwrap()));
    assert!(!harness
        .system_config_service
        .is_locale_supported(&LocaleId::try_new("es").unwrap()));
}
