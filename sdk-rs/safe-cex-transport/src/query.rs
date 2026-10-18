//! Query rendering is distinct from the JSON projection used by remote protocols.
use safe_cex_num::{JsCoercionError, JsRootCollator};
use safe_cex_types::{QueryValue, retained_json::RetainedJsonInput};
use serde::{
    Deserialize, Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryArrayFormat {
    Indexed,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKeyOrder {
    Preserve,
    JsRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryProfile {
    pub arrays: QueryArrayFormat,
    pub encode: bool,
    pub key_order: QueryKeyOrder,
}

pub fn render_query(value: &QueryValue, profile: QueryProfile) -> Result<String, JsCoercionError> {
    fn encode(value: &str, enabled: bool) -> String {
        if !enabled {
            return value.into();
        }
        let mut output = String::new();
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                output.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(output, "%{byte:02X}").expect("writing to String");
            }
        }
        output
    }
    // Sort views only. Retained numbers, key order and the producer's arrays
    // must survive URL rendering and failure restoration unchanged.
    fn entries<'a>(
        value: &'a QueryValue,
        collator: Option<&JsRootCollator>,
    ) -> Vec<(String, &'a QueryValue)> {
        let mut entries: Vec<_> = match value {
            QueryValue::Object(values) => values
                .iter()
                .map(|(key, value)| (key.clone(), value))
                .collect(),
            QueryValue::Array(values) => values
                .iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value))
                .collect(),
            _ => Vec::new(),
        };
        if let Some(collator) = collator {
            entries.sort_by(|(left, _), (right, _)| collator.compare(left, right));
        }
        entries
    }
    fn visit(
        value: &QueryValue,
        key: &str,
        profile: QueryProfile,
        collator: Option<&JsRootCollator>,
        output: &mut Vec<String>,
    ) {
        let scalar = match value {
            QueryValue::Undefined => return,
            QueryValue::Null => String::new(),
            QueryValue::Bool(value) => value.to_string(),
            QueryValue::Number(value) => safe_cex_num::js_number_string(*value),
            QueryValue::String(value) => value.clone(),
            QueryValue::Array(_) | QueryValue::Object(_) => {
                for (name, child) in entries(value, collator) {
                    let next = if matches!(value, QueryValue::Array(_))
                        && profile.arrays == QueryArrayFormat::Repeat
                    {
                        key.to_owned()
                    } else {
                        format!("{key}[{name}]")
                    };
                    visit(child, &next, profile, collator, output);
                }
                return;
            }
        };
        output.push(format!(
            "{}={}",
            encode(key, profile.encode),
            encode(&scalar, profile.encode)
        ));
    }
    let collator = match profile.key_order {
        QueryKeyOrder::Preserve => None,
        QueryKeyOrder::JsRoot => Some(JsRootCollator::try_new()?),
    };
    let mut output = Vec::new();
    for (key, child) in entries(value, collator.as_ref()) {
        visit(child, &key, profile, collator.as_ref(), &mut output);
    }
    Ok(output.join("&"))
}

pub fn query_truthy(value: &QueryValue) -> bool {
    match value {
        QueryValue::Undefined | QueryValue::Null => false,
        QueryValue::Bool(value) => *value,
        QueryValue::Number(value) => *value != 0.0 && !value.is_nan(),
        QueryValue::String(value) => !value.is_empty(),
        QueryValue::Array(_) | QueryValue::Object(_) => true,
    }
}

/// Retry restoration compares retained values, including NaN and the sign of zero.
pub fn query_identity_eq(left: &QueryValue, right: &QueryValue) -> bool {
    match (left, right) {
        (QueryValue::Number(left), QueryValue::Number(right)) => left.to_bits() == right.to_bits(),
        (QueryValue::Array(left), QueryValue::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| query_identity_eq(left, right))
        }
        (QueryValue::Object(left), QueryValue::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|((lk, lv), (rk, rv))| lk == rk && query_identity_eq(lv, rv))
        }
        _ => left == right,
    }
}

/// JSON.stringify at an actual remote protocol or diagnostic boundary only.
/// Undefined object properties are omitted; array slots and nonfinite numbers are null.
pub fn query_json_projection(value: &QueryValue) -> Option<Value> {
    Some(match value {
        QueryValue::Undefined => return None,
        QueryValue::Null => Value::Null,
        QueryValue::Bool(value) => Value::Bool(*value),
        QueryValue::Number(value) => {
            if !value.is_finite() {
                Value::Null
            } else if *value == 0.0 {
                serde_json::json!(0)
            } else {
                serde_json::from_str(&safe_cex_num::js_number_string(*value))
                    .expect("finite JS number")
            }
        }
        QueryValue::String(value) => Value::String(value.clone()),
        QueryValue::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| query_json_projection(value).unwrap_or(Value::Null))
                .collect(),
        ),
        QueryValue::Object(values) => Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    query_json_projection(value).map(|value| (key.clone(), value))
                })
                .collect(),
        ),
    })
}

struct QueryWire<'a>(&'a QueryValue);
impl Serialize for QueryWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            QueryValue::Undefined | QueryValue::Null => serializer.serialize_none(),
            QueryValue::Bool(value) => serializer.serialize_bool(*value),
            QueryValue::String(value) => serializer.serialize_str(value),
            QueryValue::Number(value) if !value.is_finite() => serializer.serialize_none(),
            QueryValue::Number(value) => {
                // Retain JS exponent spelling in physical wire bytes, not Value.
                let raw = serde_json::value::RawValue::from_string(safe_cex_num::js_number_string(
                    *value,
                ))
                .map_err(serde::ser::Error::custom)?;
                raw.serialize(serializer)
            }
            QueryValue::Array(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(&QueryWire(value))?;
                }
                seq.end()
            }
            QueryValue::Object(values) => {
                let mut map = serializer.serialize_map(None)?;
                for (key, value) in values {
                    if !matches!(value, QueryValue::Undefined) {
                        map.serialize_entry(key, &QueryWire(value))?;
                    }
                }
                map.end()
            }
        }
    }
}

pub(crate) fn serialize_query_wire<S: Serializer>(
    value: &Option<QueryValue>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => QueryWire(value).serialize(serializer),
        None => serializer.serialize_none(),
    }
}
pub(crate) fn query_wire_omitted(value: &Option<QueryValue>) -> bool {
    matches!(value, None | Some(QueryValue::Undefined))
}

fn from_wire_capture_at(value: RetainedJsonInput, depth: usize) -> Result<QueryValue, String> {
    if depth >= 128 {
        return Err("Query wire nesting exceeds the canonical 128-depth budget".into());
    }
    Ok(match value {
        RetainedJsonInput::Undefined => QueryValue::Undefined,
        RetainedJsonInput::Null => QueryValue::Null,
        RetainedJsonInput::Bool(value) => QueryValue::Bool(value),
        RetainedJsonInput::Text(value) => {
            QueryValue::String(String::from_utf16(&value).map_err(|error| error.to_string())?)
        }
        RetainedJsonInput::Float(value) => QueryValue::Number(value),
        RetainedJsonInput::Lexeme(value) => {
            QueryValue::Number(value.parse::<f64>().map_err(|error| error.to_string())?)
        }
        RetainedJsonInput::Array(values) => QueryValue::Array(
            values
                .into_iter()
                .map(|value| from_wire_capture_at(value, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        // JsObjectMap::collect -> insert orders integer keys before ordinary keys.
        RetainedJsonInput::Object(values) => QueryValue::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        String::from_utf16(&key).map_err(|error| error.to_string())?,
                        from_wire_capture_at(value, depth + 1)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
    })
}

pub(crate) fn deserialize_query_wire<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<QueryValue>, D::Error> {
    from_wire_capture_at(RetainedJsonInput::deserialize(deserializer)?, 0)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub fn try_parse_query_json(value: QueryValue) -> QueryValue {
    if let QueryValue::String(text) = &value
        && let Ok(capture @ (RetainedJsonInput::Array(_) | RetainedJsonInput::Object(_))) =
            serde_json::from_str::<RetainedJsonInput>(text)
        && let Ok(parsed) = from_wire_capture_at(capture, 0)
    {
        return parsed;
    }
    value
}

/// Axios buildURL defaults. Native numbers stay distinct until the output pair
/// is converted to text; metadata-token values use their JSON projection.
pub fn axios_default_query(
    value: &safe_cex_num::JsJsonValue,
) -> Result<String, crate::TransportError> {
    use crate::TransportError;
    use safe_cex_num::JsJsonValue as Value;
    fn axios_encode(text: &str) -> String {
        let mut output = String::new();
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'():$,".contains(&byte) {
                output.push(char::from(byte));
            } else if byte == b' ' {
                output.push('+');
            } else {
                use std::fmt::Write;
                write!(output, "%{byte:02X}").expect("writing to String");
            }
        }
        output
    }

    fn axios_depth_error(depth: usize) -> TransportError {
        TransportError::Host(format!(
            "Object is too deeply nested ({depth} levels). Max depth: 100"
        ))
    }
    fn axios_check_depth(value: &Value, depth: usize) -> Result<(), TransportError> {
        let children = axios_entries(value);
        if matches!(value, Value::Array(_) | Value::Object(_)) {
            if depth > 100 {
                return Err(axios_depth_error(depth));
            }
            for (_, child) in children {
                axios_check_depth(child, depth + 1)?;
            }
        }
        Ok(())
    }
    fn axios_entries(value: &Value) -> Vec<(String, &Value)> {
        match value {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            Value::Object(items) => {
                let mut values = indexmap::IndexMap::new();
                for (key, value) in items {
                    values.insert(key.as_str(), value);
                }
                safe_cex_num::js_own_keys(values.keys().copied())
                    .into_iter()
                    .map(|key| (key.to_owned(), values[key]))
                    .collect()
            }
            _ => Vec::new(),
        }
    }
    fn axios_scalar(value: &Value) -> Result<String, TransportError> {
        Ok(match value {
            Value::Undefined => "undefined".into(),
            Value::Null => "null".into(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => safe_cex_num::js_number_string(*value),
            Value::String(value) => value.clone(),
            Value::BigInt(value) => value.to_string(),
            Value::Array(items) => items
                .iter()
                .map(|value| {
                    if matches!(value, Value::Null | Value::Undefined) {
                        Ok(String::new())
                    } else {
                        axios_scalar(value)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
            Value::Object(items) => {
                let shadowed = items.iter().any(|(key, _)| key == "toString");
                let object = if shadowed {
                    serde_json::json!({"toString":null})
                } else {
                    serde_json::json!({})
                };
                safe_cex_num::js_string(Some(&object)).map_err(TransportError::JavaScript)?
            }
        })
    }
    fn axios_visit(
        value: &Value,
        path: &[String],
        pairs: &mut Vec<(String, String)>,
    ) -> Result<(), TransportError> {
        if path.len() > 100 {
            return Err(axios_depth_error(path.len()));
        }
        for (original_key, item) in axios_entries(value) {
            if matches!(item, Value::Null | Value::Undefined) {
                continue;
            }
            let key = safe_cex_num::js_trim(&original_key);
            let compound = matches!(item, Value::Array(_) | Value::Object(_));
            if path.is_empty() && compound {
                if key.ends_with("{}") {
                    axios_check_depth(item, 1)?;
                    let text = safe_cex_num::js_json_stringify(item)
                        .map_err(TransportError::JavaScript)?
                        .expect("compound defined");
                    pairs.push((key.to_owned(), text));
                    continue;
                }
                let flat = matches!(item, Value::Array(values) if values.iter().all(|v| !matches!(v, Value::Array(_) | Value::Object(_))));
                if flat || key.ends_with("[]") {
                    let array_length = match item {
                        Value::Array(values) => Some(values.len()),
                        Value::Object(_) => axios_entries(item)
                            .into_iter()
                            .find(|(name, _)| name == "length")
                            .and_then(|(_, value)| {
                                if let Value::Number(n) = value {
                                    Some(*n)
                                } else {
                                    None
                                }
                            })
                            .map(|n| {
                                if !n.is_finite()
                                    || n < 0.0
                                    || n.fract() != 0.0
                                    || n > u32::MAX as f64
                                {
                                    Err(TransportError::Host("Invalid array length".into()))
                                } else {
                                    Ok(n as usize)
                                }
                            })
                            .transpose()?,
                        _ => None,
                    };
                    if let Some(length) = array_length {
                        let name = format!("{}[]", key.strip_suffix("[]").unwrap_or(key));
                        for (index, child) in axios_entries(item) {
                            if index
                                .parse::<usize>()
                                .is_ok_and(|i| i < length && i.to_string() == index)
                                && !matches!(child, Value::Null | Value::Undefined)
                            {
                                pairs.push((name.clone(), axios_scalar(child)?));
                            }
                        }
                        continue;
                    }
                }
            }
            if compound {
                let mut next = path.to_vec();
                next.push(original_key);
                axios_visit(item, &next, pairs)?;
            } else {
                let name = if path.is_empty() {
                    key.to_owned()
                } else {
                    path.iter()
                        .map(String::as_str)
                        .chain(std::iter::once(key))
                        .enumerate()
                        .map(|(i, token)| {
                            let token = token.strip_suffix("[]").unwrap_or(token);
                            if i == 0 {
                                token.to_owned()
                            } else {
                                format!("[{token}]")
                            }
                        })
                        .collect()
                };
                pairs.push((name, axios_scalar(item)?));
            }
        }
        Ok(())
    }
    if !matches!(value, Value::Array(_) | Value::Object(_)) {
        let falsy = matches!(value, Value::Null | Value::Undefined | Value::Bool(false))
            || matches!(value, Value::Number(n) if *n == 0.0 || n.is_nan())
            || matches!(value, Value::String(text) if text.is_empty());
        if falsy {
            return Ok(String::new());
        }
        return Err(TransportError::JavaScript(safe_cex_num::JsCoercionError {
            name: "TypeError",
            message: "data must be an object",
        }));
    }
    let mut pairs = Vec::new();
    axios_visit(value, &[], &mut pairs)?;
    Ok(pairs
        .into_iter()
        .map(|(key, value)| format!("{}={}", axios_encode(&key), axios_encode(&value)))
        .collect::<Vec<_>>()
        .join("&"))
}
