include!("cardinality.rs.inc");
use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_venue_bitrue::types::{
    INTERVAL, ORDER_SIDE, ORDER_STATUS, ORDER_TYPE, interval, order_side, order_status, order_type,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;
fn observe<T: Serialize>(hit: TableHit<T>) -> Value {
    match hit {
        TableHit::Missing => json!({"kind":"missing"}),
        TableHit::Mapped(value) => json!({"kind":"mapped","value":value}),
        TableHit::Inherited(member) => {
            json!({"kind":"inherited","value":member.js_string(),"type":if member==ProtoMember::Prototype {"object"}else{"function"}})
        }
    }
}
#[test]
fn live_tables_preserve_inherited_identity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/tables-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "table oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&Value::Array(rows.clone()), "tables");
    for (name, expected_count, native_keys) in [
        (
            "INTERVAL",
            7,
            INTERVAL.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        ),
        (
            "ORDER_TYPE",
            4,
            ORDER_TYPE.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        ),
        (
            "ORDER_STATUS",
            11,
            ORDER_STATUS.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        ),
        (
            "ORDER_SIDE",
            2,
            ORDER_SIDE.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        ),
    ] {
        let expected_keys: Vec<_> = rows
            .iter()
            .filter(|row| row["name"] == name && row["result"]["kind"] == "mapped")
            .map(|row| row["key"].as_str().unwrap())
            .collect();
        assert_eq!(
            expected_keys.len(),
            expected_count,
            "BITRUE-SOURCE-CARDINALITY table own entries: {name}"
        );
        assert_eq!(
            native_keys, expected_keys,
            "BITRUE-NATIVE-CARDINALITY table own-entry inventory: {name}"
        );
    }
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let key = row["key"].as_str().unwrap();
        let result = match name {
            "INTERVAL" => observe(interval(key)),
            "ORDER_TYPE" => observe(order_type(key)),
            "ORDER_STATUS" => observe(order_status(key)),
            "ORDER_SIDE" => observe(order_side(key)),
            _ => panic!("unknown table"),
        };
        native_cardinality(&row["result"], &result, "tables");
        assert_eq!(result, row["result"], "table parity: {name} {key}");
    }
    println!("{} live table lookups", rows.len());
    native_comparisons(84, "tables");
}
