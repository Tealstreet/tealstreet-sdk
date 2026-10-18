//! Admission witness through the current production shared Store facade.
//! Granular writes preserve table and DTO referents; full replacement starts a new generation.
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
    let retained_rows: Vec<Js> = cases
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
        matches!(retained_rows[0].get("active").unwrap(), Js::Bool(false)),
        "previously returned DTO must observe the granular write"
    );
    assert!(matches!(retained_rows[2].get("bid").unwrap(), Js::Number(value) if value == 7.0));
}

#[test]
fn full_replacement_keeps_old_map_and_dto_generation_separate() {
    let native = Rc::new(DefaultStore::new());
    let update: StoreUpdate = serde_json::from_value(json!({
        "markets":[{"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
            "precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}}]
    })).unwrap();
    Store::update(native.as_ref(), update, false).unwrap();
    let facade = store::facade(native.clone());
    let old_map = facade.get("marketsById").unwrap();
    let first = old_map.get("m").unwrap();
    let mut replacement = native.markets()[0].clone();
    replacement.active = false;
    Store::update(
        native.as_ref(),
        StoreUpdate {
            markets: Some(vec![replacement]),
            ..StoreUpdate::default()
        },
        false,
    )
    .unwrap();
    let patch = ModelPatch::from(json!({"active":false}).as_object().unwrap().clone());
    Store::update_market(native.as_ref(), "m", &patch).unwrap();
    let second = facade.get("marketsById").unwrap().get("m").unwrap();
    assert!(matches!(first.get("active").unwrap(), Js::Bool(true)));
    assert!(matches!(second.get("active").unwrap(), Js::Bool(false)));
    assert!(!first.same_reference(&second));
    assert!(matches!(
        old_map.get("m").unwrap().get("active").unwrap(),
        Js::Bool(true)
    ));
    Store::reset(native.as_ref()).unwrap();
    assert!(
        facade
            .get("marketsById")
            .unwrap()
            .get("m")
            .unwrap()
            .is_undefined()
    );
    assert!(matches!(
        old_map.get("m").unwrap().get("active").unwrap(),
        Js::Bool(true)
    ));
}

#[test]
fn granular_writes_preserve_previously_read_dto_reference_identity() {
    let native = Rc::new(DefaultStore::new());
    let update: StoreUpdate = serde_json::from_value(json!({
        "markets":[{"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
            "precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}}]
    })).unwrap();
    Store::update(native.as_ref(), update, false).unwrap();
    let facade = store::facade(native.clone());
    let table = facade.get("marketsById").unwrap();
    let first = table.get("m").unwrap();
    let patch = ModelPatch::from(json!({"active":false}).as_object().unwrap().clone());
    Store::update_market(native.as_ref(), "m", &patch).unwrap();
    let current = facade.get("marketsById").unwrap();
    let by_symbol = facade
        .get("marketsBySymbol")
        .unwrap()
        .get("BTCUSD")
        .unwrap();
    let in_array = facade.get("markets").unwrap().get("0").unwrap();
    assert_eq!(
        json!({
            "oldRowSeesWrite":matches!(first.get("active").unwrap(),Js::Bool(false)),
            "sameTable":table.same_reference(&current),
            "sameRow":first.same_reference(&current.get("m").unwrap()),
            "sameSymbolRow":first.same_reference(&by_symbol),
            "sameArrayRow":first.same_reference(&in_array)
        }),
        json!({"oldRowSeesWrite":true,"sameTable":true,"sameRow":true,"sameSymbolRow":true,"sameArrayRow":true}),
        "maintained Store getters/indexes/state retain the same live referents during granular writes"
    );
}
