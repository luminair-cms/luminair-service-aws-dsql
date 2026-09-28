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

#[nutype(
    sanitize(trim),
    validate(
        not_empty,
        len_char_min = 2,
        len_char_max = 128,
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
pub struct RelationId(String);

impl RelationId {
    /// Derives a deterministic `RelationId` from the owner type and owner attribute,
    /// e.g. `article` and `author` -> `article-author`.
    pub fn derive(owner_type: &DocumentTypeId, owner_attr: &AttributeId) -> Self {
        let name = format!("{}-{}", owner_type.as_ref(), owner_attr.as_ref());
        Self::try_new(name)
            .expect("derived relation id from valid kebab-case identifiers is valid kebab-case")
    }
}

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

    #[test]
    fn test_relation_id_valid_kebab_case() {
        assert!(RelationId::try_new("article-author").is_ok());
        assert!(RelationId::try_new("articles-to-tags").is_ok());
        assert!(RelationId::try_new("rel-1").is_ok());
    }

    #[test]
    fn test_relation_id_rejects_invalid() {
        assert!(RelationId::try_new("article_author").is_err());
        assert!(RelationId::try_new("ArticleAuthor").is_err());
        assert!(RelationId::try_new("-rel").is_err());
        assert!(RelationId::try_new("rel-").is_err());
        assert!(RelationId::try_new("rel--item").is_err());
    }

    #[test]
    fn test_relation_id_derive() {
        let owner = DocumentTypeId::try_new("article").unwrap();
        let attr = AttributeId::try_new("author").unwrap();
        let rel_id = RelationId::derive(&owner, &attr);
        assert_eq!(rel_id.as_ref(), "article-author");
    }
}
