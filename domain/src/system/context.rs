//! SystemContext combines declarative schema definitions and system-level configuration.

use crate::schema::{DocumentType, DocumentTypeId, SchemaRegistry};
use crate::system::{LocaleId, SystemConfig};

/// Consolidated immutable system context combining schema registry and system configuration.
#[derive(Debug, Clone)]
pub struct SystemContext {
    pub schema: SchemaRegistry,
    pub config: SystemConfig,
}

impl SystemContext {
    /// Creates a new `SystemContext` from a loaded schema registry and system configuration.
    pub fn new(schema: SchemaRegistry, config: SystemConfig) -> Self {
        Self { schema, config }
    }

    /// Looks up a document type by its unique identifier.
    pub fn find_type(&self, id: &DocumentTypeId) -> Option<&DocumentType> {
        self.schema.find_type(id)
    }

    /// Looks up a document type by its collection name (plural) or single-type name (singular).
    pub fn find_type_by_name(&self, name: &str) -> Option<&DocumentType> {
        self.schema.find_type_by_name(name)
    }

    /// Returns the system default locale.
    pub fn default_locale(&self) -> &LocaleId {
        &self.config.default_locale
    }

    /// Checks if a locale is supported by the system.
    pub fn contains_locale(&self, locale: &LocaleId) -> bool {
        self.config.contains_locale(locale)
    }
}
