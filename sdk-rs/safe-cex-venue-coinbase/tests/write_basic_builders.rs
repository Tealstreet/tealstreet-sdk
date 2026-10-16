use safe_cex_venue_coinbase::starbase_orders::*;
use serde_json::{Value, json};
use std::process::Command;
#[path = "support/compare.rs"]
mod comparison;

#[test]
fn write_basic_builders_live_values_rejections_and_payload_order() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/write-basic-builders.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(cases.len() > 1100);
    for case in cases {
        let actual = match case["name"].as_str().unwrap() {
            "mapCoinbaseStarbaseTimeInForce" => {
                match map_coinbase_starbase_time_in_force(case.get("input")) {
                    Ok(value) => json!({"ok":value}),
                    Err(error) => json!({"error":error}),
                }
            }
            "buildCoinbaseStarbaseCancel" => {
                match build_coinbase_starbase_cancel(case.get("input")) {
                    Ok(value) => json!({"ok":value}),
                    Err(error) => json!({"error":error}),
                }
            }
            other => panic!("unclassified builder {other}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
