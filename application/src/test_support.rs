//! In-memory thread-safe fake repositories for application testing.

use std::collections::HashMap;
use std::sync::RwLock;

use domain::entities::access_request::AccessRequest;
use domain::entities::document_instance::DocumentInstance;
use domain::entities::published_snapshot::PublishedSnapshot;
use domain::entities::role::Role;
use domain::entities::user_role_assignment::UserRoleAssignment;
use domain::errors::DomainError;
use domain::ports::document_instance_repository::{
    FieldFilter, Page, Pagination, RelationMap,
};
use domain::ports::{
    AccessRequestRepository, DocumentInstanceRepository, RoleRepository, SnapshotRepository,
    UserRoleAssignmentRepository,
};
use domain::value_objects::{
    AccessRequestId, AttributeId, DocumentInstanceId, DocumentTypeId, RoleId, UserId,
    UserRoleAssignmentId,
};

/// Thread-safe in-memory fake for `DocumentInstanceRepository`.
#[derive(Debug, Default)]
pub struct FakeDocumentInstanceRepository {
    pub instances: RwLock<HashMap<DocumentInstanceId, DocumentInstance>>,
    pub relations: RwLock<RelationMap>,
}

impl FakeDocumentInstanceRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instance(self, instance: DocumentInstance) -> Self {
        self.instances
            .write()
            .expect("lock write")
            .insert(instance.id, instance);
        self
    }

    pub fn add_relation_data(
        &self,
        attr: AttributeId,
        parent_id: DocumentInstanceId,
        related: Vec<DocumentInstance>,
    ) {
        let mut map = self.relations.write().expect("lock write");
        let parent_map = map.entry(attr).or_default();
        parent_map.insert(parent_id, related);
    }
}

impl DocumentInstanceRepository for FakeDocumentInstanceRepository {
    async fn find_by_id(
        &self,
        _type_id: DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<Option<DocumentInstance>, DomainError> {
        let store = self.instances.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_type(
        &self,
        type_id: DocumentTypeId,
        pagination: Pagination,
        _filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError> {
        let store = self.instances.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;

        let filtered: Vec<DocumentInstance> = store
            .values()
            .filter(|inst| inst.document_type_id == type_id)
            .cloned()
            .collect();

        let total = filtered.len() as u64;
        let start = ((pagination.page.saturating_sub(1)) * pagination.page_size) as usize;
        let items: Vec<DocumentInstance> = filtered
            .into_iter()
            .skip(start)
            .take(pagination.page_size as usize)
            .collect();

        Ok(Page {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    async fn count(
        &self,
        type_id: DocumentTypeId,
        _filters: Vec<FieldFilter>,
    ) -> Result<u64, DomainError> {
        let store = self.instances.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        let count = store
            .values()
            .filter(|inst| inst.document_type_id == type_id)
            .count() as u64;
        Ok(count)
    }

    async fn fetch_relations(
        &self,
        _type_id: DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> Result<RelationMap, DomainError> {
        let relations_store = self.relations.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;

        let mut result = RelationMap::new();
        for attr in attributes {
            if let Some(by_parent) = relations_store.get(attr) {
                let mut per_attr_map = HashMap::new();
                for pid in parent_ids {
                    if let Some(items) = by_parent.get(pid) {
                        per_attr_map.insert(*pid, items.clone());
                    }
                }
                result.insert(attr.clone(), per_attr_map);
            }
        }

        Ok(result)
    }

    async fn save(&self, instance: &DocumentInstance) -> Result<(), DomainError> {
        let mut store = self.instances.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.insert(instance.id, instance.clone());
        Ok(())
    }

    async fn delete(&self, _type_id: DocumentTypeId, id: DocumentInstanceId) -> Result<(), DomainError> {
        let mut store = self.instances.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.remove(&id);
        Ok(())
    }

    async fn exists_for_type(&self, type_id: DocumentTypeId) -> Result<bool, DomainError> {
        let store = self.instances.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.values().any(|inst| inst.document_type_id == type_id))
    }
}

/// Thread-safe in-memory fake for `SnapshotRepository`.
#[derive(Debug, Default)]
pub struct FakeSnapshotRepository {
    pub snapshots: RwLock<Vec<PublishedSnapshot>>,
}

impl FakeSnapshotRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SnapshotRepository for FakeSnapshotRepository {
    async fn find_by_instance(
        &self,
        instance_id: DocumentInstanceId,
    ) -> Result<Vec<PublishedSnapshot>, DomainError> {
        let store = self.snapshots.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store
            .iter()
            .filter(|s| s.instance_id == instance_id)
            .cloned()
            .collect())
    }

    async fn find_by_revision(
        &self,
        instance_id: DocumentInstanceId,
        revision: u32,
    ) -> Result<Option<PublishedSnapshot>, DomainError> {
        let store = self.snapshots.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store
            .iter()
            .find(|s| s.instance_id == instance_id && s.revision == revision)
            .cloned())
    }

    async fn save(&self, snapshot: &PublishedSnapshot) -> Result<(), DomainError> {
        let mut store = self.snapshots.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.push(snapshot.clone());
        Ok(())
    }

    async fn delete_by_instance(&self, instance_id: DocumentInstanceId) -> Result<(), DomainError> {
        let mut store = self.snapshots.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.retain(|s| s.instance_id != instance_id);
        Ok(())
    }
}

/// Thread-safe in-memory fake for `RoleRepository`.
#[derive(Debug, Default)]
pub struct FakeRoleRepository {
    pub roles: RwLock<HashMap<RoleId, Role>>,
}

impl FakeRoleRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_role(self, role: Role) -> Self {
        self.roles
            .write()
            .expect("lock write")
            .insert(role.id, role);
        self
    }
}

impl RoleRepository for FakeRoleRepository {
    async fn find_by_id(&self, id: RoleId) -> Result<Option<Role>, DomainError> {
        let store = self.roles.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        let store = self.roles.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.values().find(|r| r.name == name).cloned())
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        let store = self.roles.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.values().cloned().collect())
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        let mut store = self.roles.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.insert(role.id, role.clone());
        Ok(())
    }
}

/// Thread-safe in-memory fake for `UserRoleAssignmentRepository`.
#[derive(Debug, Default)]
pub struct FakeUserRoleAssignmentRepository {
    pub assignments: RwLock<Vec<UserRoleAssignment>>,
}

impl FakeUserRoleAssignmentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserRoleAssignmentRepository for FakeUserRoleAssignmentRepository {
    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<UserRoleAssignment>, DomainError> {
        let store = self.assignments.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store
            .iter()
            .filter(|a| a.user_id == *user_id)
            .cloned()
            .collect())
    }

    async fn exists_admin(&self, admin_role_id: RoleId) -> Result<bool, DomainError> {
        let store = self.assignments.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.iter().any(|a| a.role_id == admin_role_id))
    }

    async fn save(&self, assignment: &UserRoleAssignment) -> Result<(), DomainError> {
        let mut store = self.assignments.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.push(assignment.clone());
        Ok(())
    }

    async fn delete(&self, id: UserRoleAssignmentId) -> Result<(), DomainError> {
        let mut store = self.assignments.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.retain(|a| a.id != id);
        Ok(())
    }
}

/// Thread-safe in-memory fake for `AccessRequestRepository`.
#[derive(Debug, Default)]
pub struct FakeAccessRequestRepository {
    pub requests: RwLock<HashMap<AccessRequestId, AccessRequest>>,
}

impl FakeAccessRequestRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AccessRequestRepository for FakeAccessRequestRepository {
    async fn find_by_id(&self, id: AccessRequestId) -> Result<Option<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Option<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store
            .values()
            .find(|r| r.user_id == *user_id && r.is_active())
            .cloned())
    }

    async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(|_| {
            DomainError::Unauthorized("failed to acquire read lock".into())
        })?;
        Ok(store
            .values()
            .filter(|r| matches!(r.status, domain::entities::access_request::AccessRequestStatus::Pending))
            .cloned()
            .collect())
    }

    async fn save(&self, request: &AccessRequest) -> Result<(), DomainError> {
        let mut store = self.requests.write().map_err(|_| {
            DomainError::Unauthorized("failed to acquire write lock".into())
        })?;
        store.insert(request.id, request.clone());
        Ok(())
    }
}
