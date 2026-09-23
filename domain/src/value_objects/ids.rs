use nutype::nutype;
use uuid::Uuid;

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct DocumentTypeId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct DocumentInstanceId(Uuid);

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 64, regex = r"^[a-z][a-z0-9_]*$"),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct AttributeId(String);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct RelationId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct SnapshotId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct RoleId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct UserRoleAssignmentId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct AccessRequestId(Uuid);

#[nutype(
    derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct SystemConfigId(Uuid);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_id_valid_slug() {
        assert!(AttributeId::try_new("title").is_ok());
        assert!(AttributeId::try_new("body_text").is_ok());
        assert!(AttributeId::try_new("field_1").is_ok());
    }

    #[test]
    fn test_attribute_id_invalid_uppercase() {
        assert!(AttributeId::try_new("Title").is_err());
        assert!(AttributeId::try_new("myField").is_err());
    }

    #[test]
    fn test_attribute_id_starts_with_digit() {
        assert!(AttributeId::try_new("1title").is_err());
        assert!(AttributeId::try_new("_private").is_err());
    }
}
