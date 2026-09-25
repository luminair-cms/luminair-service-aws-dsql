use email_address::EmailAddress;
use nutype::nutype;
use std::str::FromStr;

fn is_valid_email(s: &str) -> bool {
    EmailAddress::from_str(s).is_ok()
}

#[nutype(
    sanitize(trim, lowercase),
    validate(predicate = is_valid_email),
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
pub struct Email(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_email() {
        let email = Email::try_new("User@Example.COM").unwrap();
        assert_eq!(email.as_ref(), "user@example.com");
    }

    #[test]
    fn test_invalid_email() {
        assert!(Email::try_new("not-an-email").is_err());
        assert!(Email::try_new("user@").is_err());
        assert!(Email::try_new("@example.com").is_err());
        assert!(Email::try_new("").is_err());
    }
}
