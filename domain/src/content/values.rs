use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{Email, Url};
use crate::schema::types::{FieldType, IntegerSize, PrimitiveType};
use crate::system::ids::LocaleId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}

impl From<DomainValue> for ContentValue {
    fn from(v: DomainValue) -> Self {
        ContentValue::Scalar(v)
    }
}

impl From<PrimitiveValue> for ContentValue {
    fn from(p: PrimitiveValue) -> Self {
        ContentValue::Scalar(DomainValue::Primitive(p))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainValue {
    Primitive(PrimitiveValue),
    Email(Email),
    Url(Url),
    Json(HashMap<String, PrimitiveValue>),
}

impl DomainValue {
    pub fn matches_field_type(&self, ft: &FieldType) -> bool {
        match (self, ft) {
            (DomainValue::Primitive(p), FieldType::Primitive(pt)) => p.matches_primitive_type(pt),
            (DomainValue::Email(_), FieldType::Email) => true,
            (DomainValue::Url(_), FieldType::Url) => true,
            (DomainValue::Json(_), FieldType::Json) => true,
            _ => false,
        }
    }
}

impl From<PrimitiveValue> for DomainValue {
    fn from(p: PrimitiveValue) -> Self {
        DomainValue::Primitive(p)
    }
}

impl From<Email> for DomainValue {
    fn from(e: Email) -> Self {
        DomainValue::Email(e)
    }
}

impl From<Url> for DomainValue {
    fn from(u: Url) -> Self {
        DomainValue::Url(u)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveValue {
    Text(String),
    Uid(String),
    Uuid(Uuid),
    Integer(i64),
    Decimal(Decimal),
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
    Boolean(bool),
}

impl PrimitiveValue {
    pub fn matches_primitive_type(&self, pt: &PrimitiveType) -> bool {
        match (self, pt) {
            (PrimitiveValue::Text(_), PrimitiveType::Text) => true,
            (PrimitiveValue::Uid(_), PrimitiveType::Uid) => true,
            (PrimitiveValue::Uuid(_), PrimitiveType::Uuid) => true,
            (PrimitiveValue::Integer(val), PrimitiveType::Integer(size)) => match size {
                IntegerSize::I16 => *val >= i16::MIN as i64 && *val <= i16::MAX as i64,
                IntegerSize::I32 => *val >= i32::MIN as i64 && *val <= i32::MAX as i64,
                IntegerSize::I64 => true,
            },
            (PrimitiveValue::Decimal(_), PrimitiveType::Decimal { .. }) => true,
            (PrimitiveValue::Date(_), PrimitiveType::Date) => true,
            (PrimitiveValue::DateTime(_), PrimitiveType::DateTime) => true,
            (PrimitiveValue::Boolean(_), PrimitiveType::Boolean) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_field_type_text_ok() {
        let val: DomainValue = PrimitiveValue::Text("hello".to_string()).into();
        assert!(val.matches_field_type(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_matches_field_type_mismatch() {
        let val: DomainValue = PrimitiveValue::Text("hello".to_string()).into();
        assert!(
            !val.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(!val.matches_field_type(&FieldType::Primitive(PrimitiveType::Boolean)));
        assert!(!val.matches_field_type(&FieldType::Email));
    }

    #[test]
    fn test_matches_field_type_integer_all_sizes() {
        let small: DomainValue = PrimitiveValue::Integer(100).into();
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16
            )))
        );
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            small.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );

        let overflow_i16: DomainValue = PrimitiveValue::Integer(70000).into();
        assert!(
            !overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16
            )))
        );
        assert!(
            overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            overflow_i16.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );

        let overflow_i32: DomainValue = PrimitiveValue::Integer(3_000_000_000).into();
        assert!(
            !overflow_i32.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            )))
        );
        assert!(
            overflow_i32.matches_field_type(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            )))
        );
    }

    #[test]
    fn test_matches_field_type_json() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), PrimitiveValue::Text("value".to_string()));
        let val = DomainValue::Json(map);
        assert!(val.matches_field_type(&FieldType::Json));
        assert!(!val.matches_field_type(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_matches_field_type_exhaustive() {
        let now = Utc::now();
        let date = now.date_naive();
        let pairs: Vec<(DomainValue, FieldType)> = vec![
            (
                PrimitiveValue::Text("s".into()).into(),
                FieldType::Primitive(PrimitiveType::Text),
            ),
            (
                PrimitiveValue::Uid("slug".into()).into(),
                FieldType::Primitive(PrimitiveType::Uid),
            ),
            (
                PrimitiveValue::Uuid(Uuid::now_v7()).into(),
                FieldType::Primitive(PrimitiveType::Uuid),
            ),
            (
                DomainValue::Email(Email::try_new("test@example.com").unwrap()),
                FieldType::Email,
            ),
            (
                DomainValue::Url(Url::try_new("https://example.com").unwrap()),
                FieldType::Url,
            ),
            (
                PrimitiveValue::Integer(42).into(),
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            ),
            (
                PrimitiveValue::Decimal(Decimal::new(100, 2)).into(),
                FieldType::Primitive(PrimitiveType::Decimal {
                    precision: 10,
                    scale: 2,
                }),
            ),
            (
                PrimitiveValue::Date(date).into(),
                FieldType::Primitive(PrimitiveType::Date),
            ),
            (
                PrimitiveValue::DateTime(now).into(),
                FieldType::Primitive(PrimitiveType::DateTime),
            ),
            (
                PrimitiveValue::Boolean(true).into(),
                FieldType::Primitive(PrimitiveType::Boolean),
            ),
            (DomainValue::Json(HashMap::new()), FieldType::Json),
        ];

        for (i, (val, expected_ft)) in pairs.iter().enumerate() {
            assert!(
                val.matches_field_type(expected_ft),
                "Expected pair {i} to match"
            );
            let mismatch_ft = if *expected_ft == FieldType::Primitive(PrimitiveType::Text) {
                FieldType::Primitive(PrimitiveType::Boolean)
            } else {
                FieldType::Primitive(PrimitiveType::Text)
            };
            assert!(!val.matches_field_type(&mismatch_ft));
        }
    }
}
