include!("cardinality.rs.inc");
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_bitrue::read_helpers::*;
use safe_cex_venue_bitrue::wire::OpaqueValue;
use serde_json::{Value, json};
use std::process::Command;

fn number(value: f64) -> Value {
    json!({"type":"number", "bits": if value.is_nan() {"NaN".to_owned()} else {format!("{:016x}",value.to_bits())}})
}
fn undefined() -> Value {
    json!({"type":"undefined"})
}
fn string(value: String) -> Value {
    json!({"type":"string","value":value})
}
fn result<T, E: Into<safe_cex_exchange::runtime::ExchangeError>>(
    result: Result<T, E>,
    observe: impl FnOnce(T) -> Value,
) -> Value {
    match result {
        Ok(value) => observe(value),
        Err(error) => {
            let error = error.into();
            json!({"type":"error","name":error.name})
        }
    }
}
fn raw(arg: &Value) -> Option<Value> {
    arg.get("raw").map(|text| {
        js_from_str::<OpaqueValue>(text.as_str().unwrap())
            .unwrap()
            .value
    })
}
fn float(arg: &Value) -> Option<f64> {
    arg.get("bits")
        .map(|bits| f64::from_bits(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap()))
}
#[test]
fn read_helpers_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/read-helpers-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual read helpers oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&capture, "read_helpers");
    println!(
        "capture {}; counts {}",
        capture["header"], capture["counts"]
    );
    assert!(capture["header"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!capture["header"]["arch"].as_str().unwrap().is_empty());
    assert_eq!(
        capture["utf8"].as_array().unwrap().len(),
        4,
        "UTF-16 supplement census"
    );
    for row in capture["utf8"].as_array().unwrap() {
        assert!(
            js_from_str::<OpaqueValue>(row["raw"].as_str().unwrap()).is_err(),
            "chosen read helper UTF-8 refusal: {}",
            row["method"]
        );
    }
    for (method, expected_count) in [
        ("isSuccessCode", 21),
        ("normalizeTimestampMs", 274),
        ("normalizeTradeSymbol", 19),
        ("pickTradeOrderId", 46),
        ("resolveTradeQty", 54),
    ] {
        assert_eq!(
            capture["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["method"] == method)
                .count(),
            expected_count,
            "BITRUE-SOURCE-CARDINALITY read method: {method}"
        );
    }
    for row in capture["rows"].as_array().unwrap() {
        let method = row["method"].as_str().unwrap();
        let args = row["args"].as_array().unwrap();
        let input = raw(&args[0]);
        let store = DefaultStore::new();
        if method == "resolveTradeQty" {
            let multiplier = row["multiplier"]["raw"].as_str().unwrap_or("1");
            let market = |symbol: &str, multiplier: &str| {
                let text = format!(
                    r#"{{"id":"{symbol}","symbol":"{symbol}","base":"BTC","quote":"USDT","active":true,"precision":{{"amount":1,"price":0.01}},"limits":{{"amount":{{"min":1,"max":100}},"leverage":{{"min":1,"max":10}}}},"info":{{"multiplier":{multiplier}}}}}"#
                );
                js_from_str::<safe_cex_types::Market>(&text).unwrap()
            };
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(vec![
                        market("E-BTC-USDT", multiplier),
                        market("E-ETH-USDT", "99"),
                    ]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let actual = match method {
            "isSuccessCode" => json!({"type":"boolean","value":is_success_code(input.as_ref())}),
            "normalizeTimestampMs" => normalize_timestamp_ms(float(&args[0]))
                .map(number)
                .unwrap_or_else(undefined),
            "normalizeTradeSymbol" => result(normalize_trade_symbol(input.as_ref()), string),
            "pickTradeOrderId" => result(
                pick_trade_order_id(input.as_ref().unwrap().as_object().unwrap()),
                |id| id.map(string).unwrap_or_else(undefined),
            ),
            "resolveTradeQty" => result(
                resolve_trade_qty(
                    input.as_ref().unwrap().as_object().unwrap(),
                    &store,
                    raw(&args[1]).as_ref().unwrap().as_str().unwrap(),
                    float(&args[2]).unwrap(),
                ),
                number,
            ),
            _ => panic!("unwitnessed method {method}"),
        };
        native_cardinality(&row["result"], &actual, "read_helpers");
        assert_eq!(
            actual, row["result"],
            "read helper native parity: {method}; args {args:?}"
        );
    }
    native_comparisons(414, "read_helpers");
}
