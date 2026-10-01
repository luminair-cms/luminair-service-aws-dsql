//! Filter expression generator for SeaQuery.

use domain::content::{DomainValue, FieldFilter, PrimitiveValue};
use sea_query::{Expr, ExprTrait, SimpleExpr};

use crate::persistence::naming::DocumentTableNaming;

/// Converts a domain `FieldFilter` into a `sea_query::SimpleExpr`.
pub fn filter_to_condition(
    naming: &DocumentTableNaming<'_>,
    filter: &FieldFilter,
) -> SimpleExpr {
    let col = naming.column_iden(&filter.attribute_id);
    match &filter.value {
        DomainValue::Primitive(PrimitiveValue::Text(s)) => Expr::col(col).eq(s.as_str()),
        DomainValue::Primitive(PrimitiveValue::Uid(s)) => Expr::col(col).eq(s.as_str()),
        DomainValue::Primitive(PrimitiveValue::Uuid(u)) => Expr::col(col).eq(*u),
        DomainValue::Primitive(PrimitiveValue::Integer(i)) => Expr::col(col).eq(*i),
        DomainValue::Primitive(PrimitiveValue::Decimal(d)) => Expr::col(col).eq(*d),
        DomainValue::Primitive(PrimitiveValue::Boolean(b)) => Expr::col(col).eq(*b),
        DomainValue::Primitive(PrimitiveValue::Date(d)) => Expr::col(col).eq(*d),
        DomainValue::Primitive(PrimitiveValue::DateTime(dt)) => Expr::col(col).eq(*dt),
        DomainValue::Email(e) => Expr::col(col).eq(e.as_ref()),
        DomainValue::Url(u) => Expr::col(col).eq(u.as_ref()),
        DomainValue::Json(map) => {
            let json = serde_json::to_value(map).unwrap_or(serde_json::Value::Null);
            Expr::col(col).eq(json)
        }
    }
}
