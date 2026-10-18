// Describe each native map member before transporting the observation.
pub fn depth(row: &crate::orderbook::BinanceDepthUpdate) -> serde_json::Value {
    fn field(value: &serde_json::Value) -> serde_json::Value {
        use serde_json::{Value, json};
        match value {
            Value::Null => json!(["null"]),
            Value::Bool(value) => json!(["bool", value]),
            Value::Number(_) => json!([
                "number",
                format!(
                    "{:016x}",
                    safe_cex_num::numeric_ingress::js_value_as_f64(value)
                        .unwrap()
                        .to_bits()
                )
            ]),
            Value::String(value) => json!(["text", value.encode_utf16().collect::<Vec<_>>()]),
            Value::Array(values) => json!([
                "array",
                values.len(),
                values.iter().map(field).collect::<Vec<_>>()
            ]),
            Value::Object(values) => json!([
                "object",
                values.len(),
                values
                    .iter()
                    .map(|(key, value)| json!([
                        key.encode_utf16().collect::<Vec<_>>(),
                        field(value)
                    ]))
                    .collect::<Vec<_>>()
            ]),
        }
    }
    field(&serde_json::Value::Object(row.clone()))
}
