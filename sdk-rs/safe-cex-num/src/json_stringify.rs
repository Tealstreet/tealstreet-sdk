//! JSON.stringify for prepared data: preserve undefined until wire serialization.
use crate::{JsCoercionError, js_number_string, js_own_keys};
use serde_json::Value;
use std::collections::HashMap;

/// Owned JSON data plus values that serde_json cannot retain.
/// Object entries follow JS assignment: last value wins, first insertion stays.
#[derive(Debug, Clone, PartialEq)]
pub enum JsJsonValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsJsonValue>),
    Object(Vec<(String, JsJsonValue)>),
    BigInt(num_bigint::BigInt),
}

impl JsJsonValue {
    /// Boolean conversion before JSON serialization can omit undefined or
    /// replace nonfinite numbers with null.
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::Array(_) | Self::Object(_) => true,
            Self::BigInt(value) => value != &num_bigint::BigInt::from(0),
        }
    }
}

impl From<Value> for JsJsonValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::Bool(v),
            Value::Number(v) => Self::Number(
                v.as_f64()
                    .unwrap_or_else(|| v.to_string().parse().unwrap_or(f64::NAN)),
            ),
            Value::String(v) => Self::String(v),
            Value::Array(v) => Self::Array(v.into_iter().map(Self::from).collect()),
            Value::Object(v) => {
                Self::Object(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
        }
    }
}

/// Compact JSON.stringify without replacer/space/toJSON callbacks or cycles.
/// None is a top-level undefined result; undefined array slots become null.
pub fn js_json_stringify(value: &JsJsonValue) -> Result<Option<String>, JsCoercionError> {
    if matches!(value, JsJsonValue::Undefined) {
        return Ok(None);
    }
    let mut output = String::new();
    write(value, &mut output)?;
    Ok(Some(output))
}

fn quoted_utf16(value: &[u16], output: &mut String) {
    let mut text = Vec::new();
    safe_cex_types::js_coercion::json_quote_utf16(value, &mut text);
    output.push_str(&String::from_utf16(&text).expect("well-formed JSON quotation"));
}
fn quoted(value: &str, output: &mut String) -> Result<(), JsCoercionError> {
    quoted_utf16(&value.encode_utf16().collect::<Vec<_>>(), output);
    Ok(())
}
fn number(value: f64, output: &mut String) {
    if value.is_finite() {
        output.push_str(&js_number_string(value));
    } else {
        output.push_str("null");
    }
}

fn write(value: &JsJsonValue, output: &mut String) -> Result<(), JsCoercionError> {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => output.push_str("null"),
        JsJsonValue::Bool(v) => output.push_str(if *v { "true" } else { "false" }),
        JsJsonValue::Number(v) => {
            number(*v, output);
        }
        JsJsonValue::String(v) => quoted(v, output)?,
        JsJsonValue::Array(values) => {
            output.push('[');
            for (i, value) in values.iter().enumerate() {
                if i != 0 {
                    output.push(',');
                }
                write(value, output)?;
            }
            output.push(']');
        }
        JsJsonValue::Object(entries) => {
            let mut keys = Vec::new();
            let mut values = HashMap::new();
            for (key, value) in entries {
                if values.insert(key.as_str(), value).is_none() {
                    keys.push(key.as_str());
                }
            }
            output.push('{');
            let mut first = true;
            for key in js_own_keys(keys) {
                let value = values[key];
                if matches!(value, JsJsonValue::Undefined) {
                    continue;
                }
                if !first {
                    output.push(',');
                }
                first = false;
                quoted(key, output)?;
                output.push(':');
                write(value, output)?;
            }
            output.push('}');
        }
        JsJsonValue::BigInt(_) => {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Do not know how to serialize a BigInt",
            });
        }
    }
    Ok(())
}

/// Boolean conversion of canonical retained data before JSON wire projection.
pub fn js_retained_truthy(value: &safe_cex_types::retained_json::RetainedJsonInput) -> bool {
    value.is_truthy()
}
/// Direct canonical retained-data JSON projection; no UTF-8 prepared-data copy.
pub fn js_retained_json_stringify(
    value: &safe_cex_types::retained_json::RetainedJsonInput,
) -> Result<Option<String>, JsCoercionError> {
    value.json_stringify()
}
