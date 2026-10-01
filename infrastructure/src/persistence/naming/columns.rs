//! Typed standard column identifiers implementing `sea_query::Iden`.

use sea_query::Iden;

/// Standard columns present in all working draft document tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseSystemColumn {
    Id,
    Version,
    OwnerId,
    PublicationState,
    CreatedAt,
    UpdatedAt,
    Singleton,
}

impl BaseSystemColumn {
    pub const ALL: [Self; 7] = [
        Self::Id,
        Self::Version,
        Self::OwnerId,
        Self::PublicationState,
        Self::CreatedAt,
        Self::UpdatedAt,
        Self::Singleton,
    ];

    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Version => "version",
            Self::OwnerId => "owner_id",
            Self::PublicationState => "publication_state",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::Singleton => "_singleton",
        }
    }
}

impl Iden for BaseSystemColumn {
    #[inline]
    fn unquoted(&self) -> &str {
        self.as_str()
    }
}

/// Standard columns present in all published mirror document tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PublishedSystemColumn {
    Id,
    PublishedVersion,
    OwnerId,
    CreatedAt,
    UpdatedAt,
    PublishedAt,
    PublishedBy,
    Singleton,
}

impl PublishedSystemColumn {
    pub const ALL: [Self; 8] = [
        Self::Id,
        Self::PublishedVersion,
        Self::OwnerId,
        Self::CreatedAt,
        Self::UpdatedAt,
        Self::PublishedAt,
        Self::PublishedBy,
        Self::Singleton,
    ];

    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::PublishedVersion => "published_version",
            Self::OwnerId => "owner_id",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::PublishedAt => "published_at",
            Self::PublishedBy => "published_by",
            Self::Singleton => "_singleton",
        }
    }
}

impl Iden for PublishedSystemColumn {
    #[inline]
    fn unquoted(&self) -> &str {
        self.as_str()
    }
}

/// Standard columns for relational link / junction tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkColumn {
    OwnerId,
    TargetId,
}

impl LinkColumn {
    pub const ALL: [Self; 2] = [Self::OwnerId, Self::TargetId];

    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OwnerId => "owner_id",
            Self::TargetId => "target_id",
        }
    }
}

impl Iden for LinkColumn {
    #[inline]
    fn unquoted(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_system_column_iden() {
        assert_eq!(BaseSystemColumn::Id.as_str(), "id");
        assert_eq!(BaseSystemColumn::Id.unquoted(), "id");
        assert_eq!(BaseSystemColumn::Version.as_str(), "version");
        assert_eq!(BaseSystemColumn::OwnerId.as_str(), "owner_id");
        assert_eq!(
            BaseSystemColumn::PublicationState.as_str(),
            "publication_state"
        );
        assert_eq!(BaseSystemColumn::CreatedAt.as_str(), "created_at");
        assert_eq!(BaseSystemColumn::UpdatedAt.as_str(), "updated_at");
        assert_eq!(BaseSystemColumn::Singleton.as_str(), "_singleton");
    }

    #[test]
    fn test_published_system_column_iden() {
        assert_eq!(
            PublishedSystemColumn::PublishedVersion.as_str(),
            "published_version"
        );
        assert_eq!(
            PublishedSystemColumn::PublishedAt.as_str(),
            "published_at"
        );
        assert_eq!(
            PublishedSystemColumn::PublishedBy.as_str(),
            "published_by"
        );
    }

    #[test]
    fn test_link_column_iden() {
        assert_eq!(LinkColumn::OwnerId.as_str(), "owner_id");
        assert_eq!(LinkColumn::TargetId.as_str(), "target_id");
    }
}
