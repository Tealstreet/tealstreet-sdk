//! Independent native output witnesses for response text and rejection reasons.
use safe_cex_num::JsCoercionError;
use safe_cex_venue_binance::response_message::{
    get_binance_pm_response_message, get_binance_response_message,
};
use serde_json::{Value, json};

fn message_cases(read: fn(Option<&Value>) -> Result<String, JsCoercionError>) {
    assert_eq!(read(None).unwrap(), "");
    for (input, expected) in [
        (Value::Null, ""),
        (json!(false), ""),
        (json!(17), ""),
        (json!("message"), ""),
        (json!([]), ""),
        (json!(["message"]), ""),
        (json!({}), ""),
        (json!({"code": 0}), "code=0"),
        (json!({"code": -0.0}), "code=0"),
        (json!({"code": null}), "code=null"),
        (json!({"code": false}), "code=false"),
        (json!({"code": ""}), "code="),
        (json!({"code": []}), "code="),
        (json!({"code": [1, null, 2]}), "code=1,,2"),
        (json!({"code": {"k": 1}}), "code=[object Object]"),
        (
            json!({"msg": "first", "message": "second", "error": "third"}),
            "first",
        ),
        (
            json!({"msg": 0, "message": false, "error": "third"}),
            "third",
        ),
        (json!({"msg": [], "message": "later"}), ""),
        (json!({"msg": {}, "message": "later"}), "[object Object]"),
        (json!({"code": 0, "msg": "detail"}), "code=0 detail"),
        (
            json!({"code": "B", "msg": ["one", "two"]}),
            "code=B one,two",
        ),
        (json!({"msg": false, "message": 0, "error": ""}), ""),
    ] {
        assert_eq!(
            read(Some(&input)).unwrap(),
            expected,
            "native message: {input}"
        );
    }
    for input in [
        json!({"msg": {"toString": null}}),
        json!({"code": {"toString": null}, "msg": "detail"}),
    ] {
        assert_eq!(read(Some(&input)).unwrap_err().name, "TypeError", "{input}");
    }
}

#[test]
fn futures_response_message_preserves_native_output() {
    message_cases(get_binance_response_message);
}

#[test]
fn pm_response_message_preserves_native_output() {
    message_cases(get_binance_pm_response_message);
}
