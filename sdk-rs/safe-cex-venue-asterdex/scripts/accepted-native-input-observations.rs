use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json};
use serde_json::{Value, json};

pub fn input(value: &Value) -> Js {
    match value.get("native").and_then(Value::as_str) {
        Some("undefined") => Js::Undefined,
        Some("nan") => Js::Number(f64::NAN),
        Some("minusZero") => Js::Number(-0.0),
        Some("utf16") => Js::String(
            value["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| unit.as_u64().unwrap() as u16)
                .collect::<Vec<_>>()
                .into(),
        ),
        _ => match value {
            Value::Array(values) => Js::array(values.iter().map(input).collect()),
            _ => from_json(value),
        },
    }
}
pub fn bytes(value: Result<Vec<u8>, Thrown>) -> Value {
    match value {
        Ok(bytes) => json!({"ok":true,"length":bytes.len(),"bytes":bytes}),
        Err(error) => json!({"ok":false,"name":error.value.get("name").unwrap().to_display()}),
    }
}
pub fn text(value: Result<String, Thrown>) -> Value {
    match value {
        Ok(text) => {
            json!({"ok":true,"length":text.encode_utf16().count(),"units":text.encode_utf16().collect::<Vec<_>>()})
        }
        Err(error) => json!({"ok":false,"name":error.value.get("name").unwrap().to_display()}),
    }
}
pub fn optional_text(value: Option<String>) -> Value {
    json!({"ok":true,"present":value.is_some(),"units":value.map(|text|text.encode_utf16().collect::<Vec<_>>()).unwrap_or_default()})
}
fn scalar(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Number(value) => {
            json!({"kind":"number","bits":if value.is_nan(){"NaN".to_owned()}else{format!("{:016x}",value.to_bits())}})
        }
        Js::String(units) => json!({"kind":"string","units":units.as_ref()}),
        Js::Null => json!({"kind":"null"}),
        _ => panic!("fixture expects a directly observed scalar"),
    }
}
pub fn klines(value: Result<Js, Thrown>) -> Value {
    match value {
        Ok(rows) => {
            let rows = rows.array_values().unwrap().unwrap();
            json!({"ok":true,"length":rows.len(),"rows":rows.iter().map(|row|{
                ["timestamp","open","high","low","close","volume"].into_iter()
                    .map(|name|(name.to_owned(),scalar(&row.get(name).unwrap())))
                    .collect::<serde_json::Map<_,_>>()
            }).collect::<Vec<_>>()})
        }
        Err(error) => json!({"ok":false,"name":error.value.get("name").unwrap().to_display()}),
    }
}
