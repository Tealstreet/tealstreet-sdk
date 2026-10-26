//! BigInt conversion preserves RangeError, SyntaxError and TypeError identity.
use crate::{JsCoercionError, JsNumberObject, JsPrimitive, js_number_string, js_string, js_trim};
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsBigIntErrorKind {
    RangeError,
    SyntaxError,
    TypeError,
    HostError(&'static str),
}
impl JsBigIntErrorKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::RangeError => "RangeError",
            Self::SyntaxError => "SyntaxError",
            Self::TypeError => "TypeError",
            Self::HostError(name) => name,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsBigIntError {
    pub kind: JsBigIntErrorKind,
    /// UTF-8 display view; use message_utf16 for exact JavaScript code units.
    pub message: String,
    pub message_utf16: Vec<u16>,
}
impl std::fmt::Display for JsBigIntError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.message)
    }
}
impl std::error::Error for JsBigIntError {}
impl From<JsCoercionError> for JsBigIntError {
    fn from(error: JsCoercionError) -> Self {
        Self {
            kind: match error.name {
                "RangeError" => JsBigIntErrorKind::RangeError,
                "SyntaxError" => JsBigIntErrorKind::SyntaxError,
                "TypeError" => JsBigIntErrorKind::TypeError,
                name => JsBigIntErrorKind::HostError(name),
            },
            message: error.message.into(),
            message_utf16: error.message.encode_utf16().collect(),
        }
    }
}

fn string_bigint(input: &str) -> Result<BigInt, JsBigIntError> {
    let text = js_trim(input);
    if text.is_empty() {
        return Ok(BigInt::from(0));
    }
    let mut radix = 10;
    let mut digits = text;
    for (prefix, base) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            radix = base;
            digits = rest;
            break;
        }
    }
    let unsigned = if radix == 10 {
        digits.strip_prefix(['+', '-']).unwrap_or(digits)
    } else {
        digits
    };
    if !unsigned.is_empty()
        && unsigned.chars().all(|c| c.is_ascii() && c.is_digit(radix))
        && let Some(value) = BigInt::parse_bytes(digits.as_bytes(), radix)
    {
        return Ok(value);
    }
    let mut units: Vec<u16> = "Cannot convert ".encode_utf16().collect();
    let mut input_units = input.encode_utf16();
    units.extend(input_units.by_ref().take(1000));
    if input_units.next().is_some() {
        units.push(0x2026);
    }
    units.extend(" to a BigInt".encode_utf16());
    Err(JsBigIntError {
        kind: JsBigIntErrorKind::SyntaxError,
        message: String::from_utf16_lossy(&units),
        message_utf16: units,
    })
}

/// BigInt() on JSON values; None is undefined, never null or an empty string.
pub fn js_bigint(value: Option<&Value>) -> Result<BigInt, JsBigIntError> {
    match value {
        None => js_bigint_primitive(&JsPrimitive::Undefined),
        Some(Value::Null) => js_bigint_primitive(&JsPrimitive::Null),
        Some(Value::Bool(v)) => js_bigint_primitive(&JsPrimitive::Boolean(*v)),
        Some(Value::Number(v)) => js_bigint_primitive(&JsPrimitive::Number(
            v.as_f64()
                .unwrap_or_else(|| v.to_string().parse().unwrap_or(f64::NAN)),
        )),
        Some(Value::String(v)) => string_bigint(v),
        Some(value) => string_bigint(&js_string(Some(value))?),
    }
}

pub fn js_bigint_primitive(value: &JsPrimitive) -> Result<BigInt, JsBigIntError> {
    match value {
        JsPrimitive::BigInt(v) => Ok(v.clone()),
        JsPrimitive::Boolean(v) => Ok(BigInt::from(u8::from(*v))),
        JsPrimitive::String(v) => string_bigint(v),
        JsPrimitive::Number(v) => {
            if v.is_finite()
                && v.fract() == 0.0
                && let Some(value) = BigInt::from_f64(*v)
            {
                return Ok(value);
            }
            let message = format!(
                "The number {} cannot be converted to a BigInt because it is not an integer",
                js_number_string(*v)
            );
            Err(JsBigIntError {
                kind: JsBigIntErrorKind::RangeError,
                message_utf16: message.encode_utf16().collect(),
                message,
            })
        }
        value => {
            let name = match value {
                JsPrimitive::Undefined => "undefined",
                JsPrimitive::Null => "null",
                _ => "Symbol()",
            };
            let message = format!("Cannot convert {name} to a BigInt");
            Err(JsBigIntError {
                kind: JsBigIntErrorKind::TypeError,
                message_utf16: message.encode_utf16().collect(),
                message,
            })
        }
    }
}

/// Numeric-hint object conversion uses the same hook ordering as Number().
pub fn js_bigint_object(value: &mut impl JsNumberObject) -> Result<BigInt, JsBigIntError> {
    js_bigint_primitive(&crate::coercion::number_object_primitive(value)?)
}
