//! System configuration application service.

use std::sync::Arc;

use domain::entities::system_config::SystemConfig;
use domain::value_objects::LocaleId;

/// Port trait defining operations for inspecting system configuration.
pub trait SystemConfigService: Send + Sync + 'static {
    /// Returns the static system configuration loaded at startup.
    fn get_config(&self) -> &SystemConfig;

    /// Checks if a specific locale is supported by the system.
    fn is_locale_supported(&self, locale: &LocaleId) -> bool {
        self.get_config().contains_locale(locale)
    }

    /// Returns the system default fallback locale.
    fn default_locale(&self) -> &LocaleId {
        &self.get_config().default_locale
    }

    /// Returns the slice of all supported locales.
    fn available_locales(&self) -> &[LocaleId] {
        &self.get_config().available_locales
    }
}

/// In-memory implementation of `SystemConfigService` wrapping the startup configuration.
#[derive(Debug, Clone)]
pub struct SystemConfigServiceImpl {
    config: Arc<SystemConfig>,
}

impl SystemConfigServiceImpl {
    /// Creates a new `SystemConfigServiceImpl` wrapping the loaded configuration.
    pub fn new(config: Arc<SystemConfig>) -> Self {
        Self { config }
    }
}

impl SystemConfigService for SystemConfigServiceImpl {
    fn get_config(&self) -> &SystemConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::value_objects::SystemConfigId;
    use uuid::Uuid;

    fn make_test_config() -> Arc<SystemConfig> {
        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        Arc::new(
            SystemConfig::new(
                SystemConfigId::new(Uuid::now_v7()),
                vec![en.clone(), uk],
                en,
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_get_config_returns_static_configuration() {
        let config = make_test_config();
        let service = SystemConfigServiceImpl::new(config.clone());

        assert_eq!(service.get_config().id, config.id);
        assert_eq!(service.get_config().default_locale, config.default_locale);
    }

    #[test]
    fn test_is_locale_supported() {
        let config = make_test_config();
        let service = SystemConfigServiceImpl::new(config);

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let fr = LocaleId::try_new("fr").unwrap();

        assert!(service.is_locale_supported(&en));
        assert!(service.is_locale_supported(&uk));
        assert!(!service.is_locale_supported(&fr));
    }

    #[test]
    fn test_default_and_available_locales() {
        let config = make_test_config();
        let service = SystemConfigServiceImpl::new(config);

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();

        assert_eq!(service.default_locale(), &en);
        assert_eq!(service.available_locales(), &[en, uk]);
    }
}
