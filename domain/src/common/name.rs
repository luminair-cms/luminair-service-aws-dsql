use nutype::nutype;

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
pub struct DisplayName(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_name_trims_whitespace() {
        let name = DisplayName::try_new("  John Doe  ").expect("valid display name");
        assert_eq!(name.as_ref(), "John Doe");
    }

    #[test]
    fn test_display_name_empty_rejected() {
        assert!(DisplayName::try_new("").is_err());
        assert!(DisplayName::try_new("   ").is_err());
    }

    #[test]
    fn test_display_name_too_long_rejected() {
        let long = "a".repeat(256);
        assert!(DisplayName::try_new(&long).is_err());
    }
}
