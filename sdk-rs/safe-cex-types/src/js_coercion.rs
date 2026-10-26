//! Lowest-layer JSON String(any) conversion shared by model utilities and num.
use serde_json::Value;
/// Shared ECMAScript Number-to-String backend; num exposes the canonical API.
pub fn js_number_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsCoercionError {
    pub name: &'static str,
    pub message: &'static str,
}
impl std::fmt::Display for JsCoercionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for JsCoercionError {}
/// String(any) on the JSON wire domain; None represents undefined.
/// Callers apply their own ??/|| branch BEFORE invoking this conversion.
pub fn js_string(value: Option<&Value>) -> Result<String, JsCoercionError> {
    let Some(value) = value else {
        return Ok("undefined".into());
    };
    Ok(match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => js_number_string(
            value
                .as_f64()
                .unwrap_or_else(|| value.to_string().parse().unwrap_or(f64::NAN)),
        ),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    Ok(String::new())
                } else {
                    js_string(Some(value))
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(","),
        Value::Object(values) => object_string(values.contains_key("toString"))?.into(),
    })
}

fn object_string(has_own_to_string: bool) -> Result<&'static str, JsCoercionError> {
    if has_own_to_string {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot convert object to primitive value",
        });
    }
    Ok("[object Object]")
}

/// String(any) on retained data, before an HTTP/form encoder chooses UTF-8.
pub fn js_retained_string_utf16(
    value: &crate::retained_json::RetainedJsonInput,
) -> Result<Vec<u16>, JsCoercionError> {
    use crate::retained_json::RetainedJsonInput as Input;
    let text = match value {
        Input::Undefined => "undefined".into(),
        Input::Null => "null".into(),
        Input::Bool(value) => value.to_string(),
        Input::Float(value) => js_number_string(*value),
        Input::Lexeme(value) => js_number_string(value.parse().map_err(|_| JsCoercionError {
            name: "TypeError",
            message: "Invalid retained numeric lexeme",
        })?),
        Input::Text(value) => return Ok(value.clone()),
        Input::Array(values) => {
            let mut result = Vec::new();
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    result.push(u16::from(b','));
                }
                if !matches!(value, Input::Null | Input::Undefined) {
                    result.extend(js_retained_string_utf16(value)?);
                }
            }
            return Ok(result);
        }
        Input::Object(values) => object_string(
            values
                .iter()
                .any(|(key, _)| key.iter().copied().eq("toString".encode_utf16())),
        )?
        .into(),
    };
    Ok(text.encode_utf16().collect())
}

/// String-pattern replacement over ECMAScript UTF-16 code units.
/// No regular-expression captures: $1 and $<name> remain literal.
/// Empty needles advance by one code unit, including between surrogate halves.
pub fn js_string_replace_utf16(
    text: &[u16],
    needle: &[u16],
    replacement: &[u16],
    all: bool,
) -> Vec<u16> {
    let mut output = Vec::new();
    let mut copied = 0;
    let mut search = 0;
    loop {
        let found = if needle.is_empty() {
            (search <= text.len()).then_some(search)
        } else if search <= text.len() {
            text[search..]
                .windows(needle.len())
                .position(|window| window == needle)
                .map(|at| search + at)
        } else {
            None
        };
        let Some(at) = found else { break };
        output.extend_from_slice(&text[copied..at]);
        let mut token = 0;
        while token < replacement.len() {
            if replacement[token] == u16::from(b'$') {
                match replacement.get(token + 1).copied() {
                    Some(36) => output.push(u16::from(b'$')),
                    Some(38) => output.extend_from_slice(needle),
                    Some(96) => output.extend_from_slice(&text[..at]),
                    Some(39) => output.extend_from_slice(&text[at + needle.len()..]),
                    _ => {
                        output.push(replacement[token]);
                        token += 1;
                        continue;
                    }
                }
                token += 2;
            } else {
                output.push(replacement[token]);
                token += 1;
            }
        }
        copied = at + needle.len();
        if !all {
            break;
        }
        search = if needle.is_empty() { at + 1 } else { copied };
    }
    output.extend_from_slice(&text[copied..]);
    output
}

/// Canonical array-index property spelling, excluding the reserved 2^32 - 1 key.
pub fn js_array_index(key: &str) -> Option<u32> {
    key.parse::<u32>()
        .ok()
        .filter(|index| *index != u32::MAX && index.to_string() == key)
}

/// Enumerate unique own string keys supplied in insertion order.
/// Canonical array indices precede all other keys, ascending numerically.
pub fn js_own_keys<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort_by_key(|key| js_array_index(key).map_or((1, 0), |index| (0, index)));
    keys
}

/// Normalize an object's own-key order, without changing its values.
pub fn normalize_object_order(values: &mut serde_json::Map<String, serde_json::Value>) {
    let mut entries: Vec<_> = std::mem::take(values).into_iter().collect();
    entries.sort_by_key(|(key, _)| js_array_index(key).map_or((1, 0), |index| (0, index)));
    values.extend(entries);
}

/// Apply ordinary own-key order recursively, including objects inside arrays.
pub fn normalize_json_order(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(values) => {
            normalize_object_order(values);
            for value in values.values_mut() {
                normalize_json_order(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_json_order(value);
            }
        }
        _ => {}
    }
}

/// Well-formed JSON quoting of UTF-16, including escaped lone surrogates.
pub fn json_quote_utf16(text: &[u16], out: &mut Vec<u16>) {
    out.push(b'"' as u16);
    for (i, &c) in text.iter().enumerate() {
        let lone = match c {
            0xD800..=0xDBFF => !matches!(text.get(i + 1), Some(0xDC00..=0xDFFF)),
            0xDC00..=0xDFFF => i == 0 || !matches!(text[i - 1], 0xD800..=0xDBFF),
            _ => false,
        };
        let escaped = match c {
            0x22 => "\\\"".to_owned(),
            0x5C => "\\\\".to_owned(),
            0x08 => "\\b".to_owned(),
            0x0C => "\\f".to_owned(),
            0x0A => "\\n".to_owned(),
            0x0D => "\\r".to_owned(),
            0x09 => "\\t".to_owned(),
            c if c < 0x20 || lone => format!("\\u{c:04x}"),
            c => {
                out.push(c);
                continue;
            }
        };
        out.extend(escaped.encode_utf16());
    }
    out.push(b'"' as u16);
}

/// OrdinaryOwnPropertyKeys for retained UTF-16 keys; lone units are ordinary names.
pub fn js_own_keys_utf16<'a>(keys: impl IntoIterator<Item = &'a [u16]>) -> Vec<&'a [u16]> {
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort_by_key(|key| {
        String::from_utf16(key)
            .ok()
            .and_then(|key| js_array_index(&key))
            .map_or((1, 0), |index| (0, index))
    });
    keys
}

#[cfg(test)]
mod replacement_tests {
    use super::js_string_replace_utf16;
    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    #[test]
    fn empty_pattern_advances_by_utf16_units() {
        assert_eq!(
            js_string_replace_utf16(&[55357, 56832], &[], &[95], true),
            [95, 55357, 95, 56832, 95]
        );
        assert_eq!(
            js_string_replace_utf16(&[55357, 56832], &[], &[95], false),
            [95, 55357, 56832]
        );
        assert_eq!(
            js_string_replace_utf16(&[0xd800], &[], &[95], true),
            [95, 0xd800, 95]
        );
    }
    #[test]
    fn tokens_and_overlap_use_original_source() {
        assert_eq!(
            js_string_replace_utf16(&units("aaa"), &units("aa"), &units("X"), true),
            units("Xa")
        );
        assert_eq!(
            js_string_replace_utf16(
                &units("abc"),
                &units("b"),
                &units("$$|$&|$`|$'|$1|$<name>"),
                true
            ),
            units("a$|b|a|c|$1|$<name>c")
        );
        assert_eq!(
            js_string_replace_utf16(&units("aba"), &units("a"), &units("[$`][$']"), true),
            units("[][ba]b[ab][]")
        );
    }
}
