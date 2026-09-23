use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::value_objects::{LocaleId, SystemConfigId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemConfig {
    pub id: SystemConfigId,
    pub available_locales: Vec<LocaleId>,
    pub default_locale: LocaleId,
}

impl SystemConfig {
    pub fn new(
        id: SystemConfigId,
        available_locales: Vec<LocaleId>,
        default_locale: LocaleId,
    ) -> Result<Self, DomainError> {
        if !available_locales.contains(&default_locale) {
            return Err(DomainError::UnknownLocale(default_locale));
        }
        Ok(Self {
            id,
            available_locales,
            default_locale,
        })
    }

    pub fn contains_locale(&self, locale: &LocaleId) -> bool {
        self.available_locales.contains(locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_test_locales() -> (SystemConfigId, LocaleId, LocaleId, LocaleId) {
        (
            SystemConfigId::new(Uuid::now_v7()),
            LocaleId::try_new("en").unwrap(),
            LocaleId::try_new("uk").unwrap(),
            LocaleId::try_new("fr").unwrap(),
        )
    }

    #[test]
    fn test_contains_locale_found() {
        let (id, en, uk, _) = make_test_locales();
        let config = SystemConfig::new(id, vec![en.clone(), uk.clone()], en).unwrap();
        assert!(config.contains_locale(&uk));
    }

    #[test]
    fn test_contains_locale_not_found() {
        let (id, en, uk, fr) = make_test_locales();
        let config = SystemConfig::new(id, vec![en, uk], LocaleId::try_new("en").unwrap()).unwrap();
        assert!(!config.contains_locale(&fr));
    }

    #[test]
    fn test_contains_locale_default_included() {
        let (id, en, uk, fr) = make_test_locales();
        // default_locale 'fr' is not in [en, uk] -> error
        let res = SystemConfig::new(id, vec![en.clone(), uk], fr.clone());
        assert!(matches!(res, Err(DomainError::UnknownLocale(loc)) if loc == fr));

        // valid when default_locale is in available_locales
        let valid = SystemConfig::new(id, vec![en.clone()], en.clone()).unwrap();
        assert!(valid.contains_locale(&en));
    }
}
