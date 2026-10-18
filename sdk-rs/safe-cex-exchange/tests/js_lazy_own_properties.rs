//! Admission witness through the current production shared Store facade.
//! The table reads must remain lazy; returned model DTOs remain snapshots.
use safe_cex_exchange::js_runtime::{store, value::Js};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch, store::DefaultStore};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc};

#[test]
fn retained_store_tables_read_current_native_entries() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_lazy_own_properties_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let native = Rc::new(DefaultStore::new());
    let update: StoreUpdate = serde_json::from_value(json!({
        "markets":[source["market"].clone()], "tickers":[source["ticker"].clone()]
    }))
    .unwrap();
    Store::update(native.as_ref(), update, false).unwrap();
    let facade = store::facade(native.clone());
    let cases = source["storeCases"].as_array().unwrap();
    let tables: Vec<Js> = cases
        .iter()
        .map(|case| facade.get(case["name"].as_str().unwrap()).unwrap())
        .collect();
    let snapshots: Vec<Js> = cases
        .iter()
        .zip(&tables)
        .map(|(case, table)| table.get(case["key"].as_str().unwrap()).unwrap())
        .collect();
    let market_patch = ModelPatch::from(json!({"active":false}).as_object().unwrap().clone());
    let ticker_patch = ModelPatch::from(json!({"bid":7}).as_object().unwrap().clone());
    Store::update_market(native.as_ref(), "m", &market_patch).unwrap();
    Store::update_ticker(native.as_ref(), "t", &ticker_patch).unwrap();
    let mut observed = vec![];
    for (case, table) in cases.iter().zip(tables) {
        let item = table.get(case["key"].as_str().unwrap()).unwrap();
        let value = item.get(case["field"].as_str().unwrap()).unwrap();
        observed.push(match value {
            Js::Bool(value) => json!(value),
            Js::Number(value) => json!(value as i64),
            _ => panic!("unexpected field type"),
        });
    }
    assert_eq!(
        observed,
        vec![json!(false), json!(false), json!(7), json!(7)],
        "all four retained Store table views must read the current native entry"
    );
    assert!(
        matches!(snapshots[0].get("active").unwrap(), Js::Bool(true)),
        "previously returned native DTO is a snapshot"
    );
    assert!(matches!(snapshots[2].get("bid").unwrap(), Js::Number(value) if value == 1.0));
}
