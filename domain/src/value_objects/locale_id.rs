use nutype::nutype;

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 16, regex = r"^[a-z]{2,3}(-[A-Z]{2})?$"),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, Serialize, Deserialize, AsRef, Deref, Into)
)]
pub struct LocaleId(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_id_valid_bcp47() {
        assert!(LocaleId::try_new("en").is_ok());
        assert!(LocaleId::try_new("uk").is_ok());
        assert!(LocaleId::try_new("en-US").is_ok());
        assert!(LocaleId::try_new("uk-UA").is_ok());
    }

    #[test]
    fn test_locale_id_invalid_format() {
        assert!(LocaleId::try_new("EN").is_err());
        assert!(LocaleId::try_new("english").is_err());
        assert!(LocaleId::try_new("en-us").is_err());
        assert!(LocaleId::try_new("").is_err());
    }
}
