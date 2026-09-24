//! Commands for system configuration operations.

use domain::value_objects::LocaleId;

/// Command to update the system locales configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateLocalesCommand {
    /// List of all supported locales.
    pub available_locales: Vec<LocaleId>,
    /// The default fallback locale (must be included in `available_locales`).
    pub default_locale: LocaleId,
}

impl UpdateLocalesCommand {
    pub fn new(available_locales: Vec<LocaleId>, default_locale: LocaleId) -> Self {
        Self {
            available_locales,
            default_locale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_locales_command() {
        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();

        let cmd = UpdateLocalesCommand::new(vec![en.clone(), uk.clone()], en.clone());
        assert_eq!(cmd.available_locales, vec![en.clone(), uk]);
        assert_eq!(cmd.default_locale, en);
    }
}
