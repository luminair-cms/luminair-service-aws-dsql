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

impl PrimitiveType {
    /// Returns the PostgreSQL column type representation for DDL generation.
    pub fn sql_type_name(&self) -> String {
        match self {
            PrimitiveType::Uid => "VARCHAR(255)".to_string(),
            PrimitiveType::Uuid => "UUID".to_string(),
            PrimitiveType::Text => "TEXT".to_string(),
            PrimitiveType::Integer(size) => match size {
                IntegerSize::I16 => "SMALLINT".to_string(),
                IntegerSize::I32 => "INTEGER".to_string(),
                IntegerSize::I64 => "BIGINT".to_string(),
            },
            PrimitiveType::Decimal { precision, scale } => {
                format!("NUMERIC({}, {})", precision, scale)
            }
            PrimitiveType::Date => "DATE".to_string(),
            PrimitiveType::DateTime => "TIMESTAMPTZ".to_string(),
            PrimitiveType::Boolean => "BOOLEAN".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldType {
    Primitive(PrimitiveType),
    LocalizedText,
    Email,
    Url,
    Json,
}

impl FieldType {
    /// Returns the PostgreSQL column type representation for DDL generation.
    pub fn sql_type_name(&self) -> String {
        match self {
            FieldType::Primitive(p) => p.sql_type_name(),
            FieldType::LocalizedText => "JSONB".to_string(),
            FieldType::Email => "VARCHAR(320)".to_string(),
            FieldType::Url => "TEXT".to_string(),
            FieldType::Json => "JSONB".to_string(),
        }
    }
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
    fn test_sql_type_name_text() {
        assert_eq!(
            FieldType::Primitive(PrimitiveType::Text).sql_type_name(),
            "TEXT"
        );
    }

    #[test]
    fn test_sql_type_name_localized_text() {
        assert_eq!(FieldType::LocalizedText.sql_type_name(), "JSONB");
    }

    #[test]
    fn test_sql_type_name_decimal() {
        let decimal = FieldType::Primitive(PrimitiveType::Decimal {
            precision: 10,
            scale: 2,
        });
        assert_eq!(decimal.sql_type_name(), "NUMERIC(10, 2)");
    }

    #[test]
    fn test_sql_type_name_all_variants() {
        let all_variants = [
            FieldType::Primitive(PrimitiveType::Uid),
            FieldType::Primitive(PrimitiveType::Uuid),
            FieldType::Primitive(PrimitiveType::Text),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)),
            FieldType::Primitive(PrimitiveType::Decimal {
                precision: 18,
                scale: 4,
            }),
            FieldType::Primitive(PrimitiveType::Date),
            FieldType::Primitive(PrimitiveType::DateTime),
            FieldType::Primitive(PrimitiveType::Boolean),
            FieldType::LocalizedText,
            FieldType::Email,
            FieldType::Url,
            FieldType::Json,
        ];

        for variant in all_variants {
            let sql_name = variant.sql_type_name();
            assert!(
                !sql_name.is_empty(),
                "Type {:?} produced empty SQL type name",
                variant
            );
        }
    }
}
