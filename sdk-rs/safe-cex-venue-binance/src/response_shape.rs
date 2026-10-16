use safe_cex_num::{JsCoercionError, js_own_keys};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceResponseError {
    pub name: &'static str,
    pub message: String,
}
impl std::fmt::Display for BinanceResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for BinanceResponseError {}
impl From<JsCoercionError> for BinanceResponseError {
    fn from(error: JsCoercionError) -> Self {
        Self {
            name: error.name,
            message: error.message.into(),
        }
    }
}

// ts: describeBinanceResponseShape
pub fn describe_binance_response_shape(payload: Option<&Value>) -> String {
    describe(payload)
}
// ts: describeBinancePMResponseShape
pub fn describe_binance_pm_response_shape(payload: Option<&Value>) -> String {
    describe(payload)
}

fn describe(payload: Option<&Value>) -> String {
    match payload {
        Some(Value::Array(items)) => format!("array({})", items.len()),
        Some(Value::Null) => "null".into(),
        Some(Value::Object(record)) => {
            let keys = js_own_keys(record.keys().map(String::as_str));
            if keys.is_empty() {
                "object".into()
            } else {
                format!("object({})", keys[..keys.len().min(6)].join(","))
            }
        }
        Some(Value::String(_)) => "string".into(),
        Some(Value::Bool(_)) => "boolean".into(),
        Some(Value::Number(_)) => "number".into(),
        None => "undefined".into(),
    }
}

// ts: requireBinanceArrayResponse
pub fn require_binance_array_response<'a>(
    endpoint: &str,
    payload: Option<&'a Value>,
) -> Result<&'a [Value], BinanceResponseError> {
    if let Some(Value::Array(items)) = payload {
        return Ok(items);
    }
    let shape = describe_binance_response_shape(payload);
    let message = crate::response_message::get_binance_response_message(payload)?;
    Err(malformed("Binance", endpoint, shape, message))
}
// ts: requireBinancePMArrayResponse
pub fn require_binance_pm_array_response<'a>(
    endpoint: &str,
    payload: Option<&'a Value>,
) -> Result<&'a [Value], BinanceResponseError> {
    if let Some(Value::Array(items)) = payload {
        return Ok(items);
    }
    let shape = describe_binance_pm_response_shape(payload);
    let message = crate::response_message::get_binance_pm_response_message(payload)?;
    Err(malformed("Binance PM", endpoint, shape, message))
}
fn malformed(venue: &str, endpoint: &str, shape: String, message: String) -> BinanceResponseError {
    let suffix = if message.is_empty() {
        String::new()
    } else {
        format!(" ({message})")
    };
    BinanceResponseError {
        name: "Error",
        message: format!(
            "{venue} {endpoint} response malformed: expected array, received {shape}{suffix}"
        ),
    }
}

#[cfg(test)]
mod accepted_contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn private_descriptor_preserves_source_shape() {
        assert_eq!(describe(None), "undefined");
        for (input, expected) in [
            (Value::Null, "null"),
            (json!(false), "boolean"),
            (json!(17), "number"),
            (json!("x"), "string"),
            (json!([]), "array(0)"),
            (json!(["x", null]), "array(2)"),
            (json!({}), "object"),
        ] {
            assert_eq!(describe(Some(&input)), expected);
        }
        let mut record = serde_json::Map::new();
        for key in ["b", "10", "2", "01", "a", "z", "4294967295", "0", "y"] {
            record.insert(key.into(), Value::Null);
        }
        assert_eq!(
            describe(Some(&Value::Object(record))),
            "object(0,2,10,b,01,a)"
        );
    }

    #[test]
    fn private_malformed_constructor_preserves_error_fields() {
        for (venue, endpoint, shape, message, expected) in [
            (
                "Binance",
                "/fapi",
                "null",
                "",
                "Binance /fapi response malformed: expected array, received null",
            ),
            (
                "Binance PM",
                "/papi",
                "object(code,msg)",
                "code=0 bad",
                "Binance PM /papi response malformed: expected array, received object(code,msg) (code=0 bad)",
            ),
            (
                "Binance",
                "",
                "undefined",
                " ",
                "Binance  response malformed: expected array, received undefined ( )",
            ),
        ] {
            let error = malformed(venue, endpoint, shape.into(), message.into());
            assert_eq!(error.name, "Error");
            assert_eq!(error.message, expected);
            assert_eq!(error.to_string(), expected);
        }
    }
}
