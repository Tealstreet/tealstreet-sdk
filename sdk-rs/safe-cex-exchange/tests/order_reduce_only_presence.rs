//! reduceOnly presence through the native model codec and real Store facade.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_json, from_typed, to_typed},
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_types::Order;
use serde_json::{Value, json};
use std::{process::Command, rc::Rc};
fn row(amount: f64, state: u64) -> Js {
    let row = from_json(
        &json!({"id":"o","symbol":"BTC","amount":amount,"filled":0,"remaining":amount,"lastUpdate":1}),
    );
    if state != 0 {
        row.define(
            "reduceOnly",
            if state == 1 {
                Js::Undefined
            } else {
                Js::Bool(state == 3)
            },
        )
        .unwrap();
    }
    row
}
fn state(row: &Js) -> u64 {
    if !row.has_own("reduceOnly") {
        0
    } else {
        match row.get("reduceOnly").unwrap() {
            Js::Undefined => 1,
            Js::Bool(false) => 2,
            Js::Bool(true) => 3,
            value => panic!("reduceOnly lost its strict bool domain: {value:?}"),
        }
    }
}
#[test]
fn reduce_only_native_presence_and_store_updates_match_actual_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/order_reduce_only_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        cases.len(),
        48,
        "actual source three methods times four-by-four presence states"
    );
    for case in cases {
        let old_state = case["oldState"].as_u64().unwrap();
        let incoming_state = case["newState"].as_u64().unwrap();
        for expected in [old_state, incoming_state] {
            let decoded: Order = to_typed(&row(2., expected)).expect(
                "native Order must admit absent and own undefined reduceOnly without defaulting",
            );
            let prepared: safe_cex_types::PreparedOrderOpts = to_typed(&row(2., expected))
                .expect("internal prepared amendments retain the same strict presence carrier");
            assert_eq!(state(&from_typed(&prepared).unwrap()), expected);
            assert_eq!(
                state(&from_typed(&decoded).unwrap()),
                expected,
                "native codec preserves reduceOnly presence"
            );
            assert_eq!(
                serde_json::to_value(&decoded)
                    .unwrap()
                    .get("reduceOnly")
                    .cloned(),
                match expected {
                    0 | 1 => None,
                    2 => Some(json!(false)),
                    3 => Some(json!(true)),
                    _ => unreachable!(),
                }
            );
        }
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone()).unwrap();
        facade
            .call_method("addOrUpdateOrder", vec![row(2., old_state)])
            .unwrap();
        let incoming = row(3., incoming_state);
        let args = match case["method"].as_str().unwrap() {
            "addOrUpdateOrders" => vec![Js::array(vec![incoming])],
            "updateOrder" => vec![Js::object(vec![("id", Js::str("o"))]), incoming],
            "addOrUpdateOrder" => vec![incoming],
            method => panic!("unknown source method {method}"),
        };
        facade
            .call_method(case["method"].as_str().unwrap(), args)
            .unwrap();
        let actual = facade.get("orders").unwrap().get("0").unwrap();
        assert_eq!(
            state(&actual),
            case["state"].as_u64().unwrap(),
            "actual maintained Store {case}"
        );
        let expected: Value = serde_json::from_str(case["wire"].as_str().unwrap()).unwrap();
        let actual = serde_json::to_value(&native.orders().unwrap()[0]).unwrap();
        assert_eq!(actual.get("reduceOnly"), expected.get("reduceOnly"));
        assert_eq!(
            native.orders().unwrap()[0].amount,
            3.,
            "presence handling keeps other carried fields"
        );
    }
}
#[test]
fn reduce_only_output_and_placement_fields_refuse_non_booleans() {
    for invalid in [
        Js::Null,
        Js::Number(0.),
        Js::Number(1.),
        Js::str("false"),
        Js::str("true"),
        Js::array(vec![]),
        Js::object(vec![]),
    ] {
        let value = row(2., 2);
        value.define("reduceOnly", invalid.clone()).unwrap();
        assert!(
            to_typed::<Order>(&value).is_err(),
            "Order.reduceOnly must never coerce {invalid:?}"
        );
        let placement = from_json(&json!({"symbol":"BTC","type":"limit","side":"buy","amount":2}));
        placement.define("reduceOnly", invalid.clone()).unwrap();
        assert!(
            to_typed::<safe_cex_types::PlaceOrderOpts>(&placement).is_err(),
            "public placement stays strict for {invalid:?}"
        );
    }
}
