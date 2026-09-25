use nutype::nutype;
use std::str::FromStr;
use url::Url as ExternalUrl;

fn is_valid_url(s: &str) -> bool {
    ExternalUrl::from_str(s).is_ok()
}

#[nutype(
    sanitize(trim),
    validate(predicate = is_valid_url),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        AsRef,
        Deref,
        Hash,
        Display,
        FromStr,
        Into,
        Serialize,
        Deserialize
    )
)]
pub struct Url(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_url() {
        let url = Url::try_new("  https://example.com/path?query=1  ").unwrap();
        assert_eq!(url.as_ref(), "https://example.com/path?query=1");
    }

    #[test]
    fn test_invalid_url() {
        assert!(Url::try_new("not a url").is_err());
        assert!(Url::try_new("http://").is_err());
        assert!(Url::try_new("").is_err());
    }
}
