//! In-memory thread-safe fake repositories for application testing.

use std::collections::HashMap;
use std::sync::RwLock;

use domain::auth::{
    AccessRequest, AccessRequestId, AccessRequestRepository, AccessRequestStatus, Role, RoleId,
    RoleRepository, UserId, UserRoleAssignment, UserRoleAssignmentId, UserRoleAssignmentRepository,
};
use domain::content::{
    DocumentInstance, DocumentInstanceId, DocumentInstanceRepository, FieldFilter, Page,
    Pagination, RelationMap,
};
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentTypeId};

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
        _type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<Option<DocumentInstance>, DomainError> {
        let store = self
            .instances
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_type(
        &self,
        type_id: &'static DocumentTypeId,
        pagination: Pagination,
        _filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError> {
        let store = self
            .instances
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;

        let filtered: Vec<DocumentInstance> = store
            .values()
            .filter(|inst| inst.document_type_id == *type_id)
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
        type_id: &'static DocumentTypeId,
        _filters: Vec<FieldFilter>,
    ) -> Result<u64, DomainError> {
        let store = self
            .instances
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        let count = store
            .values()
            .filter(|inst| inst.document_type_id == *type_id)
            .count() as u64;
        Ok(count)
    }

    async fn fetch_relations(
        &self,
        _type_id: &'static DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> Result<RelationMap, DomainError> {
        let store = self
            .relations
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;

        let mut result: RelationMap = HashMap::new();
        for attr in attributes {
            if let Some(by_parent) = store.get(attr) {
                let mut filtered_by_parent = HashMap::new();
                for pid in parent_ids {
                    if let Some(instances) = by_parent.get(pid) {
                        filtered_by_parent.insert(*pid, instances.clone());
                    }
                }
                result.insert(attr.clone(), filtered_by_parent);
            }
        }
        Ok(result)
    }

    async fn save(&self, instance: &DocumentInstance) -> Result<(), DomainError> {
        let mut store = self
            .instances
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        store.insert(instance.id, instance.clone());

        let mut rel_store = self
            .relations
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;

        for by_parent in rel_store.values_mut() {
            by_parent.remove(&instance.id);
        }

        for (attr, resolved_list) in &instance.relations {
            let by_parent = rel_store.entry(attr.clone()).or_default();
            let related_instances: Vec<DocumentInstance> = resolved_list
                .iter()
                .filter_map(|r| store.get(&r.target_instance_id).cloned())
                .collect();
            by_parent.insert(instance.id, related_instances);
        }

        Ok(())
    }

    async fn delete(
        &self,
        _type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<(), DomainError> {
        let mut store = self
            .instances
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        store.remove(&id);

        let mut rel_store = self
            .relations
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        for by_parent in rel_store.values_mut() {
            by_parent.remove(&id);
        }

        Ok(())
    }

    async fn exists_for_type(&self, type_id: &'static DocumentTypeId) -> Result<bool, DomainError> {
        let store = self
            .instances
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.values().any(|inst| inst.document_type_id == *type_id))
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
        let store = self
            .roles
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_ids(&self, ids: &[RoleId]) -> Result<Vec<Role>, DomainError> {
        let store = self
            .roles
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(ids.iter().filter_map(|id| store.get(id).cloned()).collect())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        let store = self
            .roles
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.values().find(|r| r.name == name).cloned())
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        let store = self
            .roles
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.values().cloned().collect())
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        let mut store = self
            .roles
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
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
        let store = self
            .assignments
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store
            .iter()
            .filter(|a| a.user_id == *user_id)
            .cloned()
            .collect())
    }

    async fn exists_admin(&self, admin_role_id: RoleId) -> Result<bool, DomainError> {
        let store = self
            .assignments
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.iter().any(|a| a.role_id == admin_role_id))
    }

    async fn save(&self, assignment: &UserRoleAssignment) -> Result<(), DomainError> {
        let mut store = self
            .assignments
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        store.retain(|a| !(a.user_id == assignment.user_id && a.role_id == assignment.role_id));
        store.push(assignment.clone());
        Ok(())
    }

    async fn save_all(&self, assignments: &[UserRoleAssignment]) -> Result<(), DomainError> {
        let mut store = self
            .assignments
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        for assignment in assignments {
            store.retain(|a| !(a.user_id == assignment.user_id && a.role_id == assignment.role_id));
            store.push(assignment.clone());
        }
        Ok(())
    }

    async fn delete(&self, id: UserRoleAssignmentId) -> Result<(), DomainError> {
        let mut store = self
            .assignments
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
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
        let store = self
            .requests
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Option<AccessRequest>, DomainError> {
        let store = self
            .requests
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store
            .values()
            .filter(|r| r.user_id == *user_id)
            .max_by_key(|r| r.requested_at)
            .cloned())
    }

    async fn find_active_by_user(
        &self,
        user_id: &UserId,
    ) -> Result<Option<AccessRequest>, DomainError> {
        let store = self
            .requests
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store
            .values()
            .find(|r| r.user_id == *user_id && r.is_active())
            .cloned())
    }

    async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError> {
        let store = self
            .requests
            .read()
            .map_err(|_| DomainError::Unauthorized("failed to acquire read lock".into()))?;
        Ok(store
            .values()
            .filter(|r| matches!(r.status, AccessRequestStatus::Pending))
            .cloned()
            .collect())
    }

    async fn save(&self, request: &AccessRequest) -> Result<(), DomainError> {
        let mut store = self
            .requests
            .write()
            .map_err(|_| DomainError::Unauthorized("failed to acquire write lock".into()))?;
        store.insert(request.id, request.clone());
        Ok(())
    }
}
