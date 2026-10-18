//! Native descriptors, array borrowing and typed diagnostic conversion.
use safe_cex_types::js_coercion::JsCoercionError;
use safe_cex_venue_binance::response_shape::{
    BinanceResponseError, describe_binance_pm_response_shape, describe_binance_response_shape,
    require_binance_array_response, require_binance_pm_array_response,
};
use serde_json::{Value, json};

fn descriptions(read: fn(Option<&Value>) -> String) {
    assert_eq!(read(None), "undefined");
    for (input, expected) in [
        (Value::Null, "null"),
        (json!(false), "boolean"),
        (json!(17), "number"),
        (json!("x"), "string"),
        (json!([]), "array(0)"),
        (json!(["x", null]), "array(2)"),
        (json!({}), "object"),
    ] {
        assert_eq!(read(Some(&input)), expected);
    }
    let mut record = serde_json::Map::new();
    for key in ["b", "10", "2", "01", "a", "z", "4294967295", "0", "y"] {
        record.insert(key.into(), Value::Null);
    }
    assert_eq!(read(Some(&Value::Object(record))), "object(0,2,10,b,01,a)");
}

#[test]
fn futures_response_descriptor_has_source_key_order_and_cutoff() {
    descriptions(describe_binance_response_shape);
}
#[test]
fn pm_response_descriptor_has_source_key_order_and_cutoff() {
    descriptions(describe_binance_pm_response_shape);
}

type Require = for<'a> fn(&str, Option<&'a Value>) -> Result<&'a [Value], BinanceResponseError>;
fn arrays(read: Require, venue: &str) {
    let input = json!(["first", -0.0, {"code": "tail"}]);
    let rows = read("/path", Some(&input)).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(std::ptr::eq(
        rows.as_ptr(),
        input.as_array().unwrap().as_ptr()
    ));
    assert_eq!(rows[0].as_str(), Some("first"));
    assert_eq!(rows[1].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rows[2]["code"].as_str(), Some("tail"));
    for (input, shape) in [
        (None, "undefined"),
        (Some(Value::Null), "null"),
        (Some(json!(false)), "boolean"),
    ] {
        let error = read("/path", input.as_ref()).unwrap_err();
        assert_eq!(error.name, "Error");
        assert_eq!(
            error.message,
            format!("{venue} /path response malformed: expected array, received {shape}")
        );
    }
    let input = json!({"code":0,"msg":"bad"});
    let error = read("/path", Some(&input)).unwrap_err();
    assert_eq!(error.name, "Error");
    assert_eq!(
        error.message,
        format!(
            "{venue} /path response malformed: expected array, received object(code,msg) (code=0 bad)"
        )
    );
    let input = json!({"msg":{"toString":null}});
    let error = read("/path", Some(&input)).unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "Cannot convert object to primitive value");
}
#[test]
fn futures_array_guard_borrows_rows_and_preserves_error_fields() {
    arrays(require_binance_array_response, "Binance");
}
#[test]
fn pm_array_guard_borrows_rows_and_preserves_error_fields() {
    arrays(require_binance_pm_array_response, "Binance PM");
}
#[test]
fn response_error_from_preserves_both_native_fields() {
    for (name, message) in [("TypeError", "wrong type"), ("RangeError", "outside range")] {
        let actual = <BinanceResponseError as From<JsCoercionError>>::from(JsCoercionError {
            name,
            message,
        });
        assert_eq!(actual.name, name);
        assert_eq!(actual.message, message);
        assert_eq!(actual.to_string(), message);
    }
}
