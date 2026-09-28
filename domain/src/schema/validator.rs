use crate::content::instance::DocumentContent;
use crate::content::values::{ContentValue, DomainValue, PrimitiveValue};
use crate::errors::DomainError;
use crate::schema::document_type::DocumentType;
use crate::schema::field_definition::FieldConstraint;
use crate::schema::ids::AttributeId;
use crate::schema::types::FieldType;
use crate::system::config::SystemConfig;
use crate::system::ids::LocaleId;

/// Evaluates string constraints (MinLength, MaxLength, Pattern) against a string value.
/// If `locale` is provided, includes it in the error reason.
fn evaluate_str_constraint(
    attr_id: &AttributeId,
    constraint: &FieldConstraint,
    s: &str,
    locale: Option<&LocaleId>,
) -> Option<DomainError> {
    let prefix = match locale {
        Some(loc) => format!("locale '{}': ", loc.as_ref()),
        None => String::new(),
    };
    let violated = |reason: String| -> Option<DomainError> {
        Some(DomainError::InvalidFieldValue {
            attribute_id: attr_id.clone(),
            reason: format!("{prefix}{reason}"),
        })
    };

    match constraint {
        FieldConstraint::MinLength(min) => {
            let len = s.chars().count();
            if len < *min {
                return violated(format!("length {} is below minimum {}", len, min));
            }
        }
        FieldConstraint::MaxLength(max) => {
            let len = s.chars().count();
            if len > *max {
                return violated(format!("length {} exceeds maximum {}", len, max));
            }
        }
        FieldConstraint::Pattern(pat) => {
            // Compile regex; treat compilation failure as a constraint violation to surface config errors
            match regex::Regex::new(pat) {
                Ok(re) if !re.is_match(s) => {
                    return violated(format!("value '{}' does not match pattern '{}'", s, pat));
                }
                Err(e) => {
                    return violated(format!("invalid regex pattern '{}': {}", pat, e));
                }
                _ => {}
            }
        }
        _ => {}
    }
    None
}

/// Evaluates a single `FieldConstraint` against a scalar `DomainValue`.
/// Returns `Some(DomainError)` if the constraint is violated, `None` if satisfied.
fn evaluate_constraint(
    attr_id: &AttributeId,
    constraint: &FieldConstraint,
    value: &DomainValue,
) -> Option<DomainError> {
    let violated = |reason: String| -> Option<DomainError> {
        Some(DomainError::InvalidFieldValue {
            attribute_id: attr_id.clone(),
            reason,
        })
    };

    match (constraint, value) {
        (
            FieldConstraint::MinLength(_)
            | FieldConstraint::MaxLength(_)
            | FieldConstraint::Pattern(_),
            DomainValue::Primitive(PrimitiveValue::Text(s))
            | DomainValue::Primitive(PrimitiveValue::Uid(s)),
        ) => evaluate_str_constraint(attr_id, constraint, s, None),
        (FieldConstraint::MinInteger(min), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n < *min {
                return violated(format!("value {} is below minimum {}", n, min));
            }
            None
        }
        (FieldConstraint::MaxInteger(max), DomainValue::Primitive(PrimitiveValue::Integer(n))) => {
            if *n > *max {
                return violated(format!("value {} exceeds maximum {}", n, max));
            }
            None
        }
        (FieldConstraint::MinDecimal(min), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d < min {
                return violated(format!("value {} is below minimum {}", d, min));
            }
            None
        }
        (FieldConstraint::MaxDecimal(max), DomainValue::Primitive(PrimitiveValue::Decimal(d))) => {
            if d > max {
                return violated(format!("value {} exceeds maximum {}", d, max));
            }
            None
        }
        // Constraint not applicable to this value type — silently skip (type validation catches mismatches)
        _ => None,
    }
}

/// Validates document instance content against a `DocumentType` definition and `SystemConfig`.
pub fn validate_content(
    doc_type: &DocumentType,
    content: &DocumentContent,
    system_config: &SystemConfig,
) -> Result<(), Vec<DomainError>> {
    let mut errors = Vec::new();

    // 1. Validate declared fields
    for field_def in &doc_type.fields {
        let attr_id = &field_def.id;
        match content.fields.get(attr_id) {
            None | Some(ContentValue::Null) => {
                if field_def.required {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: "field is required but missing or null".to_string(),
                    });
                }
            }
            Some(ContentValue::Scalar(val)) => {
                if !val.matches_field_type(&field_def.field_type) {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: format!(
                            "value does not match field type {:?}",
                            field_def.field_type
                        ),
                    });
                } else {
                    // Evaluate FieldConstraints against the scalar value
                    for constraint in &field_def.constraints {
                        if let Some(err) = evaluate_constraint(attr_id, constraint, val) {
                            errors.push(err);
                        }
                    }
                }
            }
            Some(ContentValue::LocalizedText(map)) => {
                if field_def.field_type != FieldType::LocalizedText {
                    errors.push(DomainError::InvalidFieldValue {
                        attribute_id: attr_id.clone(),
                        reason: "localized text value provided for non-localized field".to_string(),
                    });
                } else {
                    let mut sorted_locales: Vec<_> = map.keys().collect();
                    sorted_locales.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
                    for locale in sorted_locales {
                        let localized_str = &map[locale];
                        if !system_config.contains_locale(locale) {
                            errors.push(DomainError::UnknownLocale(locale.clone()));
                        } else {
                            for constraint in &field_def.constraints {
                                if let Some(err) = evaluate_str_constraint(
                                    attr_id,
                                    constraint,
                                    localized_str,
                                    Some(locale),
                                ) {
                                    errors.push(err);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Reject undeclared fields
    for attr_id in content.fields.keys() {
        if !doc_type.fields.contains(attr_id) {
            errors.push(DomainError::UnknownAttribute(attr_id.clone()));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexSet;
    use std::collections::HashMap;
    use uuid::Uuid;

    use crate::common::{Email, Url};
    use crate::content::instance::PublicationState;
    use crate::schema::document_type::{DocumentKind, DocumentTypeInfo, DocumentTypeOptions};
    use crate::schema::field_definition::FieldDefinition;
    use crate::schema::ids::DocumentTypeId;
    use crate::schema::types::PrimitiveType;
    use crate::system::ids::SystemConfigId;

    fn make_test_setup() -> (DocumentType, SystemConfig) {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let title_attr = AttributeId::try_new("title").unwrap();
        let body_attr = AttributeId::try_new("body").unwrap();

        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: title_attr,
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![],
        });
        fields.insert(FieldDefinition {
            id: body_attr,
            field_type: FieldType::LocalizedText,
            required: false,
            unique: false,
            constraints: vec![],
        });

        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Articles".into(),
                singular_name: "article".into(),
                plural_name: "articles".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
        };

        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone(), uk],
            en,
        )
        .unwrap();

        (doc_type, config)
    }

    #[test]
    fn test_validate_content_correct() {
        let (doc_type, config) = make_test_setup();

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
        );
        let mut loc_map = HashMap::new();
        loc_map.insert(LocaleId::try_new("en").unwrap(), "Content".to_string());
        fields.insert(
            AttributeId::try_new("body").unwrap(),
            ContentValue::LocalizedText(loc_map),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        assert!(validate_content(&doc_type, &content, &config).is_ok());
    }

    #[test]
    fn test_validate_content_wrong_type() {
        let (doc_type, config) = make_test_setup();

        let mut fields = HashMap::new();
        // Title expects Text, providing Integer
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(123))),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        let errs = validate_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], DomainError::InvalidFieldValue { .. }));
    }

    #[test]
    fn test_validate_content_unknown_locale() {
        let (doc_type, config) = make_test_setup();

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
        );
        let mut loc_map = HashMap::new();
        let fr = LocaleId::try_new("fr").unwrap(); // not in config available_locales
        loc_map.insert(fr.clone(), "Bonjour".to_string());
        fields.insert(
            AttributeId::try_new("body").unwrap(),
            ContentValue::LocalizedText(loc_map),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        let errs = validate_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], DomainError::UnknownLocale(loc) if *loc == fr));
    }

    #[test]
    fn test_validate_content_field_constraint_min_length_violated() {
        let type_id = DocumentTypeId::try_new("slugged").unwrap();
        let slug_attr = AttributeId::try_new("slug").unwrap();
        let mut fields_def = IndexSet::new();
        fields_def.insert(FieldDefinition {
            id: slug_attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![FieldConstraint::MinLength(5)],
        });
        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Slugged".into(),
                singular_name: "slugged".into(),
                plural_name: "sluggeds".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();

        let mut content_fields = HashMap::new();
        content_fields.insert(
            slug_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("hi".into()))),
        );
        let content = DocumentContent {
            fields: content_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = validate_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], DomainError::InvalidFieldValue { .. }));
        assert!(errs[0].to_string().contains("below minimum"));
    }

    #[test]
    fn test_validate_content_field_constraint_pattern_violated() {
        let type_id = DocumentTypeId::try_new("slugged").unwrap();
        let slug_attr = AttributeId::try_new("slug").unwrap();
        let mut fields_def = IndexSet::new();
        fields_def.insert(FieldDefinition {
            id: slug_attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![FieldConstraint::Pattern("^[a-z0-9-]+$".into())],
        });
        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Slugged".into(),
                singular_name: "slugged".into(),
                plural_name: "sluggeds".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();

        let mut content_fields = HashMap::new();
        content_fields.insert(
            slug_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "INVALID SLUG!".into(),
            ))),
        );
        let content = DocumentContent {
            fields: content_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = validate_content(&doc_type, &content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].to_string().contains("does not match pattern"));
    }

    #[test]
    fn test_validate_content_multiple_errors_returned() {
        // Both title (wrong type) and body (unknown locale) fail → both errors should be returned
        let (doc_type, config) = make_test_setup();

        let mut fields = HashMap::new();
        // Title: wrong type (Integer instead of Text)
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(999))),
        );
        // Body: unknown locale
        let mut loc_map = HashMap::new();
        loc_map.insert(LocaleId::try_new("fr").unwrap(), "Bonjour".to_string());
        fields.insert(
            AttributeId::try_new("body").unwrap(),
            ContentValue::LocalizedText(loc_map),
        );

        let content = DocumentContent {
            fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };

        let errs = validate_content(&doc_type, &content, &config).unwrap_err();
        // Must return ALL errors, not just the first one
        assert_eq!(errs.len(), 2, "expected 2 errors but got {:?}", errs);
    }

    #[test]
    fn test_validate_content_email_and_url_self_validation() {
        let type_id = DocumentTypeId::try_new("contact").unwrap();
        let email_attr = AttributeId::try_new("email").unwrap();
        let website_attr = AttributeId::try_new("website").unwrap();
        let mut fields_def = IndexSet::new();
        fields_def.insert(FieldDefinition {
            id: email_attr.clone(),
            field_type: FieldType::Email,
            required: true,
            unique: false,
            constraints: vec![],
        });
        fields_def.insert(FieldDefinition {
            id: website_attr.clone(),
            field_type: FieldType::Url,
            required: true,
            unique: false,
            constraints: vec![],
        });
        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Contact".into(),
                singular_name: "contact".into(),
                plural_name: "contacts".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(Uuid::now_v7()), vec![en.clone()], en).unwrap();

        // 1. Valid email and url value objects
        let valid_email = Email::try_new("User.Name@Example.COM").unwrap();
        let valid_url = Url::try_new("https://example.com/api").unwrap();

        let mut valid_fields = HashMap::new();
        valid_fields.insert(
            email_attr.clone(),
            ContentValue::Scalar(DomainValue::Email(valid_email)),
        );
        valid_fields.insert(
            website_attr.clone(),
            ContentValue::Scalar(DomainValue::Url(valid_url)),
        );
        let valid_content = DocumentContent {
            fields: valid_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        assert!(validate_content(&doc_type, &valid_content, &config).is_ok());

        // 2. Email and Url value objects validate themselves upon construction
        assert!(Email::try_new("not-an-email").is_err());
        assert!(Url::try_new("not-a-url").is_err());

        // 3. Type mismatch: providing a Primitive Text instead of Email/Url fails validation
        let mut mismatch_fields = HashMap::new();
        mismatch_fields.insert(
            email_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("test".into()))),
        );
        mismatch_fields.insert(
            website_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "https://ex.com".into(),
            ))),
        );
        let mismatch_content = DocumentContent {
            fields: mismatch_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = validate_content(&doc_type, &mismatch_content, &config).unwrap_err();
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn test_validate_content_localized_text_constraints() {
        let type_id = DocumentTypeId::try_new("post").unwrap();
        let summary_attr = AttributeId::try_new("summary").unwrap();
        let mut fields_def = IndexSet::new();
        fields_def.insert(FieldDefinition {
            id: summary_attr.clone(),
            field_type: FieldType::LocalizedText,
            required: true,
            unique: false,
            constraints: vec![
                FieldConstraint::MinLength(5),
                FieldConstraint::MaxLength(20),
            ],
        });
        let doc_type = DocumentType {
            id: type_id,
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: "Post".into(),
                singular_name: "post".into(),
                plural_name: "posts".into(),
                description: None,
            },
            options: DocumentTypeOptions {
                draft_and_publish: false,
            },
            fields: fields_def,
        };
        let en = LocaleId::try_new("en").unwrap();
        let uk = LocaleId::try_new("uk").unwrap();
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone(), uk],
            en,
        )
        .unwrap();

        // 1. Valid per-locale content
        let mut valid_loc_map = HashMap::new();
        valid_loc_map.insert(
            LocaleId::try_new("en").unwrap(),
            "Short summary".to_string(),
        );
        valid_loc_map.insert(
            LocaleId::try_new("uk").unwrap(),
            "Короткий опис".to_string(),
        );
        let mut valid_fields = HashMap::new();
        valid_fields.insert(
            summary_attr.clone(),
            ContentValue::LocalizedText(valid_loc_map),
        );
        let valid_content = DocumentContent {
            fields: valid_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        assert!(validate_content(&doc_type, &valid_content, &config).is_ok());

        // 2. "en" is valid (13 chars), but "uk" is too short (2 chars < 5)
        let mut invalid_loc_map = HashMap::new();
        invalid_loc_map.insert(
            LocaleId::try_new("en").unwrap(),
            "Valid english".to_string(),
        );
        invalid_loc_map.insert(LocaleId::try_new("uk").unwrap(), "Hi".to_string());
        let mut invalid_fields = HashMap::new();
        invalid_fields.insert(summary_attr, ContentValue::LocalizedText(invalid_loc_map));
        let invalid_content = DocumentContent {
            fields: invalid_fields,
            publication_state: PublicationState::Draft {
                last_published_revision: None,
            },
        };
        let errs = validate_content(&doc_type, &invalid_content, &config).unwrap_err();
        assert_eq!(errs.len(), 1);
        let err_msg = errs[0].to_string();
        assert!(
            err_msg.contains("locale 'uk'"),
            "unexpected message: {}",
            err_msg
        );
        assert!(
            err_msg.contains("below minimum 5"),
            "unexpected message: {}",
            err_msg
        );
    }
}
