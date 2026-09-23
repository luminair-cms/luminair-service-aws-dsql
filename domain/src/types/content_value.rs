use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::value_objects::LocaleId;
use super::domain_value::DomainValue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentValue {
    Scalar(DomainValue),
    LocalizedText(HashMap<LocaleId, String>),
    Null,
}
