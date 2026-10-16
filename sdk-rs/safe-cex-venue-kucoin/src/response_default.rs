use safe_cex_transport::RestResponseBody;
use serde_json::{Map, Value};

pub(crate) fn decode(body: &RestResponseBody) -> Result<Value, String> {
    if let RestResponseBody::Transformed(value) = body {
        return Ok(value.clone());
    }
    let value = body.clone().into_data();
    let Value::String(text) = &value else {
        return Ok(value);
    };
    if text.is_empty() || serde_json::from_str::<serde::de::IgnoredAny>(text).is_err() {
        return Ok(value);
    }
    parse_valid_json(text).map_err(|error| {
        format!("KuCoin account response representation refusal at JSON/UTF-8 boundary: {error}")
    })
}

enum Frame {
    Array(Vec<Value>),
    Object(Map<String, Value>, String),
}
fn whitespace(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}
fn string(text: &str, cursor: &mut usize) -> Result<String, String> {
    let start = *cursor;
    *cursor += 1;
    let bytes = text.as_bytes();
    while let Some(byte) = bytes.get(*cursor) {
        *cursor += 1;
        match byte {
            b'\\' => *cursor += 1,
            b'"' => return serde_json::from_str(&text[start..*cursor]).map_err(|e| e.to_string()),
            _ => {}
        }
    }
    Err("unterminated JSON string".into())
}
fn key(text: &str, cursor: &mut usize) -> Result<String, String> {
    whitespace(text.as_bytes(), cursor);
    let key = string(text, cursor)?;
    whitespace(text.as_bytes(), cursor);
    *cursor += 1;
    Ok(key)
}
fn number(token: &str) -> Result<Value, String> {
    let parsed = token.parse::<f64>().map_err(|e| e.to_string())?;
    let spelling = if parsed.is_finite() && !(parsed == 0.0 && parsed.is_sign_negative()) {
        safe_cex_num::js_number_string(parsed)
    } else {
        token.to_owned()
    };
    serde_json::from_str(&spelling).map_err(|e| e.to_string())
}
fn parse_valid_json(text: &str) -> Result<Value, String> {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut stack = Vec::new();
    loop {
        whitespace(bytes, &mut cursor);
        let mut value = match bytes[cursor] {
            b'[' | b'{' => {
                let object = bytes[cursor] == b'{';
                cursor += 1;
                whitespace(bytes, &mut cursor);
                if bytes[cursor] == if object { b'}' } else { b']' } {
                    cursor += 1;
                    if object {
                        Value::Object(Map::new())
                    } else {
                        Value::Array(Vec::new())
                    }
                } else {
                    stack.push(if object {
                        Frame::Object(Map::new(), key(text, &mut cursor)?)
                    } else {
                        Frame::Array(Vec::new())
                    });
                    continue;
                }
            }
            b'"' => Value::String(string(text, &mut cursor)?),
            _ => {
                let start = cursor;
                while bytes
                    .get(cursor)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
                {
                    cursor += 1;
                }
                match &text[start..cursor] {
                    "null" => Value::Null,
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    token => number(token)?,
                }
            }
        };
        loop {
            let Some(frame) = stack.last_mut() else {
                return Ok(value);
            };
            match frame {
                Frame::Array(values) => values.push(value),
                Frame::Object(values, key) => {
                    values.insert(std::mem::take(key), value);
                }
            }
            whitespace(bytes, &mut cursor);
            let separator = bytes[cursor];
            cursor += 1;
            if separator == b',' {
                if let Frame::Object(_, next_key) = stack.last_mut().unwrap() {
                    *next_key = key(text, &mut cursor)?;
                }
                break;
            }
            value = match stack.pop().unwrap() {
                Frame::Array(values) => Value::Array(values),
                Frame::Object(mut values, _) => {
                    safe_cex_types::js_coercion::normalize_object_order(&mut values);
                    Value::Object(values)
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(text: &str) -> Value {
        decode(&RestResponseBody::Encoded(text.as_bytes().to_vec())).unwrap()
    }
    #[test]
    fn native_axios_transform_floor() {
        assert_eq!(raw(""), Value::String("".into()));
        assert_eq!(raw("not-json"), Value::String("not-json".into()));
        assert_eq!(
            raw(r#"{"msg":"\ud800",oops}"#),
            Value::String(r#"{"msg":"\ud800",oops}"#.into())
        );
        assert_eq!(raw("null"), Value::Null);
        assert_eq!(raw("[]"), Value::Array(vec![]));
        assert_eq!(raw("{}"), Value::Object(Map::new()));
        assert_eq!(
            raw(r#""quote\"slash\\""#),
            Value::String("quote\"slash\\".into())
        );
        assert_eq!(raw("null true"), Value::String("null true".into()));
        assert_eq!(
            raw("9007199254740993").as_u64(),
            Some(9007199254740992),
            "KUCOIN-STAGING native number rounding"
        );
        assert_eq!(
            raw("-1e-400")
                .as_number()
                .unwrap()
                .to_string()
                .parse::<f64>()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            raw("1e400")
                .as_number()
                .unwrap()
                .to_string()
                .parse::<f64>()
                .unwrap()
                .to_bits(),
            f64::INFINITY.to_bits()
        );
        let object = raw(
            r#"{"z":1,"2":2,"01":3,"4294967295":4,"1":5,"z":6,"$serde_json::private::Number":"42"}"#,
        );
        let object = object.as_object().unwrap();
        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "1",
                "2",
                "z",
                "01",
                "4294967295",
                "$serde_json::private::Number"
            ]
        );
        assert_eq!(object["z"].as_u64(), Some(6));
        assert_eq!(object["$serde_json::private::Number"].as_str(), Some("42"));
        let text = format!("{}0{}", "[".repeat(170), "]".repeat(170));
        let deep = raw(&text);
        let mut value = &deep;
        for _ in 0..170 {
            value = &value.as_array().unwrap()[0];
        }
        assert_eq!(value.as_u64(), Some(0), "KUCOIN-STAGING native depth 170");
        assert!(
            decode(&RestResponseBody::Encoded(br#"{"msg":"\ud800"}"#.to_vec()))
                .unwrap_err()
                .contains("representation refusal")
        );
        let quoted = Value::String("{\"code\":\"200000\"}".into());
        assert_eq!(
            decode(&RestResponseBody::Untransformed(quoted.clone())).unwrap()["code"],
            "200000"
        );
        assert_eq!(
            decode(&RestResponseBody::Transformed(quoted.clone())).unwrap(),
            quoted,
            "KUCOIN-STAGING transformed strings stay unchanged"
        );
    }
}
