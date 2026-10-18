//! Production native codec, JSON builtin and Store facade presence semantics.
use safe_cex_exchange::js_runtime::{
    globals, store,
    value::{Js, from_json, from_typed, to_typed},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{Order, Position};
use serde_json::{Value, json};
use std::{
    process::Command,
    rc::{Rc, Weak},
};

fn order() -> Value {
    json!({"id":"o", "symbol":"BTCUSDT", "amount":2, "filled":0, "remaining":2,
        "reduceOnly":false, "lastUpdate":1})
}
fn stringify(value: &Js) -> String {
    let global = Js::object(vec![]);
    globals::install_builtins::<()>(&global, Weak::new());
    global
        .get("JSON")
        .unwrap()
        .call_method("stringify", vec![value.clone()])
        .unwrap()
        .to_display()
}

#[test]
fn absent_undefined_and_known_order_fields_survive_the_native_bridge() {
    let value = from_json(&order());
    let absent: Order = to_typed(&value).unwrap();
    let native = from_typed(&absent).unwrap();
    assert!(!native.has_own("type") && !native.has_own("price"));
    for key in [
        "type",
        "status",
        "side",
        "price",
        "parentId",
        "close",
        "info",
        "clientOrderId",
        "timeInForce",
        "triggerType",
        "tpTriggerType",
        "slTriggerType",
        "takeProfit",
        "stopLoss",
        "extra",
    ] {
        value.define(key, Js::Undefined).unwrap();
    }
    let undefined: Order = to_typed(&value).unwrap();
    let native = from_typed(&undefined).unwrap();
    for key in [
        "type",
        "status",
        "side",
        "price",
        "info",
        "clientOrderId",
        "stopLoss",
        "extra",
    ] {
        assert!(native.has_own(key), "own undefined {key}");
        assert!(native.get(key).unwrap().is_undefined());
        assert!(
            !serde_json::to_value(&undefined)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key(key)
        );
    }
    for number in [0.0, -0.0, 1.25, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        value.define("price", Js::Number(number)).unwrap();
        let decoded: Order = to_typed(&value).unwrap();
        let Js::Number(actual) = from_typed(&decoded).unwrap().get("price").unwrap() else {
            panic!("native number lost")
        };
        assert_eq!(actual.to_bits(), number.to_bits());
        let wire = serde_json::to_value(&decoded).unwrap();
        assert_eq!(wire["price"].is_null(), !number.is_finite());
        if number.is_finite() {
            assert_eq!(wire["price"].as_f64().unwrap(), number);
        }
    }
    value.define("price", Js::Null).unwrap();
    assert!(to_typed::<Order>(&value).is_err(), "null is not undefined");
}

#[test]
fn store_insert_and_partial_patch_retain_own_undefined() {
    let store = Rc::new(DefaultStore::new());
    let facade = store::facade(store.clone());
    let value = from_json(&order());
    value.define("type", Js::Undefined).unwrap();
    value.define("price", Js::Undefined).unwrap();
    facade.call_method("addOrUpdateOrder", vec![value]).unwrap();
    assert_eq!(store.orders().len(), 1);
    let inserted = facade.get("orders").unwrap().get("0").unwrap();
    assert!(inserted.has_own("type") && inserted.get("type").unwrap().is_undefined());
    assert!(inserted.has_own("price") && inserted.get("price").unwrap().is_undefined());
    facade
        .call_method(
            "updateOrder",
            vec![
                Js::object(vec![("id", Js::str("o"))]),
                Js::object(vec![
                    ("price", Js::Number(7.0)),
                    ("stopLoss", Js::Number(5.0)),
                    ("clientOrderId", Js::str("client")),
                ]),
            ],
        )
        .unwrap();
    facade
        .call_method(
            "updateOrder",
            vec![
                Js::object(vec![("id", Js::str("o"))]),
                Js::object(vec![
                    ("price", Js::Undefined),
                    ("stopLoss", Js::Undefined),
                    ("clientOrderId", Js::Undefined),
                ]),
            ],
        )
        .unwrap();
    let row = facade.get("orders").unwrap().get("0").unwrap();
    assert!(row.has_own("price") && row.get("price").unwrap().is_undefined());
    assert!(row.has_own("stopLoss") && row.get("stopLoss").unwrap().is_undefined());
    assert!(row.has_own("clientOrderId") && row.get("clientOrderId").unwrap().is_undefined());
}

#[test]
fn model_wire_bytes_and_native_presence_match_node_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-types/tests/support/model-field-corpus.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 126);
    for case in cases {
        let input: Value = serde_json::from_str(case["input"].as_str().unwrap()).unwrap();
        let value = from_json(&input);
        for (key, state) in case["fields"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["states"].as_array().unwrap())
        {
            if state == 1 {
                value.define(key.as_str().unwrap(), Js::Undefined).unwrap();
            }
        }
        let (native, wire) = if case["model"] == "order" {
            let model = to_typed::<Order>(&value).unwrap();
            (
                from_typed(&model).unwrap(),
                serde_json::to_value(&model).unwrap(),
            )
        } else {
            let model = to_typed::<Position>(&value).unwrap();
            (
                from_typed(&model).unwrap(),
                serde_json::to_value(&model).unwrap(),
            )
        };
        assert_eq!(
            stringify(&from_json(&wire)),
            case["wire"].as_str().unwrap(),
            "JSON codec round-trip {case}"
        );
        assert_eq!(stringify(&native), case["wire"].as_str().unwrap(), "{case}");
        let keys: Vec<_> = native
            .own_keys()
            .iter()
            .map(|key| String::from_utf16_lossy(key))
            .collect();
        assert_eq!(json!(keys), case["keys"], "own-key presence {case}");
    }
}

#[test]
fn prepared_input_reads_own_undefined_optional_fields_without_widening_public_input() {
    use safe_cex_types::{ModelField, PlaceOrderOpts, PreparedOrderOpts};
    let input = from_json(&json!({"symbol":"BTCUSDT", "amount":2}));
    for key in [
        "type",
        "side",
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "reduceOnly",
        "timeInForce",
        "triggerType",
        "tpTriggerType",
        "slTriggerType",
        "clientOrderId",
        "maxSlippagePct",
        "close",
        "impliedVolatility",
    ] {
        input.define(key, Js::Undefined).unwrap();
    }
    let prepared: PreparedOrderOpts = to_typed(&input).unwrap();
    assert!(matches!(prepared.r#type, ModelField::Undefined));
    assert!(matches!(prepared.side, ModelField::Undefined));
    assert!(prepared.price.is_none() && prepared.client_order_id.is_none());
    assert!(prepared.trigger_price.is_none() && prepared.stop_loss.is_none());
    assert!(prepared.take_profit.is_none() && prepared.reduce_only.is_none());
    assert!(prepared.time_in_force.is_none() && prepared.trigger_type.is_none());
    assert!(prepared.tp_trigger_type.is_none() && prepared.sl_trigger_type.is_none());
    assert!(prepared.max_slippage_pct.is_none() && prepared.close.is_none());
    assert!(prepared.implied_volatility.is_none());
    assert!(to_typed::<PlaceOrderOpts>(&input).is_err());
    input.define("price", Js::Null).unwrap();
    assert!(to_typed::<PreparedOrderOpts>(&input).is_err());
}
