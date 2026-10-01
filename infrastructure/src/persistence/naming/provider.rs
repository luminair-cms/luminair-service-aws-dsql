//! Table, column, index, and constraint naming providers.

use domain::schema::{AttributeId, DocumentKind, DocumentType};
use sea_query::Alias;

use super::sanitize::kebab_to_snake;

/// Naming provider for a document type's base table, published mirror table,
/// attribute columns, and relational link tables.
#[derive(Debug, Clone)]
pub struct DocumentTableNaming<'a> {
    pub doc_type: Option<&'a DocumentType>,
    table_name: String,
    draft_and_publish: bool,
}

impl<'a> DocumentTableNaming<'a> {
    /// Creates a provider from a domain `DocumentType`.
    pub fn new(doc_type: &'a DocumentType) -> Self {
        let raw_name = match doc_type.kind {
            DocumentKind::Collection => &doc_type.info.plural_name,
            DocumentKind::SingleType => &doc_type.info.singular_name,
        };
        let table_name = kebab_to_snake(raw_name);
        Self {
            doc_type: Some(doc_type),
            table_name,
            draft_and_publish: doc_type.options.draft_and_publish,
        }
    }

    /// Creates a provider directly from table metadata without requiring a full `DocumentType`.
    pub fn from_parts(
        kind: DocumentKind,
        singular_name: &str,
        plural_name: &str,
        draft_and_publish: bool,
    ) -> Self {
        let raw_name = match kind {
            DocumentKind::Collection => plural_name,
            DocumentKind::SingleType => singular_name,
        };
        let table_name = kebab_to_snake(raw_name);
        Self {
            doc_type: None,
            table_name,
            draft_and_publish,
        }
    }

    /// Physical database table name for working draft instances (e.g. `articles`, `site_setting`).
    #[inline]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// SeaQuery table identifier for the working draft table.
    #[inline]
    pub fn table_iden(&self) -> Alias {
        Alias::new(&self.table_name)
    }

    /// Returns `true` if draft and publish is enabled for this document type.
    #[inline]
    pub fn is_draft_and_publish(&self) -> bool {
        self.draft_and_publish
    }

    /// Physical database table name for the published mirror table.
    /// E.g. `articles` -> `articles__published`. Returns `None` if draft-and-publish is false.
    pub fn published_table_name(&self) -> Option<String> {
        if self.draft_and_publish {
            Some(format!("{}__published", self.table_name))
        } else {
            None
        }
    }

    /// SeaQuery table identifier for the published mirror table.
    pub fn published_table_iden(&self) -> Option<Alias> {
        self.published_table_name().map(Alias::new)
    }

    /// Derives the column name for an attribute.
    #[inline]
    pub fn column_name(&self, attr: &AttributeId) -> String {
        kebab_to_snake(attr.as_ref())
    }

    /// SeaQuery column identifier for an attribute.
    #[inline]
    pub fn column_iden(&self, attr: &AttributeId) -> Alias {
        Alias::new(self.column_name(attr))
    }

    /// Derives foreign key column name for an attribute (e.g., `author` -> `author_id`).
    #[inline]
    pub fn foreign_key_column_name(&self, attr: &AttributeId) -> String {
        format!("{}_id", kebab_to_snake(attr.as_ref()))
    }

    /// SeaQuery identifier for foreign key column.
    #[inline]
    pub fn foreign_key_column_iden(&self, attr: &AttributeId) -> Alias {
        Alias::new(self.foreign_key_column_name(attr))
    }

    /// Creates a naming provider for a relational link table owned by this document type.
    #[inline]
    pub fn link_table(&self, attr: &AttributeId) -> LinkTableNaming {
        LinkTableNaming::new(&self.table_name, attr)
    }

    /// Derives a standard B-tree index name for a column on this table.
    #[inline]
    pub fn index_name(&self, column: &str) -> String {
        format!("idx_{}_{column}", self.table_name)
    }

    /// Foreign key constraint name referencing the base table from the published table.
    pub fn published_fk_name(&self) -> Option<String> {
        self.published_table_name()
            .map(|pub_name| format!("fk_{pub_name}_id"))
    }
}

/// Naming provider for relational universal link / junction tables.
#[derive(Debug, Clone)]
pub struct LinkTableNaming {
    table_name: String,
}

impl LinkTableNaming {
    /// Creates a link table provider for an owner table and owner attribute.
    pub fn new(owner_table: &str, owner_attr: &AttributeId) -> Self {
        let attr_snake = kebab_to_snake(owner_attr.as_ref());
        Self {
            table_name: format!("{owner_table}__{attr_snake}_link"),
        }
    }

    /// Creates a link table provider from an already derived link table name.
    pub fn from_table_name(table_name: impl Into<String>) -> Self {
        Self {
            table_name: table_name.into(),
        }
    }

    /// Physical draft link table name (e.g. `articles__tags_link`).
    #[inline]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// SeaQuery identifier for the draft link table.
    #[inline]
    pub fn table_iden(&self) -> Alias {
        Alias::new(&self.table_name)
    }

    /// Physical published mirror link table name (e.g. `articles__tags_link__published`).
    #[inline]
    pub fn published_table_name(&self) -> String {
        format!("{}__published", self.table_name)
    }

    /// SeaQuery identifier for the published link table.
    #[inline]
    pub fn published_table_iden(&self) -> Alias {
        Alias::new(self.published_table_name())
    }

    /// Unique index name enforcing HasOne singular relationship on owner side.
    #[inline]
    pub fn owner_unique_index_name(&self) -> String {
        format!("uq_{}_owner", self.table_name)
    }

    /// Index name on target_id column for reverse lookup performance.
    #[inline]
    pub fn target_index_name(&self) -> String {
        format!("idx_{}_target", self.table_name)
    }

    /// Foreign key constraint name referencing owner table.
    #[inline]
    pub fn link_owner_fk_name(&self) -> String {
        format!("fk_{}_owner", self.table_name)
    }

    /// Foreign key constraint name referencing target table.
    #[inline]
    pub fn link_target_fk_name(&self) -> String {
        format!("fk_{}_target", self.table_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::{DocumentTypeId, DocumentTypeInfo, DocumentTypeOptions};
    use indexmap::IndexSet;

    fn sample_article_type(draft_and_publish: bool) -> DocumentType {
        DocumentType {
            id: DocumentTypeId::try_new("article").unwrap(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Article".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions { draft_and_publish },
            fields: IndexSet::new(),
        }
    }

    #[test]
    fn test_document_table_naming_collection() {
        let dt = sample_article_type(true);
        let naming = DocumentTableNaming::new(&dt);

        assert_eq!(naming.table_name(), "articles");
        assert_eq!(naming.published_table_name().unwrap(), "articles__published");
        assert_eq!(
            naming.published_fk_name().unwrap(),
            "fk_articles__published_id"
        );

        let attr = AttributeId::try_new("hero-image").unwrap();
        assert_eq!(naming.column_name(&attr), "hero_image");
        assert_eq!(naming.foreign_key_column_name(&attr), "hero_image_id");
        assert_eq!(naming.index_name("hero_image"), "idx_articles_hero_image");
    }

    #[test]
    fn test_link_table_naming() {
        let dt = sample_article_type(true);
        let naming = DocumentTableNaming::new(&dt);
        let tag_attr = AttributeId::try_new("tags").unwrap();
        let link = naming.link_table(&tag_attr);

        assert_eq!(link.table_name(), "articles__tags_link");
        assert_eq!(
            link.published_table_name(),
            "articles__tags_link__published"
        );
        assert_eq!(link.owner_unique_index_name(), "uq_articles__tags_link_owner");
        assert_eq!(link.target_index_name(), "idx_articles__tags_link_target");
        assert_eq!(link.link_owner_fk_name(), "fk_articles__tags_link_owner");
        assert_eq!(link.link_target_fk_name(), "fk_articles__tags_link_target");
    }
}
