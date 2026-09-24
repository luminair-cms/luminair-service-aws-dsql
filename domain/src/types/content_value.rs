use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::domain_value::DomainValue;
use crate::value_objects::LocaleId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}
