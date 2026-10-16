//! Independent native output witnesses for response text and rejection reasons.
use safe_cex_num::JsCoercionError;
use safe_cex_types::OrderRejectReason;
use safe_cex_venue_binance::{
    private_order_logic::get_order_reject_reason,
    response_message::{get_binance_pm_response_message, get_binance_response_message},
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

#[test]
fn rejection_reason_preserves_native_variant_and_absence() {
    for (input, expected) in [
        (json!({}), None),
        (json!({"r": null}), None),
        (json!({"r": false}), None),
        (json!({"r": 0}), None),
        (json!({"r": ""}), None),
        (json!({"r": "insufficient margin"}), None),
        (json!({"r": "post_only"}), Some(OrderRejectReason::PostOnly)),
        (json!({"r": "gtx"}), Some(OrderRejectReason::PostOnly)),
        (
            json!({"r": "MaKeR violation"}),
            Some(OrderRejectReason::PostOnly),
        ),
        (
            json!({"r": "NOT_POST_ONLY"}),
            Some(OrderRejectReason::PostOnly),
        ),
        (json!({"r": "makerish"}), Some(OrderRejectReason::PostOnly)),
        (json!({"r": "g t x"}), None),
        (json!({"r": ["gtx"]}), Some(OrderRejectReason::PostOnly)),
        (
            json!({"r": ["x", null, "maker"]}),
            Some(OrderRejectReason::PostOnly),
        ),
        (json!({"r": ["post", "only"]}), None),
        (json!({"r": {}}), None),
        (json!({"r": 123}), None),
        (
            json!({"r": "\u{feff}post_only"}),
            Some(OrderRejectReason::PostOnly),
        ),
    ] {
        let actual = get_order_reject_reason(input.as_object().unwrap()).unwrap();
        assert_eq!(actual, expected, "native rejection reason: {input}");
    }
    let input = json!({"r": {"toString": null}});
    assert_eq!(
        get_order_reject_reason(input.as_object().unwrap())
            .unwrap_err()
            .name,
        "TypeError"
    );
}
