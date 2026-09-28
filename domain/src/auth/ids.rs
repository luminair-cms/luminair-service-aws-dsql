use nutype::nutype;
use uuid::Uuid;

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 255),
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
pub struct UserId(String);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_trims_whitespace() {
        let user_id = UserId::try_new("  sub123  ").expect("valid user id");
        assert_eq!(user_id.as_ref(), "sub123");
    }

    #[test]
    fn test_user_id_empty_rejected() {
        assert!(UserId::try_new("").is_err());
        assert!(UserId::try_new("   ").is_err());
    }
}
