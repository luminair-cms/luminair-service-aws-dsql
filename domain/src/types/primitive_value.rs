use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::field_type::{IntegerSize, PrimitiveType};

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
