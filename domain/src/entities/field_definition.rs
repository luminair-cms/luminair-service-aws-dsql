use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::types::field_type::{FieldType, PrimitiveType};
use crate::value_objects::AttributeId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldConstraint {
    Pattern(String),
    MinLength(usize),
    MaxLength(usize),
    MinInteger(i64),
    MaxInteger(i64),
    MinDecimal(Decimal),
    MaxDecimal(Decimal),
}

impl FieldConstraint {
    pub fn is_applicable_for(&self, ft: &FieldType) -> bool {
        match self {
            FieldConstraint::Pattern(_) => matches!(
                ft,
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                    | FieldType::LocalizedText
            ),
            FieldConstraint::MinLength(_) | FieldConstraint::MaxLength(_) => matches!(
                ft,
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                    | FieldType::LocalizedText
            ),
            FieldConstraint::MinInteger(_) | FieldConstraint::MaxInteger(_) => {
                matches!(ft, FieldType::Primitive(PrimitiveType::Integer(_)))
            }
            FieldConstraint::MinDecimal(_) | FieldConstraint::MaxDecimal(_) => {
                matches!(ft, FieldType::Primitive(PrimitiveType::Decimal { .. }))
            }
        }
    }
}

use std::borrow::Borrow;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub id: AttributeId,
    pub field_type: FieldType,
    pub required: bool,
    pub unique: bool,
    pub constraints: Vec<FieldConstraint>,
}

impl PartialEq for FieldDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for FieldDefinition {}

impl Hash for FieldDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.as_ref().hash(state);
    }
}

impl Borrow<AttributeId> for FieldDefinition {
    fn borrow(&self) -> &AttributeId {
        &self.id
    }
}

impl Borrow<str> for FieldDefinition {
    fn borrow(&self) -> &str {
        self.id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::field_type::IntegerSize;

    #[test]
    fn test_constraint_applicable_pattern_on_text() {
        let constraint = FieldConstraint::Pattern("^[a-z]+$".to_string());
        assert!(constraint.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
        assert!(constraint.is_applicable_for(&FieldType::Primitive(PrimitiveType::Uid)));
        assert!(!constraint.is_applicable_for(&FieldType::Email));
        assert!(!constraint.is_applicable_for(&FieldType::Url));
        assert!(constraint.is_applicable_for(&FieldType::LocalizedText));
    }

    #[test]
    fn test_constraint_applicable_pattern_on_integer() {
        let constraint = FieldConstraint::Pattern("^[a-z]+$".to_string());
        assert!(
            !constraint.is_applicable_for(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(!constraint.is_applicable_for(&FieldType::Primitive(PrimitiveType::Boolean)));
        assert!(!constraint.is_applicable_for(&FieldType::Json));
    }

    #[test]
    fn test_constraint_min_max_on_integer() {
        let min_c = FieldConstraint::MinInteger(0);
        let max_c = FieldConstraint::MaxInteger(100);
        let int_type = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64));

        assert!(min_c.is_applicable_for(&int_type));
        assert!(max_c.is_applicable_for(&int_type));
        assert!(!min_c.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_constraint_min_max_on_decimal() {
        let min_c = FieldConstraint::MinDecimal(Decimal::new(0, 0));
        let max_c = FieldConstraint::MaxDecimal(Decimal::new(100, 0));
        let dec_type = FieldType::Primitive(PrimitiveType::Decimal {
            precision: 10,
            scale: 2,
        });

        assert!(min_c.is_applicable_for(&dec_type));
        assert!(max_c.is_applicable_for(&dec_type));
        assert!(
            !min_c.is_applicable_for(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
    }

    #[test]
    fn test_constraint_length_on_text_uid_email() {
        let min_len = FieldConstraint::MinLength(5);
        let max_len = FieldConstraint::MaxLength(50);

        assert!(min_len.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
        assert!(min_len.is_applicable_for(&FieldType::Primitive(PrimitiveType::Uid)));
        assert!(!min_len.is_applicable_for(&FieldType::Email));
        assert!(!min_len.is_applicable_for(&FieldType::Url));
        assert!(min_len.is_applicable_for(&FieldType::LocalizedText));

        assert!(max_len.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
        assert!(max_len.is_applicable_for(&FieldType::Primitive(PrimitiveType::Uid)));
        assert!(!max_len.is_applicable_for(&FieldType::Email));
        assert!(!min_len.is_applicable_for(&FieldType::Primitive(PrimitiveType::Boolean)));
    }

    #[test]
    fn test_index_set_lookup_by_borrow() {
        use indexmap::IndexSet;

        let attr = AttributeId::try_new("title").unwrap();
        let field = FieldDefinition {
            id: attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            required: true,
            unique: false,
            constraints: vec![],
        };

        let mut set = IndexSet::new();
        assert!(set.insert(field));

        // Lookup via &AttributeId
        assert!(set.get(&attr).is_some());
        // Lookup via &str
        assert!(set.get("title").is_some());
        assert!(set.get("unknown").is_none());
        // Check contains
        assert!(set.contains("title"));
        assert!(set.contains(&attr));
    }
}
