use safe_cex_venue_cryptocom::api::{
    is_cryptocom_signature_rejection, is_non_retryable_private_write, is_public_endpoint,
};
use serde_json::Value;
use std::process::Command;

#[test]
fn live_endpoint_classifiers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api_endpoints_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "endpoint_source:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    // Independent input inventory: counts and call arguments do not come from source observations.
    let inputs = [
        None,
        Some(""),
        Some("/public/get-instruments"),
        Some("https://api.crypto.com/exchange/v1/public/get-instruments?x=1"),
        Some("prefix/public/get-book-suffix"),
        Some("/PUBLIC/get-instruments"),
        Some("/public/get-tickers"),
        Some("/public/get-ticker"),
        Some("/public/get-trades"),
        Some("/public/get-candlestick"),
        Some("/public/get-valuations"),
        Some("/public/get-expired-settlement-price"),
        Some("/public/get-insurance"),
        Some("/private/create-order"),
        Some("/private/advanced/create-order"),
        Some("/private/amend-order"),
        Some("/private/advanced/amend-order"),
        Some("/private/cancel-order"),
        Some("/private/advanced/cancel-order"),
        Some("/private/cancel-all-orders"),
        Some("/private/close-position"),
        Some("/private/change-account-leverage"),
        Some("/private/get-open-orders"),
        Some("/private/user-balance"),
        Some("/private/create-order-extra"),
        Some("/public/get-book?next=/private/cancel-order"),
        Some("🙂/private/close-position"),
        Some("/private/create-withdrawal"),
        Some("/public/not-real"),
    ];
    assert_eq!(cases.len(), 29, "endpoint_source_count");
    assert_eq!(inputs.len(), 29, "endpoint_inventory_count");
    let mut comparisons = 0;
    for (index, input) in inputs.into_iter().enumerate() {
        assert_eq!(
            cases[index]["input"],
            serde_json::json!(input),
            "endpoint_input_content:{index}"
        );
        assert_eq!(
            is_public_endpoint(input.unwrap_or("")),
            cases[index]["public"].as_bool().unwrap(),
            "endpoint_native_public:{index}"
        );
        assert_eq!(
            is_non_retryable_private_write(input),
            cases[index]["write"].as_bool().unwrap(),
            "endpoint_native_write:{index}"
        );
        comparisons += 2;
    }
    assert_eq!(comparisons, 58, "endpoint_executed_comparisons");
    // Number() trims and reads hex, so padded and 0x forms reject too; an array never does.
    let expected = [
        true, true, true, true, false, false, false, false, false, false, false,
    ];
    let inputs = corpus["signature"]["inputs"].as_array().unwrap();
    let results = corpus["signature"]["results"].as_array().unwrap();
    assert_eq!(inputs.len(), expected.len(), "signature_source_count");
    for ((data, rejected), source) in inputs.iter().zip(expected).zip(results) {
        assert_eq!(
            is_cryptocom_signature_rejection(data).unwrap(),
            rejected,
            "signature_native:{data}"
        );
        assert_eq!(*source, Value::Bool(rejected), "signature_source:{data}");
    }
}
