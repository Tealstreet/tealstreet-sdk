use safe_cex_exchange::js_runtime::value::{Js, Str, Thrown, error_object, from_json};
use safe_cex_num::JsCoercionError;
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
    let payload = payload.map(from_json).unwrap_or(Js::Undefined);
    String::from_utf16(&native_shape(&payload)).expect("UTF8 input produces UTF8 shape")
}

pub(crate) fn native_shape(payload: &Js) -> Str {
    if let Js::Array(rows) = payload {
        return format!("array({})", rows.borrow().len())
            .encode_utf16()
            .collect::<Vec<_>>()
            .into();
    }
    if matches!(payload, Js::Null) {
        return "null".encode_utf16().collect::<Vec<_>>().into();
    }
    if payload.type_of() != "object" {
        return payload.type_of().encode_utf16().collect::<Vec<_>>().into();
    }
    let keys = payload.own_keys();
    if keys.is_empty() {
        return "object".encode_utf16().collect::<Vec<_>>().into();
    }
    let mut text: Vec<_> = "object(".encode_utf16().collect();
    for (index, key) in keys.iter().take(6).enumerate() {
        if index > 0 {
            text.push(u16::from(b','));
        }
        text.extend_from_slice(key);
    }
    text.push(u16::from(b')'));
    text.into()
}

pub(crate) fn native_array_guard(venue: &str, endpoint: &str, payload: &Js) -> Result<(), Thrown> {
    if matches!(payload, Js::Array(_)) {
        return Ok(());
    }
    let shape = native_shape(payload);
    let message = crate::response_message::native_response_message(payload)?;
    Err(native_malformed(venue, endpoint, &shape, &message))
}

fn native_malformed(venue: &str, endpoint: &str, shape: &[u16], message: &[u16]) -> Thrown {
    let mut text: Vec<_> =
        format!("{venue} {endpoint} response malformed: expected array, received ")
            .encode_utf16()
            .collect();
    text.extend_from_slice(shape);
    if !message.is_empty() {
        text.extend(" (".encode_utf16());
        text.extend_from_slice(message);
        text.push(u16::from(b')'));
    }
    let error = error_object("Error", "");
    error
        .set("message", Js::utf16(&text))
        .expect("fresh error message");
    Thrown::new(error)
}

// ts: requireBinanceArrayResponse
pub fn require_binance_array_response<'a>(
    endpoint: &str,
    payload: Option<&'a Value>,
) -> Result<&'a [Value], BinanceResponseError> {
    let native = payload.map(from_json).unwrap_or(Js::Undefined);
    native_array_guard("Binance", endpoint, &native).map_err(|error| {
        let error = error.to_exchange_error();
        BinanceResponseError {
            name: if error.name == "TypeError" {
                "TypeError"
            } else {
                "Error"
            },
            message: error.message,
        }
    })?;
    Ok(payload.and_then(Value::as_array).expect("validated array"))
}
// ts: requireBinancePMArrayResponse
pub fn require_binance_pm_array_response<'a>(
    endpoint: &str,
    payload: Option<&'a Value>,
) -> Result<&'a [Value], BinanceResponseError> {
    let native = payload.map(from_json).unwrap_or(Js::Undefined);
    native_array_guard("Binance PM", endpoint, &native).map_err(|error| {
        let error = error.to_exchange_error();
        BinanceResponseError {
            name: if error.name == "TypeError" {
                "TypeError"
            } else {
                "Error"
            },
            message: error.message,
        }
    })?;
    Ok(payload.and_then(Value::as_array).expect("validated array"))
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
            let error = native_malformed(
                venue,
                endpoint,
                &shape.encode_utf16().collect::<Vec<_>>(),
                &message.encode_utf16().collect::<Vec<_>>(),
            )
            .to_exchange_error();
            assert_eq!(error.name, "Error");
            assert_eq!(error.message, expected);
            assert_eq!(error.to_string(), expected);
        }
    }
}
