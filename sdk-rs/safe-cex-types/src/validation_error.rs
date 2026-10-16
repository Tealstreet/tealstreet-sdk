//! Runtime validation errors preserve JSON values that violate the TS string declaration.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// No String(value) coercion: a numeric or object error stays numeric or an object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AccountValidationError(Value);
impl AccountValidationError {
    pub fn from_value(value: Value) -> Self {
        Self(value)
    }
    pub fn as_value(&self) -> &Value {
        &self.0
    }
    pub fn into_value(self) -> Value {
        self.0
    }
}
impl From<String> for AccountValidationError {
    fn from(value: String) -> Self {
        Self(Value::String(value))
    }
}
impl From<&str> for AccountValidationError {
    fn from(value: &str) -> Self {
        Self(Value::String(value.to_owned()))
    }
}
impl<'de> Deserialize<'de> for AccountValidationError {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::retained_json::deserialize_retained_json(deserializer).map(Self)
    }
}
