//! Native evidence for three individually reviewed non-decoder predicates.
use safe_cex_venue_binance::order_logic::{
    detect_balance_issue, is_binance_position_mode_already_set_error, is_post_only_rejection_error,
};
use serde_json::{Value, json};

#[test]
fn position_mode_error_is_a_strict_boolean_recognizer() {
    assert!(!is_binance_position_mode_already_set_error(None));
    for code in [json!(-4059), json!("-4059")] {
        assert!(is_binance_position_mode_already_set_error(Some(&json!({
            "response": {"data": {"code": code}}
        }))));
    }
    for input in [
        Value::Null,
        json!(false),
        json!(-4059),
        json!("-4059"),
        json!([]),
        json!({}),
        json!({"code": -4059}),
        json!({"response": {"code": -4059}}),
        json!({"response": {"data": {"code": "-4.059e3"}}}),
        json!({"response": {"data": {"code": [-4059]}}}),
        json!({"response": {"data": {"code": -4058}}}),
    ] {
        assert!(
            !is_binance_position_mode_already_set_error(Some(&input)),
            "{input}"
        );
    }
}

#[test]
fn deprecated_balance_predicate_ignores_account_data() {
    assert!(!detect_balance_issue(None));
    for input in [
        Value::Null,
        json!(false),
        json!(true),
        json!(0),
        json!("0"),
        json!([]),
        json!({}),
        json!({"availableBalance": 0, "assets": [{"asset": "USDT", "balance": "100"}]}),
    ] {
        assert!(!detect_balance_issue(Some(&input)), "{input}");
    }
}

#[test]
fn post_only_predicate_preserves_code_and_message_precedence() {
    assert!(!is_post_only_rejection_error(None).unwrap());
    for (input, expected) in [
        (json!({"response": {"data": {"code": -5022}}}), true),
        (json!({"code": -5022}), true),
        (
            json!({"response": {"data": {"code": null}}, "code": -5022}),
            true,
        ),
        (
            json!({"response": {"data": {"code": 0}}, "code": -5022}),
            false,
        ),
        (
            json!({"response": {"data": {"code": "-5022"}}, "code": -5022}),
            false,
        ),
        (json!({"code": "-5022"}), false),
        (json!({"message": "POST ONLY ORDER WILL BE REJECTED"}), true),
        (json!({"message": "could not be executed as maker"}), true),
        (
            json!({"response": {"data": {"msg": "no match"}}, "message": "could not be executed as maker"}),
            false,
        ),
        (
            json!({"response": {"data": {"msg": ""}}, "message": "could not be executed as maker"}),
            true,
        ),
        (
            json!({"response": {"data": {"msg": false}}, "message": "could not be executed as maker"}),
            true,
        ),
        (json!({"message": ["could not be executed as maker"]}), true),
        (json!({"message": {}}), false),
        (Value::Null, false),
        (json!([]), false),
    ] {
        assert_eq!(
            is_post_only_rejection_error(Some(&input)).unwrap(),
            expected,
            "{input}"
        );
    }
    let error = json!({"message": {"toString": null}});
    assert_eq!(
        is_post_only_rejection_error(Some(&error)).unwrap_err().name,
        "TypeError"
    );
}
