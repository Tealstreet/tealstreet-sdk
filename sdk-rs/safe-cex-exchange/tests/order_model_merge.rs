//! Order upserts observe property presence through the production Store facade.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_json, to_json},
};
use safe_cex_num::js_json_stringify;
use safe_cex_store::store::DefaultStore;
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::Command, rc::Rc};

fn order(amount: f64) -> Js {
    from_json(
        &json!({"id":"o", "symbol":"BTC", "amount":amount, "filled":0,
        "remaining":amount, "reduceOnly":false, "lastUpdate":1}),
    )
}

fn carry(row: &Js, field: &str, state: u64, value: Js) {
    if state != 0 {
        row.define(field, if state == 1 { Js::Undefined } else { value })
            .unwrap();
    }
}

fn upsert(facade: &Js, method: &str, value: Js) {
    let argument = if method == "addOrUpdateOrders" {
        Js::array(vec![value])
    } else {
        value
    };
    facade.call_method(method, vec![argument]).unwrap();
}

#[test]
fn order_upserts_match_source_absent_undefined_and_value_for_all_model_fields() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/order_model_merge_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 270, "ORDER-MERGE source case floor");
    let mut fields = BTreeSet::new();
    let mut compared = 0;
    for case in cases {
        let field = case["field"].as_str().unwrap();
        fields.insert(field.to_owned());
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone());
        let old = order(2.0);
        carry(
            &old,
            field,
            case["oldState"].as_u64().unwrap(),
            from_json(&case["oldValue"]),
        );
        upsert(&facade, "addOrUpdateOrder", old);
        let incoming = order(3.0);
        carry(
            &incoming,
            field,
            case["newState"].as_u64().unwrap(),
            from_json(&case["newValue"]),
        );
        let method = case["method"].as_str().unwrap();
        upsert(&facade, method, incoming);
        assert_eq!(native.orders().len(), 1);
        let actual = facade.get("orders").unwrap().get("0").unwrap();
        let state = if !actual.has_own(field) {
            0
        } else if actual.get(field).unwrap().is_undefined() {
            1
        } else {
            2
        };
        assert_eq!(
            state, case["state"],
            "ORDER-MERGE {method} {field} old={} incoming={}",
            case["oldState"], case["newState"]
        );
        if state == 2 {
            assert_eq!(
                js_json_stringify(&to_json(&actual.get(field).unwrap()).unwrap().into()).unwrap(),
                js_json_stringify(&case["value"].clone().into()).unwrap(),
                "ORDER-MERGE {field} retained value"
            );
        }
        assert_eq!(native.orders()[0].amount, 3.0);
        compared += 1;
    }
    assert_eq!(compared, 270, "ORDER-MERGE native comparison floor");
    assert_eq!(
        fields,
        BTreeSet::from_iter(
            [
                "parentId",
                "status",
                "type",
                "side",
                "price",
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
            ]
            .map(String::from)
        )
    );
}

#[test]
fn absent_order_number_properties_preserve_native_bits_and_undefined_overwrites() {
    let mut compared = 0;
    for method in ["addOrUpdateOrder", "addOrUpdateOrders"] {
        for field in ["price", "takeProfit", "stopLoss"] {
            for number in [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let native = Rc::new(DefaultStore::new());
                let facade = store::facade(native);
                let old = order(2.0);
                old.define(field, Js::Number(number)).unwrap();
                upsert(&facade, "addOrUpdateOrder", old);
                upsert(&facade, method, order(3.0));
                let row = facade.get("orders").unwrap().get("0").unwrap();
                let Js::Number(actual) = row.get(field).unwrap() else {
                    panic!("ORDER-MERGE absent {field} lost its native number");
                };
                assert_eq!(
                    actual.to_bits(),
                    number.to_bits(),
                    "ORDER-MERGE {field} native bits"
                );
                let incoming = order(4.0);
                incoming.define(field, Js::Undefined).unwrap();
                upsert(&facade, method, incoming);
                let row = facade.get("orders").unwrap().get("0").unwrap();
                assert!(
                    row.has_own(field) && row.get(field).unwrap().is_undefined(),
                    "ORDER-MERGE {field} own undefined overwrites number"
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 24);
}
