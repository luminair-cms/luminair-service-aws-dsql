use nutype::nutype;
use uuid::Uuid;

#[nutype(
    sanitize(trim),
    validate(
        not_empty,
        len_char_min = 2,
        len_char_max = 64,
        regex = r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$"
    ),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        Serialize,
        Deserialize,
        AsRef,
        Deref,
        Into
    )
)]
pub struct DocumentTypeId(String);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct DocumentInstanceId(Uuid);

#[nutype(
    sanitize(trim),
    validate(
        not_empty,
        len_char_min = 2,
        len_char_max = 64,
        regex = r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$"
    ),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        Serialize,
        Deserialize,
        AsRef,
        Deref,
        Into
    )
)]
pub struct AttributeId(String);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct RelationId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct SnapshotId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct RoleId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct UserRoleAssignmentId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct AccessRequestId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct SystemConfigId(Uuid);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_type_id_valid() {
        assert!(DocumentTypeId::try_new("article").is_ok());
        assert!(DocumentTypeId::try_new("partner-booking-category").is_ok());
        assert!(DocumentTypeId::try_new("site-settings").is_ok());
    }

    #[test]
    fn test_document_type_id_rejects_underscores_and_uppercase() {
        assert!(DocumentTypeId::try_new("partner_category").is_err());
        assert!(DocumentTypeId::try_new("Article").is_err());
        assert!(DocumentTypeId::try_new("partnerBookingCategory").is_err());
    }

    #[test]
    fn test_document_type_id_rejects_invalid_hyphenation() {
        assert!(DocumentTypeId::try_new("-article").is_err());
        assert!(DocumentTypeId::try_new("article-").is_err());
        assert!(DocumentTypeId::try_new("article--content").is_err());
    }

    #[test]
    fn test_attribute_id_valid_kebab_case() {
        assert!(AttributeId::try_new("title").is_ok());
        assert!(AttributeId::try_new("body-text").is_ok());
        assert!(AttributeId::try_new("field-1").is_ok());
    }

    #[test]
    fn test_attribute_id_rejects_underscores_and_uppercase() {
        assert!(AttributeId::try_new("body_text").is_err());
        assert!(AttributeId::try_new("Title").is_err());
        assert!(AttributeId::try_new("myField").is_err());
    }

    #[test]
    fn test_attribute_id_starts_with_digit_or_hyphen() {
        assert!(AttributeId::try_new("1title").is_err());
        assert!(AttributeId::try_new("-private").is_err());
        assert!(AttributeId::try_new("_private").is_err());
    }
}
