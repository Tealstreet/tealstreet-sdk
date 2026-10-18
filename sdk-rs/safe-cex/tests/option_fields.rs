//! Actual shared Store mutations and the JS facade preserve option model fields.
use safe_cex_exchange::js_runtime::{
    store::facade,
    value::{Js, from_json, from_typed, to_json},
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_types::{
    OptionGreeks, OptionRight, OptionTerms, OptionTermsSettlement, OptionTermsStyle, PositionOption,
};
use serde_json::{Value, json};
use std::rc::Rc;
fn tagged(value: &Js) -> Value {
    if let Js::Number(n) = value {
        return json!(if *n == 0.0 && n.is_sign_negative() {
            "-0".into()
        } else {
            safe_cex_num::js_number_string(*n)
        });
    }
    if value.is_undefined() {
        return Value::Null;
    }
    if value.is_object() {
        return Value::Object(
            value
                .own_entries()
                .unwrap()
                .into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), tagged(&v)))
                .collect(),
        );
    }
    to_json(value).unwrap()
}
fn snapshot(store: &Js) -> Value {
    json!({"market":tagged(&store.get("markets").unwrap().get("0").unwrap().get("option").unwrap()),"position":tagged(&store.get("positions").unwrap().get("0").unwrap().get("option").unwrap())})
}
#[test]
fn option_fields_survive_shared_store_patches_merges_and_source_facade_reads() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/option_store_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 5);
    let market = from_json(
        &json!({"id":"m","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":9},"leverage":{"min":1,"max":1}}}),
    );
    let position = from_json(
        &json!({"id":"p","symbol":"BTC","side":"long","entryPrice":1,"notional":1,"leverage":1,"contracts":1,"unrealizedPnl":0,"liquidationPrice":0}),
    );
    for (n, row) in [2.5, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]
        .into_iter()
        .zip(rows)
    {
        let core = Rc::new(DefaultStore::new());
        let store = facade(core);
        store
            .call_method(
                "update",
                vec![Js::object(vec![
                    ("markets", Js::array(vec![market.clone()])),
                    ("positions", Js::array(vec![position.clone()])),
                ])],
            )
            .unwrap();
        let terms = OptionTerms {
            underlying: "BTC".into(),
            expiry: n,
            strike: n,
            right: OptionRight::Put,
            style: OptionTermsStyle::European,
            settlement: OptionTermsSettlement::Cash,
            premium_asset: "USDT".into(),
            settle_asset: "USDT".into(),
        };
        let mut option = PositionOption {
            greeks: Some(OptionGreeks {
                delta: n,
                gamma: 3.0,
                vega: 2.0,
                theta: -4.0,
                units: "test".into(),
            }),
        };
        let mut snapshots = vec![];
        store
            .call_method(
                "updateMarket",
                vec![
                    Js::object(vec![("id", Js::str("m"))]),
                    Js::object(vec![("option", from_typed(&terms).unwrap())]),
                ],
            )
            .unwrap();
        snapshots.push(snapshot(&store));
        store
            .call_method(
                "updatePosition",
                vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    Js::object(vec![("option", from_typed(&option).unwrap())]),
                ],
            )
            .unwrap();
        snapshots.push(snapshot(&store));
        option.greeks.as_mut().unwrap().gamma = 8.0;
        store
            .call_method(
                "updatePositions",
                vec![Js::array(vec![Js::array(vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    Js::object(vec![("option", from_typed(&option).unwrap())]),
                ])])],
            )
            .unwrap();
        snapshots.push(snapshot(&store));
        for method in [
            "addOrUpdatePosition",
            "addOrUpdatePositions",
            "addOrUpdatePositionsFull",
        ] {
            let input = if method == "addOrUpdatePosition" {
                position.clone()
            } else {
                Js::array(vec![position.clone()])
            };
            store.call_method(method, vec![input]).unwrap();
            snapshots.push(snapshot(&store));
        }
        store
            .call_method(
                "update",
                vec![
                    Js::object(vec![("markets", Js::array(vec![market.clone()]))]),
                    Js::Bool(true),
                ],
            )
            .unwrap();
        snapshots.push(snapshot(&store));
        store
            .call_method(
                "updatePosition",
                vec![
                    Js::object(vec![("id", Js::str("p"))]),
                    Js::object(vec![("option", Js::object(vec![]))]),
                ],
            )
            .unwrap();
        snapshots.push(snapshot(&store));
        assert_eq!(snapshots.len(), 8);
        assert_eq!(
            json!(snapshots),
            row["snapshots"],
            "source numeric input {}",
            row["input"]
        );
    }
}
