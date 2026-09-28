use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntegerSize {
    I16,
    I32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    Uid,
    Uuid,
    Text,
    Integer(IntegerSize),
    Decimal { precision: u8, scale: u8 },
    Date,
    DateTime,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldType {
    Primitive(PrimitiveType),
    LocalizedText,
    Email,
    Url,
    Json,
}

impl From<PrimitiveType> for FieldType {
    fn from(p: PrimitiveType) -> Self {
        FieldType::Primitive(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_primitive_type() {
        assert_eq!(
            FieldType::from(PrimitiveType::Text),
            FieldType::Primitive(PrimitiveType::Text)
        );
        assert_eq!(
            FieldType::from(PrimitiveType::Boolean),
            FieldType::Primitive(PrimitiveType::Boolean)
        );
    }

    #[test]
    fn test_field_type_equality_and_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(FieldType::Primitive(PrimitiveType::Uid));
        set.insert(FieldType::Primitive(PrimitiveType::Integer(
            IntegerSize::I32,
        )));
        set.insert(FieldType::LocalizedText);
        set.insert(FieldType::Email);
        set.insert(FieldType::Url);
        set.insert(FieldType::Json);

        assert!(set.contains(&FieldType::LocalizedText));
        assert!(set.contains(&FieldType::Email));
        assert_eq!(set.len(), 6);
    }
}
