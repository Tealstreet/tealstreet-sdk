use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};

pub fn observe(value: &Input) -> Value {
    match value {
        Input::Undefined => json!({"undefined":true}),
        Input::Null => Value::Null,
        Input::Bool(value) => Value::Bool(*value),
        Input::Text(units) => String::from_utf16(units)
            .map(Value::String)
            .unwrap_or_else(|_| json!({"utf16":units})),
        Input::Float(_) | Input::Lexeme(_) => {
            let value = match value {
                Input::Float(value) => *value,
                Input::Lexeme(value) => value.parse::<f64>().unwrap(),
                _ => unreachable!(),
            };
            json!({"number":if value.is_nan() { "NaN".to_owned() } else { format!("{:016x}", value.to_bits()) }})
        }
        Input::Array(values) => Value::Array(values.iter().map(observe).collect()),
        Input::Object(fields) => {
            if fields
                .iter()
                .any(|(key, _)| String::from_utf16(key).is_err())
            {
                return json!({"entries":fields.iter().map(|(key, value)| (key, observe(value))).collect::<Vec<_>>()});
            }
            Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| (String::from_utf16(key).unwrap(), observe(value)))
                    .collect(),
            )
        }
    }
}
