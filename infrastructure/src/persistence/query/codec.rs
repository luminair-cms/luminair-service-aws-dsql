//! Row decoding and value serialization between domain content and database types.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use domain::common::{Email, Url};
use domain::content::{ContentValue, DomainValue, PrimitiveValue};
use domain::errors::DomainError;
use domain::schema::{FieldType, IntegerSize, PrimitiveType};
use domain::system::LocaleId;
use sea_query::Value;
use sqlx::Row;
use uuid::Uuid;

/// Helper function to create a storage error.
#[inline]
pub fn storage_err(e: impl std::fmt::Display) -> DomainError {
    DomainError::Storage(e.to_string())
}

/// Reads a single dynamic content value from a database row based on its field definition.
pub fn read_content_value(
    row: &sqlx::postgres::PgRow,
    col_name: &str,
    ft: &FieldType,
) -> Result<ContentValue, DomainError> {
    match ft {
        FieldType::Primitive(PrimitiveType::Text) => {
            let s: Option<String> = row.try_get(col_name).map_err(storage_err)?;
            Ok(s.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(s))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Uid) => {
            let s: Option<String> = row.try_get(col_name).map_err(storage_err)?;
            Ok(s.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uid(s))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Uuid) => {
            let u: Option<Uuid> = row.try_get(col_name).map_err(storage_err)?;
            Ok(u.map(|u| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uuid(u))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Integer(size)) => {
            let val: Option<i64> = match size {
                IntegerSize::I16 => row
                    .try_get::<Option<i16>, _>(col_name)
                    .map_err(storage_err)?
                    .map(|i| i as i64),
                IntegerSize::I32 => row
                    .try_get::<Option<i32>, _>(col_name)
                    .map_err(storage_err)?
                    .map(|i| i as i64),
                IntegerSize::I64 => row
                    .try_get::<Option<i64>, _>(col_name)
                    .map_err(storage_err)?,
            };
            Ok(val
                .map(|i| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => {
            let d: Option<rust_decimal::Decimal> = row.try_get(col_name).map_err(storage_err)?;
            Ok(d.map(|d| {
                ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(d)))
            })
            .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Boolean) => {
            let b: Option<bool> = row.try_get(col_name).map_err(storage_err)?;
            Ok(b.map(|b| {
                ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Boolean(b)))
            })
            .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::Date) => {
            let d: Option<chrono::NaiveDate> = row.try_get(col_name).map_err(storage_err)?;
            Ok(d.map(|d| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Date(d))))
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Primitive(PrimitiveType::DateTime) => {
            let dt: Option<DateTime<Utc>> = row.try_get(col_name).map_err(storage_err)?;
            Ok(dt
                .map(|dt| {
                    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::DateTime(dt)))
                })
                .unwrap_or(ContentValue::Null))
        }
        FieldType::Email => {
            let s: Option<String> = row.try_get(col_name).map_err(storage_err)?;
            match s {
                Some(s) => {
                    let email = Email::try_new(s).map_err(storage_err)?;
                    Ok(ContentValue::Scalar(DomainValue::Email(email)))
                }
                None => Ok(ContentValue::Null),
            }
        }
        FieldType::Url => {
            let s: Option<String> = row.try_get(col_name).map_err(storage_err)?;
            match s {
                Some(s) => {
                    let url = Url::try_new(s).map_err(storage_err)?;
                    Ok(ContentValue::Scalar(DomainValue::Url(url)))
                }
                None => Ok(ContentValue::Null),
            }
        }
        FieldType::LocalizedText => {
            let json_opt: Option<serde_json::Value> = row.try_get(col_name).map_err(storage_err)?;
            match json_opt {
                Some(serde_json::Value::Null) | None => Ok(ContentValue::Null),
                Some(json) => {
                    let map: HashMap<LocaleId, String> =
                        serde_json::from_value(json).map_err(storage_err)?;
                    Ok(ContentValue::LocalizedText(map))
                }
            }
        }
        FieldType::Json => {
            let json_opt: Option<serde_json::Value> = row.try_get(col_name).map_err(storage_err)?;
            match json_opt {
                Some(serde_json::Value::Null) | None => Ok(ContentValue::Null),
                Some(json) => {
                    let map: HashMap<String, PrimitiveValue> =
                        serde_json::from_value(json).map_err(storage_err)?;
                    Ok(ContentValue::Scalar(DomainValue::Json(map)))
                }
            }
        }
    }
}

/// Converts a domain `ContentValue` and its schema `FieldType` into a typed `sea_query::Value`.
pub fn to_sea_value(
    val: Option<&ContentValue>,
    ft: &FieldType,
) -> Result<Value, DomainError> {
    match (val, ft) {
        (None | Some(ContentValue::Null), ft) => Ok(typed_null_value(ft)),
        (Some(ContentValue::LocalizedText(map)), FieldType::LocalizedText) => {
            let json = serde_json::to_value(map).map_err(storage_err)?;
            Ok(Value::Json(Some(Box::new(json))))
        }
        (Some(ContentValue::Scalar(DomainValue::Json(map))), FieldType::Json) => {
            let json = serde_json::to_value(map).map_err(storage_err)?;
            Ok(Value::Json(Some(Box::new(json))))
        }
        (Some(ContentValue::Scalar(DomainValue::Email(e))), FieldType::Email) => {
            Ok(Value::String(Some(e.as_ref().to_string())))
        }
        (Some(ContentValue::Scalar(DomainValue::Url(u))), FieldType::Url) => {
            Ok(Value::String(Some(u.as_ref().to_string())))
        }
        (Some(ContentValue::Scalar(DomainValue::Primitive(p))), _) => match p {
            PrimitiveValue::Text(s) => Ok(Value::String(Some(s.clone()))),
            PrimitiveValue::Uid(s) => Ok(Value::String(Some(s.clone()))),
            PrimitiveValue::Uuid(u) => Ok(Value::Uuid(Some(*u))),
            PrimitiveValue::Integer(i) => match ft {
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => {
                    Ok(Value::SmallInt(Some(*i as i16)))
                }
                FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => {
                    Ok(Value::Int(Some(*i as i32)))
                }
                _ => Ok(Value::BigInt(Some(*i))),
            },
            PrimitiveValue::Decimal(d) => Ok(Value::Decimal(Some(*d))),
            PrimitiveValue::Boolean(b) => Ok(Value::Bool(Some(*b))),
            PrimitiveValue::Date(d) => Ok(Value::ChronoDate(Some(*d))),
            PrimitiveValue::DateTime(dt) => Ok(Value::ChronoDateTimeUtc(Some(*dt))),
        },
        _ => {
            // Incompatible type variant for field - return error rather than silent data corruption
            Err(DomainError::Storage(format!(
                "incompatible content value for field type {ft:?}"
            )))
        }
    }
}

/// Returns the correctly typed NULL `Value` variant for a given `FieldType`.
fn typed_null_value(ft: &FieldType) -> Value {
    match ft {
        FieldType::Primitive(PrimitiveType::Text) | FieldType::Primitive(PrimitiveType::Uid) => {
            Value::String(None)
        }
        FieldType::Primitive(PrimitiveType::Uuid) => Value::Uuid(None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => Value::SmallInt(None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => Value::Int(None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)) => Value::BigInt(None),
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => Value::Decimal(None),
        FieldType::Primitive(PrimitiveType::Boolean) => Value::Bool(None),
        FieldType::Primitive(PrimitiveType::Date) => Value::ChronoDate(None),
        FieldType::Primitive(PrimitiveType::DateTime) => Value::ChronoDateTimeUtc(None),
        FieldType::Email | FieldType::Url => Value::String(None),
        FieldType::LocalizedText | FieldType::Json => Value::Json(None),
    }
}
