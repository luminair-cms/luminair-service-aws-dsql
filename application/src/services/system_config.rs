//! System configuration application service.

use domain::system::{LocaleId, SystemConfig, SystemContext};

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
#[derive(Debug, Clone, Copy)]
pub struct SystemConfigServiceImpl {
    context: &'static SystemContext,
}

impl SystemConfigServiceImpl {
    /// Creates a new `SystemConfigServiceImpl` wrapping the loaded configuration.
    pub fn new(context: &'static SystemContext) -> Self {
        Self { context }
    }
}

impl SystemConfigService for SystemConfigServiceImpl {
    fn get_config(&self) -> &SystemConfig {
        &self.context.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::SchemaRegistry;
    use domain::system::SystemConfigId;
    use uuid::Uuid;

    fn make_test_context() -> &'static SystemContext {
        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone(), uk],
            en,
        )
        .unwrap();
        let schema = SchemaRegistry::default();
        Box::leak(Box::new(SystemContext::new(schema, config)))
    }

    #[test]
    fn test_get_config_returns_static_configuration() {
        let context = make_test_context();
        let service = SystemConfigServiceImpl::new(context);

        assert_eq!(service.get_config().id, context.config.id);
        assert_eq!(
            service.get_config().default_locale,
            context.config.default_locale
        );
    }

    #[test]
    fn test_is_locale_supported() {
        let context = make_test_context();
        let service = SystemConfigServiceImpl::new(context);

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let fr = LocaleId::try_new("fr").unwrap();

        assert!(service.is_locale_supported(&en));
        assert!(service.is_locale_supported(&uk));
        assert!(!service.is_locale_supported(&fr));
    }

    #[test]
    fn test_default_and_available_locales() {
        let context = make_test_context();
        let service = SystemConfigServiceImpl::new(context);

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();

        assert_eq!(service.default_locale(), &en);
        assert_eq!(service.available_locales(), &[en, uk]);
    }
}
