use safe_cex_venue_bitget::{account_predicates::*, api::*, family::*};
use serde_json::Value;

fn check(oracle: Value) {
    assert_eq!(
        BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE, oracle["unsupportedMessage"],
        "BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE"
    );
    for c in oracle["accountCases"].as_array().unwrap() {
        let code: Option<ApiCode> = c
            .get("code")
            .map(|code| serde_json::from_value(code.clone()).unwrap());
        let message = c.get("message").and_then(Value::as_str);
        assert_eq!(
            is_bitget_uta_classic_unsupported(code.as_ref(), message),
            c["unsupported"],
            "isBitgetUtaClassicUnsupported {c}"
        );
        assert_eq!(
            get_bitget_api_error_message(message, c["fallback"].as_str().unwrap()),
            c["error"],
            "getBitgetApiErrorMessage {c}"
        );
        assert_eq!(
            is_bitget_environment_mismatch(message),
            c["environment"],
            "isBitgetEnvironmentMismatch {c}"
        );
    }
    for c in oracle["accountUtf16Cases"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(c["messageJson"].as_str().unwrap()).is_err(),
            "Rust UTF-8 account boundary"
        );
        assert_eq!(
            c["errorJson"], c["messageJson"],
            "TS getBitgetApiErrorMessage surrogate witness"
        );
        assert_eq!(
            c["unsupported"], true,
            "TS isBitgetUtaClassicUnsupported surrogate witness"
        );
        assert_eq!(
            c["environment"], true,
            "TS isBitgetEnvironmentMismatch surrogate witness"
        );
    }

    for c in oracle["utf16Cases"].as_array().unwrap() {
        for key in ["idJson", "messageJson", "exchangeJson"] {
            assert!(
                serde_json::from_str::<Value>(c[key].as_str().unwrap()).is_err(),
                "Rust UTF-8 boundary changed: {key}"
            );
        }
        assert_eq!(c["member"], false, "TS surrogate family name");
        for key in ["virtual", "attached", "notFound", "silence"] {
            assert_eq!(c[key], true, "TS surrogate witness {key}");
        }
    }
    assert_eq!(
        serde_json::to_value(BITGET_LIKE_EXCHANGES).unwrap(),
        oracle["members"],
        "BITGET_LIKE_EXCHANGES"
    );
    for c in oracle["familyCases"].as_array().unwrap() {
        let exchange = c.get("exchange").and_then(Value::as_str);
        let id = c.get("id").and_then(Value::as_str);
        assert_eq!(
            is_bitget_like_exchange(exchange),
            c["member"],
            "isBitgetLikeExchange {c}"
        );
        assert_eq!(
            is_bitget_virtual_attached_tp_sl_order_id(id),
            c["virtual"],
            "isBitgetVirtualAttachedTpSlOrderId {c}"
        );
        assert_eq!(
            is_bitget_virtual_attached_tp_sl_order(id, exchange),
            c["attached"],
            "isBitgetVirtualAttachedTpSlOrder {c}"
        );
    }
    for c in oracle["apiCases"].as_array().unwrap() {
        assert_eq!(
            is_bitget_uta_strategy_order_not_found(c.get("message")),
            c["notFound"],
            "isBitgetUtaStrategyOrderNotFound {c}"
        );
        assert_eq!(
            should_silence_bitget_response_error(
                c["code"].as_str().unwrap(),
                c.get("message"),
                c.get("url").and_then(Value::as_str)
            ),
            c["silence"],
            "shouldSilenceBitgetResponseError {c}"
        );
    }
    for c in oracle["validationCases"].as_array().unwrap() {
        assert_eq!(
            should_suppress_validation_error_emission(&c["exchange"]),
            c["suppress"],
            "shouldSuppressValidationErrorEmission {c}"
        );
    }
}
#[test]
fn frozen_pure_predicates() {
    check(serde_json::from_str(include_str!("pure-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires Node and TypeScript"]
fn live_pure_predicates() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/pure-oracle.cjs"
        ))
        .output()
        .expect("execute Node");
    assert!(
        result.status.success(),
        "pure oracle failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let oracle: Value = serde_json::from_slice(&result.stdout).unwrap();
    check(oracle.clone());
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("pure-oracle.json")).unwrap(),
        "maintained pure TS oracle drift"
    );
}
