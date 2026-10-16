include!("cardinality.rs.inc");
use safe_cex_venue_bitrue::types::*;
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn configuration_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/configuration.cjs"
        ))
        .output()
        .expect("execute actual TypeScript oracle");
    assert!(
        output.status.success(),
        "TypeScript oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stderr));
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&actual, "configuration");
    let native = json!({
        "RECV_WINDOW": format!("{:016x}", RECV_WINDOW.to_bits()),
        "BROKER_ID": BROKER_ID,
        "BASE_URL": BASE_URL,
        "BASE_WS_URL": BASE_WS_URL,
        "ENDPOINTS": ENDPOINTS,
        "INTERVAL": INTERVAL,
        "ORDER_TYPE": ORDER_TYPE,
        "ORDER_STATUS": ORDER_STATUS,
        "ORDER_SIDE": ORDER_SIDE,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
    });
    assert_eq!(
        actual.as_object().unwrap().len(),
        11,
        "configuration row census"
    );
    assert_eq!(
        native.as_object().unwrap().len(),
        actual.as_object().unwrap().len(),
        "BITRUE-NATIVE-CARDINALITY configuration rows"
    );
    for (row, expected) in native.as_object().unwrap() {
        native_cardinality(&actual[row], expected, "configuration");
        assert_eq!(&actual[row], expected, "configuration parity: {row}");
    }
    native_comparisons(11, "configuration");
}
