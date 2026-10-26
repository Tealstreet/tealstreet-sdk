//! Distinct JavaScript coercions used at venue JSON boundaries.
use crate::js_whitespace;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
pub use safe_cex_types::js_coercion::{JsCoercionError, js_string};
/// parseFloat's longest decimal PREFIX, not Number's full-string conversion.
pub fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(js_whitespace);
    let bytes = text.as_bytes();
    let mut at = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    if text[at..].starts_with("Infinity") {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mut digits = 0;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
        digits += 1;
    }
    if at < bytes.len() && bytes[at] == b'.' {
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if at < bytes.len() && matches!(bytes[at], b'e' | b'E') {
        let end = at;
        at += 1;
        if at < bytes.len() && matches!(bytes[at], b'+' | b'-') {
            at += 1;
        }
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if at == start {
            at = end;
        }
    }
    text[..at].parse().unwrap_or(f64::NAN)
}
/// parseInt's signed integer prefix. None/0 infer decimal, except a 0x prefix.
pub fn parse_int(text: &str, radix: Option<i32>) -> f64 {
    let text = text.trim_start_matches(js_whitespace);
    let (negative, mut rest) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let mut radix = radix.unwrap_or(0);
    if radix != 0 && !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    if radix == 0 {
        radix = if rest.starts_with("0x") || rest.starts_with("0X") {
            16
        } else {
            10
        };
    }
    if radix == 16 && (rest.starts_with("0x") || rest.starts_with("0X")) {
        rest = &rest[2..];
    }
    let end = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii() && c.is_digit(radix as u32))
        .last()
        .map_or(0, |(at, c)| at + c.len_utf8());
    if end == 0 {
        return f64::NAN;
    }
    let value = if radix == 10 || (radix as u32).is_power_of_two() {
        BigUint::parse_bytes(&rest.as_bytes()[..end], radix as u32)
            .unwrap()
            .to_f64()
            .unwrap_or(f64::INFINITY)
    } else {
        // V8 13.6 measured generic-radix approximation: bounded 32-bit chunks.
        // https://github.com/v8/v8/blob/13.6.233/src/numbers/conversions.cc
        let digits = rest[..end].trim_start_matches('0');
        let mut result = 0.0_f64;
        let mut chars = digits.chars().peekable();
        while chars.peek().is_some() {
            let (mut part, mut multiplier) = (0_u32, 1_u32);
            while let Some(c) = chars.peek() {
                let next = multiplier * radix as u32;
                if next > u32::MAX / 36 {
                    break;
                }
                part = part * radix as u32 + c.to_digit(radix as u32).unwrap();
                multiplier = next;
                chars.next();
            }
            result = result.mul_add(f64::from(multiplier), f64::from(part));
        }
        result
    };
    if negative { -value } else { value }
}

/// JavaScript Boolean(any) on JSON values; None represents undefined.
/// Arrays and objects are true even when their String conversion is empty.
pub fn js_truthy(value: Option<&serde_json::Value>) -> bool {
    use serde_json::Value;
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => {
            let number = value
                .as_f64()
                .unwrap_or_else(|| value.to_string().parse().unwrap_or(f64::NAN));
            number != 0.0 && !number.is_nan()
        }
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// Number(any) / unary + on the JSON wire domain; None is undefined.
/// JSON cannot hold callable hooks: own valueOf is skipped, own toString throws.
pub fn js_number(value: Option<&serde_json::Value>) -> Result<f64, JsCoercionError> {
    use serde_json::Value;
    Ok(match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Number(value)) => value
            .as_f64()
            .unwrap_or_else(|| value.to_string().parse().unwrap_or(f64::NAN)),
        Some(Value::String(value)) => crate::number_from_string(value),
        Some(value @ (Value::Array(_) | Value::Object(_))) => {
            crate::number_from_string(&js_string(Some(value))?)
        }
    })
}

/// Number(any) over native retained data, without a JSON projection.
pub fn js_retained_number(
    value: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<f64, JsCoercionError> {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    Ok(match value {
        None | Some(Input::Undefined) => f64::NAN,
        Some(Input::Null) => 0.0,
        Some(Input::Bool(value)) => f64::from(u8::from(*value)),
        Some(Input::Float(value)) => *value,
        Some(Input::Lexeme(value)) => value.parse().unwrap_or(f64::NAN),
        Some(value) => {
            let units = safe_cex_types::js_coercion::js_retained_string_utf16(value)?;
            String::from_utf16(&units).map_or(f64::NAN, |text| crate::number_from_string(&text))
        }
    })
}

/// String(any) at a caller's explicit UTF-8 string boundary.
pub fn js_retained_string(
    value: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<String, JsCoercionError> {
    use safe_cex_types::{js_coercion::js_retained_string_utf16, retained_json::RetainedJsonInput};
    let units = js_retained_string_utf16(value.unwrap_or(&RetainedJsonInput::Undefined))?;
    String::from_utf16(&units).map_err(|_| JsCoercionError {
        name: "TypeError",
        message: "Cannot represent JavaScript UTF-16 string as UTF-8",
    })
}

/// parseFloat stops at every surrogate just as it stops at other nonnumeric units.
pub fn js_retained_parse_float(
    value: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<f64, JsCoercionError> {
    use safe_cex_types::{js_coercion::js_retained_string_utf16, retained_json::RetainedJsonInput};
    let units = js_retained_string_utf16(value.unwrap_or(&RetainedJsonInput::Undefined))?;
    Ok(parse_float(&String::from_utf16_lossy(&units)))
}

/// Primitive results of host-provided JavaScript object conversion hooks.
#[derive(Debug, Clone, PartialEq)]
pub enum JsPrimitive {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    BigInt(num_bigint::BigInt),
    Symbol,
}

/// Adapter for callable hooks; None means absent/non-callable or an object result.
/// Hosts preserve thrown errors in Result and invoke callable hooks themselves.
pub trait JsNumberObject {
    /// None means no exotic hook; a non-callable hook or object result must error.
    fn to_primitive(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError> {
        Ok(None)
    }
    fn value_of(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError>;
    fn to_string(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError>;
}

/// Number's explicit BigInt conversion differs from abstract ToNumber / unary +.
pub fn js_number_primitive(value: &JsPrimitive) -> Result<f64, JsCoercionError> {
    Ok(match value {
        JsPrimitive::Undefined => f64::NAN,
        JsPrimitive::Null => 0.0,
        JsPrimitive::Boolean(value) => f64::from(u8::from(*value)),
        JsPrimitive::Number(value) => *value,
        JsPrimitive::String(value) => crate::number_from_string(value),
        JsPrimitive::BigInt(value) => value.to_f64().unwrap_or_else(|| {
            if value.sign() == num_bigint::Sign::Minus {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        }),
        JsPrimitive::Symbol => {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot convert a Symbol value to a number",
            });
        }
    })
}

/// Number(object), with ordinary number-hint valueOf then toString ordering.
/// Symbol.toPrimitive, when present, precedes the ordinary hooks.
pub fn js_number_object(value: &mut impl JsNumberObject) -> Result<f64, JsCoercionError> {
    js_number_primitive(&number_object_primitive(value)?)
}

pub(crate) fn number_object_primitive(
    value: &mut impl JsNumberObject,
) -> Result<JsPrimitive, JsCoercionError> {
    if let Some(value) = value.to_primitive()? {
        return Ok(value);
    }
    if let Some(value) = value.value_of()? {
        return Ok(value);
    }
    if let Some(value) = value.to_string()? {
        return Ok(value);
    }
    Err(JsCoercionError {
        name: "TypeError",
        message: "Cannot convert object to primitive value",
    })
}

/// Abstract ToNumber / unary +; unlike Number(), BigInt is a TypeError.
pub fn js_to_number(value: &JsPrimitive) -> Result<f64, JsCoercionError> {
    if matches!(value, JsPrimitive::BigInt(_)) {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot convert a BigInt value to a number",
        });
    }
    js_number_primitive(value)
}

pub fn js_to_number_object(value: &mut impl JsNumberObject) -> Result<f64, JsCoercionError> {
    js_to_number(&number_object_primitive(value)?)
}
