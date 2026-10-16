use safe_cex_venue_kucoin::{spot_types, types};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn live_typescript_tables() {
    assert_native_table("ENDPOINTS", types::ENDPOINTS, 32);
    assert_native_table("INTERVAL", types::INTERVAL, 12);
    assert_native_table("WS_INTERVAL", types::WS_INTERVAL, 12);
    assert_native_table("POSITION_SIDE", types::POSITION_SIDE, 2);
    assert_native_table("ORDER_STATUS", types::ORDER_STATUS, 10);
    assert_native_table("ORDER_TYPE", types::ORDER_TYPE, 12);
    assert_native_table("ORDER_SIDE", types::ORDER_SIDE, 16);
    assert_native_table("TIME_IN_FORCE", types::TIME_IN_FORCE, 7);
    assert_native_table(
        "KUCOIN_SPOT_ENDPOINTS",
        spot_types::KUCOIN_SPOT_ENDPOINTS,
        17,
    );
    assert_native_table("KUCOIN_SPOT_INTERVAL", spot_types::KUCOIN_SPOT_INTERVAL, 12);
    assert_native_table(
        "KUCOIN_SPOT_ORDER_STATUS",
        spot_types::KUCOIN_SPOT_ORDER_STATUS,
        8,
    );
    assert_native_table(
        "KUCOIN_SPOT_ORDER_TYPE",
        spot_types::KUCOIN_SPOT_ORDER_TYPE,
        2,
    );
    assert_native_table(
        "KUCOIN_SPOT_TIME_IN_FORCE",
        spot_types::KUCOIN_SPOT_TIME_IN_FORCE,
        3,
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/table-pairs.cjs"
        ))
        .output()
        .expect("run live TS tables");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let live: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        live["pairs"]["kucoin"].as_object().unwrap().len(),
        8,
        "KUCOIN-TABLE futures pair inventory"
    );
    assert_eq!(
        live["pairs"]["kucoinspot"].as_object().unwrap().len(),
        5,
        "KUCOIN-TABLE spot pair inventory"
    );
    assert_eq!(
        json!(types::ENDPOINTS),
        live["pairs"]["kucoin"]["ENDPOINTS"],
        "KUCOIN-TABLE ENDPOINTS ordered pairs"
    );
    assert_eq!(
        json!(types::INTERVAL),
        live["pairs"]["kucoin"]["INTERVAL"],
        "KUCOIN-TABLE INTERVAL ordered pairs"
    );
    assert_eq!(
        json!(types::WS_INTERVAL),
        live["pairs"]["kucoin"]["WS_INTERVAL"],
        "KUCOIN-TABLE WS_INTERVAL ordered pairs"
    );
    assert_eq!(
        json!(types::POSITION_SIDE),
        live["pairs"]["kucoin"]["POSITION_SIDE"],
        "KUCOIN-TABLE POSITION_SIDE ordered pairs"
    );
    assert_eq!(
        json!(types::ORDER_STATUS),
        live["pairs"]["kucoin"]["ORDER_STATUS"],
        "KUCOIN-TABLE ORDER_STATUS ordered pairs"
    );
    assert_eq!(
        json!(types::ORDER_TYPE),
        live["pairs"]["kucoin"]["ORDER_TYPE"],
        "KUCOIN-TABLE ORDER_TYPE ordered pairs"
    );
    assert_eq!(
        json!(types::ORDER_SIDE),
        live["pairs"]["kucoin"]["ORDER_SIDE"],
        "KUCOIN-TABLE ORDER_SIDE ordered pairs"
    );
    assert_eq!(
        json!(types::TIME_IN_FORCE),
        live["pairs"]["kucoin"]["TIME_IN_FORCE"],
        "KUCOIN-TABLE TIME_IN_FORCE ordered pairs"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_ENDPOINTS),
        live["pairs"]["kucoinspot"]["KUCOIN_SPOT_ENDPOINTS"],
        "KUCOIN-TABLE KUCOIN_SPOT_ENDPOINTS ordered pairs"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_INTERVAL),
        live["pairs"]["kucoinspot"]["KUCOIN_SPOT_INTERVAL"],
        "KUCOIN-TABLE KUCOIN_SPOT_INTERVAL ordered pairs"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_ORDER_STATUS),
        live["pairs"]["kucoinspot"]["KUCOIN_SPOT_ORDER_STATUS"],
        "KUCOIN-TABLE KUCOIN_SPOT_ORDER_STATUS ordered pairs"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_ORDER_TYPE),
        live["pairs"]["kucoinspot"]["KUCOIN_SPOT_ORDER_TYPE"],
        "KUCOIN-TABLE KUCOIN_SPOT_ORDER_TYPE ordered pairs"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_TIME_IN_FORCE),
        live["pairs"]["kucoinspot"]["KUCOIN_SPOT_TIME_IN_FORCE"],
        "KUCOIN-TABLE KUCOIN_SPOT_TIME_IN_FORCE ordered pairs"
    );
    assert_eq!(
        live["kucoin"].as_object().unwrap().len(),
        13,
        "kucoin export inventory"
    );
    assert_eq!(
        live["kucoinspot"].as_object().unwrap().len(),
        11,
        "kucoinspot export inventory"
    );
    assert_eq!(
        json!(types::RECV_WINDOW),
        live["kucoin"]["RECV_WINDOW"],
        "RECV_WINDOW"
    );
    assert_eq!(
        json!(types::BROKER_ID),
        live["kucoin"]["BROKER_ID"],
        "BROKER_ID"
    );
    assert_eq!(
        json!(types::BASE_URL),
        live["kucoin"]["BASE_URL"],
        "BASE_URL"
    );
    assert_eq!(
        types_map(types::ENDPOINTS),
        live["kucoin"]["ENDPOINTS"],
        "ENDPOINTS"
    );
    assert_eq!(
        json!(types::GRAFANA_ENDPOINTS_EVERY),
        live["kucoin"]["GRAFANA_ENDPOINTS_EVERY"],
        "GRAFANA_ENDPOINTS_EVERY"
    );
    assert_eq!(
        json!(types::GRAFANA_ENDPOINTS_PERIODIC),
        live["kucoin"]["GRAFANA_ENDPOINTS_PERIODIC"],
        "GRAFANA_ENDPOINTS_PERIODIC"
    );
    assert_eq!(
        types_map(types::INTERVAL),
        live["kucoin"]["INTERVAL"],
        "INTERVAL"
    );
    assert_eq!(
        types_map(types::WS_INTERVAL),
        live["kucoin"]["WS_INTERVAL"],
        "WS_INTERVAL"
    );
    assert_eq!(
        types_map(types::POSITION_SIDE),
        live["kucoin"]["POSITION_SIDE"],
        "POSITION_SIDE"
    );
    assert_eq!(
        types_map(types::ORDER_STATUS),
        live["kucoin"]["ORDER_STATUS"],
        "ORDER_STATUS"
    );
    assert_eq!(
        types_map(types::ORDER_TYPE),
        live["kucoin"]["ORDER_TYPE"],
        "ORDER_TYPE"
    );
    assert_eq!(
        types_map(types::ORDER_SIDE),
        live["kucoin"]["ORDER_SIDE"],
        "ORDER_SIDE"
    );
    assert_eq!(
        types_map(types::TIME_IN_FORCE),
        live["kucoin"]["TIME_IN_FORCE"],
        "TIME_IN_FORCE"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_RECV_WINDOW),
        live["kucoinspot"]["KUCOIN_SPOT_RECV_WINDOW"],
        "KUCOIN_SPOT_RECV_WINDOW"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_BROKER_ID),
        live["kucoinspot"]["KUCOIN_SPOT_BROKER_ID"],
        "KUCOIN_SPOT_BROKER_ID"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_BASE_URL),
        live["kucoinspot"]["KUCOIN_SPOT_BASE_URL"],
        "KUCOIN_SPOT_BASE_URL"
    );
    assert_eq!(
        types_map(spot_types::KUCOIN_SPOT_ENDPOINTS),
        live["kucoinspot"]["KUCOIN_SPOT_ENDPOINTS"],
        "KUCOIN_SPOT_ENDPOINTS"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_PUBLIC_ENDPOINTS),
        live["kucoinspot"]["KUCOIN_SPOT_PUBLIC_ENDPOINTS"],
        "KUCOIN_SPOT_PUBLIC_ENDPOINTS"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY),
        live["kucoinspot"]["KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY"],
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY"
    );
    assert_eq!(
        json!(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC),
        live["kucoinspot"]["KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC"],
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC"
    );
    assert_eq!(
        types_map(spot_types::KUCOIN_SPOT_INTERVAL),
        live["kucoinspot"]["KUCOIN_SPOT_INTERVAL"],
        "KUCOIN_SPOT_INTERVAL"
    );
    assert_eq!(
        types_map(spot_types::KUCOIN_SPOT_ORDER_STATUS),
        live["kucoinspot"]["KUCOIN_SPOT_ORDER_STATUS"],
        "KUCOIN_SPOT_ORDER_STATUS"
    );
    assert_eq!(
        types_map(spot_types::KUCOIN_SPOT_ORDER_TYPE),
        live["kucoinspot"]["KUCOIN_SPOT_ORDER_TYPE"],
        "KUCOIN_SPOT_ORDER_TYPE"
    );
    assert_eq!(
        types_map(spot_types::KUCOIN_SPOT_TIME_IN_FORCE),
        live["kucoinspot"]["KUCOIN_SPOT_TIME_IN_FORCE"],
        "KUCOIN_SPOT_TIME_IN_FORCE"
    );
}
fn assert_native_table<T>(name: &str, pairs: &[(&str, T)], expected: usize) {
    assert_eq!(
        pairs.len(),
        expected,
        "KUCOIN-TABLE {name} native pair count"
    );
    let unique = pairs
        .iter()
        .map(|(key, _)| *key)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        pairs.len(),
        "KUCOIN-TABLE {name} native unique keys"
    );
}

fn types_map<T: serde::Serialize>(pairs: &[(&str, T)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), json!(value)))
            .collect(),
    )
}

#[test]
fn live_table_gate_rejects_in_memory_source_mutation() {
    let preload = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle-controls.cjs");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "live_typescript_tables", "--nocapture"])
        .env("NODE_OPTIONS", format!("--require={preload}"))
        .env("KUCOIN_ORACLE_MUTATION", "table")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(101),
        "mutated table child must assert-fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("RECV_WINDOW"), "wrong failure: {stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("live_typescript_tables ... FAILED"));
}
