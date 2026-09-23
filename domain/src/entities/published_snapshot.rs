use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::content_value::ContentValue;
use crate::value_objects::{AttributeId, DocumentInstanceId, SnapshotId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedSnapshot {
    pub id: SnapshotId,
    pub instance_id: DocumentInstanceId,
    pub type_name: String,
    pub revision: u32,
    pub published_at: DateTime<Utc>,
    pub published_by: Option<UserId>,
    pub fields: HashMap<AttributeId, ContentValue>,
}
