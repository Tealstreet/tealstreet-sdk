use safe_cex_num::numeric_ingress::js_deserialize_passthrough;
use safe_cex_venue_bitunix::value_helpers::{
    has_finite_trigger_price, is_api_trading_supported, parse_finite_number,
};
use serde_json::{Value, json};
use std::process::Command;
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            value.to_bits()
        }
    )
}
#[test]
fn actual_typescript_value_helpers() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/value-helpers-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual TS value helpers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 225, "value helper corpus census");
    let mut refused = 0;
    let mut parsed_count = 0;
    let mut support_count = 0;
    let mut trigger_count = 0;
    for row in rows {
        let raw = row["raw"].as_str();
        let value = match raw {
            None => None,
            Some(raw) => {
                match js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        assert_eq!(raw, "\"\\ud800\"", "only declared UTF8 refusal");
                        assert_eq!(
                            row["parse"],
                            json!({"type":"number","value":row["fallback"]}),
                            "source surrogate fallback"
                        );
                        assert_eq!(
                            row["supported"],
                            json!({"type":"boolean","value":true}),
                            "source surrogate supported"
                        );
                        assert_eq!(
                            row["trigger"],
                            json!({"type":"boolean","value":false}),
                            "source surrogate trigger"
                        );
                        refused += 1;
                        continue;
                    }
                }
            }
        };
        let fallback =
            f64::from_bits(u64::from_str_radix(row["fallback"].as_str().unwrap(), 16).unwrap());
        let parsed = match parse_finite_number(value.as_ref(), fallback) {
            Ok(n) => json!({"type":"number","value":bits(n)}),
            Err(error) => json!({"error":error.name,"message":error.message}),
        };
        parsed_count += 1;
        assert_eq!(
            parsed, row["parse"],
            "parseFiniteNumber {raw:?} fallback {}",
            row["fallback"]
        );
        support_count += 1;
        assert_eq!(
            json!({"type":"boolean","value":is_api_trading_supported(value.as_ref())}),
            row["supported"],
            "isApiTradingSupported {raw:?}"
        );
        let trigger = match has_finite_trigger_price(value.as_ref()) {
            Ok(v) => json!({"type":"boolean","value":v}),
            Err(error) => json!({"error":error.name,"message":error.message}),
        };
        trigger_count += 1;
        assert_eq!(trigger, row["trigger"], "hasFiniteTriggerPrice {raw:?}");
    }
    assert_eq!(parsed_count, 220, "VALUE parse native comparison count");
    assert_eq!(support_count, 220, "VALUE support native comparison count");
    assert_eq!(trigger_count, 220, "VALUE trigger native comparison count");
    assert_eq!(refused, 5, "UTF8 refusal across fallbacks");
}

#[test]
fn accepted_numeric_contracts_retain_actual_error_name_and_message() {
    let contracts: Value = serde_json::from_str(include_str!(
        "../../safe-cex-parity/accepted-set/contracts/safe-cex-venue-bitunix.json"
    ))
    .unwrap();
    let mut errors = 0;
    for owner in [
        "value_helpers::parse_finite_number",
        "value_helpers::has_finite_trigger_price",
        "history_numbers::order_to_number",
    ] {
        let contract = &contracts[format!("safe_cex_venue_bitunix::{owner}")];
        for case in contract["cases"].as_array().unwrap() {
            let value = case["input"].get("value");
            let result = match owner {
                "value_helpers::parse_finite_number" => {
                    parse_finite_number(value, case["input"]["fallback"].as_f64().unwrap())
                        .map(|_| ())
                }
                "value_helpers::has_finite_trigger_price" => {
                    has_finite_trigger_price(value).map(|_| ())
                }
                _ => safe_cex_venue_bitunix::history_numbers::order_to_number(value).map(|_| ()),
            };
            if let Err(error) = result {
                errors += 1;
                assert_eq!(
                    case["output"],
                    json!({"name":error.name,"message":error.message}),
                    "accepted {owner} must retain the real native error class and cause"
                );
            }
        }
    }
    assert_eq!(
        errors, 3,
        "all three authored coercion-error witnesses must execute"
    );
}
