//! Raw JSON ingress for spot wire models. UTF-16 strings and binary64 numbers
//! remain native values; serde_json::Value and other formats are not ingress APIs.
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsString(pub Vec<u16>);

pub(crate) fn string_token(token: &str) -> Result<Vec<u16>, &'static str> {
    if !token.starts_with('"') || !token.ends_with('"') {
        return Err("expected a JSON string");
    }
    // RawValue has already validated JSON syntax; keep each escaped code unit
    // rather than asking a UTF-8 String visitor to reject unpaired surrogates.
    let mut chars = token[1..token.len() - 1].chars();
    let mut units = Vec::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            units.extend(character.encode_utf16(&mut [0; 2]).iter().copied());
            continue;
        }
        let escaped = chars.next().ok_or("incomplete escape")?;
        units.push(match escaped {
            '"' => 0x22,
            '\\' => 0x5c,
            '/' => 0x2f,
            'b' => 0x08,
            'f' => 0x0c,
            'n' => 0x0a,
            'r' => 0x0d,
            't' => 0x09,
            'u' => {
                let mut value = 0u16;
                for _ in 0..4 {
                    value = value * 16
                        + chars
                            .next()
                            .and_then(|c| c.to_digit(16))
                            .ok_or("invalid Unicode escape")? as u16;
                }
                value
            }
            _ => return Err("invalid JSON escape"),
        });
    }
    Ok(units)
}
impl<'de> Deserialize<'de> for JsString {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(decoder)?;
        let units = string_token(raw.get()).map_err(serde::de::Error::custom)?;
        Ok(Self(units))
    }
}
impl Serialize for JsString {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        let mut text = String::from("\"");
        for unit in &self.0 {
            use std::fmt::Write;
            write!(text, "\\u{unit:04x}").unwrap();
        }
        text.push('"');
        RawValue::from_string(text)
            .map_err(serde::ser::Error::custom)?
            .serialize(encoder)
    }
}

pub(crate) fn number_token(token: &str) -> Result<f64, &'static str> {
    if !matches!(token.as_bytes().first(), Some(b'-' | b'0'..=b'9')) {
        return Err("expected a JSON number");
    }
    token.parse().map_err(|_| "invalid binary64 number")
}
pub(crate) fn number<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<f64, D::Error> {
    let raw = Box::<RawValue>::deserialize(decoder)?;
    number_token(raw.get()).map_err(serde::de::Error::custom)
}
pub(crate) fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

/// Retain declared fields from a validated raw object without decoding its keys
/// through UTF-8. JSON.parse takes the last duplicate value, while preserving
/// the key's first insertion. Unknown properties are outside the model graph.
pub(crate) fn object_fields(token: &str, known: &[&str]) -> Result<String, String> {
    let bytes = token.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("expected a spot object record".into());
    }
    let mut offset = 1;
    let skip = |offset: &mut usize| {
        while matches!(bytes.get(*offset), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            *offset += 1;
        }
    };
    let mut fields: Vec<(&str, String)> = Vec::new();
    loop {
        skip(&mut offset);
        if bytes.get(offset) == Some(&b'}') {
            break;
        }
        if bytes.get(offset) != Some(&b'"') {
            return Err("expected an object key".into());
        }
        let start = offset;
        offset += 1;
        while let Some(character) = bytes.get(offset) {
            offset += 1;
            if *character == b'\\' {
                offset += 1;
            } else if *character == b'"' {
                break;
            }
        }
        let key = string_token(&token[start..offset]).map_err(str::to_owned)?;
        skip(&mut offset);
        if bytes.get(offset) != Some(&b':') {
            return Err("expected key/value separator".into());
        }
        offset += 1;
        skip(&mut offset);
        let mut decoder = serde_json::Deserializer::from_str(&token[offset..]);
        let value =
            Box::<RawValue>::deserialize(&mut decoder).map_err(|error| error.to_string())?;
        offset += value.get().len();
        if let Some(name) = known
            .iter()
            .find(|name| name.encode_utf16().eq(key.iter().copied()))
        {
            if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == name) {
                *old = value.get().to_owned();
            } else {
                fields.push((name, value.get().to_owned()));
            }
        }
        skip(&mut offset);
        match bytes.get(offset) {
            Some(b',') => offset += 1,
            Some(b'}') => break,
            _ => return Err("expected object separator".into()),
        }
    }
    Ok(format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(name, value)| format!("{}:{value}", serde_json::to_string(name).unwrap()))
            .collect::<Vec<_>>()
            .join(",")
    ))
}
