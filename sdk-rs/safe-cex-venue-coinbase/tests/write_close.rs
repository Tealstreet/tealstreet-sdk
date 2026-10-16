use safe_cex_venue_coinbase::starbase_orders::*;
use serde_json::{Value, json};
use std::process::Command;
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;
#[test]
fn close_builder_live_shape_symbol_price_precedence_and_payload() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/write-close.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    capture_metadata::assert_capture_metadata(&Value::Array(cases.clone()), 242);
    assert_eq!(cases.len(), 242);
    for case in cases {
        let input = &case["intent"];
        let intent = CoinbaseStarbaseClosePositionIntent {
            price: case
                .get("priceBits")
                .map(|v| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())),
            saved_symbol: input["savedSymbol"].as_str().unwrap().into(),
            order_type: match input["type"].as_str().unwrap() {
                "limit" => CoinbaseStarbaseClosePositionType::Limit,
                "market" => CoinbaseStarbaseClosePositionType::Market,
                other => panic!("unclassified type {other}"),
            },
        };
        let actual = match build_coinbase_starbase_close_position(&intent) {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
